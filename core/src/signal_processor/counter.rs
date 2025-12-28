//! Counter increment and trigger checking logic.
//!
//! Counters track occurrences during boss encounters (e.g., add spawns, ability casts).
//! This module handles detecting when counters should increment based on game events.

use crate::boss::{BossEncounterDefinition, CounterTrigger};
use crate::combat_log::{CombatEvent, EntityType};
use crate::game_data::{effect_id, effect_type_id};
use crate::state::SessionCache;

use super::GameSignal;

/// Check for counter increments based on events and emit CounterChanged signals.
pub fn check_counter_increments(
    event: &CombatEvent,
    cache: &mut SessionCache,
    current_signals: &[GameSignal],
) -> Vec<GameSignal> {
    let Some(def_idx) = cache.active_boss_idx else {
        return Vec::new();
    };

    let def = &cache.boss_definitions[def_idx];
    let mut signals = Vec::new();

    for counter in &def.counters {
        if check_counter_trigger(&counter.increment_on, event, current_signals, def) {
            let (old_value, new_value) = cache.boss_state.modify_counter(
                &counter.id,
                counter.decrement,
                counter.set_value,
            );

            signals.push(GameSignal::CounterChanged {
                counter_id: counter.id.clone(),
                old_value,
                new_value,
                timestamp: event.timestamp,
            });
        }
    }

    signals
}

/// Check if a counter trigger is satisfied by the current event/signals.
pub fn check_counter_trigger(
    trigger: &CounterTrigger,
    event: &CombatEvent,
    current_signals: &[GameSignal],
    boss_def: &BossEncounterDefinition,
) -> bool {
    match trigger {
        CounterTrigger::CombatStart => {
            current_signals.iter().any(|s| matches!(s, GameSignal::CombatStarted { .. }))
        }
        CounterTrigger::CombatEnd => {
            current_signals.iter().any(|s| matches!(s, GameSignal::CombatEnded { .. }))
        }
        CounterTrigger::AbilityCast { ability_ids, source } => {
            if event.effect.effect_id != effect_id::ABILITYACTIVATE {
                return false;
            }
            if !ability_ids.contains(&(event.action.action_id as u64)) {
                return false;
            }
            if let Some(source_name) = source {
                let resolved_name = crate::context::resolve(event.source_entity.name);
                if !resolved_name.eq_ignore_ascii_case(source_name) {
                    return false;
                }
            }
            true
        }
        CounterTrigger::EffectApplied { effect_ids, target } => {
            if event.effect.type_id != effect_type_id::APPLYEFFECT {
                return false;
            }
            if !effect_ids.contains(&(event.effect.effect_id as u64)) {
                return false;
            }
            if let Some(target_name) = target {
                if target_name == "local_player" {
                    if event.target_entity.entity_type != EntityType::Player {
                        return false;
                    }
                } else {
                    let resolved_name = crate::context::resolve(event.target_entity.name);
                    if !resolved_name.eq_ignore_ascii_case(target_name) {
                        return false;
                    }
                }
            }
            true
        }
        CounterTrigger::EffectRemoved { effect_ids, target } => {
            if event.effect.type_id != effect_type_id::REMOVEEFFECT {
                return false;
            }
            if !effect_ids.contains(&(event.effect.effect_id as u64)) {
                return false;
            }
            if let Some(target_name) = target {
                if target_name == "local_player" {
                    if event.target_entity.entity_type != EntityType::Player {
                        return false;
                    }
                } else {
                    let resolved_name = crate::context::resolve(event.target_entity.name);
                    if !resolved_name.eq_ignore_ascii_case(target_name) {
                        return false;
                    }
                }
            }
            true
        }
        CounterTrigger::PhaseEntered { phase_id } => {
            current_signals.iter().any(|s| {
                matches!(s, GameSignal::PhaseChanged { new_phase, .. } if new_phase == phase_id)
            })
        }
        CounterTrigger::PhaseEnded { phase_id } => {
            current_signals.iter().any(|s| {
                matches!(s, GameSignal::PhaseChanged { old_phase: Some(old), .. } if old == phase_id)
                    || matches!(s, GameSignal::PhaseEndTriggered { phase_id: p, .. } if p == phase_id)
            })
        }
        CounterTrigger::AnyPhaseChange => {
            current_signals.iter().any(|s| matches!(s, GameSignal::PhaseChanged { .. }))
        }
        CounterTrigger::TimerExpires { .. } => {
            // Timer expiration handled by TimerManager, not event stream
            false
        }
        CounterTrigger::TimerStarts { .. } => {
            // Timer starts handled by TimerManager, not event stream
            false
        }
        CounterTrigger::EntityFirstSeen { npc_id, entity_name, .. } => {
            current_signals.iter().any(|s| {
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
        CounterTrigger::EntityDeath { npc_id, entity_name, .. } => {
            current_signals.iter().any(|s| {
                if let GameSignal::EntityDeath { npc_id: sig_npc_id, entity_name: sig_name, .. } = s {
                    if let Some(required_id) = npc_id
                        && sig_npc_id != required_id {
                            return false;
                    }
                    if let Some(required_name) = entity_name {
                        // Look up entity in boss definition to get NPC IDs
                        if let Some(entity_def) = boss_def.entities.iter()
                            .find(|e| e.name.eq_ignore_ascii_case(required_name))
                        {
                            if !entity_def.ids.contains(sig_npc_id) {
                                return false;
                            }
                        } else {
                            // Fallback: direct name comparison
                            if !required_name.eq_ignore_ascii_case(sig_name) {
                                return false;
                            }
                        }
                    }
                    true
                } else {
                    false
                }
            })
        }
        CounterTrigger::CounterReaches { counter_id, value } => {
            current_signals.iter().any(|s| {
                matches!(s, GameSignal::CounterChanged { counter_id: cid, new_value, .. }
                    if cid == counter_id && *new_value == *value)
            })
        }
        CounterTrigger::BossHpBelow { hp_percent, entity, boss_name } => {
            current_signals.iter().any(|s| {
                if let GameSignal::BossHpChanged { current_hp, max_hp, entity_name, .. } = s {
                    let hp_pct = if *max_hp > 0 {
                        (*current_hp as f32 / *max_hp as f32) * 100.0
                    } else {
                        100.0
                    };
                    if hp_pct > *hp_percent {
                        return false;
                    }
                    if let Some(name) = entity.as_ref().or(boss_name.as_ref()) {
                        if !entity_name.eq_ignore_ascii_case(name) {
                            return false;
                        }
                    }
                    true
                } else {
                    false
                }
            })
        }
        CounterTrigger::Never => false,
    }
}
