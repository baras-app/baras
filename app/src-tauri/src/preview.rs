//! Editor preview of a timer/effect entry on its overlays
//!
//! The preview lives in `SharedState::preview`. The overlay router merges it into
//! live data passing through; `run` feeds the overlays that have no live data flowing.

use std::sync::Arc;
use std::time::{Duration, Instant};

use baras_core::OverlayHealthEntry;
use baras_core::dsl::format_placeholders;
use baras_overlay::icons::IconCache;
use baras_overlay::{
    AlertEntry, AlertsData, BossEffectIcon, BossHealthData, CooldownData, CooldownEntry, DotEntry, DotTarget,
    DotTrackerData, EffectABEntry, EffectsABData, OverlayData, RaidEffect, RaidFrame, RaidFrameData, TimerData,
    TimerEntry,
};
use baras_types::preview::{OverlayPreviewRequest, PreviewAlertTiming, PreviewAudio, PreviewTarget};

use crate::audio::{AudioEvent, AudioSender};

use crate::overlay::{OverlayCommand, OverlayType, SharedOverlayState};
use crate::state::SharedState;

/// How long a preview stays on screen (longer when its audio cues start earlier)
const PREVIEW_SECS: f32 = 8.0;
/// Fallback bar length for entries without a duration
const DEFAULT_TOTAL_SECS: f32 = 10.0;
/// A target that saw live data this recently is fed by the router, not by `run`
const LIVE_WINDOW: Duration = Duration::from_millis(150);
const TICK: Duration = Duration::from_millis(50);
const PREVIEW_ENTITY_ID: i64 = -1;
/// Sample names for `{source}`/`{target}` and the source/target display options
const SOURCE_NAME: &str = "Source";
const TARGET_NAME: &str = "Target";

type Icon = Arc<(u32, u32, Vec<u8>)>;

#[derive(Clone)]
pub struct ActivePreview {
    id: u64,
    text: String,
    total_secs: f32,
    /// Remaining time the preview starts at: it shows the tail end of the entry's
    /// duration so expiry-relative audio cues play at their real timing
    start_remaining: f32,
    color: [u8; 4],
    icon_ability_id: u64,
    icon: Option<Icon>,
    show_icon: bool,
    display_source: bool,
    show_target: bool,
    show_single_stack: bool,
    show_countdown: bool,
    stack_priority: bool,
    stacks: u8,
    stack_in_text: bool,
    targets: Vec<PreviewTarget>,
    started: Instant,
    last_live: [Option<Instant>; PreviewTarget::COUNT],
}

impl ActivePreview {
    fn is_running(&self) -> bool {
        self.remaining_secs() > 0.0
    }

    fn remaining_secs(&self) -> f32 {
        (self.start_remaining - self.started.elapsed().as_secs_f32()).max(0.0)
    }

    fn is_live(&self, target: PreviewTarget) -> bool {
        self.last_live[target as usize].is_some_and(|at| at.elapsed() < LIVE_WINDOW)
    }

    fn inject(&self, data: &mut OverlayData) {
        let remaining_secs = self.remaining_secs();
        match data {
            OverlayData::TimersA(d) | OverlayData::TimersB(d) => d.entries.push(TimerEntry {
                name: self.text.clone(),
                remaining_secs,
                total_secs: self.total_secs,
                color: self.color,
                icon_ability_id: self.show_icon.then_some(self.icon_ability_id),
                icon: self.icon.clone().filter(|_| self.show_icon),
            }),
            OverlayData::EffectsA(d) | OverlayData::EffectsB(d) | OverlayData::EffectsC(d) => {
                d.effects.push(EffectABEntry {
                    effect_id: self.icon_ability_id,
                    icon_ability_id: self.icon_ability_id,
                    name: self.text.clone(),
                    display_text: self.text.clone(),
                    remaining_secs,
                    total_secs: self.total_secs,
                    color: self.color,
                    // Same rule as live entries: a count of 1 only shows with show_single_stack
                    stacks: if self.show_single_stack || self.stacks > 1 {
                        self.stacks.max(1)
                    } else {
                        0
                    },
                    stack_in_text: self.stack_in_text,
                    source_name: SOURCE_NAME.to_string(),
                    target_name: TARGET_NAME.to_string(),
                    icon: self.icon.clone(),
                    show_icon: self.show_icon,
                    display_source: self.display_source,
                    show_target: self.show_target,
                    max_total_secs: None,
                    max_remaining_secs: None,
                    inactive: false,
                    stack_priority: self.stack_priority,
                    show_countdown: self.show_countdown,
                })
            }
            OverlayData::Cooldowns(d) | OverlayData::CooldownsB(d) => d.entries.push(CooldownEntry {
                ability_id: self.icon_ability_id,
                name: self.text.clone(),
                remaining_secs,
                total_secs: self.total_secs,
                icon_ability_id: self.icon_ability_id,
                charges: self.stacks,
                max_charges: self.stacks,
                color: self.color,
                source_name: SOURCE_NAME.to_string(),
                target_name: TARGET_NAME.to_string(),
                icon: self.icon.clone(),
                show_icon: self.show_icon,
                display_source: self.display_source,
                show_target: self.show_target,
                is_in_ready_state: false,
            }),
            OverlayData::DotTracker(d) => {
                let dot = DotEntry {
                    effect_id: self.icon_ability_id,
                    icon_ability_id: self.icon_ability_id,
                    name: self.text.clone(),
                    remaining_secs,
                    total_secs: self.total_secs,
                    color: self.color,
                    source_name: SOURCE_NAME.to_string(),
                    target_name: TARGET_NAME.to_string(),
                    icon: self.icon.clone(),
                    show_icon: self.show_icon,
                };
                match d.targets.first_mut() {
                    Some(target) => target.dots.push(dot),
                    None => d.targets.push(DotTarget {
                        entity_id: PREVIEW_ENTITY_ID,
                        name: "Preview Target".to_string(),
                        dots: vec![dot],
                    }),
                }
            }
            OverlayData::Raid(d) => {
                let mut effect = RaidEffect::new(self.icon_ability_id, self.text.clone())
                    .with_charges(if self.show_single_stack {
                        self.stacks.max(1)
                    } else {
                        self.stacks
                    })
                    .with_show_single_stack(self.show_single_stack)
                    .with_color_rgba(self.color)
                    .with_show_icon(self.show_icon)
                    .with_duration(Duration::from_secs_f32(self.total_secs))
                    .with_expiry(Instant::now() + Duration::from_secs_f32(remaining_secs));
                if let Some(icon) = &self.icon {
                    effect = effect.with_icon(icon.clone());
                }
                let own = d.frames.iter().position(|f| f.is_self).unwrap_or(0);
                match d.frames.get_mut(own) {
                    Some(frame) => frame.effects.push(effect),
                    None => d.frames.push(RaidFrame {
                        name: "Preview".to_string(),
                        hp_percent: 1.0,
                        effects: vec![effect],
                        ..RaidFrame::empty(0)
                    }),
                }
            }
            OverlayData::BossHealth(d) => {
                let entity_id = match d.entries.first() {
                    Some(entry) => entry.entity_id,
                    None => {
                        d.entries.push(OverlayHealthEntry {
                            entity_id: PREVIEW_ENTITY_ID,
                            name: "Preview Boss".to_string(),
                            target_name: None,
                            target_role: None,
                            current: 75,
                            max: 100,
                            first_seen_at: None,
                            hp_markers: Vec::new(),
                            active_shields: Vec::new(),
                            pushes_at: None,
                        });
                        PREVIEW_ENTITY_ID
                    }
                };
                d.boss_icons.entry(entity_id).or_default().push(BossEffectIcon {
                    effect_id: self.icon_ability_id,
                    icon_ability_id: self.icon_ability_id,
                    name: self.text.clone(),
                    remaining_secs,
                    total_secs: self.total_secs,
                    color: self.color,
                    show_icon: self.show_icon,
                    icon: self.icon.clone(),
                });
            }
            _ => {}
        }
    }
}

fn target_of(data: &OverlayData) -> Option<PreviewTarget> {
    Some(match data {
        OverlayData::TimersA(_) => PreviewTarget::TimersA,
        OverlayData::TimersB(_) => PreviewTarget::TimersB,
        OverlayData::EffectsA(_) => PreviewTarget::EffectsA,
        OverlayData::EffectsB(_) => PreviewTarget::EffectsB,
        OverlayData::EffectsC(_) => PreviewTarget::EffectsC,
        OverlayData::Cooldowns(_) => PreviewTarget::Cooldowns,
        OverlayData::CooldownsB(_) => PreviewTarget::CooldownsB,
        OverlayData::DotTracker(_) => PreviewTarget::DotTracker,
        OverlayData::Raid(_) => PreviewTarget::RaidFrames,
        OverlayData::BossHealth(_) => PreviewTarget::BossHealth,
        _ => return None,
    })
}

fn overlay_type(target: PreviewTarget) -> OverlayType {
    match target {
        PreviewTarget::TimersA => OverlayType::TimersA,
        PreviewTarget::TimersB => OverlayType::TimersB,
        PreviewTarget::EffectsA => OverlayType::EffectsA,
        PreviewTarget::EffectsB => OverlayType::EffectsB,
        PreviewTarget::EffectsC => OverlayType::EffectsC,
        PreviewTarget::Cooldowns => OverlayType::Cooldowns,
        PreviewTarget::CooldownsB => OverlayType::CooldownsB,
        PreviewTarget::DotTracker => OverlayType::DotTracker,
        PreviewTarget::RaidFrames => OverlayType::Raid,
        PreviewTarget::BossHealth => OverlayType::BossHealth,
        PreviewTarget::Alerts => OverlayType::Alerts,
    }
}

/// Merge the running preview into live overlay data on its way to an overlay thread
pub fn with_preview(shared: &SharedState, mut data: OverlayData) -> OverlayData {
    let mut slot = shared.preview.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(preview) = slot.as_mut()
        && let Some(target) = target_of(&data)
        && preview.targets.contains(&target)
        && preview.is_running()
    {
        preview.last_live[target as usize] = Some(Instant::now());
        preview.inject(&mut data);
    }
    data
}

/// Overlay data as it stands without the preview (what `run` builds on and restores to)
async fn base_data(
    target: PreviewTarget,
    shared: &Arc<SharedState>,
    icon_cache: Option<&Arc<IconCache>>,
    clearing: bool,
) -> OverlayData {
    match target {
        PreviewTarget::TimersA => OverlayData::TimersA(TimerData::default()),
        PreviewTarget::TimersB => OverlayData::TimersB(TimerData::default()),
        PreviewTarget::EffectsA => OverlayData::EffectsA(EffectsABData { effects: vec![] }),
        PreviewTarget::EffectsB => OverlayData::EffectsB(EffectsABData { effects: vec![] }),
        PreviewTarget::EffectsC => OverlayData::EffectsC(EffectsABData { effects: vec![] }),
        PreviewTarget::Cooldowns => OverlayData::Cooldowns(CooldownData::default()),
        PreviewTarget::CooldownsB => OverlayData::CooldownsB(CooldownData::default()),
        PreviewTarget::DotTracker => OverlayData::DotTracker(DotTrackerData::default()),
        PreviewTarget::RaidFrames => OverlayData::Raid(
            crate::service::build_raid_frame_data(shared, true, icon_cache)
                .await
                .unwrap_or(RaidFrameData { frames: vec![] }),
        ),
        PreviewTarget::BossHealth => OverlayData::BossHealth(BossHealthData {
            force_clear: clearing,
            ..Default::default()
        }),
        PreviewTarget::Alerts => OverlayData::Alerts(AlertsData { entries: vec![] }),
    }
}

/// Start a preview. Returns the labels of requested overlays that are not enabled.
pub async fn start(
    req: OverlayPreviewRequest,
    shared: Arc<SharedState>,
    overlay_state: SharedOverlayState,
    icon_cache: Option<Arc<IconCache>>,
    audio_tx: AudioSender,
) -> Result<Vec<&'static str>, String> {
    let txs: Vec<_> = {
        let state = overlay_state.lock().map_err(|e| e.to_string())?;
        req.targets
            .iter()
            .map(|&t| (t, state.get_tx(overlay_type(t)).cloned()))
            .collect()
    };
    let missing: Vec<_> = txs
        .iter()
        .filter(|(_, tx)| tx.is_none())
        .map(|(t, _)| t.label())
        .collect();
    let txs: Vec<_> = txs.into_iter().filter_map(|(t, tx)| Some((t, tx?))).collect();
    let icon_ability_id = req.icon_ability_id.unwrap_or(0);
    let icon = icon_cache.as_ref().and_then(|cache| cache.get_icon(icon_ability_id));
    let stacks = req.stacks.unwrap_or(0);
    let resolve = |text: &str| format_placeholders(text, SOURCE_NAME, TARGET_NAME, req.stacks);

    let total_secs = if req.duration_secs > 0.0 {
        req.duration_secs
    } else {
        DEFAULT_TOTAL_SECS
    };
    let audio_lead = req
        .audio
        .as_ref()
        .map_or(0.0, |a| a.offset.max(a.countdown_start) as f32 + 1.0);
    let alert_lead = match req.alert_timing {
        PreviewAlertTiming::Countdown(window) => window + 1.0,
        _ => 0.0,
    };
    let preview = ActivePreview {
        id: 0,
        text: resolve(&req.text),
        total_secs,
        start_remaining: total_secs.min(PREVIEW_SECS.max(audio_lead).max(alert_lead)),
        color: req.color,
        icon_ability_id,
        icon,
        show_icon: req.show_icon,
        display_source: req.display_source,
        show_target: req.show_target,
        show_single_stack: req.show_single_stack,
        show_countdown: req.show_countdown,
        stack_priority: req.stack_priority,
        stacks,
        stack_in_text: req.text.contains("{stack}"),
        targets: txs.iter().map(|(t, _)| *t).collect(),
        started: Instant::now(),
        last_live: [None; PreviewTarget::COUNT],
    };

    // Start alerts fire now; expire/countdown alerts are timed against expiry below
    let alert_text = resolve(req.alert_text.as_deref().unwrap_or(&req.text));
    let (alert_color, alert_icon_id, alert_icon) = (req.color, req.icon_ability_id, preview.icon.clone());
    let send_alert = move |tx: &tokio::sync::mpsc::Sender<OverlayCommand>, remaining: Option<f32>| {
        let entry = AlertEntry {
            // A stable id makes the overlay update the countdown entry in place
            id: remaining.map(|_| "preview".to_string()),
            text: match remaining {
                Some(secs) => format!("{alert_text} ({secs:.1})"),
                None => alert_text.clone(),
            },
            color: alert_color,
            created_at: Instant::now(),
            duration_secs: 5.0,
            remaining_secs: remaining,
            icon_ability_id: alert_icon_id,
            icon: alert_icon.clone(),
        };
        let _ = tx.try_send(OverlayCommand::UpdateData(OverlayData::Alerts(AlertsData {
            entries: vec![entry],
        })));
    };
    let alert_timing = req.alert_timing;
    let alert_tx = txs
        .iter()
        .find(|(t, _)| *t == PreviewTarget::Alerts)
        .map(|(_, tx)| tx.clone());
    let alert_tx = match (alert_tx, alert_timing) {
        (Some(tx), PreviewAlertTiming::Start) => {
            send_alert(&tx, None);
            None
        }
        (tx, _) => tx,
    };

    // Instant alerts sound with the alert; everything else is timed against expiry below
    let audio = match req.audio {
        Some(PreviewAudio {
            instant: true, file, ..
        }) => {
            let _ = audio_tx.try_send(AudioEvent::Alert {
                text: preview.text.clone(),
                custom_sound: file,
            });
            None
        }
        audio => audio,
    };

    let txs: Vec<_> = txs.into_iter().filter(|(t, _)| *t != PreviewTarget::Alerts).collect();
    if txs.is_empty() && audio.is_none() && alert_tx.is_none() {
        return Ok(missing);
    }

    let text = preview.text.clone();
    let start_remaining = preview.start_remaining;
    let id = {
        let mut slot = shared.preview.lock().unwrap_or_else(|p| p.into_inner());
        let id = slot.as_ref().map_or(1, |p| p.id.wrapping_add(1));
        *slot = Some(ActivePreview { id, ..preview });
        id
    };

    tauri::async_runtime::spawn(async move {
        // Starting here (not above) keeps countdown numbers beyond the duration silent
        let mut prev_remaining = start_remaining;
        loop {
            // Snapshot under the lock; a newer preview (different id) supersedes this one
            let (current, superseded) = {
                let mut slot = shared.preview.lock().unwrap_or_else(|p| p.into_inner());
                match slot.as_ref() {
                    Some(p) if p.id == id && p.is_running() => (Some(p.clone()), false),
                    Some(p) if p.id == id => {
                        *slot = None;
                        (None, false)
                    }
                    _ => (None, true),
                }
            };

            if let Some(audio) = &audio
                && !superseded
            {
                let remaining = current.as_ref().map_or(0.0, |p| p.remaining_secs());
                play_due_audio(&audio_tx, audio, &text, start_remaining, prev_remaining, remaining);
                prev_remaining = remaining;
            }

            if let Some(tx) = &alert_tx
                && !superseded
            {
                let remaining = current.as_ref().map_or(0.0, |p| p.remaining_secs());
                match alert_timing {
                    PreviewAlertTiming::Countdown(window) if remaining > 0.0 && remaining <= window => {
                        send_alert(tx, Some(remaining))
                    }
                    PreviewAlertTiming::Expire if current.is_none() => send_alert(tx, None),
                    _ => {}
                }
            }

            for (target, tx) in &txs {
                // Live data for this overlay already carries (or will drop) the preview
                if current.as_ref().is_some_and(|p| p.is_live(*target)) {
                    continue;
                }
                let mut data = base_data(*target, &shared, icon_cache.as_ref(), current.is_none()).await;
                if let Some(preview) = &current {
                    preview.inject(&mut data);
                }
                let _ = tx.send(OverlayCommand::UpdateData(data)).await;
            }

            if current.is_none() {
                break;
            }
            tokio::time::sleep(TICK).await;
        }
    });

    Ok(missing)
}

/// Fire the audio cues whose trigger point lies in `(remaining, prev_remaining]`,
/// mirroring the live timer/effect audio rules.
fn play_due_audio(
    audio_tx: &AudioSender,
    audio: &PreviewAudio,
    text: &str,
    start_remaining: f32,
    prev_remaining: f32,
    remaining: f32,
) {
    let crossed = |at: f32| prev_remaining > at && remaining <= at;

    // Voice countdown announces N as remaining enters [N, N+0.3)
    for seconds in 1..=audio.countdown_start {
        if crossed(seconds as f32 + 0.3) {
            let _ = audio_tx.try_send(AudioEvent::Countdown {
                timer_name: text.to_string(),
                seconds,
                voice_pack: audio.countdown_voice.clone().unwrap_or_else(|| "Amy".to_string()),
            });
        }
    }

    // Sound plays `offset` seconds before expiry (0 = on expiry), or right away when
    // the offset is longer than the duration
    if crossed((audio.offset as f32).min(start_remaining - TICK.as_secs_f32())) {
        let _ = audio_tx.try_send(AudioEvent::Alert {
            text: text.to_string(),
            custom_sound: audio.file.clone(),
        });
    }
}
