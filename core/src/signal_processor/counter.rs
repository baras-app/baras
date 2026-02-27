//! Counter increment and trigger checking logic.
//!
//! Counters track occurrences during boss encounters (e.g., add spawns, ability casts).
//! This module handles detecting when counters should increment based on game events.
//!
//! All trigger matching delegates to the unified functions in `trigger_eval`
//! to ensure consistent behavior across timers, phases, and counters.

use crate::combat_log::CombatEvent;
use crate::state::SessionCache;

use super::GameSignal;
use super::trigger_eval::{self, FilterContext};

/// Check for counter increments/decrements based on events and emit CounterChanged signals.
pub fn check_counter_increments(
    event: &CombatEvent,
    cache: &mut SessionCache,
    current_signals: &[GameSignal],
) -> Vec<GameSignal> {
    // Clone the Arc (cheap) to hold definitions while we mutate cache
    let (definitions, def_idx, boss_ids, local_player_id, current_target_id) = {
        let Some(enc) = cache.current_encounter() else {
            return Vec::new();
        };
        let Some(idx) = enc.active_boss_idx() else {
            return Vec::new();
        };
        let boss_ids = enc.boss_entity_ids();
        let local_player_id = Some(cache.player.id).filter(|&id| id != 0);
        let current_target_id =
            local_player_id.and_then(|pid| enc.local_player_target_id(pid));
        (enc.boss_definitions_arc(), idx, boss_ids, local_player_id, current_target_id)
    };
    let def = &definitions[def_idx];

    let filter_ctx = FilterContext {
        entities: &def.entities,
        local_player_id,
        current_target_id,
        boss_entity_ids: &boss_ids,
    };

    let mut signals = Vec::new();

    for counter in &def.counters {
        // Check increment_on trigger
        if check_counter_trigger(&counter.increment_on, event, current_signals, &filter_ctx) {
            let Some(enc) = cache.current_encounter_mut() else {
                tracing::error!(
                    "BUG: encounter missing in check_counter_increments (increment_on)"
                );
                continue;
            };
            let (old_value, new_value) = enc.modify_counter(
                &counter.id,
                counter.decrement, // Legacy: use decrement flag for increment_on
                counter.set_value,
            );

            signals.push(GameSignal::CounterChanged {
                counter_id: counter.id.clone(),
                old_value,
                new_value,
                timestamp: event.timestamp,
            });
        }

        // Check decrement_on trigger (always decrements)
        if let Some(ref decrement_trigger) = counter.decrement_on
            && check_counter_trigger(decrement_trigger, event, current_signals, &filter_ctx)
        {
            let Some(enc) = cache.current_encounter_mut() else {
                tracing::error!(
                    "BUG: encounter missing in check_counter_increments (decrement_on)"
                );
                continue;
            };
            let (old_value, new_value) = enc.modify_counter(
                &counter.id,
                true, // Always decrement
                None, // Never set_value for decrement_on
            );

            signals.push(GameSignal::CounterChanged {
                counter_id: counter.id.clone(),
                old_value,
                new_value,
                timestamp: event.timestamp,
            });
        }

        // Check reset_on trigger (resets to initial_value)
        if check_counter_trigger(&counter.reset_on, event, current_signals, &filter_ctx) {
            let Some(enc) = cache.current_encounter_mut() else {
                tracing::error!("BUG: encounter missing in check_counter_increments (reset_on)");
                continue;
            };
            let old_value = enc.get_counter(&counter.id);
            let new_value = counter.initial_value;

            // Only emit signal if value actually changes
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

/// Check for counter changes triggered by timer events (expires/starts).
/// Called after TimerManager processes signals to handle timer→counter triggers.
pub fn check_counter_timer_triggers(
    expired_timer_ids: &[String],
    started_timer_ids: &[String],
    cache: &mut SessionCache,
    timestamp: chrono::NaiveDateTime,
) -> Vec<GameSignal> {
    if expired_timer_ids.is_empty() && started_timer_ids.is_empty() {
        return Vec::new();
    }

    // Clone the Arc (cheap) to hold definitions while we mutate cache
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
        // Check increment_on for timer triggers
        if trigger_eval::check_timer_trigger(
            &counter.increment_on,
            expired_timer_ids,
            started_timer_ids,
        ) {
            let Some(enc) = cache.current_encounter_mut() else {
                tracing::error!(
                    "BUG: encounter missing in check_counter_timer_triggers (increment_on)"
                );
                continue;
            };
            let (old_value, new_value) =
                enc.modify_counter(&counter.id, counter.decrement, counter.set_value);
            signals.push(GameSignal::CounterChanged {
                counter_id: counter.id.clone(),
                old_value,
                new_value,
                timestamp,
            });
        }

        // Check decrement_on for timer triggers
        if let Some(ref trigger) = counter.decrement_on {
            if trigger_eval::check_timer_trigger(trigger, expired_timer_ids, started_timer_ids) {
                let Some(enc) = cache.current_encounter_mut() else {
                    tracing::error!(
                        "BUG: encounter missing in check_counter_timer_triggers (decrement_on)"
                    );
                    continue;
                };
                let (old_value, new_value) = enc.modify_counter(
                    &counter.id,
                    true, // Always decrement
                    None,
                );
                signals.push(GameSignal::CounterChanged {
                    counter_id: counter.id.clone(),
                    old_value,
                    new_value,
                    timestamp,
                });
            }
        }

        // Check reset_on for timer triggers
        if trigger_eval::check_timer_trigger(
            &counter.reset_on,
            expired_timer_ids,
            started_timer_ids,
        ) {
            let Some(enc) = cache.current_encounter_mut() else {
                tracing::error!(
                    "BUG: encounter missing in check_counter_timer_triggers (reset_on)"
                );
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
///
/// Delegates to the unified trigger evaluation functions in `trigger_eval`.
fn check_counter_trigger(
    trigger: &crate::dsl::Trigger,
    event: &CombatEvent,
    current_signals: &[GameSignal],
    filter_ctx: &FilterContext,
) -> bool {
    // Try event-based triggers first (from CombatEvent)
    if trigger_eval::check_event_trigger(trigger, event, Some(filter_ctx)) {
        return true;
    }

    // Then check signal-based triggers (from GameSignal)
    trigger_eval::check_signal_trigger(trigger, current_signals, filter_ctx)
}
