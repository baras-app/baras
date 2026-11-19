//! Timer overlay data with audio events

use crate::state::SharedState;
use baras_core::timers::FiredAlert;
use baras_overlay::{TimerData, TimerEntry};
use std::sync::Arc;
use std::sync::atomic::Ordering;

/// Named bundle returned by `build_timer_data_with_audio`.
pub(super) struct TimerDataBundle {
    pub(super) timers_a: TimerData,
    pub(super) timers_b: TimerData,
    pub(super) ability_queue: baras_overlay::AbilityQueueData,
    pub(super) countdowns: Vec<(String, u8, String)>,
    pub(super) alerts: Vec<FiredAlert>,
}

/// Build timer data with audio events (countdowns and alerts).
///
/// Fans out each timer to all of its `display_targets` (TimersA, TimersB, and/or
/// AbilityQueue), so a single timer can appear on multiple overlays simultaneously.
/// Assembles the GCD entry from `active_gcd` and queued entries from timers with
/// `is_queued = true` (which bypass the `remaining <= 0.0` guard).
pub(super) async fn build_timer_data_with_audio(
    shared: &Arc<SharedState>,
    icon_cache: Option<&Arc<baras_overlay::icons::IconCache>>,
) -> Option<TimerDataBundle> {
    use baras_core::timers::TimerDisplayTarget;
    use baras_overlay::{AbilityQueueData, AbilityQueueEntry};

    let session_guard = shared.session.read().await;
    let session = session_guard.as_ref()?;
    let session = session.read().await;

    // Get active timers from timer manager (Live mode only, mutable for countdown checking)
    let timer_mgr = session.timer_manager()?;
    let mut timer_mgr = timer_mgr.lock().unwrap_or_else(|p| p.into_inner());

    // Always take alerts (even after combat ends, timer expirations need to play)
    let mut alerts = timer_mgr.take_fired_alerts();

    // Check for audio offset alerts (early warning sounds before timer expires)
    let offset_alerts = timer_mgr.check_audio_offsets();
    alerts.extend(offset_alerts);

    // Also get alerts from effect tracker (effect start/end alerts)
    if let Some(effect_tracker) = session.effect_tracker() {
        let mut tracker = effect_tracker.lock().unwrap_or_else(|p| p.into_inner());
        alerts.extend(tracker.take_fired_alerts());
    }

    // If not in combat, return only alerts (no countdown checks)
    let in_combat = shared.in_combat.load(Ordering::SeqCst);
    if !in_combat {
        return Some(TimerDataBundle {
            timers_a: TimerData::default(),
            timers_b: TimerData::default(),
            ability_queue: AbilityQueueData::default(),
            countdowns: Vec::new(),
            alerts,
        });
    }

    // Check for countdowns to announce (uses realtime internally)
    let countdowns = timer_mgr.check_all_countdowns();

    // Compute interpolated game time once for all timers
    let interp_time = timer_mgr.interpolated_game_time();

    let mut entries_a = Vec::new();
    let mut entries_b = Vec::new();
    let mut aq_entries: Vec<AbilityQueueEntry> = Vec::new();

    // Pre-collect remaining-seconds of all currently-active (non-queued) timers
    // keyed by definition_id so we can mark ability queue entries as blocked
    // only when a blocker will still be running when the current GCD resolves.
    // Multi-target instances of the same definition collapse to the longest
    // remaining (pessimistic blocker). Names are intentionally not used here
    // because they're display-only and not guaranteed unique.
    let blocker_remaining: std::collections::HashMap<&str, f32> = {
        let mut map: std::collections::HashMap<&str, f32> = std::collections::HashMap::new();
        for t in timer_mgr.active_timers() {
            if t.is_queued {
                continue;
            }
            let rem = interp_time.map(|ti| t.remaining_secs(ti)).unwrap_or(0.0);
            map.entry(t.definition_id.as_str())
                .and_modify(|cur| *cur = cur.max(rem))
                .or_insert(rem);
        }
        map
    };
    let gcd_remaining = timer_mgr
        .active_gcd()
        .and_then(|g| interp_time.map(|ti| g.remaining_secs(ti)))
        .unwrap_or(0.0);

    // GCD tier-1 entry: synthesized from active_gcd if present
    if let Some(gcd) = timer_mgr.active_gcd()
        && let Some(t) = interp_time
    {
        let remaining = gcd.remaining_secs(t);
        let total = gcd
            .expires_at
            .signed_duration_since(gcd.started_at)
            .num_milliseconds()
            .max(0) as f32
            / 1000.0;
        aq_entries.push(AbilityQueueEntry {
            definition_id: String::new(),
            name: String::new(),
            remaining_secs: remaining,
            total_secs: total,
            color: [255, 255, 255, 255],
            queue_priority: 255,
            is_pinned: true,
            is_queued: false,
            is_blocked: false,
            countdown_bar: false,
            hide_from_next: false,
            icon_ability_id: None,
            icon: None,
        });
    }

    for timer in timer_mgr.active_timers() {
        // Skip timers hidden by role filtering
        if timer.role_hidden {
            continue;
        }

        let remaining = interp_time.map(|t| timer.remaining_secs(t)).unwrap_or(0.0);

        // Load icon from cache if ability ID is set
        let icon = timer.icon_ability_id.and_then(|ability_id| {
            icon_cache.and_then(|cache| {
                cache
                    .get_icon(ability_id)
                    .map(|data| std::sync::Arc::new((data.width, data.height, data.rgba)))
            })
        });

        let total_secs = if timer.show_at_secs > 0.0 {
            timer.show_at_secs
        } else {
            timer.duration.as_secs_f32()
        };

        // Timers A / B: progress-bar entries. Filtered to active, visible timers.
        let bar_visible = remaining > 0.0 && timer.is_visible(remaining);
        if bar_visible
            && (timer.displays_on(TimerDisplayTarget::TimersA) || timer.displays_on(TimerDisplayTarget::TimersB))
        {
            let entry = TimerEntry {
                name: timer.name.clone(),
                remaining_secs: remaining,
                total_secs,
                color: timer.color,
                icon_ability_id: timer.icon_ability_id,
                icon: icon.clone(),
            };
            if timer.displays_on(TimerDisplayTarget::TimersA) {
                entries_a.push(entry.clone());
            }
            if timer.displays_on(TimerDisplayTarget::TimersB) {
                entries_b.push(entry);
            }
        }

        // Ability Queue: queued entries (is_queued=true) bypass the remaining <= 0.0 guard.
        // Also treat a queue_on_expire timer that has reached zero but hasn't been
        // ticked yet as already queued — otherwise the row disappears for one frame
        // (when remaining hits 0 the `remaining > 0.0` filter drops it) and pops
        // back as READY on the next tick, causing visible flicker and layout
        // reflow on the rows below.
        let pending_queue = timer.queue_on_expire && !timer.is_queued && !timer.can_repeat() && remaining <= 0.0;
        let display_is_queued = timer.is_queued || pending_queue;
        if timer.displays_on(TimerDisplayTarget::AbilityQueue) && (display_is_queued || remaining > 0.0) {
            let blocked_by_timer = timer.queue_blocking_timers.iter().any(|id| {
                blocker_remaining
                    .get(id.as_str())
                    .is_some_and(|&rem| rem > gcd_remaining)
            });
            let current_encounter = session.session_cache.as_ref().and_then(|c| c.current_encounter());
            let blocked_by_condition = match (&timer.queue_blocking_condition, current_encounter) {
                (Some(c), Some(enc)) => enc.evaluate_condition(c),
                _ => false,
            };
            let is_blocked = blocked_by_timer || blocked_by_condition;
            aq_entries.push(AbilityQueueEntry {
                definition_id: timer.definition_id.clone(),
                name: timer.name.clone(),
                remaining_secs: remaining,
                total_secs,
                color: timer.color,
                queue_priority: timer.queue_priority,
                is_pinned: false,
                is_queued: display_is_queued,
                is_blocked,
                countdown_bar: timer.queue_countdown_bar,
                hide_from_next: timer.queue_hide_from_next,
                icon_ability_id: timer.icon_ability_id,
                icon,
            });
        }
    }

    // ─── Ability Queue: unique-next audio cue ────────────────────────────────
    // Determine the timer that solo-occupies the highest-priority eligible
    // slot, then ask TimerManager whether that's a fresh transition (with a
    // 500ms dedup window to absorb frame-boundary jitter near the GCD edge).
    let unique_next_id: Option<String> = {
        let max_prio = aq_entries
            .iter()
            .filter(|e| {
                !e.is_pinned && !e.is_blocked && !e.hide_from_next && (e.is_queued || e.remaining_secs <= gcd_remaining)
            })
            .map(|e| e.queue_priority)
            .max();
        max_prio.and_then(|p| {
            let mut winners = aq_entries.iter().filter(|e| {
                !e.is_pinned
                    && !e.is_blocked
                    && !e.hide_from_next
                    && (e.is_queued || e.remaining_secs <= gcd_remaining)
                    && e.queue_priority == p
            });
            let first = winners.next()?;
            if winners.next().is_some() {
                None
            } else {
                Some(first.definition_id.clone())
            }
        })
    };
    if let Some(now) = interp_time {
        // Pull the candidate's at_gcd_remaining threshold (if any) before the
        // mutable call below — borrow checker won't let active_timers() and
        // update_uniquely_next() overlap.
        let threshold_met = unique_next_id
            .as_deref()
            .map(|id| {
                let threshold = timer_mgr
                    .active_timers()
                    .into_iter()
                    .find(|t| t.definition_id == id)
                    .and_then(|t| t.queue_next_audio.as_ref())
                    .and_then(|a| a.at_gcd_remaining);
                threshold.is_none_or(|t| gcd_remaining <= t)
            })
            .unwrap_or(true);
        let fire = timer_mgr.update_uniquely_next(
            unique_next_id.as_deref(),
            threshold_met,
            now,
            chrono::Duration::milliseconds(500),
        );
        if let Some(fire_id) = fire
            && let Some(timer) = timer_mgr
                .active_timers()
                .into_iter()
                .find(|t| t.definition_id == fire_id)
            && let Some(audio) = timer.queue_next_audio.as_ref().filter(|a| a.enabled)
        {
            alerts.push(baras_core::timers::FiredAlert {
                id: timer.definition_id.clone(),
                name: timer.name.clone(),
                text: timer.name.clone(),
                color: Some(timer.color),
                timestamp: now,
                alert_text_enabled: false,
                audio_enabled: true,
                audio_file: audio.file.clone(),
                icon_ability_id: timer.icon_ability_id,
                remaining_secs: None,
            });
        }
    }

    Some(TimerDataBundle {
        timers_a: TimerData { entries: entries_a },
        timers_b: TimerData { entries: entries_b },
        ability_queue: AbilityQueueData { entries: aq_entries },
        countdowns,
        alerts,
    })
}
