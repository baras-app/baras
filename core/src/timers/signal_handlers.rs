//! Signal handler functions for TimerManager
//!
//! Contains all the game signal handling logic extracted from TimerManager.
//! Each function takes `&mut TimerManager` and processes a specific signal type.

use chrono::NaiveDateTime;

use crate::combat_log::EntityType;
use crate::context::IStr;
use crate::entity_filter::EntityFilterMatching;

use super::{TimerManager, TimerTrigger};

/// Handle ability activation
pub(super) fn handle_ability(
    manager: &mut TimerManager,
    ability_id: i64,
    ability_name: IStr,
    source_id: i64,
    source_type: EntityType,
    source_name: IStr,
    source_npc_id: i64,
    target_id: i64,
    target_type: EntityType,
    target_name: IStr,
    target_npc_id: i64,
    timestamp: NaiveDateTime,
) {
    // Convert i64 to u64 for matching (game IDs are always positive)
    let ability_id = ability_id as u64;
    let ability_name_str = crate::context::resolve(ability_name);

    // Debug: log all ability activations to verify signal flow
    eprintln!("[TIMER DEBUG] Ability activated: {} (id: {})", ability_name_str, ability_id);

    // Debug: show which timers have AbilityCast triggers
    for d in manager.definitions.values() {
        if d.matches_ability_with_name(ability_id, Some(&ability_name_str)) {
            eprintln!("[TIMER DEBUG] Timer '{}' matches ability! Checking context...", d.name);
            let is_active = manager.is_definition_active(d);
            let matches_filters = manager.matches_source_target_filters(
                d, source_id, source_type, source_name, source_npc_id,
                target_id, target_type, target_name, target_npc_id,
            );
            eprintln!("[TIMER DEBUG]   is_active={}, matches_filters={}", is_active, matches_filters);
        }
    }

    let matching: Vec<_> = manager.definitions
        .values()
        .filter(|d| {
            let matches_ability = d.matches_ability_with_name(ability_id, Some(&ability_name_str));
            let is_active = manager.is_definition_active(d);
            let matches_filters = manager.matches_source_target_filters(
                d, source_id, source_type, source_name, source_npc_id,
                target_id, target_type, target_name, target_npc_id,
            );
            if matches_ability && !is_active {
                let diff_str = manager.context.difficulty.map(|d| d.config_key()).unwrap_or("none");
                eprintln!("[TIMER] Ability {} ({}) matches timer '{}' but context filter failed (enc={:?}, boss={:?}, diff={})",
                    ability_id, ability_name_str, d.name, manager.context.encounter_name, manager.context.boss_name, diff_str);
            }
            matches_ability && is_active && matches_filters
        })
        .cloned()
        .collect();

    for def in matching {
        eprintln!("[TIMER] Starting timer '{}' (ability {} / {})", def.name, ability_id, ability_name_str);
        manager.start_timer(&def, timestamp, Some(target_id));
    }

    // Check for cancel triggers on ability cast
    manager.cancel_timers_matching(
        |t| matches!(t, TimerTrigger::AbilityCast { abilities, .. } if abilities.iter().any(|s| s.matches(ability_id, Some(&ability_name_str)))),
        &format!("ability {} cast", ability_id)
    );
}

/// Handle effect applied
pub(super) fn handle_effect_applied(
    manager: &mut TimerManager,
    effect_id: i64,
    source_id: i64,
    source_type: EntityType,
    source_name: IStr,
    source_npc_id: i64,
    target_id: i64,
    target_type: EntityType,
    target_name: IStr,
    target_npc_id: i64,
    timestamp: NaiveDateTime,
) {
    // Convert i64 to u64 for matching (game IDs are always positive)
    let effect_id = effect_id as u64;

    let matching: Vec<_> = manager.definitions
        .values()
        .filter(|d| {
            d.matches_effect_applied(effect_id)
                && manager.is_definition_active(d)
                && manager.matches_source_target_filters(
                    d, source_id, source_type, source_name, source_npc_id,
                    target_id, target_type, target_name, target_npc_id,
                )
        })
        .cloned()
        .collect();

    for def in matching {
        manager.start_timer(&def, timestamp, Some(target_id));
    }

    // Check for cancel triggers on effect applied
    manager.cancel_timers_matching(
        |t| matches!(t, TimerTrigger::EffectApplied { effects, .. } if effects.iter().any(|s| s.matches(effect_id, None))),
        &format!("effect {} applied", effect_id)
    );
}

/// Handle effect removed
///
/// Note: EffectRemoved signals don't include NPC IDs in the game log,
/// so npc_id params will typically be 0.
pub(super) fn handle_effect_removed(
    manager: &mut TimerManager,
    effect_id: i64,
    source_id: i64,
    source_type: EntityType,
    source_name: IStr,
    source_npc_id: i64,
    target_id: i64,
    target_type: EntityType,
    target_name: IStr,
    target_npc_id: i64,
    timestamp: NaiveDateTime,
) {
    // Convert i64 to u64 for matching (game IDs are always positive)
    let effect_id = effect_id as u64;

    let matching: Vec<_> = manager.definitions
        .values()
        .filter(|d| {
            d.matches_effect_removed(effect_id)
                && manager.is_definition_active(d)
                && manager.matches_source_target_filters(
                    d, source_id, source_type, source_name, source_npc_id,
                    target_id, target_type, target_name, target_npc_id,
                )
        })
        .cloned()
        .collect();

    for def in matching {
        manager.start_timer(&def, timestamp, Some(target_id));
    }

    // Check for cancel triggers on effect removed
    manager.cancel_timers_matching(
        |t| matches!(t, TimerTrigger::EffectRemoved { effects, .. } if effects.iter().any(|s| s.matches(effect_id, None))),
        &format!("effect {} removed", effect_id)
    );
}

/// Handle boss HP change - check for HP threshold triggers
pub(super) fn handle_boss_hp_change(
    manager: &mut TimerManager,
    npc_id: i64,
    npc_name: &str,
    previous_hp: f32,
    current_hp: f32,
    timestamp: NaiveDateTime,
) {
    // Don't fire HP threshold alerts when boss is dead (HP = 0)
    if current_hp <= 0.0 {
        return;
    }

    let matching: Vec<_> = manager.definitions
        .values()
        .filter(|d| d.matches_boss_hp_threshold(npc_id, Some(npc_name), previous_hp, current_hp) && manager.is_definition_active(d))
        .cloned()
        .collect();

    for def in matching {
        eprintln!("[TIMER] Starting HP threshold timer '{}' (HP crossed below {}% for {})",
            def.name,
            match &def.trigger {
                TimerTrigger::BossHpBelow { hp_percent, .. } => *hp_percent,
                _ => 0.0,
            },
            npc_name
        );
        manager.start_timer(&def, timestamp, None);
    }

    // Check for cancel triggers on boss HP threshold
    let npc_name_owned = npc_name.to_string();
    manager.cancel_timers_matching(
        |t| matches!(t, TimerTrigger::BossHpBelow { hp_percent, entity }
            if previous_hp > *hp_percent && current_hp <= *hp_percent
            && (entity.is_empty() || entity.matches_npc_id(npc_id) || entity.matches_name(&npc_name_owned))),
        &format!("boss HP below threshold for {}", npc_name)
    );
}

/// Handle phase change - check for PhaseEntered triggers
pub(super) fn handle_phase_change(manager: &mut TimerManager, phase_id: &str, timestamp: NaiveDateTime) {
    let matching: Vec<_> = manager.definitions
        .values()
        .filter(|d| d.matches_phase_entered(phase_id) && manager.is_definition_active(d))
        .cloned()
        .collect();

    for def in matching {
        eprintln!("[TIMER] Starting phase-triggered timer '{}' (phase: {})", def.name, phase_id);
        manager.start_timer(&def, timestamp, None);
    }

    // Check for cancel triggers on phase entered
    let phase_id_owned = phase_id.to_string();
    manager.cancel_timers_matching(
        |t| matches!(t, TimerTrigger::PhaseEntered { phase_id: pid } if pid == &phase_id_owned),
        &format!("phase {} entered", phase_id)
    );
}

/// Handle phase ended - check for PhaseEnded triggers
pub(super) fn handle_phase_ended(manager: &mut TimerManager, phase_id: &str, timestamp: NaiveDateTime) {
    let matching: Vec<_> = manager.definitions
        .values()
        .filter(|d| d.matches_phase_ended(phase_id) && manager.is_definition_active(d))
        .cloned()
        .collect();

    for def in matching {
        eprintln!("[TIMER] Starting phase-ended timer '{}' (phase {} ended)", def.name, phase_id);
        manager.start_timer(&def, timestamp, None);
    }

    // Check for cancel triggers on phase ended
    let phase_id_owned = phase_id.to_string();
    manager.cancel_timers_matching(
        |t| matches!(t, TimerTrigger::PhaseEnded { phase_id: pid } if pid == &phase_id_owned),
        &format!("phase {} ended", phase_id)
    );
}

/// Handle counter change - check for CounterReaches triggers
pub(super) fn handle_counter_change(
    manager: &mut TimerManager,
    counter_id: &str,
    old_value: u32,
    new_value: u32,
    timestamp: NaiveDateTime,
) {
    let matching: Vec<_> = manager.definitions
        .values()
        .filter(|d| d.matches_counter_reaches(counter_id, old_value, new_value) && manager.is_definition_active(d))
        .cloned()
        .collect();

    for def in matching {
        eprintln!("[TIMER] Starting counter-triggered timer '{}' (counter {} reached {})",
            def.name, counter_id, new_value);
        manager.start_timer(&def, timestamp, None);
    }

    // Check for cancel triggers on counter change
    let counter_id_owned = counter_id.to_string();
    manager.cancel_timers_matching(
        |t| matches!(t, TimerTrigger::CounterReaches { counter_id: cid, value }
            if cid == &counter_id_owned && old_value < *value && new_value >= *value),
        &format!("counter {} reached {}", counter_id, new_value)
    );
}

/// Handle NPC first seen - check for NpcAppears triggers
pub(super) fn handle_npc_first_seen(manager: &mut TimerManager, npc_id: i64, npc_name: &str, timestamp: NaiveDateTime) {
    let matching: Vec<_> = manager.definitions
        .values()
        .filter(|d| {
            let matches = d.matches_npc_appears(npc_id, Some(npc_name));
            if matches {
                let is_active = manager.is_definition_active(d);
                if !is_active {
                    let diff_str = manager.context.difficulty.map(|x| x.config_key()).unwrap_or("none");
                    eprintln!("[TIMER] NPC {} matches timer '{}' but context filter failed (enc={:?}, boss={:?}, diff={}, timer_diffs={:?})",
                        npc_name, d.id, manager.context.encounter_name, manager.context.boss_name, diff_str, d.difficulties);
                }
                return is_active;
            }
            false
        })
        .cloned()
        .collect();

    for def in matching {
        eprintln!("[TIMER] Starting npc-appears timer '{}' (NPC {} appeared)", def.name, npc_name);
        manager.start_timer(&def, timestamp, None);
    }

    // Check for cancel triggers on NPC appears
    let npc_name_owned = npc_name.to_string();
    manager.cancel_timers_matching(
        |t| matches!(t, TimerTrigger::NpcAppears { entity }
            if !entity.is_empty() && (entity.matches_npc_id(npc_id) || entity.matches_name(&npc_name_owned))),
        &format!("NPC {} appeared", npc_name)
    );
}

/// Handle entity death - check for EntityDeath triggers
pub(super) fn handle_entity_death(manager: &mut TimerManager, npc_id: i64, entity_name: &str, timestamp: NaiveDateTime) {
    let matching: Vec<_> = manager.definitions
        .values()
        .filter(|d| d.matches_entity_death(npc_id, Some(entity_name)) && manager.is_definition_active(d))
        .cloned()
        .collect();

    for def in matching {
        eprintln!("[TIMER] Starting death-triggered timer '{}' ({} died)", def.name, entity_name);
        manager.start_timer(&def, timestamp, None);
    }

    // Check for cancel triggers on entity death
    let entity_name_owned = entity_name.to_string();
    manager.cancel_timers_matching(
        |t| matches!(t, TimerTrigger::EntityDeath { entity }
            if entity.is_empty() || entity.matches_npc_id(npc_id) || entity.matches_name(&entity_name_owned)),
        &format!("entity {} died", entity_name)
    );
}

/// Handle target set - check for TargetSet triggers (e.g., sphere targeting player)
pub(super) fn handle_target_set(
    manager: &mut TimerManager,
    source_entity_id: i64,
    source_npc_id: i64,
    source_name: IStr,
    target_id: i64,
    target_entity_type: EntityType,
    target_name: IStr,
    timestamp: NaiveDateTime,
) {
    let source_name_str = crate::context::resolve(source_name);
    let target_name_str = crate::context::resolve(target_name);

    eprintln!("[TIMER DEBUG] TargetSet: {} (entity_id: {}, npc_id: {}) → {} (entity_id: {})",
        source_name_str, source_entity_id, source_npc_id, target_name_str, target_id);

    let matching: Vec<_> = manager.definitions
        .values()
        .filter(|d| {
            let matches_trigger = d.matches_target_set(source_npc_id, Some(&source_name_str));
            if matches_trigger {
                let is_active = manager.is_definition_active(d);
                eprintln!("[TIMER DEBUG]   Timer '{}' matches trigger, is_active={}", d.name, is_active);
                return is_active;
            }
            false
        })
        .cloned()
        .collect();

    for def in matching {
        // Check source filter (e.g., boss, any_npc - the NPC doing the targeting)
        // Source is always an NPC for TargetChanged signals
        if !def.source.matches(source_entity_id, EntityType::Npc, source_name, source_npc_id, manager.local_player_id, &manager.boss_entity_ids) {
            continue;
        }
        // Check target filter (e.g., local_player, any_player, etc.)
        if !def.target.matches(target_id, target_entity_type, target_name, 0, manager.local_player_id, &manager.boss_entity_ids) {
            continue;
        }
        eprintln!("[TIMER] Starting target-set timer '{}' (targeted by {} [{}])", def.name, source_name_str, source_npc_id);
        manager.start_timer(&def, timestamp, None);
    }

    // Check for cancel triggers on target set
    let source_name_owned = source_name_str.to_string();
    manager.cancel_timers_matching(
        |t| matches!(t, TimerTrigger::TargetSet { entity }
            if !entity.is_empty() && (entity.matches_npc_id(source_npc_id) || entity.matches_name(&source_name_owned))),
        &format!("target set by {}", source_name_owned)
    );
}

/// Handle damage taken - check for DamageTaken triggers (tank busters, raid damage, etc.)
pub(super) fn handle_damage_taken(
    manager: &mut TimerManager,
    ability_id: i64,
    ability_name: IStr,
    source_id: i64,
    source_type: EntityType,
    source_name: IStr,
    source_npc_id: i64,
    target_id: i64,
    target_type: EntityType,
    target_name: IStr,
    timestamp: NaiveDateTime,
) {
    let ability_id = ability_id as u64;
    let ability_name_str = crate::context::resolve(ability_name);

    let matching: Vec<_> = manager.definitions
        .values()
        .filter(|d| {
            d.matches_damage_taken(ability_id, Some(&ability_name_str))
                && manager.is_definition_active(d)
                && manager.matches_source_target_filters(
                    d, source_id, source_type, source_name, source_npc_id,
                    target_id, target_type, target_name, 0,
                )
        })
        .cloned()
        .collect();

    for def in matching {
        eprintln!("[TIMER] Starting damage-taken timer '{}' (ability {} / {})", def.name, ability_id, ability_name_str);
        manager.start_timer(&def, timestamp, Some(target_id));
    }

    // Check for cancel triggers on damage taken
    manager.cancel_timers_matching(
        |t| matches!(t, TimerTrigger::DamageTaken { abilities, .. } if abilities.iter().any(|s| s.matches(ability_id, Some(&ability_name_str)))),
        &format!("damage taken from {}", ability_name_str)
    );
}

/// Handle time elapsed - check for TimeElapsed triggers
pub(super) fn handle_time_elapsed(manager: &mut TimerManager, timestamp: NaiveDateTime) {
    let Some(start_time) = manager.combat_start_time else {
        return;
    };

    let new_combat_secs = (timestamp - start_time).num_milliseconds() as f32 / 1000.0;
    let old_combat_secs = manager.last_combat_secs;

    // Only check if time has progressed
    if new_combat_secs <= old_combat_secs {
        return;
    }

    let matching: Vec<_> = manager.definitions
        .values()
        .filter(|d| d.matches_time_elapsed(old_combat_secs, new_combat_secs) && manager.is_definition_active(d))
        .cloned()
        .collect();

    for def in matching {
        eprintln!("[TIMER] Starting time-triggered timer '{}' ({:.1}s into combat)", def.name, new_combat_secs);
        manager.start_timer(&def, timestamp, None);
    }

    // Check for cancel triggers on time elapsed
    manager.cancel_timers_matching(
        |t| matches!(t, TimerTrigger::TimeElapsed { secs } if old_combat_secs < *secs && new_combat_secs >= *secs),
        &format!("{:.1}s elapsed", new_combat_secs)
    );

    manager.last_combat_secs = new_combat_secs;
}

/// Handle combat start - start combat-triggered timers
pub(super) fn handle_combat_start(manager: &mut TimerManager, timestamp: NaiveDateTime) {
    manager.in_combat = true;
    manager.combat_start_time = Some(timestamp);
    manager.last_combat_secs = 0.0;

    // Debug: show current context
    eprintln!("[TIMER] combat_start context: area_id={:?}, enc={:?}, boss={:?}, diff={:?}",
        manager.context.area_id, manager.context.encounter_name,
        manager.context.boss_name, manager.context.difficulty);

    let matching: Vec<_> = manager.definitions
        .values()
        .filter(|d| {
            let has_trigger = d.triggers_on_combat_start();
            let is_active = manager.is_definition_active(d);
            if has_trigger && !is_active {
                eprintln!("[TIMER] combat_start timer '{}' skipped - context mismatch (area_ids={:?}, boss={:?}, diffs={:?})",
                    d.id, d.area_ids, d.boss, d.difficulties);
            }
            has_trigger && is_active
        })
        .cloned()
        .collect();

    eprintln!("[TIMER] combat_start matched {} timers", matching.len());
    for def in matching {
        manager.start_timer(&def, timestamp, None);
    }
}

/// Clear all combat-scoped timers
pub(super) fn clear_combat_timers(manager: &mut TimerManager) {
    manager.in_combat = false;
    manager.active_timers.clear();
    manager.fired_alerts.clear();
    manager.current_phase = None;
    manager.counters.clear();
    manager.boss_hp_by_npc.clear();
    manager.boss_entity_ids.clear();
    manager.combat_start_time = None;
    manager.last_combat_secs = 0.0;
}
