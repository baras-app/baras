//! Effect audio and Effects A/B, cooldown and DoT overlay data

use crate::state::MutexExt;
use crate::state::SharedState;
use baras_core::context::resolve;
use baras_core::effects::EffectAudio;
use baras_core::{ActiveEffect, DisplayTarget};
use baras_overlay::{CooldownData, CooldownEntry, DotEntry, DotTarget, DotTrackerData, EffectABEntry, EffectsABData};
use std::sync::Arc;

/// Effect audio due on this poll (Live mode only)
pub(super) async fn process_effect_audio(shared: &std::sync::Arc<SharedState>) -> Vec<EffectAudio> {
    let session_guard = shared.session.read().await;
    let Some(session_arc) = session_guard.as_ref() else {
        return Vec::new();
    };
    let session = session_arc.read().await;
    let Some(effect_tracker) = session.effect_tracker() else {
        return Vec::new();
    };
    let mut tracker = effect_tracker.locked();
    tracker.take_audio()
}

/// Calculate remaining time for an effect in seconds
/// Uses interpolated game time for accurate values without clock skew
pub(super) fn calculate_remaining_secs(
    effect: &ActiveEffect,
    interp_time: Option<chrono::NaiveDateTime>,
) -> Option<f32> {
    let remaining = interp_time.and_then(|t| effect.remaining_secs(t)).unwrap_or(0.0);
    if remaining <= 0.0 { None } else { Some(remaining) }
}

/// Like `calculate_remaining_secs` but excludes the cooldown ready-state tail.
/// Non-cooldown overlays use this so an effect that also displays on the
/// cooldowns overlay disappears when its base duration ends instead of
/// lingering (with inflated time) through the ready state.
pub(super) fn calculate_base_remaining_secs(
    effect: &ActiveEffect,
    interp_time: Option<chrono::NaiveDateTime>,
) -> Option<f32> {
    let remaining = interp_time.and_then(|t| effect.remaining_secs(t)).unwrap_or(0.0);
    let base = effect.remaining_base_secs(remaining);
    if base <= 0.0 { None } else { Some(base) }
}

/// Base total duration in seconds (excludes the cooldown ready-state tail)
pub(super) fn base_total_secs(effect: &ActiveEffect) -> Option<f32> {
    effect
        .duration
        .map(|d| (d.as_secs_f32() - effect.cooldown_ready_secs).max(0.0))
}

/// Load an icon's RGBA data from the cache
fn load_ab_icon(
    icon_cache: Option<&Arc<baras_overlay::icons::IconCache>>,
    ability_id: u64,
) -> Option<Arc<(u32, u32, Vec<u8>)>> {
    icon_cache.and_then(|cache| cache.get_icon(ability_id))
}

/// Convert an active effect to an Effects A/B display entry.
/// `pinned` (uptime) entries ignore the show_at_secs threshold and allow
/// indefinite durations (rendered without a countdown).
/// `def` supplies per-effect display flags (stack emphasis, single-stack).
fn active_to_ab_entry(
    effect: &ActiveEffect,
    interp_time: Option<chrono::NaiveDateTime>,
    icon_cache: Option<&Arc<baras_overlay::icons::IconCache>>,
    pinned: bool,
    def: Option<&baras_core::EffectDefinition>,
) -> Option<EffectABEntry> {
    if !pinned {
        let remaining_total = interp_time.and_then(|t| effect.remaining_secs(t)).unwrap_or(0.0);
        // Skip effects hidden by show_at_secs threshold
        if !effect.is_visible(remaining_total) {
            return None;
        }
    }
    let (total_secs, remaining_secs) = if pinned {
        (
            base_total_secs(effect).unwrap_or(0.0),
            calculate_base_remaining_secs(effect, interp_time).unwrap_or(0.0),
        )
    } else {
        (
            base_total_secs(effect)?,
            calculate_base_remaining_secs(effect, interp_time)?,
        )
    };

    let max_total_secs = effect
        .max_expires_at
        .map(|max_exp| (max_exp - effect.applied_at).num_milliseconds() as f32 / 1000.0);
    let max_remaining_secs = effect
        .max_expires_at
        .and_then(|max_exp| interp_time.map(|t| (max_exp - t).num_milliseconds() as f32 / 1000.0));

    // show_single_stack: always display a count, floored at 1.
    // Otherwise hide a lone "1" (matching raid frames) — real counts show from 2.
    let stacks = if def.is_some_and(|d| d.show_single_stack) {
        effect.stacks.max(1)
    } else if effect.stacks > 1 {
        effect.stacks
    } else {
        0
    };

    Some(EffectABEntry {
        effect_id: effect.game_effect_id,
        icon_ability_id: effect.icon_ability_id,
        name: effect.name.clone(),
        display_text: effect.display_text.clone(),
        remaining_secs,
        total_secs,
        color: effect.color,
        stacks,
        stack_in_text: effect.stack_in_text(),
        source_name: resolve(effect.source_name).to_string(),
        target_name: resolve(effect.target_name).to_string(),
        icon: load_ab_icon(icon_cache, effect.icon_ability_id),
        show_icon: effect.show_icon,
        display_source: effect.display_source,
        show_target: effect.show_target && effect.target_entity_id != effect.source_entity_id,
        max_total_secs,
        max_remaining_secs,
        inactive: false,
        stack_priority: def.is_some_and(|d| d.stack_priority),
        show_countdown: def.is_none_or(|d| d.show_countdown),
    })
}

/// Clear "show when absent" uptime placeholders after a player logout
/// (blank new log file or game process exit). The overlay data loop picks
/// up the cleared tracker state on its next tick.
pub(super) async fn clear_uptime_tracking(shared: &Arc<SharedState>) {
    let session_guard = shared.session.read().await;
    let Some(session) = session_guard.as_ref() else {
        return;
    };
    let session = session.read().await;
    if let Some(effect_tracker) = session.effect_tracker() {
        let mut tracker = effect_tracker.locked();
        tracker.clear_uptime_tracking();
    }
}

/// Build Effects A/B overlay data: uptime entries pinned first (gray
/// placeholders while inactive), then regular active effects by apply time.
pub(super) async fn build_effects_ab_data(
    shared: &Arc<SharedState>,
    icon_cache: Option<&Arc<baras_overlay::icons::IconCache>>,
    target: DisplayTarget,
) -> Option<EffectsABData> {
    let session_guard = shared.session.read().await;
    let session = session_guard.as_ref()?;
    let session = session.read().await;

    let effect_tracker = session.effect_tracker()?;
    let mut tracker = effect_tracker.locked();

    tracker.remember_uptime_icons();
    let tracker = &*tracker;
    let uptime_defs = tracker.uptime_definitions(target);
    if !tracker.has_active_effects() && uptime_defs.is_empty() {
        return None;
    }

    let interp_time = tracker.interpolated_game_time();

    let mut effects: Vec<&ActiveEffect> = match target {
        DisplayTarget::EffectsB => tracker.effects_b().collect(),
        DisplayTarget::EffectsC => tracker.effects_c().collect(),
        _ => tracker.effects_a().collect(),
    };
    effects.sort_by_key(|e| e.applied_at);

    let mut entries: Vec<EffectABEntry> = Vec::with_capacity(uptime_defs.len() + effects.len());

    for def in &uptime_defs {
        let entry = effects
            .iter()
            .find(|e| e.definition_id == def.id)
            .and_then(|e| active_to_ab_entry(e, interp_time, icon_cache, true, Some(def)))
            .unwrap_or_else(|| {
                let icon_id = tracker.uptime_placeholder_icon(def);
                EffectABEntry {
                    effect_id: icon_id,
                    icon_ability_id: icon_id,
                    name: def.name.clone(),
                    display_text: def.display_text().to_string(),
                    remaining_secs: 0.0,
                    total_secs: 0.0,
                    color: def.effective_color(),
                    stacks: 0,
                    stack_in_text: false,
                    source_name: String::new(),
                    target_name: String::new(),
                    icon: load_ab_icon(icon_cache, icon_id),
                    show_icon: def.show_icon,
                    display_source: false,
                    show_target: false,
                    max_total_secs: None,
                    max_remaining_secs: None,
                    inactive: true,
                    stack_priority: def.stack_priority,
                    show_countdown: def.show_countdown,
                }
            });
        entries.push(entry);
    }

    let uptime_ids: std::collections::HashSet<&str> = uptime_defs.iter().map(|d| d.id.as_str()).collect();
    entries.extend(
        effects
            .iter()
            .filter(|e| !uptime_ids.contains(e.definition_id.as_str()))
            .filter_map(|e| {
                active_to_ab_entry(e, interp_time, icon_cache, false, tracker.definition(&e.definition_id))
            }),
    );

    Some(EffectsABData { effects: entries })
}

/// Build cooldowns overlay data from active effects
pub(super) async fn build_cooldowns_data(
    shared: &Arc<SharedState>,
    icon_cache: Option<&Arc<baras_overlay::icons::IconCache>>,
    target: DisplayTarget,
) -> Option<CooldownData> {
    let session_guard = shared.session.read().await;
    let session = session_guard.as_ref()?;
    let session = session.read().await;

    let effect_tracker = session.effect_tracker()?;
    let tracker = effect_tracker.locked();

    if !tracker.has_active_effects() {
        return None;
    }

    let interp_time = tracker.interpolated_game_time();

    let mut effects: Vec<_> = match target {
        DisplayTarget::CooldownsB => tracker.cooldown_b_effects().collect(),
        _ => tracker.cooldown_effects().collect(),
    };

    // Sort by remaining time (shortest first)
    effects.sort_by(|a, b| {
        let a_remaining = calculate_remaining_secs(a, interp_time).unwrap_or(f32::MAX);
        let b_remaining = calculate_remaining_secs(b, interp_time).unwrap_or(f32::MAX);
        a_remaining
            .partial_cmp(&b_remaining)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let entries: Vec<CooldownEntry> = effects
        .into_iter()
        .filter_map(|effect| {
            let remaining_total = interp_time.and_then(|t| effect.remaining_secs(t)).unwrap_or(0.0);
            // Skip effects hidden by show_at_secs threshold
            if !effect.is_visible(remaining_total) {
                return None;
            }
            // Duration includes ready_secs for tracker lifetime, subtract for display
            let tracker_total = effect.duration?.as_secs_f32();
            let total_secs = tracker_total - effect.cooldown_ready_secs;

            // Remaining time until tracker expires.
            // Do NOT use `?` here — include entries at 0 as "Ready" until the tracker's
            // next tick marks them timer_expired and removes them from cooldown_effects().
            // Between log events the interpolated clock can pass expires_at before the
            // tracker has a chance to run tick(), so dropping at 0 would cut the ready
            // state short (or skip it entirely for effects with cooldown_ready_secs = 0).
            let tracker_remaining = interp_time.and_then(|t| effect.remaining_secs(t)).unwrap_or(0.0);

            // Display remaining = tracker remaining minus ready period (clamped to 0)
            // So display hits 0 when entering ready state, not when effect disappears
            let remaining_secs = (tracker_remaining - effect.cooldown_ready_secs).max(0.0);

            // In ready state whenever the display countdown has reached 0.
            // Removing the cooldown_ready_secs > 0 guard so entries without an explicit
            // ready period also show "Ready" briefly until the tracker clears them.
            let is_in_ready_state = remaining_secs <= 0.0;

            // Load icon from cache
            let icon = icon_cache.and_then(|cache| cache.get_icon(effect.icon_ability_id));

            Some(CooldownEntry {
                ability_id: effect.game_effect_id,
                icon_ability_id: effect.icon_ability_id,
                name: effect.display_text.clone(),
                remaining_secs,
                total_secs,
                color: effect.color,
                charges: effect.stacks,
                max_charges: effect.stacks, // Default to current stacks (no max info available)
                source_name: resolve(effect.source_name).to_string(),
                target_name: resolve(effect.target_name).to_string(),
                icon,
                show_icon: effect.show_icon,
                display_source: effect.display_source,
                show_target: effect.show_target && effect.target_entity_id != effect.source_entity_id,
                is_in_ready_state,
            })
        })
        .collect();

    Some(CooldownData { entries })
}

/// Build DOT tracker overlay data from active effects
pub(super) async fn build_dot_tracker_data(
    shared: &Arc<SharedState>,
    icon_cache: Option<&Arc<baras_overlay::icons::IconCache>>,
) -> Option<DotTrackerData> {
    let session_guard = shared.session.read().await;
    let session = session_guard.as_ref()?;
    let session = session.read().await;

    let effect_tracker = session.effect_tracker()?;
    let tracker = effect_tracker.locked();

    if !tracker.has_active_effects() {
        return None;
    }

    let interp_time = tracker.interpolated_game_time();

    // Get DOTs grouped by target
    let dots_by_target = tracker.dot_tracker_effects();
    if dots_by_target.is_empty() {
        return None;
    }

    let mut targets: Vec<DotTarget> = dots_by_target
        .into_iter()
        .filter_map(|(target_id, effects)| {
            let target_name = resolve(effects.first()?.target_name).to_string();

            let dots: Vec<DotEntry> = effects
                .into_iter()
                .filter_map(|effect| {
                    let remaining_total = interp_time.and_then(|t| effect.remaining_secs(t)).unwrap_or(0.0);
                    // Skip effects hidden by show_at_secs threshold
                    if !effect.is_visible(remaining_total) {
                        return None;
                    }
                    let total_secs = base_total_secs(effect)?;
                    let remaining_secs = calculate_base_remaining_secs(effect, interp_time)?;

                    // Load icon from cache
                    let icon = icon_cache.and_then(|cache| cache.get_icon(effect.icon_ability_id));

                    Some(DotEntry {
                        effect_id: effect.game_effect_id,
                        icon_ability_id: effect.icon_ability_id,
                        name: if effect.display_text.is_empty() {
                            effect.name.clone()
                        } else {
                            effect.display_text.clone()
                        },
                        remaining_secs,
                        total_secs,
                        color: effect.color,
                        source_name: resolve(effect.source_name).to_string(),
                        target_name: resolve(effect.target_name).to_string(),
                        icon,
                        show_icon: effect.show_icon,
                    })
                })
                .collect();

            if dots.is_empty() {
                return None;
            }

            Some(DotTarget {
                entity_id: target_id,
                name: target_name,
                dots,
            })
        })
        .collect();

    // Sort targets by entity ID for stable ordering
    targets.sort_by_key(|t| t.entity_id);

    Some(DotTrackerData { targets })
}
