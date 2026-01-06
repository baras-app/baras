//! Counter increment and trigger checking logic.
//!
//! Counters track occurrences during boss encounters (e.g., add spawns, ability casts).
//! This module handles detecting when counters should increment based on game events.

use crate::dsl::BossEncounterDefinition;
use crate::combat_log::{CombatEvent, EntityType};
use crate::game_data::{effect_id, effect_type_id};
use crate::state::SessionCache;
use crate::dsl::{EntitySelectorExt, Trigger};

use super::GameSignal;

/// Check for counter increments/decrements based on events and emit CounterChanged signals.
pub fn check_counter_increments(
    event: &CombatEvent,
    cache: &mut SessionCache,
    current_signals: &[GameSignal],
) -> Vec<GameSignal> {
    // Clone the definition upfront to avoid borrow conflicts
    let (counters, def) = {
        let Some(enc) = cache.current_encounter() else {
            return Vec::new();
        };
        let Some(def_idx) = enc.active_boss_idx() else {
            return Vec::new();
        };
        let def = enc.boss_definitions()[def_idx].clone();
        (def.counters.clone(), def)
    };

    let mut signals = Vec::new();

    for counter in &counters {
        // Check increment_on trigger
        if check_counter_trigger(&counter.increment_on, event, current_signals, &def) {
            let enc = cache.current_encounter_mut().unwrap();
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
            && check_counter_trigger(decrement_trigger, event, current_signals, &def) {
                let enc = cache.current_encounter_mut().unwrap();
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
        if check_counter_trigger(&counter.reset_on, event, current_signals, &def) {
            let enc = cache.current_encounter_mut().unwrap();
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

    let counters = {
        let Some(enc) = cache.current_encounter() else {
            return Vec::new();
        };
        let Some(def_idx) = enc.active_boss_idx() else {
            return Vec::new();
        };
        enc.boss_definitions()[def_idx].counters.clone()
    };

    let mut signals = Vec::new();

    for counter in &counters {
        // Check increment_on for timer triggers
        if matches_timer_trigger(&counter.increment_on, expired_timer_ids, started_timer_ids) {
            let enc = cache.current_encounter_mut().unwrap();
            let (old_value, new_value) = enc.modify_counter(
                &counter.id,
                counter.decrement,
                counter.set_value,
            );
            signals.push(GameSignal::CounterChanged {
                counter_id: counter.id.clone(),
                old_value,
                new_value,
                timestamp,
            });
        }

        // Check decrement_on for timer triggers
        if let Some(ref trigger) = counter.decrement_on {
            if matches_timer_trigger(trigger, expired_timer_ids, started_timer_ids) {
                let enc = cache.current_encounter_mut().unwrap();
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
        if matches_timer_trigger(&counter.reset_on, expired_timer_ids, started_timer_ids) {
            let enc = cache.current_encounter_mut().unwrap();
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

/// Check if a trigger matches any expired or started timer IDs.
/// Handles TimerExpires, TimerStarted, and AnyOf wrappers.
fn matches_timer_trigger(
    trigger: &Trigger,
    expired_timer_ids: &[String],
    started_timer_ids: &[String],
) -> bool {
    match trigger {
        Trigger::TimerExpires { timer_id } => expired_timer_ids.contains(timer_id),
        Trigger::TimerStarted { timer_id } => started_timer_ids.contains(timer_id),
        Trigger::AnyOf { conditions } => conditions
            .iter()
            .any(|c| matches_timer_trigger(c, expired_timer_ids, started_timer_ids)),
        _ => false,
    }
}

/// Check if a counter trigger is satisfied by the current event/signals.
pub fn check_counter_trigger(
    trigger: &Trigger,
    event: &CombatEvent,
    current_signals: &[GameSignal],
    boss_def: &BossEncounterDefinition,
) -> bool {
    match trigger {
        Trigger::CombatStart => {
            current_signals.iter().any(|s| matches!(s, GameSignal::CombatStarted { .. }))
        }
        Trigger::CombatEnd => {
            current_signals.iter().any(|s| matches!(s, GameSignal::CombatEnded { .. }))
        }
        Trigger::AbilityCast { abilities, source } => {
            if event.effect.effect_id != effect_id::ABILITYACTIVATE {
                return false;
            }
            let ability_id = event.action.action_id as u64;
            let ability_name = crate::context::resolve(event.action.name);
            if !abilities.is_empty()
                && !abilities.iter().any(|s| s.matches(ability_id, Some(ability_name)))
            {
                return false;
            }
            // Check source filter if specified
            if !source.is_any() {
                let source_name = crate::context::resolve(event.source_entity.name);
                if !source.matches_name(source_name)
                    && !source.matches_npc_id(event.source_entity.class_id)
                {
                    return false;
                }
            }
            true
        }
        Trigger::EffectApplied { effects, target, .. } => {
            if event.effect.type_id != effect_type_id::APPLYEFFECT {
                return false;
            }
            let eff_id = event.effect.effect_id as u64;
            let eff_name = crate::context::resolve(event.effect.effect_name);
            if !effects.is_empty() && !effects.iter().any(|s| s.matches(eff_id, Some(eff_name))) {
                return false;
            }
            // Check target filter if specified
            if !target.is_any() {
                // Special case: "local_player" matches player entities
                if target.is_local_player() {
                    if event.target_entity.entity_type != EntityType::Player {
                        return false;
                    }
                } else {
                    let target_name = crate::context::resolve(event.target_entity.name);
                    if !target.matches_name(target_name)
                        && !target.matches_npc_id(event.target_entity.class_id)
                    {
                        return false;
                    }
                }
            }
            true
        }
        Trigger::EffectRemoved { effects, target, .. } => {
            if event.effect.type_id != effect_type_id::REMOVEEFFECT {
                return false;
            }
            let eff_id = event.effect.effect_id as u64;
            let eff_name = crate::context::resolve(event.effect.effect_name);
            if !effects.is_empty() && !effects.iter().any(|s| s.matches(eff_id, Some(eff_name))) {
                return false;
            }
            // Check target filter if specified
            if !target.is_any() {
                if target.is_local_player() {
                    if event.target_entity.entity_type != EntityType::Player {
                        return false;
                    }
                } else {
                    let target_name = crate::context::resolve(event.target_entity.name);
                    if !target.matches_name(target_name)
                        && !target.matches_npc_id(event.target_entity.class_id)
                    {
                        return false;
                    }
                }
            }
            true
        }
        Trigger::PhaseEntered { phase_id } => {
            current_signals.iter().any(|s| {
                matches!(s, GameSignal::PhaseChanged { new_phase, .. } if new_phase == phase_id)
            })
        }
        Trigger::PhaseEnded { phase_id } => {
            current_signals.iter().any(|s| {
                matches!(s, GameSignal::PhaseChanged { old_phase: Some(old), .. } if old == phase_id)
                    || matches!(s, GameSignal::PhaseEndTriggered { phase_id: p, .. } if p == phase_id)
            })
        }
        Trigger::AnyPhaseChange => {
            current_signals.iter().any(|s| matches!(s, GameSignal::PhaseChanged { .. }))
        }
        Trigger::NpcAppears { selector } => {
            current_signals.iter().any(|s| {
                if let GameSignal::NpcFirstSeen { npc_id, entity_name, .. } = s {
                    // Use unified matching: roster alias → NPC ID → name
                    selector.matches_with_roster(&boss_def.entities, *npc_id, Some(entity_name))
                } else {
                    false
                }
            })
        }
        Trigger::EntityDeath { selector } => {
            current_signals.iter().any(|s| {
                if let GameSignal::EntityDeath { npc_id, entity_name, .. } = s {
                    // If entity filter is empty, match any death
                    if selector.is_empty() {
                        return true;
                    }
                    // Use unified matching: roster alias → NPC ID → name
                    selector.matches_with_roster(&boss_def.entities, *npc_id, Some(entity_name))
                } else {
                    false
                }
            })
        }
        Trigger::CounterReaches { counter_id, value } => {
            current_signals.iter().any(|s| {
                matches!(s, GameSignal::CounterChanged { counter_id: cid, new_value, .. }
                    if cid == counter_id && *new_value == *value)
            })
        }
        Trigger::BossHpBelow { hp_percent, selector } => {
            current_signals.iter().any(|s| {
                if let GameSignal::BossHpChanged { new_hp_percent, entity_name, .. } = s {
                    if *new_hp_percent > *hp_percent {
                        return false;
                    }
                    // Check entity filter if specified
                    if !selector.is_empty()
                        && !selector.matches_name_only(entity_name) {
                            return false;
                    }
                    true
                } else {
                    false
                }
            })
        }
        Trigger::Never => false,

        // Timer triggers not supported for counters
        Trigger::TimerExpires { .. } | Trigger::TimerStarted { .. } => false,

        Trigger::DamageTaken { abilities, source, target } => {
            // Check for DamageTaken signal in current signals
            current_signals.iter().any(|sig| {
                if let GameSignal::DamageTaken {
                    ability_id,
                    ability_name,
                    source_npc_id,
                    source_name,
                    target_name,
                    ..
                } = sig
                {
                    let ability_name_str = crate::context::resolve(*ability_name);
                    if !abilities.is_empty()
                        && !abilities.iter().any(|s| s.matches(*ability_id as u64, Some(ability_name_str)))
                    {
                        return false;
                    }
                    if !source.is_any() {
                        let source_name_str = crate::context::resolve(*source_name);
                        if !source.matches_name(source_name_str)
                            && !source.matches_npc_id(*source_npc_id)
                        {
                            return false;
                        }
                    }
                    if !target.is_any() {
                        // Targets are typically players (no NPC ID), so only match by name
                        let target_name_str = crate::context::resolve(*target_name);
                        if !target.matches_name(target_name_str) {
                            return false;
                        }
                    }
                    true
                } else {
                    false
                }
            })
        }

        // Other triggers not applicable to counter increment
        Trigger::TimeElapsed { .. }
        | Trigger::BossHpAbove { .. }
        | Trigger::TargetSet { .. }
        | Trigger::Manual => false,

        Trigger::AnyOf { conditions } => {
            conditions.iter().any(|c| check_counter_trigger(c, event, current_signals, boss_def))
        }
    }
}
