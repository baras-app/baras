//! Phase transition logic for boss encounters.
//!
//! Phases represent distinct stages of a boss fight (e.g., "Walker 1", "Burn Phase").
//! This module handles detecting phase transitions based on various triggers.
//!
//! All trigger matching delegates to the unified functions in `trigger_eval`
//! to ensure consistent behavior across timers, phases, and counters.

use chrono::NaiveDateTime;

use crate::combat_log::CombatEvent;
use crate::dsl::Trigger;
use crate::state::SessionCache;

use super::trigger_eval::{self, FilterContext};
use super::GameSignal;

// ═══════════════════════════════════════════════════════════════════════════
// Phase Transition Checks
// ═══════════════════════════════════════════════════════════════════════════

/// Check for phase transitions based on HP changes.
pub fn check_hp_phase_transitions(
    cache: &mut SessionCache,
    old_hp: f32,
    new_hp: f32,
    npc_id: i64,
    entity_name: &str,
    timestamp: NaiveDateTime,
) -> Vec<GameSignal> {
    // First pass: find matching phase using immutable borrow
    let match_data = {
        let Some(enc) = cache.current_encounter() else {
            return Vec::new();
        };
        let Some(def_idx) = enc.active_boss_idx() else {
            return Vec::new();
        };

        let def = &enc.boss_definitions()[def_idx];
        let mut found = None;
        for phase in &def.phases {
            if enc.current_phase.as_ref() == Some(&phase.id) {
                continue;
            }

            if let Some(ref required) = phase.preceded_by {
                let last_phase = enc.current_phase.as_ref().or(enc.previous_phase.as_ref());
                if last_phase != Some(required) {
                    continue;
                }
            }

            // Check conditions (new unified + legacy counter_condition)
            if !enc.evaluate_merged_conditions(
                &phase.conditions,
                &[],
                phase.counter_condition.as_ref(),
            ) {
                continue;
            }

            if check_hp_trigger(
                &phase.start_trigger,
                &def.entities,
                old_hp,
                new_hp,
                npc_id,
                entity_name,
            ) {
                // Capture data needed for mutation and signal construction
                found = Some((
                    enc.current_phase.clone(), // old_phase for signal
                    phase.id.clone(),          // new_phase_id
                    def.id.clone(),            // boss_id
                    phase.resets_counters.clone(),
                    def.counters.clone(),
                ));
                break;
            }
        }
        found
    };

    // Second pass: mutate if we found a match
    if let Some((old_phase, new_phase_id, boss_id, resets, counter_defs)) = match_data {
        let Some(enc) = cache.current_encounter_mut() else {
            tracing::error!(
                "BUG: encounter disappeared mid-function in check_hp_phase_transitions"
            );
            return Vec::new();
        };
        enc.set_phase(&new_phase_id, timestamp);
        enc.reset_counters_to_initial(&resets, &counter_defs);
        enc.challenge_tracker.set_phase(&new_phase_id, timestamp);

        return vec![GameSignal::PhaseChanged {
            boss_id,
            old_phase,
            new_phase: new_phase_id,
            timestamp,
        }];
    }

    Vec::new()
}

/// Check for phase transitions based on ability/effect events.
pub fn check_ability_phase_transitions(
    event: &CombatEvent,
    cache: &mut SessionCache,
    current_signals: &[GameSignal],
) -> Vec<GameSignal> {
    // First pass: find matching phase using immutable borrow
    let match_data = {
        let Some(enc) = cache.current_encounter() else {
            return Vec::new();
        };
        let Some(def_idx) = enc.active_boss_idx() else {
            return Vec::new();
        };

        let def = &enc.boss_definitions()[def_idx];

        // Build filter context for source/target checking
        let boss_ids = enc.boss_entity_ids();
        let local_player_id = Some(cache.player.id).filter(|&id| id != 0);
        let current_target_id = local_player_id.and_then(|pid| enc.local_player_target_id(pid));
        let filter_ctx = FilterContext {
            entities: &def.entities,
            local_player_id,
            current_target_id,
            boss_entity_ids: &boss_ids,
        };

        let mut found = None;
        for phase in &def.phases {
            if enc.current_phase.as_ref() == Some(&phase.id) {
                continue;
            }

            if let Some(ref required) = phase.preceded_by {
                let last_phase = enc.current_phase.as_ref().or(enc.previous_phase.as_ref());
                if last_phase != Some(required) {
                    continue;
                }
            }

            // Check conditions (new unified + legacy counter_condition)
            if !enc.evaluate_merged_conditions(
                &phase.conditions,
                &[],
                phase.counter_condition.as_ref(),
            ) {
                continue;
            }

            let trigger_matched =
                trigger_eval::check_event_trigger(&phase.start_trigger, event, Some(&filter_ctx))
                    || trigger_eval::check_signal_trigger(
                        &phase.start_trigger,
                        current_signals,
                        &filter_ctx,
                    );

            if trigger_matched {
                // Capture data needed for mutation and signal construction
                found = Some((
                    enc.current_phase.clone(), // old_phase for signal
                    phase.id.clone(),          // new_phase_id
                    def.id.clone(),            // boss_id
                    phase.resets_counters.clone(),
                    def.counters.clone(),
                ));
                break;
            }
        }
        found
    };

    // Second pass: mutate if we found a match
    if let Some((old_phase, new_phase_id, boss_id, resets, counter_defs)) = match_data {
        let Some(enc) = cache.current_encounter_mut() else {
            tracing::error!(
                "BUG: encounter disappeared mid-function in check_ability_phase_transitions"
            );
            return Vec::new();
        };
        enc.set_phase(&new_phase_id, event.timestamp);
        enc.reset_counters_to_initial(&resets, &counter_defs);
        enc.challenge_tracker
            .set_phase(&new_phase_id, event.timestamp);

        return vec![GameSignal::PhaseChanged {
            boss_id,
            old_phase,
            new_phase: new_phase_id,
            timestamp: event.timestamp,
        }];
    }

    Vec::new()
}

/// Check for phase transitions based on entity signals (NpcAppears, EntityDeath, etc.).
pub fn check_entity_phase_transitions(
    cache: &mut SessionCache,
    current_signals: &[GameSignal],
    timestamp: NaiveDateTime,
) -> Vec<GameSignal> {
    // First pass: find matching phase using immutable borrow
    let match_data = {
        let Some(enc) = cache.current_encounter() else {
            return Vec::new();
        };
        let Some(def_idx) = enc.active_boss_idx() else {
            return Vec::new();
        };

        let def = &enc.boss_definitions()[def_idx];

        // Build filter context for signal-based trigger evaluation
        let boss_ids = enc.boss_entity_ids();
        let local_player_id = Some(cache.player.id).filter(|&id| id != 0);
        let current_target_id = local_player_id.and_then(|pid| enc.local_player_target_id(pid));
        let filter_ctx = FilterContext {
            entities: &def.entities,
            local_player_id,
            current_target_id,
            boss_entity_ids: &boss_ids,
        };

        let mut found = None;
        for phase in &def.phases {
            if enc.current_phase.as_ref() == Some(&phase.id) {
                continue;
            }

            if let Some(ref required) = phase.preceded_by {
                let last_phase = enc.current_phase.as_ref().or(enc.previous_phase.as_ref());
                if last_phase != Some(required) {
                    continue;
                }
            }

            // Check conditions (new unified + legacy counter_condition)
            if !enc.evaluate_merged_conditions(
                &phase.conditions,
                &[],
                phase.counter_condition.as_ref(),
            ) {
                continue;
            }

            if trigger_eval::check_signal_trigger(
                &phase.start_trigger,
                current_signals,
                &filter_ctx,
            ) {
                // Capture data needed for mutation and signal construction
                found = Some((
                    enc.current_phase.clone(), // old_phase for signal
                    phase.id.clone(),          // new_phase_id
                    def.id.clone(),            // boss_id
                    phase.resets_counters.clone(),
                    def.counters.clone(),
                ));
                break; // Only one phase transition per event
            }
        }
        found
    };

    // Second pass: mutate if we found a match
    if let Some((old_phase, new_phase_id, boss_id, resets, counter_defs)) = match_data {
        let Some(enc) = cache.current_encounter_mut() else {
            tracing::error!(
                "BUG: encounter disappeared mid-function in check_entity_phase_transitions"
            );
            return Vec::new();
        };
        enc.set_phase(&new_phase_id, timestamp);
        enc.reset_counters_to_initial(&resets, &counter_defs);
        enc.challenge_tracker.set_phase(&new_phase_id, timestamp);

        return vec![GameSignal::PhaseChanged {
            boss_id,
            old_phase,
            new_phase: new_phase_id,
            timestamp,
        }];
    }

    Vec::new()
}

/// Check for phase transitions based on combat time (TimeElapsed triggers).
pub fn check_time_phase_transitions(
    cache: &mut SessionCache,
    timestamp: NaiveDateTime,
) -> Vec<GameSignal> {
    // First: update combat time (requires mutable borrow)
    let (old_time, new_time) = {
        let Some(enc) = cache.current_encounter_mut() else {
            return Vec::new();
        };
        if enc.active_boss_idx().is_none() {
            return Vec::new();
        }
        enc.update_combat_time(timestamp)
    };

    if new_time <= old_time {
        return Vec::new();
    }

    // Second pass: find matching phase using immutable borrow
    let match_data = {
        let Some(enc) = cache.current_encounter() else {
            tracing::error!(
                "BUG: encounter disappeared after update_combat_time in check_time_phase_transitions"
            );
            return Vec::new();
        };
        let Some(def_idx) = enc.active_boss_idx() else {
            tracing::error!(
                "BUG: no active boss after update_combat_time in check_time_phase_transitions"
            );
            return Vec::new();
        };

        let def = &enc.boss_definitions()[def_idx];

        let mut found = None;
        for phase in &def.phases {
            if enc.current_phase.as_ref() == Some(&phase.id) {
                continue;
            }

            if let Some(ref required) = phase.preceded_by {
                let last_phase = enc.current_phase.as_ref().or(enc.previous_phase.as_ref());
                if last_phase != Some(required) {
                    continue;
                }
            }

            // Check conditions (new unified + legacy counter_condition)
            if !enc.evaluate_merged_conditions(
                &phase.conditions,
                &[],
                phase.counter_condition.as_ref(),
            ) {
                continue;
            }

            if check_time_trigger(&phase.start_trigger, old_time, new_time) {
                // Capture data needed for mutation and signal construction
                found = Some((
                    enc.current_phase.clone(), // old_phase for signal
                    phase.id.clone(),          // new_phase_id
                    def.id.clone(),            // boss_id
                    phase.resets_counters.clone(),
                    def.counters.clone(),
                ));
                break;
            }
        }
        found
    };

    // Third pass: mutate if we found a match
    if let Some((old_phase, new_phase_id, boss_id, resets, counter_defs)) = match_data {
        let Some(enc) = cache.current_encounter_mut() else {
            tracing::error!(
                "BUG: encounter disappeared mid-function in check_time_phase_transitions"
            );
            return Vec::new();
        };
        enc.set_phase(&new_phase_id, timestamp);
        enc.reset_counters_to_initial(&resets, &counter_defs);
        enc.challenge_tracker.set_phase(&new_phase_id, timestamp);

        return vec![GameSignal::PhaseChanged {
            boss_id,
            old_phase,
            new_phase: new_phase_id,
            timestamp,
        }];
    }

    Vec::new()
}

/// Check for phase transitions triggered by timer events (expires/starts).
/// Called after TimerManager processes signals to handle timer→phase triggers.
///
/// This is the phase-system counterpart to `check_counter_timer_triggers()`.
pub fn check_timer_phase_transitions(
    expired_timer_ids: &[String],
    started_timer_ids: &[String],
    cache: &mut SessionCache,
    timestamp: NaiveDateTime,
) -> Vec<GameSignal> {
    if expired_timer_ids.is_empty() && started_timer_ids.is_empty() {
        return Vec::new();
    }

    let mut all_signals = Vec::new();

    // Check phase start triggers against timer events
    let start_match = {
        let Some(enc) = cache.current_encounter() else {
            return Vec::new();
        };
        let Some(def_idx) = enc.active_boss_idx() else {
            return Vec::new();
        };

        let def = &enc.boss_definitions()[def_idx];

        let mut found = None;
        for phase in &def.phases {
            if enc.current_phase.as_ref() == Some(&phase.id) {
                continue;
            }

            if let Some(ref required) = phase.preceded_by {
                let last_phase = enc.current_phase.as_ref().or(enc.previous_phase.as_ref());
                if last_phase != Some(required) {
                    continue;
                }
            }

            // Check conditions (new unified + legacy counter_condition)
            if !enc.evaluate_merged_conditions(
                &phase.conditions,
                &[],
                phase.counter_condition.as_ref(),
            ) {
                continue;
            }

            if trigger_eval::check_timer_trigger(
                &phase.start_trigger,
                expired_timer_ids,
                started_timer_ids,
            ) {
                found = Some((
                    enc.current_phase.clone(),
                    phase.id.clone(),
                    def.id.clone(),
                    phase.resets_counters.clone(),
                    def.counters.clone(),
                ));
                break;
            }
        }
        found
    };

    if let Some((old_phase, new_phase_id, boss_id, resets, counter_defs)) = start_match {
        let Some(enc) = cache.current_encounter_mut() else {
            tracing::error!(
                "BUG: encounter disappeared mid-function in check_timer_phase_transitions"
            );
            return Vec::new();
        };
        enc.set_phase(&new_phase_id, timestamp);
        enc.reset_counters_to_initial(&resets, &counter_defs);
        enc.challenge_tracker.set_phase(&new_phase_id, timestamp);

        all_signals.push(GameSignal::PhaseChanged {
            boss_id,
            old_phase,
            new_phase: new_phase_id,
            timestamp,
        });
    }

    // Check current phase's end trigger against timer events
    let end_match = {
        let Some(enc) = cache.current_encounter() else {
            return all_signals;
        };
        let Some(def_idx) = enc.active_boss_idx() else {
            return all_signals;
        };
        let Some(current_phase_id) = &enc.current_phase else {
            return all_signals;
        };

        let def = &enc.boss_definitions()[def_idx];
        let Some(phase) = def.phases.iter().find(|p| &p.id == current_phase_id) else {
            return all_signals;
        };
        let Some(ref end_trigger) = phase.end_trigger else {
            return all_signals;
        };

        if trigger_eval::check_timer_trigger(end_trigger, expired_timer_ids, started_timer_ids) {
            Some(current_phase_id.clone())
        } else {
            None
        }
    };

    if let Some(phase_id) = end_match {
        all_signals.push(GameSignal::PhaseEndTriggered {
            phase_id,
            timestamp,
        });
    }

    all_signals
}

/// Check if the current phase's end_trigger fired.
/// Emits PhaseEndTriggered signal which other phases can use as a start_trigger.
pub fn check_phase_end_triggers(
    event: &CombatEvent,
    cache: &SessionCache,
    current_signals: &[GameSignal],
) -> Vec<GameSignal> {
    let Some(enc) = cache.current_encounter() else {
        return Vec::new();
    };
    let Some(def_idx) = enc.active_boss_idx() else {
        return Vec::new();
    };
    let Some(current_phase_id) = &enc.current_phase else {
        return Vec::new();
    };

    let def = &enc.boss_definitions()[def_idx];

    let Some(phase) = def.phases.iter().find(|p| &p.id == current_phase_id) else {
        return Vec::new();
    };

    let Some(ref end_trigger) = phase.end_trigger else {
        return Vec::new();
    };

    // Build filter context for source/target checking
    let boss_ids = enc.boss_entity_ids();
    let local_player_id = Some(cache.player.id).filter(|&id| id != 0);
    let current_target_id = local_player_id.and_then(|pid| enc.local_player_target_id(pid));
    let filter_ctx = FilterContext {
        entities: &def.entities,
        local_player_id,
        current_target_id,
        boss_entity_ids: &boss_ids,
    };

    // Check ability/effect-based triggers (from CombatEvent)
    if trigger_eval::check_event_trigger(end_trigger, event, Some(&filter_ctx)) {
        return vec![GameSignal::PhaseEndTriggered {
            phase_id: current_phase_id.clone(),
            timestamp: event.timestamp,
        }];
    }

    // Check all signal-based triggers (entity death, phase entered/ended, counter,
    // HP thresholds, combat start, damage taken, healing taken, etc.)
    if trigger_eval::check_signal_trigger(end_trigger, current_signals, &filter_ctx) {
        return vec![GameSignal::PhaseEndTriggered {
            phase_id: current_phase_id.clone(),
            timestamp: event.timestamp,
        }];
    }

    Vec::new()
}

// ═══════════════════════════════════════════════════════════════════════════
// Trigger Matching Helpers (delegate to unified Trigger methods)
// ═══════════════════════════════════════════════════════════════════════════

/// Check if an HP-based phase trigger is satisfied.
/// Delegates to unified `Trigger::matches_boss_hp_below` and `matches_boss_hp_above`.
pub fn check_hp_trigger(
    trigger: &Trigger,
    entities: &[crate::dsl::EntityDefinition],
    old_hp: f32,
    new_hp: f32,
    npc_id: i64,
    entity_name: &str,
) -> bool {
    trigger.matches_boss_hp_below(entities, npc_id, entity_name, old_hp, new_hp)
        || trigger.matches_boss_hp_above(entities, npc_id, entity_name, old_hp, new_hp)
}

/// Check if a TimeElapsed trigger is satisfied (time crossed threshold).
/// Delegates to unified `Trigger::matches_time_elapsed`.
pub fn check_time_trigger(trigger: &Trigger, old_time: f32, new_time: f32) -> bool {
    trigger.matches_time_elapsed(old_time, new_time)
}
