//! Phase transition logic for boss encounters.
//!
//! Phases represent distinct stages of a boss fight (e.g., "Walker 1", "Burn Phase").
//! This module handles detecting phase transitions based on various triggers:
//! - HP thresholds (BossHpBelow/Above)
//! - Ability/effect events (AbilityCast, EffectApplied/Removed)
//! - Entity lifecycle (EntityFirstSeen, EntityDeath)
//! - Time elapsed (TimeElapsed)
//! - Counter values (CounterReaches)

use chrono::NaiveDateTime;

use crate::boss::{BossEncounterState, PhaseTrigger};
use crate::combat_log::CombatEvent;
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
    timestamp: NaiveDateTime,
) -> Vec<GameSignal> {
    let Some(def_idx) = cache.active_boss_idx else {
        return Vec::new();
    };

    let def = &cache.boss_definitions[def_idx];
    let counter_defs = def.counters.clone();
    let current_phase = cache.boss_state.current_phase.clone();
    let previous_phase = cache.boss_state.previous_phase.clone();

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
            if !cache.boss_state.check_counter_condition(cond) {
                continue;
            }
        }

        if check_hp_trigger(&phase.start_trigger, old_hp, new_hp, npc_id, &cache.boss_state) {
            let old_phase = cache.boss_state.current_phase.clone();
            let new_phase_id = phase.id.clone();
            let boss_id = def.id.clone();
            let resets = phase.resets_counters.clone();

            cache.boss_state.set_phase(&new_phase_id, timestamp);
            cache.boss_state.reset_counters_to_initial(&resets, &counter_defs);

            if let Some(enc) = cache.current_encounter_mut() {
                enc.challenge_tracker.set_phase(&new_phase_id, timestamp);
            }

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
    let Some(def_idx) = cache.active_boss_idx else {
        return Vec::new();
    };

    let def = &cache.boss_definitions[def_idx];
    let counter_defs = def.counters.clone();
    let current_phase = cache.boss_state.current_phase.clone();
    let previous_phase = cache.boss_state.previous_phase.clone();

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
            if !cache.boss_state.check_counter_condition(cond) {
                continue;
            }
        }

        let trigger_matched = check_ability_trigger(&phase.start_trigger, event)
            || check_signal_phase_trigger(&phase.start_trigger, current_signals);

        if trigger_matched {
            let old_phase = cache.boss_state.current_phase.clone();
            let new_phase_id = phase.id.clone();
            let boss_id = def.id.clone();
            let resets = phase.resets_counters.clone();

            cache.boss_state.set_phase(&new_phase_id, event.timestamp);
            cache.boss_state.reset_counters_to_initial(&resets, &counter_defs);

            if let Some(enc) = cache.current_encounter_mut() {
                enc.challenge_tracker.set_phase(&new_phase_id, event.timestamp);
            }

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

/// Check for phase transitions based on entity signals (EntityFirstSeen, EntityDeath).
pub fn check_entity_phase_transitions(
    cache: &mut SessionCache,
    current_signals: &[GameSignal],
    timestamp: NaiveDateTime,
) -> Vec<GameSignal> {
    let Some(def_idx) = cache.active_boss_idx else {
        return Vec::new();
    };

    let phases: Vec<_> = cache.boss_definitions[def_idx].phases.clone();
    let counter_defs = cache.boss_definitions[def_idx].counters.clone();
    let boss_id = cache.boss_definitions[def_idx].id.clone();
    let current_phase = cache.boss_state.current_phase.clone();
    let previous_phase = cache.boss_state.previous_phase.clone();

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
            if !cache.boss_state.check_counter_condition(cond) {
                continue;
            }
        }

        if check_signal_phase_trigger(&phase.start_trigger, current_signals) {
            let old_phase = cache.boss_state.current_phase.clone();
            let new_phase_id = phase.id.clone();
            let resets = phase.resets_counters.clone();

            cache.boss_state.set_phase(&new_phase_id, timestamp);
            cache.boss_state.reset_counters_to_initial(&resets, &counter_defs);

            if let Some(enc) = cache.current_encounter_mut() {
                enc.challenge_tracker.set_phase(&new_phase_id, timestamp);
            }

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
    let Some(def_idx) = cache.active_boss_idx else {
        return Vec::new();
    };

    let (old_time, new_time) = cache.boss_state.update_combat_time(timestamp);

    if new_time <= old_time {
        return Vec::new();
    }

    let phases: Vec<_> = cache.boss_definitions[def_idx].phases.clone();
    let counter_defs = cache.boss_definitions[def_idx].counters.clone();
    let boss_id = cache.boss_definitions[def_idx].id.clone();
    let current_phase = cache.boss_state.current_phase.clone();
    let previous_phase = cache.boss_state.previous_phase.clone();

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
            if !cache.boss_state.check_counter_condition(cond) {
                continue;
            }
        }

        if check_time_trigger(&phase.start_trigger, old_time, new_time) {
            let old_phase = cache.boss_state.current_phase.clone();
            let new_phase_id = phase.id.clone();
            let resets = phase.resets_counters.clone();

            cache.boss_state.set_phase(&new_phase_id, timestamp);
            cache.boss_state.reset_counters_to_initial(&resets, &counter_defs);

            if let Some(enc) = cache.current_encounter_mut() {
                enc.challenge_tracker.set_phase(&new_phase_id, timestamp);
            }

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
    let Some(def_idx) = cache.active_boss_idx else {
        return Vec::new();
    };
    let Some(current_phase_id) = &cache.boss_state.current_phase else {
        return Vec::new();
    };

    let def = &cache.boss_definitions[def_idx];

    let Some(phase) = def.phases.iter().find(|p| &p.id == current_phase_id) else {
        return Vec::new();
    };

    let Some(ref end_trigger) = phase.end_trigger else {
        return Vec::new();
    };

    if check_ability_trigger(end_trigger, event) {
        return vec![GameSignal::PhaseEndTriggered {
            phase_id: current_phase_id.clone(),
            timestamp: event.timestamp,
        }];
    }

    if check_signal_phase_trigger(end_trigger, current_signals) {
        return vec![GameSignal::PhaseEndTriggered {
            phase_id: current_phase_id.clone(),
            timestamp: event.timestamp,
        }];
    }

    Vec::new()
}

// ═══════════════════════════════════════════════════════════════════════════
// Trigger Matching Helpers
// ═══════════════════════════════════════════════════════════════════════════

/// Check if an HP-based phase trigger is satisfied.
pub fn check_hp_trigger(
    trigger: &PhaseTrigger,
    old_hp: f32,
    new_hp: f32,
    npc_id: i64,
    state: &BossEncounterState,
) -> bool {
    match trigger {
        PhaseTrigger::BossHpBelow { hp_percent, npc_id: trigger_npc, boss_name, .. } => {
            let crossed = old_hp > *hp_percent && new_hp <= *hp_percent;
            if !crossed {
                return false;
            }

            if let Some(required_npc) = trigger_npc {
                return npc_id == *required_npc;
            }

            if let Some(name) = boss_name {
                return state.hp_by_name.contains_key(name);
            }

            true
        }
        PhaseTrigger::BossHpAbove { hp_percent, npc_id: trigger_npc, boss_name, .. } => {
            let crossed = old_hp < *hp_percent && new_hp >= *hp_percent;
            if !crossed {
                return false;
            }

            if let Some(required_npc) = trigger_npc {
                return npc_id == *required_npc;
            }

            if let Some(name) = boss_name {
                return state.hp_by_name.contains_key(name);
            }

            true
        }
        PhaseTrigger::AnyOf { conditions } => {
            conditions.iter().any(|c| check_hp_trigger(c, old_hp, new_hp, npc_id, state))
        }
        _ => false,
    }
}

/// Check if an ability/effect-based phase trigger is satisfied.
pub fn check_ability_trigger(trigger: &PhaseTrigger, event: &CombatEvent) -> bool {
    match trigger {
        PhaseTrigger::AbilityCast { ability_ids } => {
            if event.effect.effect_id != effect_id::ABILITYACTIVATE {
                return false;
            }
            ability_ids.contains(&(event.action.action_id as u64))
        }
        PhaseTrigger::EffectApplied { effect_ids } => {
            if event.effect.type_id != effect_type_id::APPLYEFFECT {
                return false;
            }
            effect_ids.contains(&(event.effect.effect_id as u64))
        }
        PhaseTrigger::EffectRemoved { effect_ids } => {
            if event.effect.type_id != effect_type_id::REMOVEEFFECT {
                return false;
            }
            effect_ids.contains(&(event.effect.effect_id as u64))
        }
        PhaseTrigger::AnyOf { conditions } => {
            conditions.iter().any(|c| check_ability_trigger(c, event))
        }
        _ => false,
    }
}

/// Check if a signal-based phase trigger is satisfied (EntityFirstSeen, EntityDeath, etc.).
pub fn check_signal_phase_trigger(trigger: &PhaseTrigger, signals: &[GameSignal]) -> bool {
    match trigger {
        PhaseTrigger::EntityFirstSeen { npc_id, entity_name, .. } => {
            signals.iter().any(|s| {
                if let GameSignal::NpcFirstSeen { npc_id: sig_npc_id, entity_name: sig_name, .. } = s {
                    if let Some(required_id) = npc_id {
                        return sig_npc_id == required_id;
                    }
                    if let Some(required_name) = entity_name {
                        return sig_name.contains(required_name);
                    }
                    false
                } else {
                    false
                }
            })
        }
        PhaseTrigger::EntityDeath { npc_id, entity_name, .. } => {
            signals.iter().any(|s| {
                if let GameSignal::EntityDeath { npc_id: sig_npc_id, entity_name: sig_name, .. } = s {
                    if let Some(required_id) = npc_id
                       && sig_npc_id != required_id {
                            return false;
                    }
                    if let Some(required_name) = entity_name
                        && !required_name.eq_ignore_ascii_case(sig_name) {
                            return false;
                    }
                    true
                } else {
                    false
                }
            })
        }
        PhaseTrigger::PhaseEnded { phase_id, phase_ids } => {
            signals.iter().any(|s| {
                if let GameSignal::PhaseEndTriggered { phase_id: sig_phase_id, .. } = s {
                    if let Some(required) = phase_id {
                        if sig_phase_id == required {
                            return true;
                        }
                    }
                    if phase_ids.iter().any(|p| p == sig_phase_id) {
                        return true;
                    }
                    false
                } else {
                    false
                }
            })
        }
        PhaseTrigger::CounterReaches { counter_id, value } => {
            signals.iter().any(|s| {
                matches!(s, GameSignal::CounterChanged { counter_id: cid, new_value, .. }
                    if cid == counter_id && *new_value == *value)
            })
        }
        PhaseTrigger::AnyOf { conditions } => {
            conditions.iter().any(|c| check_signal_phase_trigger(c, signals))
        }
        _ => false,
    }
}

/// Check if a TimeElapsed trigger is satisfied (time crossed threshold).
pub fn check_time_trigger(trigger: &PhaseTrigger, old_time: f32, new_time: f32) -> bool {
    match trigger {
        PhaseTrigger::TimeElapsed { secs } => {
            old_time < *secs && new_time >= *secs
        }
        PhaseTrigger::AnyOf { conditions } => {
            conditions.iter().any(|c| check_time_trigger(c, old_time, new_time))
        }
        _ => false,
    }
}
