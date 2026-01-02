//! Phase transition logic for boss encounters.
//!
//! Phases represent distinct stages of a boss fight (e.g., "Walker 1", "Burn Phase").
//! This module handles detecting phase transitions based on various triggers.

use chrono::NaiveDateTime;

use crate::combat_log::CombatEvent;
use crate::encounter::CombatEncounter;
use crate::game_data::{effect_id, effect_type_id};
use crate::state::SessionCache;
use crate::triggers::{EntitySelectorExt, Trigger};

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

        if check_hp_trigger(&phase.start_trigger, old_hp, new_hp, npc_id, enc) {
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
            || check_signal_phase_trigger(&phase.start_trigger, current_signals);

        if trigger_matched {
            let old_phase = enc.current_phase.clone();
            let new_phase_id = phase.id.clone();
            let boss_id = def.id.clone();
            let resets = phase.resets_counters.clone();

            let enc = cache.current_encounter_mut().unwrap();
            enc.set_phase(&new_phase_id, event.timestamp);
            enc.reset_counters_to_initial(&resets, &counter_defs);
            enc.challenge_tracker.set_phase(&new_phase_id, event.timestamp);

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

    let phases: Vec<_> = enc.boss_definitions()[def_idx].phases.clone();
    let counter_defs = enc.boss_definitions()[def_idx].counters.clone();
    let boss_id = enc.boss_definitions()[def_idx].id.clone();
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

        if check_signal_phase_trigger(&phase.start_trigger, current_signals) {
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
    trigger: &Trigger,
    old_hp: f32,
    new_hp: f32,
    npc_id: i64,
    enc: &CombatEncounter,
) -> bool {
    match trigger {
        Trigger::BossHpBelow { hp_percent, selector } => {
            let crossed = old_hp > *hp_percent && new_hp <= *hp_percent;
            if !crossed {
                return false;
            }

            // Check entity filter if specified
            if selector.is_empty() {
                return true; // No filter = any boss
            }

            if selector.matches_npc_id(npc_id) {
                return true;
            }

            // Check by name in hp_by_name (for name-based selectors)
            if let Some(name) = selector.first_name() {
                return enc.hp_by_name.contains_key(name);
            }

            false
        }
        Trigger::BossHpAbove { hp_percent, selector } => {
            let crossed = old_hp < *hp_percent && new_hp >= *hp_percent;
            if !crossed {
                return false;
            }

            if selector.is_empty() {
                return true;
            }

            if selector.matches_npc_id(npc_id) {
                return true;
            }

            // Check by name in hp_by_name (for name-based selectors)
            if let Some(name) = selector.first_name() {
                return enc.hp_by_name.contains_key(name);
            }

            false
        }
        Trigger::AnyOf { conditions } => {
            conditions.iter().any(|c| check_hp_trigger(c, old_hp, new_hp, npc_id, enc))
        }
        _ => false,
    }
}

/// Check if an ability/effect-based phase trigger is satisfied.
pub fn check_ability_trigger(trigger: &Trigger, event: &CombatEvent) -> bool {
    match trigger {
        Trigger::AbilityCast { abilities, .. } => {
            if event.effect.effect_id != effect_id::ABILITYACTIVATE {
                return false;
            }
            let ability_id = event.action.action_id as u64;
            let ability_name = crate::context::resolve(event.action.name);
            abilities.is_empty()
                || abilities.iter().any(|s| s.matches(ability_id, Some(&ability_name)))
        }
        Trigger::EffectApplied { effects, .. } => {
            if event.effect.type_id != effect_type_id::APPLYEFFECT {
                return false;
            }
            let eff_id = event.effect.effect_id as u64;
            let eff_name = crate::context::resolve(event.effect.effect_name);
            effects.is_empty() || effects.iter().any(|s| s.matches(eff_id, Some(&eff_name)))
        }
        Trigger::EffectRemoved { effects, .. } => {
            if event.effect.type_id != effect_type_id::REMOVEEFFECT {
                return false;
            }
            let eff_id = event.effect.effect_id as u64;
            let eff_name = crate::context::resolve(event.effect.effect_name);
            effects.is_empty() || effects.iter().any(|s| s.matches(eff_id, Some(&eff_name)))
        }
        Trigger::AnyOf { conditions } => {
            conditions.iter().any(|c| check_ability_trigger(c, event))
        }
        _ => false,
    }
}

/// Check if a signal-based phase trigger is satisfied (NpcAppears, EntityDeath, etc.).
pub fn check_signal_phase_trigger(trigger: &Trigger, signals: &[GameSignal]) -> bool {
    match trigger {
        Trigger::NpcAppears { selector } => {
            signals.iter().any(|s| {
                if let GameSignal::NpcFirstSeen { npc_id, entity_name, .. } = s {
                    if selector.matches_npc_id(*npc_id) {
                        return true;
                    }
                    if selector.matches_name_only(entity_name) {
                        return true;
                    }
                }
                false
            })
        }
        Trigger::EntityDeath { selector } => {
            signals.iter().any(|s| {
                if let GameSignal::EntityDeath { npc_id, entity_name, .. } = s {
                    if selector.is_empty() {
                        return true; // No filter = any death
                    }
                    if selector.matches_npc_id(*npc_id) {
                        return true;
                    }
                    if selector.matches_name_only(entity_name) {
                        return true;
                    }
                }
                false
            })
        }
        Trigger::PhaseEnded { phase_id } => {
            signals.iter().any(|s| {
                matches!(s, GameSignal::PhaseEndTriggered { phase_id: sig_phase_id, .. }
                    if sig_phase_id == phase_id)
            })
        }
        Trigger::CounterReaches { counter_id, value } => {
            signals.iter().any(|s| {
                matches!(s, GameSignal::CounterChanged { counter_id: cid, new_value, .. }
                    if cid == counter_id && *new_value == *value)
            })
        }
        Trigger::AnyOf { conditions } => {
            conditions.iter().any(|c| check_signal_phase_trigger(c, signals))
        }
        _ => false,
    }
}

/// Check if a TimeElapsed trigger is satisfied (time crossed threshold).
pub fn check_time_trigger(trigger: &Trigger, old_time: f32, new_time: f32) -> bool {
    match trigger {
        Trigger::TimeElapsed { secs } => {
            old_time < *secs && new_time >= *secs
        }
        Trigger::AnyOf { conditions } => {
            conditions.iter().any(|c| check_time_trigger(c, old_time, new_time))
        }
        _ => false,
    }
}
