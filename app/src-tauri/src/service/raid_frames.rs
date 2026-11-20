//! Raid frame overlay data

use super::effect_data::calculate_base_remaining_secs;
use super::raid_detection;
use crate::state::MutexExt;
use crate::state::SharedState;
use baras_core::context::resolve;
use baras_core::game_data::{Discipline, Role};
use baras_core::{ActiveEffect, DisplayTarget};
use baras_overlay::{PlayerRole, RaidEffect, RaidFrame, RaidFrameData};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tracing::{debug, info};

/// Give OCR names their log identity as soon as the roster knows them.
///
/// Registration will otherwise wait until combat, not when probing someone.
pub(super) async fn promote_provisional_slots(shared: &Arc<SharedState>) {
    let provisional = {
        let registry = shared.raid_registry.locked();
        registry.provisional_entries()
    };
    if provisional.is_empty() {
        return;
    }

    let waiting = provisional.len();
    let candidates = raid_detection::raid_detection_candidates(shared).await;
    if candidates.is_empty() {
        debug!("Provisional match skipped: {waiting} names waiting, no log roster yet");
        return;
    }

    // Name only, as health total and percentage are not reliable at this stage.
    let observations: Vec<baras_core::raid_detect::RowObservation> = provisional
        .into_iter()
        .map(|(slot, name)| baras_core::raid_detect::RowObservation {
            row: slot as usize,
            name_text: Some(name),
            ..Default::default()
        })
        .collect();

    let assignments = baras_core::raid_detect::assign_rows(
        &observations,
        &candidates,
        &baras_core::raid_detect::MatchConfig::default(),
    );
    // Event-driven now, so silence must differ from not running.
    if assignments.is_empty() {
        debug!(
            waiting,
            candidates = candidates.len(),
            "Provisional match ran, nothing promoted"
        );
        return;
    }

    for assignment in &assignments {
        info!(
            slot = assignment.row,
            name = %assignment.name,
            confidence = assignment.confidence,
            "Promoted provisional name to a log player"
        );
    }

    {
        let mut registry = shared.raid_registry.locked();
        registry.assign_slots(
            assignments
                .into_iter()
                .filter_map(|a| u8::try_from(a.row).ok().map(|row| (row, a.entity_id, a.name))),
        );
    }
}

/// Build raid frame data from the effect tracker and registry
///
/// Uses RaidSlotRegistry to maintain stable player positions.
/// Players are registered ONLY when the local player applies a NEW effect to them
/// (via the new_targets queue), not on every tick.
fn provisional_raid_frame(slot: u8, name: &str, ambiguous: bool) -> RaidFrame {
    RaidFrame {
        slot,
        player_id: None,
        name: name.to_string(),
        hp_percent: 1.0,
        role: PlayerRole::Dps,
        class_icon: None,
        effects: Vec::new(),
        is_self: false,
        ambiguous,
    }
}

fn provisional_raid_frame_data(shared: &Arc<SharedState>) -> Option<RaidFrameData> {
    let registry = shared.raid_registry.locked();
    let frames: Vec<_> = (0..registry.max_slots())
        .filter_map(|slot| {
            registry
                .get_provisional(slot)
                .map(|name| provisional_raid_frame(slot, name, registry.is_ambiguous(slot)))
        })
        .collect();
    (!frames.is_empty()).then_some(RaidFrameData { frames })
}

pub(crate) async fn build_raid_frame_data(
    shared: &Arc<SharedState>,
    rearranging: bool,
    icon_cache: Option<&Arc<baras_overlay::icons::IconCache>>,
) -> Option<RaidFrameData> {
    let session_guard = shared.session.read().await;
    let Some(session) = session_guard.as_ref() else {
        return provisional_raid_frame_data(shared);
    };
    let session = session.read().await;

    // Get effect tracker (Live mode only)
    let Some(effect_tracker) = session.effect_tracker() else {
        return provisional_raid_frame_data(shared);
    };
    let mut tracker = effect_tracker.lock().unwrap_or_else(|poisoned| {
        tracing::warn!("Effect tracker mutex was poisoned, recovering");
        poisoned.into_inner()
    });

    // Lock registry (recover from poison to keep raid frames working)
    let mut registry = shared.raid_registry.lock().unwrap_or_else(|poisoned| {
        tracing::warn!("Raid registry mutex was poisoned, recovering");
        poisoned.into_inner()
    });

    // Early out: skip building data if no effects AND no registered players
    // We need to keep sending updates while effects exist OR players are registered
    // so that removals/clears are reflected in the overlay
    // Skip this check in rearrange mode to always show frames
    if !rearranging && !tracker.has_active_effects() && registry.is_empty() {
        return None;
    }

    // Compute interpolated game time once for all effects
    let interp_time = tracker.interpolated_game_time();

    // Get local player ID for is_self flag
    let local_player_id = session.session_cache.as_ref().map(|c| c.player.id).unwrap_or(0);

    // Process new targets queue - these are entities that JUST received an effect from local player
    // The registry handles duplicate rejection via try_register
    let new_targets = tracker.take_new_targets();
    if !new_targets.is_empty() {
        // Remember these players past a frame clear: a re-run OCR pass can
        // only rebind names it still has candidates for, and out-of-combat
        // targets never reach the discipline roster.
        let mut roster = shared.ability_roster.locked();
        for target in new_targets {
            let name = resolve(target.name).to_string();
            if let Some(at) = session.last_event_time {
                roster.observe_raw(target.entity_id, &name, (0, 0), at);
            }
            // A new player is the only thing that can settle a provisional slot, so
            // it is what schedules the match rather than a timer.
            if registry.try_register(target.entity_id, name).is_some() {
                shared.roster_changed.store(true, Ordering::Relaxed);
            }
        }
    }

    // Group effects by target for registered players only
    let mut effects_by_target: std::collections::HashMap<i64, Vec<RaidEffect>> = std::collections::HashMap::new();

    for effect in tracker.active_effects() {
        // Skip effects not destined for raid frames or already removed
        if !effect.display_targets.contains(&DisplayTarget::RaidFrames)
            || effect.removed_at.is_some()
            || effect.timer_expired
        {
            continue;
        }

        let target_id = effect.target_entity_id;

        // Only group effects for already-registered players
        if registry.is_registered(target_id) {
            let show_single_stack = tracker
                .definition(&effect.definition_id)
                .is_some_and(|d| d.show_single_stack);
            effects_by_target
                .entry(target_id)
                .or_default()
                .push(convert_to_raid_effect(
                    effect,
                    icon_cache,
                    interp_time,
                    show_single_stack,
                ));
        }
    }

    // Build frames from registry (stable slot order)
    let max_slots = registry.max_slots();
    let mut frames = Vec::with_capacity(max_slots as usize);

    for slot in 0..max_slots {
        if let Some(player) = registry.get_player(slot) {
            let mut effects = effects_by_target.remove(&player.entity_id).unwrap_or_default();

            // Sort effects by effect_id for stable visual ordering
            effects.sort_by_key(|e| e.effect_id);

            // Map discipline to role and class icon (defaults to DPS if unknown)
            let discipline = player.discipline_id.and_then(Discipline::from_guid);

            let role = discipline
                .map(|d| match d.role() {
                    Role::Tank => PlayerRole::Tank,
                    Role::Healer => PlayerRole::Healer,
                    Role::Dps => PlayerRole::Dps,
                })
                .unwrap_or(PlayerRole::Dps);

            let class_icon = discipline.map(|d| d.icon_name().to_string());

            frames.push(RaidFrame {
                slot,
                player_id: Some(player.entity_id),
                name: player.name.clone(),
                hp_percent: 1.0,
                role,
                class_icon,
                effects,
                is_self: player.entity_id == local_player_id,
                ambiguous: registry.is_ambiguous(slot),
            });
        } else if let Some(name) = registry.get_provisional(slot) {
            frames.push(provisional_raid_frame(slot, name, registry.is_ambiguous(slot)));
        }
    }

    Some(RaidFrameData { frames })
}

/// Convert an ActiveEffect (core) to RaidEffect (overlay)
///
/// Uses interpolated game time to compute remaining seconds, then derives
/// a monotonic `Instant` expiry for the overlay's own countdown rendering.
fn convert_to_raid_effect(
    effect: &ActiveEffect,
    icon_cache: Option<&Arc<baras_overlay::icons::IconCache>>,
    interp_time: Option<chrono::NaiveDateTime>,
    show_single_stack: bool,
) -> RaidEffect {
    let charges = if show_single_stack {
        effect.stacks.max(1)
    } else {
        effect.stacks
    };
    // Effects on raid frames are typically HoTs/shields (is_buff defaults to true in RaidEffect::new())
    let mut raid_effect = RaidEffect::new(effect.game_effect_id, effect.name.clone())
        .with_charges(charges)
        .with_show_single_stack(show_single_stack)
        .with_color_rgba(effect.color)
        .with_show_icon(effect.show_icon);

    // Compute expiry Instant from game-time remaining (base duration only —
    // the cooldown ready-state tail is a cooldowns-overlay concept)
    if let Some(dur) = effect.duration {
        let remaining_secs = calculate_base_remaining_secs(effect, interp_time).unwrap_or(0.0);
        if remaining_secs > 0.0 {
            let base_dur = dur.saturating_sub(std::time::Duration::from_secs_f32(effect.cooldown_ready_secs));
            let remaining_dur = std::time::Duration::from_secs_f32(remaining_secs);
            let expires_at = std::time::Instant::now() + remaining_dur;
            raid_effect = raid_effect.with_duration(base_dur).with_expiry(expires_at);
        }
    }

    // Load icon if cache available
    if let Some(cache) = icon_cache
        && let Some(data) = cache.get_icon(effect.icon_ability_id)
    {
        raid_effect = raid_effect.with_icon(data);
    }

    raid_effect
}
