//! Counter increment and trigger checking logic.
//!
//! Counters track occurrences during boss encounters (e.g., add spawns, ability casts).
//! This module handles detecting when counters should increment based on game events.
//!
//! Counters can operate in two modes:
//! - **Manual triggers**: increment_on/decrement_on/reset_on fire on matching events
//! - **Effect stack tracking**: automatically tracks the stack count of a specified effect
//!
//! All trigger matching delegates to the unified functions in `trigger_eval`
//! to ensure consistent behavior across timers, phases, and counters.

use std::collections::HashSet;

use baras_types::StackAggregation;

use crate::combat_log::CombatEvent;
use crate::dsl::CounterDefinition;
use crate::dsl::EntityFilterMatching;
use crate::state::SessionCache;

use super::GameSignal;
use super::trigger_eval::{self, FilterContext};

/// Check for counter increments/decrements based on the raw combat event AND accumulated signals.
///
/// Called once per event at the start of the counter↔phase evaluation loop.
/// This handles event-based triggers (AbilityCast, EffectApplied, etc.) as well as
/// signal-based triggers against the full signal batch up to this point.
pub fn check_counter_increments(
    event: &CombatEvent,
    cache: &mut SessionCache,
    current_signals: &[GameSignal],
) -> Vec<GameSignal> {
    let (definitions, def_idx, boss_ids, local_player_id, current_target_id, entity_positions, pvp_factions) = {
        let Some(enc) = cache.current_encounter() else {
            return Vec::new();
        };
        let Some(idx) = enc.active_boss_idx() else {
            return Vec::new();
        };
        let boss_ids = enc.boss_entity_ids();
        let local_player_id = Some(cache.player.id).filter(|&id| id != 0);
        let current_target_id = local_player_id.and_then(|pid| enc.local_player_target_id(pid));
        (
            enc.boss_definitions_arc(),
            idx,
            boss_ids,
            local_player_id,
            current_target_id,
            enc.entity_positions(),
            enc.pvp_faction_share(),
        )
    };
    let def = &definitions[def_idx];

    let filter_ctx = FilterContext {
        entities: &def.entities,
        local_player_id,
        current_target_id,
        boss_entity_ids: boss_ids.as_ref(),
        entity_positions,
        pvp_factions,
    };

    let mut signals = Vec::new();

    for counter in &def.counters {
        if !counter.enabled {
            continue;
        }
        // Skip effect stack counters — they are handled by check_effect_stack_counters()
        if counter.track_effect_stacks.is_some() {
            continue;
        }
        // Check increment_on trigger (event + signals)
        if check_counter_trigger(&counter.increment_on, event, current_signals, &filter_ctx) {
            let Some(enc) = cache.current_encounter_mut() else {
                tracing::error!("BUG: encounter missing in check_counter_increments (increment_on)");
                continue;
            };
            let (old_value, new_value) = enc.modify_counter(&counter.id, counter.decrement, counter.set_value);

            signals.push(GameSignal::CounterChanged {
                counter_id: counter.id.clone(),
                old_value,
                new_value,
                timestamp: event.timestamp,
            });
        }

        // Check decrement_on trigger (event + signals)
        if let Some(ref decrement_trigger) = counter.decrement_on
            && check_counter_trigger(decrement_trigger, event, current_signals, &filter_ctx)
        {
            let Some(enc) = cache.current_encounter_mut() else {
                tracing::error!("BUG: encounter missing in check_counter_increments (decrement_on)");
                continue;
            };
            let (old_value, new_value) = enc.modify_counter(&counter.id, true, None);

            signals.push(GameSignal::CounterChanged {
                counter_id: counter.id.clone(),
                old_value,
                new_value,
                timestamp: event.timestamp,
            });
        }

        // Check reset_on trigger (event + signals)
        if check_counter_trigger(&counter.reset_on, event, current_signals, &filter_ctx) {
            let Some(enc) = cache.current_encounter_mut() else {
                tracing::error!("BUG: encounter missing in check_counter_increments (reset_on)");
                continue;
            };
            let old_value = enc.get_counter(&counter.id);
            let new_value = counter.initial_value;

            if old_value != new_value {
                enc.set_counter(&counter.id, new_value);
                signals.push(GameSignal::CounterChanged {
                    counter_id: counter.id.clone(),
                    old_value,
                    new_value,
                    timestamp: event.timestamp,
                });
            }
        }
    }

    signals
}

/// Check for counter increments/decrements based on NEW signals only (no event matching).
///
/// Called on each iteration of the counter↔phase fixed-point loop with only the
/// signals produced since the last watermark. This ensures counters react to
/// PhaseChanged, CounterChanged, and other signals without double-counting.
pub fn check_counter_signal_triggers(
    cache: &mut SessionCache,
    new_signals: &[GameSignal],
    timestamp: chrono::NaiveDateTime,
) -> Vec<GameSignal> {
    if new_signals.is_empty() {
        return Vec::new();
    }

    let (definitions, def_idx, boss_ids, local_player_id, current_target_id, entity_positions, pvp_factions) = {
        let Some(enc) = cache.current_encounter() else {
            return Vec::new();
        };
        let Some(idx) = enc.active_boss_idx() else {
            return Vec::new();
        };
        let boss_ids = enc.boss_entity_ids();
        let local_player_id = Some(cache.player.id).filter(|&id| id != 0);
        let current_target_id = local_player_id.and_then(|pid| enc.local_player_target_id(pid));
        (
            enc.boss_definitions_arc(),
            idx,
            boss_ids,
            local_player_id,
            current_target_id,
            enc.entity_positions(),
            enc.pvp_faction_share(),
        )
    };
    let def = &definitions[def_idx];

    let filter_ctx = FilterContext {
        entities: &def.entities,
        local_player_id,
        current_target_id,
        boss_entity_ids: boss_ids.as_ref(),
        entity_positions,
        pvp_factions,
    };

    let mut signals = Vec::new();

    for counter in &def.counters {
        if !counter.enabled {
            continue;
        }
        // Skip effect stack counters — they are handled by check_effect_stack_counters()
        if counter.track_effect_stacks.is_some() {
            continue;
        }
        // Check increment_on trigger (signals only)
        if trigger_eval::check_signal_trigger(&counter.increment_on, new_signals, &filter_ctx) {
            let Some(enc) = cache.current_encounter_mut() else {
                tracing::error!("BUG: encounter missing in check_counter_signal_triggers (increment_on)");
                continue;
            };
            let (old_value, new_value) = enc.modify_counter(&counter.id, counter.decrement, counter.set_value);

            signals.push(GameSignal::CounterChanged {
                counter_id: counter.id.clone(),
                old_value,
                new_value,
                timestamp,
            });
        }

        // Check decrement_on trigger (signals only)
        if let Some(ref decrement_trigger) = counter.decrement_on
            && trigger_eval::check_signal_trigger(decrement_trigger, new_signals, &filter_ctx)
        {
            let Some(enc) = cache.current_encounter_mut() else {
                tracing::error!("BUG: encounter missing in check_counter_signal_triggers (decrement_on)");
                continue;
            };
            let (old_value, new_value) = enc.modify_counter(&counter.id, true, None);

            signals.push(GameSignal::CounterChanged {
                counter_id: counter.id.clone(),
                old_value,
                new_value,
                timestamp,
            });
        }

        // Check reset_on trigger (signals only)
        if trigger_eval::check_signal_trigger(&counter.reset_on, new_signals, &filter_ctx) {
            let Some(enc) = cache.current_encounter_mut() else {
                tracing::error!("BUG: encounter missing in check_counter_signal_triggers (reset_on)");
                continue;
            };
            let old_value = enc.get_counter(&counter.id);
            let new_value = counter.initial_value;

            if old_value != new_value {
                enc.set_counter(&counter.id, new_value);
                signals.push(GameSignal::CounterChanged {
                    counter_id: counter.id.clone(),
                    old_value,
                    new_value,
                    timestamp,
                });
            }
        }
    }

    signals
}

/// Check for counter changes triggered by timer events (expires/starts).
/// Called after TimerManager processes signals to handle timer→counter triggers.
pub fn check_counter_timer_triggers(
    expired_timer_ids: &[String],
    started_timer_ids: &[String],
    canceled_timer_ids: &[String],
    cache: &mut SessionCache,
    timestamp: chrono::NaiveDateTime,
) -> Vec<GameSignal> {
    if expired_timer_ids.is_empty() && started_timer_ids.is_empty() && canceled_timer_ids.is_empty() {
        return Vec::new();
    }

    let (definitions, def_idx) = {
        let Some(enc) = cache.current_encounter() else {
            return Vec::new();
        };
        let Some(idx) = enc.active_boss_idx() else {
            return Vec::new();
        };
        (enc.boss_definitions_arc(), idx)
    };
    let def = &definitions[def_idx];

    let mut signals = Vec::new();

    for counter in &def.counters {
        if !counter.enabled {
            continue;
        }
        // Skip effect stack counters — they are handled by check_effect_stack_counters()
        if counter.track_effect_stacks.is_some() {
            continue;
        }
        // Check increment_on for timer triggers
        if trigger_eval::check_timer_trigger(
            &counter.increment_on,
            expired_timer_ids,
            started_timer_ids,
            canceled_timer_ids,
        ) {
            let Some(enc) = cache.current_encounter_mut() else {
                tracing::error!("BUG: encounter missing in check_counter_timer_triggers (increment_on)");
                continue;
            };
            let (old_value, new_value) = enc.modify_counter(&counter.id, counter.decrement, counter.set_value);
            signals.push(GameSignal::CounterChanged {
                counter_id: counter.id.clone(),
                old_value,
                new_value,
                timestamp,
            });
        }

        // Check decrement_on for timer triggers
        if let Some(ref trigger) = counter.decrement_on
            && trigger_eval::check_timer_trigger(trigger, expired_timer_ids, started_timer_ids, canceled_timer_ids)
        {
            let Some(enc) = cache.current_encounter_mut() else {
                tracing::error!("BUG: encounter missing in check_counter_timer_triggers (decrement_on)");
                continue;
            };
            let (old_value, new_value) = enc.modify_counter(&counter.id, true, None);
            signals.push(GameSignal::CounterChanged {
                counter_id: counter.id.clone(),
                old_value,
                new_value,
                timestamp,
            });
        }

        // Check reset_on for timer triggers
        if trigger_eval::check_timer_trigger(
            &counter.reset_on,
            expired_timer_ids,
            started_timer_ids,
            canceled_timer_ids,
        ) {
            let Some(enc) = cache.current_encounter_mut() else {
                tracing::error!("BUG: encounter missing in check_counter_timer_triggers (reset_on)");
                continue;
            };
            let old_value = enc.get_counter(&counter.id);
            let new_value = counter.initial_value;
            if old_value != new_value {
                enc.set_counter(&counter.id, new_value);
                signals.push(GameSignal::CounterChanged {
                    counter_id: counter.id.clone(),
                    old_value,
                    new_value,
                    timestamp,
                });
            }
        }
    }

    signals
}

/// Check if a counter trigger is satisfied by the current event/signals.
fn check_counter_trigger(
    trigger: &crate::dsl::Trigger,
    event: &CombatEvent,
    current_signals: &[GameSignal],
    filter_ctx: &FilterContext,
) -> bool {
    if trigger_eval::check_event_trigger(trigger, event, Some(filter_ctx)) {
        return true;
    }
    trigger_eval::check_signal_trigger(trigger, current_signals, filter_ctx)
}

// ═══════════════════════════════════════════════════════════════════════════
// Effect Stack Counter Evaluation
// ═══════════════════════════════════════════════════════════════════════════

/// Check for effect stack counter updates based on effect-related signals.
///
/// For counters with `track_effect_stacks` config, this:
/// 1. Updates per-entity effect stack state from EffectApplied/EffectChargesChanged/EffectRemoved
/// 2. Aggregates across entities matching the target filter
/// 3. Emits CounterChanged if the aggregated value changed
///
/// Called after effect signals are emitted, before the counter↔phase fixed-point loop.
pub fn check_effect_stack_counters(
    cache: &mut SessionCache,
    signals: &[GameSignal],
    timestamp: chrono::NaiveDateTime,
) -> Vec<GameSignal> {
    let (definitions, def_idx, boss_ids, local_player_id, current_target_id) = {
        let Some(enc) = cache.current_encounter() else {
            return Vec::new();
        };
        let Some(idx) = enc.active_boss_idx() else {
            return Vec::new();
        };
        let boss_ids = enc.boss_entity_ids();
        let local_player_id = Some(cache.player.id).filter(|&id| id != 0);
        let current_target_id = local_player_id.and_then(|pid| enc.local_player_target_id(pid));
        (
            enc.boss_definitions_arc(),
            idx,
            boss_ids,
            local_player_id,
            current_target_id,
        )
    };
    let def = &definitions[def_idx];

    // Collect effect stack counters
    let stack_counters: Vec<&CounterDefinition> = def
        .counters
        .iter()
        .filter(|c| c.enabled && c.track_effect_stacks.is_some())
        .collect();

    if stack_counters.is_empty() {
        return Vec::new();
    }

    // First pass: update per-entity stack state from signals
    let mut state_changed = false;
    for signal in signals {
        match signal {
            GameSignal::EffectApplied {
                effect_id,
                effect_name,
                target_id,
                charges,
                ..
            } => {
                let eff_name = crate::context::resolve(*effect_name);
                for counter in &stack_counters {
                    let config = counter.track_effect_stacks.as_ref().unwrap();
                    if config
                        .effects
                        .iter()
                        .any(|s| s.matches(*effect_id as u64, Some(eff_name)))
                    {
                        let stacks = charges.unwrap_or(1);
                        if let Some(enc) = cache.current_encounter_mut() {
                            enc.update_effect_stacks(*effect_id, *target_id, stacks);
                            state_changed = true;
                        }
                    }
                }
            }
            GameSignal::EffectChargesChanged {
                effect_id,
                effect_name,
                target_id,
                charges,
                ..
            } => {
                let eff_name = crate::context::resolve(*effect_name);
                for counter in &stack_counters {
                    let config = counter.track_effect_stacks.as_ref().unwrap();
                    if config
                        .effects
                        .iter()
                        .any(|s| s.matches(*effect_id as u64, Some(eff_name)))
                        && let Some(enc) = cache.current_encounter_mut()
                    {
                        enc.update_effect_stacks(*effect_id, *target_id, *charges);
                        state_changed = true;
                    }
                }
            }
            GameSignal::EffectRemoved {
                effect_id,
                effect_name,
                target_id,
                ..
            } => {
                let eff_name = crate::context::resolve(*effect_name);
                for counter in &stack_counters {
                    let config = counter.track_effect_stacks.as_ref().unwrap();
                    if config
                        .effects
                        .iter()
                        .any(|s| s.matches(*effect_id as u64, Some(eff_name)))
                        && let Some(enc) = cache.current_encounter_mut()
                    {
                        enc.remove_effect_stacks(*effect_id, *target_id);
                        state_changed = true;
                    }
                }
            }
            _ => {}
        }
    }

    if !state_changed {
        return Vec::new();
    }

    // Second pass: aggregate and update counter values
    let mut out = Vec::new();
    for counter in &stack_counters {
        let config = counter.track_effect_stacks.as_ref().unwrap();
        let Some(enc) = cache.current_encounter() else {
            break;
        };

        let aggregated = aggregate_effect_stacks(
            enc,
            config,
            &def.entities,
            local_player_id,
            current_target_id,
            boss_ids.as_ref(),
        );

        let old_value = enc.get_counter(&counter.id);
        if aggregated != old_value {
            // Need mutable access to update
            let Some(enc) = cache.current_encounter_mut() else {
                break;
            };
            enc.set_counter(&counter.id, aggregated);
            out.push(GameSignal::CounterChanged {
                counter_id: counter.id.clone(),
                old_value,
                new_value: aggregated,
                timestamp,
            });
        }
    }

    out
}

/// Aggregate effect stacks across entities matching the target filter.
fn aggregate_effect_stacks(
    enc: &crate::encounter::CombatEncounter,
    config: &baras_types::EffectStackConfig,
    entities: &[crate::dsl::EntityDefinition],
    local_player_id: Option<i64>,
    current_target_id: Option<i64>,
    boss_entity_ids: &HashSet<i64>,
) -> u32 {
    // Collect all matching effect IDs' stack maps
    let mut matching_stacks: Vec<u8> = Vec::new();

    for (effect_id, entity_stacks) in &enc.effect_stacks {
        // Check if this effect_id matches any selector in the config
        // We don't have the effect name here, so match by ID only.
        // Name-based matching was already handled during state updates.
        let matches_effect = config.effects.iter().any(|s| s.matches(*effect_id as u64, None));

        if !matches_effect {
            continue;
        }

        for (&entity_id, &stacks) in entity_stacks {
            // Apply target filter
            let entity_type = if enc.players.contains_key(&entity_id) {
                crate::combat_log::EntityType::Player
            } else {
                crate::combat_log::EntityType::Npc
            };
            let npc_id = enc.npcs.get(&entity_id).map(|n| n.class_id).unwrap_or(0);
            // For entity name, we use an empty IStr since we don't have it stored.
            // The filter matching for LocalPlayer/AnyPlayer/Boss uses entity_type and IDs,
            // not names, so this is fine for the common cases.
            let entity_name = enc
                .npcs
                .get(&entity_id)
                .map(|n| n.name)
                .or_else(|| enc.players.get(&entity_id).map(|p| p.name))
                .unwrap_or_default();

            if config.target.matches(
                entities,
                entity_id,
                entity_type,
                entity_name,
                npc_id,
                local_player_id,
                current_target_id,
                boss_entity_ids,
                enc.pvp_faction_context(),
            ) {
                matching_stacks.push(stacks);
            }
        }
    }

    if matching_stacks.is_empty() {
        return 0;
    }

    match config.aggregation {
        StackAggregation::Max => matching_stacks.iter().copied().max().unwrap_or(0) as u32,
        StackAggregation::Sum => matching_stacks.iter().copied().map(|s| s as u32).sum(),
        StackAggregation::Min => matching_stacks.iter().copied().min().unwrap_or(0) as u32,
    }
}
