//! Phase transition logic for boss encounters.
//!
//! Phases represent distinct stages of a boss fight (e.g., "Walker 1", "Burn Phase").
//! This module handles detecting phase transitions based on various triggers.

use chrono::NaiveDateTime;

use crate::combat_log::CombatEvent;
use crate::dsl::EntityDefinition;
use crate::dsl::Trigger;
use crate::game_data::{effect_id, effect_type_id};
use crate::state::SessionCache;

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
    let Some(enc) = cache.current_encounter() else {
        return Vec::new();
    };
    let Some(def_idx) = enc.active_boss_idx() else {
        return Vec::new();
    };

    let def = &enc.boss_definitions()[def_idx];
    let counter_defs = def.counters.clone();
    let current_phase = enc.current_phase.clone();
    let previous_phase = enc.previous_phase.clone();

    for phase in &def.phases {
        if current_phase.as_ref() == Some(&phase.id) {
            continue;
        }

        if let Some(ref required) = phase.preceded_by {
            let last_phase = current_phase.as_ref().or(previous_phase.as_ref());
            if last_phase != Some(required) {
                continue;
            }
        }

        if let Some(ref cond) = phase.counter_condition {
            if !enc.check_counter_condition(cond) {
                continue;
            }
        }

        if check_hp_trigger(
            &phase.start_trigger,
            &def.entities,
            old_hp,
            new_hp,
            npc_id,
            entity_name,
        ) {
            let old_phase = enc.current_phase.clone();
            let new_phase_id = phase.id.clone();
            let boss_id = def.id.clone();
            let resets = phase.resets_counters.clone();

            let enc = cache.current_encounter_mut().unwrap();
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
    }

    Vec::new()
}

/// Check for phase transitions based on ability/effect events.
pub fn check_ability_phase_transitions(
    event: &CombatEvent,
    cache: &mut SessionCache,
    current_signals: &[GameSignal],
) -> Vec<GameSignal> {
    let Some(enc) = cache.current_encounter() else {
        return Vec::new();
    };
    let Some(def_idx) = enc.active_boss_idx() else {
        return Vec::new();
    };

    let def = &enc.boss_definitions()[def_idx];
    let counter_defs = def.counters.clone();
    let current_phase = enc.current_phase.clone();
    let previous_phase = enc.previous_phase.clone();

    for phase in &def.phases {
        if current_phase.as_ref() == Some(&phase.id) {
            continue;
        }

        if let Some(ref required) = phase.preceded_by {
            let last_phase = current_phase.as_ref().or(previous_phase.as_ref());
            if last_phase != Some(required) {
                continue;
            }
        }

        if let Some(ref cond) = phase.counter_condition {
            if !enc.check_counter_condition(cond) {
                continue;
            }
        }

        let trigger_matched = check_ability_trigger(&phase.start_trigger, event)
            || check_signal_phase_trigger(&phase.start_trigger, &def.entities, current_signals);

        if trigger_matched {
            let old_phase = enc.current_phase.clone();
            let new_phase_id = phase.id.clone();
            let boss_id = def.id.clone();
            let resets = phase.resets_counters.clone();

            let enc = cache.current_encounter_mut().unwrap();
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
    }

    Vec::new()
}

/// Check for phase transitions based on entity signals (NpcAppears, EntityDeath).
pub fn check_entity_phase_transitions(
    cache: &mut SessionCache,
    current_signals: &[GameSignal],
    timestamp: NaiveDateTime,
) -> Vec<GameSignal> {
    let Some(enc) = cache.current_encounter() else {
        return Vec::new();
    };
    let Some(def_idx) = enc.active_boss_idx() else {
        return Vec::new();
    };

    let def = &enc.boss_definitions()[def_idx];
    let phases: Vec<_> = def.phases.clone();
    let counter_defs = def.counters.clone();
    let boss_id = def.id.clone();
    let entities = def.entities.clone();
    let current_phase = enc.current_phase.clone();
    let previous_phase = enc.previous_phase.clone();

    let mut signals = Vec::new();

    for phase in &phases {
        if current_phase.as_ref() == Some(&phase.id) {
            continue;
        }

        if let Some(ref required) = phase.preceded_by {
            let last_phase = current_phase.as_ref().or(previous_phase.as_ref());
            if last_phase != Some(required) {
                continue;
            }
        }

        if let Some(ref cond) = phase.counter_condition {
            if !enc.check_counter_condition(cond) {
                continue;
            }
        }

        if check_signal_phase_trigger(&phase.start_trigger, &entities, current_signals) {
            let old_phase = enc.current_phase.clone();
            let new_phase_id = phase.id.clone();
            let resets = phase.resets_counters.clone();

            let enc = cache.current_encounter_mut().unwrap();
            enc.set_phase(&new_phase_id, timestamp);
            enc.reset_counters_to_initial(&resets, &counter_defs);
            enc.challenge_tracker.set_phase(&new_phase_id, timestamp);

            signals.push(GameSignal::PhaseChanged {
                boss_id: boss_id.clone(),
                old_phase,
                new_phase: new_phase_id,
                timestamp,
            });

            break; // Only one phase transition per event
        }
    }

    signals
}

/// Check for phase transitions based on combat time (TimeElapsed triggers).
pub fn check_time_phase_transitions(
    cache: &mut SessionCache,
    timestamp: NaiveDateTime,
) -> Vec<GameSignal> {
    let Some(enc) = cache.current_encounter_mut() else {
        return Vec::new();
    };
    if enc.active_boss_idx().is_none() {
        return Vec::new();
    }

    let (old_time, new_time) = enc.update_combat_time(timestamp);

    if new_time <= old_time {
        return Vec::new();
    }

    // Need to reborrow after mutation
    let enc = cache.current_encounter().unwrap();
    let def_idx = enc.active_boss_idx().unwrap();

    let phases: Vec<_> = enc.boss_definitions()[def_idx].phases.clone();
    let counter_defs = enc.boss_definitions()[def_idx].counters.clone();
    let boss_id = enc.boss_definitions()[def_idx].id.clone();
    let current_phase = enc.current_phase.clone();
    let previous_phase = enc.previous_phase.clone();

    for phase in &phases {
        if current_phase.as_ref() == Some(&phase.id) {
            continue;
        }

        if let Some(ref required) = phase.preceded_by {
            let last_phase = current_phase.as_ref().or(previous_phase.as_ref());
            if last_phase != Some(required) {
                continue;
            }
        }

        if let Some(ref cond) = phase.counter_condition {
            if !enc.check_counter_condition(cond) {
                continue;
            }
        }

        if check_time_trigger(&phase.start_trigger, old_time, new_time) {
            let old_phase = enc.current_phase.clone();
            let new_phase_id = phase.id.clone();
            let resets = phase.resets_counters.clone();

            let enc = cache.current_encounter_mut().unwrap();
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
    }

    Vec::new()
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

    // Check ability/effect-based triggers
    if check_ability_trigger(end_trigger, event) {
        return vec![GameSignal::PhaseEndTriggered {
            phase_id: current_phase_id.clone(),
            timestamp: event.timestamp,
        }];
    }

    // Check signal-based triggers (entity death, phase ended, counter reached)
    if check_signal_phase_trigger(end_trigger, &def.entities, current_signals) {
        return vec![GameSignal::PhaseEndTriggered {
            phase_id: current_phase_id.clone(),
            timestamp: event.timestamp,
        }];
    }

    // Check HP-based triggers from BossHpChanged signals
    for signal in current_signals {
        if let GameSignal::BossHpChanged {
            npc_id,
            entity_name,
            old_hp_percent,
            new_hp_percent,
            timestamp,
            ..
        } = signal
        {
            if check_hp_trigger(
                end_trigger,
                &def.entities,
                *old_hp_percent,
                *new_hp_percent,
                *npc_id,
                entity_name,
            ) {
                return vec![GameSignal::PhaseEndTriggered {
                    phase_id: current_phase_id.clone(),
                    timestamp: *timestamp,
                }];
            }
        }
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
    entities: &[EntityDefinition],
    old_hp: f32,
    new_hp: f32,
    npc_id: i64,
    entity_name: &str,
) -> bool {
    trigger.matches_boss_hp_below(entities, npc_id, entity_name, old_hp, new_hp)
        || trigger.matches_boss_hp_above(entities, npc_id, entity_name, old_hp, new_hp)
}

/// Check if an ability/effect-based phase trigger is satisfied.
/// First checks event type, then delegates to unified Trigger methods.
pub fn check_ability_trigger(trigger: &Trigger, event: &CombatEvent) -> bool {
    // Check AbilityCast triggers
    if event.effect.effect_id == effect_id::ABILITYACTIVATE {
        let ability_id = event.action.action_id as u64;
        let ability_name = crate::context::resolve(event.action.name);
        if trigger.matches_ability(ability_id, Some(ability_name)) {
            return true;
        }
    }

    // Check EffectApplied triggers
    if event.effect.type_id == effect_type_id::APPLYEFFECT {
        let effect_id = event.effect.effect_id as u64;
        let effect_name = crate::context::resolve(event.effect.effect_name);
        if trigger.matches_effect_applied(effect_id, Some(effect_name)) {
            return true;
        }
    }

    // Check EffectRemoved triggers
    if event.effect.type_id == effect_type_id::REMOVEEFFECT {
        let effect_id = event.effect.effect_id as u64;
        let effect_name = crate::context::resolve(event.effect.effect_name);
        if trigger.matches_effect_removed(effect_id, Some(effect_name)) {
            return true;
        }
    }

    false
}

/// Check if a signal-based phase trigger is satisfied (NpcAppears, EntityDeath, etc.).
/// Iterates through signals and delegates matching to unified Trigger methods.
pub fn check_signal_phase_trigger(
    trigger: &Trigger,
    entities: &[EntityDefinition],
    signals: &[GameSignal],
) -> bool {
    for signal in signals {
        match signal {
            GameSignal::NpcFirstSeen {
                npc_id,
                entity_name,
                ..
            } => {
                if trigger.matches_npc_appears(entities, *npc_id, entity_name) {
                    return true;
                }
            }
            GameSignal::EntityDeath {
                npc_id,
                entity_name,
                ..
            } => {
                if trigger.matches_entity_death(entities, *npc_id, entity_name) {
                    return true;
                }
            }
            GameSignal::PhaseEndTriggered { phase_id, .. } => {
                if trigger.matches_phase_ended(phase_id) {
                    return true;
                }
            }
            GameSignal::CounterChanged {
                counter_id,
                old_value,
                new_value,
                ..
            } => {
                if trigger.matches_counter_reaches(counter_id, *old_value, *new_value) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// Check if a TimeElapsed trigger is satisfied (time crossed threshold).
/// Delegates to unified `Trigger::matches_time_elapsed`.
pub fn check_time_trigger(trigger: &Trigger, old_time: f32, new_time: f32) -> bool {
    trigger.matches_time_elapsed(old_time, new_time)
}
