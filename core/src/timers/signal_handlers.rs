//! Signal handler functions for TimerManager
//!
//! Contains all the game signal handling logic extracted from TimerManager.
//! Each function takes `&mut TimerManager` and processes a specific signal type.

use chrono::NaiveDateTime;

use crate::combat_log::EntityType;
use crate::context::IStr;
use crate::dsl::EntityDefinition;
use crate::encounter::CombatEncounter;

use super::{TimerManager, TimerTrigger};
use crate::dsl::EntitySelectorExt;

/// Get the entity roster from the current encounter, or empty slice if none.
fn get_entities(encounter: Option<&CombatEncounter>) -> &[EntityDefinition] {
    static EMPTY: &[EntityDefinition] = &[];
    encounter
        .and_then(|e| e.active_boss_idx())
        .map(|idx| {
            encounter.unwrap().boss_definitions()[idx]
                .entities
                .as_slice()
        })
        .unwrap_or(EMPTY)
}

/// Handle ability activation
pub(super) fn handle_ability(
    manager: &mut TimerManager,
    encounter: Option<&CombatEncounter>,
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
    let ability_id = ability_id as u64;
    let ability_name_str = crate::context::resolve(ability_name);

    let matching: Vec<_> = manager
        .definitions
        .values()
        .filter(|d| {
            d.matches_ability_with_name(ability_id, Some(ability_name_str))
                && manager.is_definition_active(d, encounter)
                && manager.matches_source_target_filters(
                    &d.trigger,
                    get_entities(encounter),
                    source_id,
                    source_type,
                    source_name,
                    source_npc_id,
                    target_id,
                    target_type,
                    target_name,
                    target_npc_id,
                )
        })
        .cloned()
        .collect();

    for def in matching {
        manager.start_timer(&def, timestamp, Some(target_id));
    }

    // Check for cancel triggers on ability cast
    manager.cancel_timers_matching(
        |t| matches!(t, TimerTrigger::AbilityCast { abilities, .. } if abilities.iter().any(|s| s.matches(ability_id, Some(ability_name_str)))),
        &format!("ability {} cast", ability_id)
    );
}

/// Handle effect applied
pub(super) fn handle_effect_applied(
    manager: &mut TimerManager,
    encounter: Option<&CombatEncounter>,
    effect_id: i64,
    effect_name: &str,
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

    let matching: Vec<_> = manager
        .definitions
        .values()
        .filter(|d| {
            d.matches_effect_applied(effect_id, Some(effect_name))
                && manager.is_definition_active(d, encounter)
                && manager.matches_source_target_filters(
                    &d.trigger,
                    get_entities(encounter),
                    source_id,
                    source_type,
                    source_name,
                    source_npc_id,
                    target_id,
                    target_type,
                    target_name,
                    target_npc_id,
                )
        })
        .cloned()
        .collect();

    for def in matching {
        manager.start_timer(&def, timestamp, Some(target_id));
    }

    // Check for cancel triggers on effect applied
    manager.cancel_timers_matching(
        |t| matches!(t, TimerTrigger::EffectApplied { effects, .. } if effects.iter().any(|s| s.matches(effect_id, Some(effect_name)))),
        &format!("effect {} applied", effect_name)
    );
}

/// Handle effect removed
///
/// Note: EffectRemoved signals don't include NPC IDs in the game log,
/// so npc_id params will typically be 0.
pub(super) fn handle_effect_removed(
    manager: &mut TimerManager,
    encounter: Option<&CombatEncounter>,
    effect_id: i64,
    effect_name: &str,
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

    let matching: Vec<_> = manager
        .definitions
        .values()
        .filter(|d| {
            d.matches_effect_removed(effect_id, Some(effect_name))
                && manager.is_definition_active(d, encounter)
                && manager.matches_source_target_filters(
                    &d.trigger,
                    get_entities(encounter),
                    source_id,
                    source_type,
                    source_name,
                    source_npc_id,
                    target_id,
                    target_type,
                    target_name,
                    target_npc_id,
                )
        })
        .cloned()
        .collect();

    for def in matching {
        manager.start_timer(&def, timestamp, Some(target_id));
    }

    // Check for cancel triggers on effect removed
    manager.cancel_timers_matching(
        |t| matches!(t, TimerTrigger::EffectRemoved { effects, .. } if effects.iter().any(|s| s.matches(effect_id, Some(effect_name)))),
        &format!("effect {} removed", effect_name)
    );
}

/// Handle boss HP change - check for HP threshold triggers
pub(super) fn handle_boss_hp_change(
    manager: &mut TimerManager,
    encounter: Option<&CombatEncounter>,
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

    let matching: Vec<_> = manager
        .definitions
        .values()
        .filter(|d| {
            d.matches_boss_hp_threshold(
                get_entities(encounter),
                npc_id,
                Some(npc_name),
                previous_hp,
                current_hp,
            ) && manager.is_definition_active(d, encounter)
        })
        .cloned()
        .collect();

    for def in matching {
        manager.start_timer(&def, timestamp, None);
    }

    // Check for cancel triggers on boss HP threshold
    let npc_name_owned = npc_name.to_string();
    manager.cancel_timers_matching(
        |t| matches!(t, TimerTrigger::BossHpBelow { hp_percent, selector }
            if previous_hp > *hp_percent && current_hp <= *hp_percent
            && (selector.is_empty() || selector.matches_npc_id(npc_id) || selector.matches_name_only(&npc_name_owned))),
        &format!("boss HP below threshold for {}", npc_name)
    );
}

/// Handle phase change - check for PhaseEntered triggers
pub(super) fn handle_phase_change(
    manager: &mut TimerManager,
    encounter: Option<&CombatEncounter>,
    phase_id: &str,
    timestamp: NaiveDateTime,
) {
    let matching: Vec<_> = manager
        .definitions
        .values()
        .filter(|d| d.matches_phase_entered(phase_id) && manager.is_definition_active(d, encounter))
        .cloned()
        .collect();

    for def in matching {
        manager.start_timer(&def, timestamp, None);
    }

    // Check for cancel triggers on phase entered
    let phase_id_owned = phase_id.to_string();
    manager.cancel_timers_matching(
        |t| matches!(t, TimerTrigger::PhaseEntered { phase_id: pid } if pid == &phase_id_owned),
        &format!("phase {} entered", phase_id),
    );
}

/// Handle phase ended - check for PhaseEnded triggers
pub(super) fn handle_phase_ended(
    manager: &mut TimerManager,
    encounter: Option<&CombatEncounter>,
    phase_id: &str,
    timestamp: NaiveDateTime,
) {
    let matching: Vec<_> = manager
        .definitions
        .values()
        .filter(|d| d.matches_phase_ended(phase_id) && manager.is_definition_active(d, encounter))
        .cloned()
        .collect();

    for def in matching {
        manager.start_timer(&def, timestamp, None);
    }

    // Check for cancel triggers on phase ended
    let phase_id_owned = phase_id.to_string();
    manager.cancel_timers_matching(
        |t| matches!(t, TimerTrigger::PhaseEnded { phase_id: pid } if pid == &phase_id_owned),
        &format!("phase {} ended", phase_id),
    );
}

/// Handle counter change - check for CounterReaches triggers
pub(super) fn handle_counter_change(
    manager: &mut TimerManager,
    encounter: Option<&CombatEncounter>,
    counter_id: &str,
    old_value: u32,
    new_value: u32,
    timestamp: NaiveDateTime,
) {
    let matching: Vec<_> = manager
        .definitions
        .values()
        .filter(|d| {
            d.matches_counter_reaches(counter_id, old_value, new_value)
                && manager.is_definition_active(d, encounter)
        })
        .cloned()
        .collect();

    for def in matching {
        manager.start_timer(&def, timestamp, None);
    }

    // Check for cancel triggers on counter change
    let counter_id_owned = counter_id.to_string();
    manager.cancel_timers_matching(
        |t| {
            matches!(t, TimerTrigger::CounterReaches { counter_id: cid, value }
            if cid == &counter_id_owned && old_value < *value && new_value >= *value)
        },
        &format!("counter {} reached {}", counter_id, new_value),
    );
}

/// Handle NPC first seen - check for NpcAppears triggers
pub(super) fn handle_npc_first_seen(
    manager: &mut TimerManager,
    encounter: Option<&CombatEncounter>,
    npc_id: i64,
    npc_name: &str,
    timestamp: NaiveDateTime,
) {
    let matching: Vec<_> = manager
        .definitions
        .values()
        .filter(|d| {
            d.matches_npc_appears(get_entities(encounter), npc_id, Some(npc_name))
                && manager.is_definition_active(d, encounter)
        })
        .cloned()
        .collect();

    for def in matching {
        manager.start_timer(&def, timestamp, None);
    }

    // Check for cancel triggers on NPC appears
    let npc_name_owned = npc_name.to_string();
    manager.cancel_timers_matching(
        |t| matches!(t, TimerTrigger::NpcAppears { selector }
            if !selector.is_empty() && (selector.matches_npc_id(npc_id) || selector.matches_name_only(&npc_name_owned))),
        &format!("NPC {} appeared", npc_name)
    );
}

/// Handle entity death - check for EntityDeath triggers
pub(super) fn handle_entity_death(
    manager: &mut TimerManager,
    encounter: Option<&CombatEncounter>,
    npc_id: i64,
    entity_name: &str,
    timestamp: NaiveDateTime,
) {
    let matching: Vec<_> = manager
        .definitions
        .values()
        .filter(|d| {
            d.matches_entity_death(get_entities(encounter), npc_id, Some(entity_name))
                && manager.is_definition_active(d, encounter)
        })
        .cloned()
        .collect();

    for def in matching {
        manager.start_timer(&def, timestamp, None);
    }

    // Check for cancel triggers on entity death
    let entity_name_owned = entity_name.to_string();
    manager.cancel_timers_matching(
        |t| matches!(t, TimerTrigger::EntityDeath { selector }
            if selector.is_empty() || selector.matches_npc_id(npc_id) || selector.matches_name_only(&entity_name_owned)),
        &format!("entity {} died", entity_name)
    );
}

/// Handle target set - check for TargetSet triggers (e.g., sphere targeting player)
pub(super) fn handle_target_set(
    manager: &mut TimerManager,
    encounter: Option<&CombatEncounter>,
    source_entity_id: i64,
    source_npc_id: i64,
    source_name: IStr,
    target_id: i64,
    target_entity_type: EntityType,
    target_name: IStr,
    timestamp: NaiveDateTime,
) {
    let source_name_str = crate::context::resolve(source_name);

    let matching: Vec<_> = manager
        .definitions
        .values()
        .filter(|d| {
            d.matches_target_set(source_npc_id, Some(source_name_str))
                && manager.is_definition_active(d, encounter)
                && manager.matches_source_target_filters(
                    &d.trigger,
                    get_entities(encounter),
                    source_entity_id,
                    EntityType::Npc,
                    source_name,
                    source_npc_id,
                    target_id,
                    target_entity_type,
                    target_name,
                    0,
                )
        })
        .cloned()
        .collect();

    for def in matching {
        manager.start_timer(&def, timestamp, None);
    }

    // Check for cancel triggers on target set
    let source_name_owned = source_name_str.to_string();
    manager.cancel_timers_matching(
        |t| matches!(t, TimerTrigger::TargetSet { selector, .. }
            if !selector.is_empty() && (selector.matches_npc_id(source_npc_id) || selector.matches_name_only(&source_name_owned))),
        &format!("target set by {}", source_name_owned)
    );
}

/// Handle damage taken - check for DamageTaken triggers (tank busters, raid damage, etc.)
pub(super) fn handle_damage_taken(
    manager: &mut TimerManager,
    encounter: Option<&CombatEncounter>,
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

    let matching: Vec<_> = manager
        .definitions
        .values()
        .filter(|d| {
            d.matches_damage_taken(ability_id, Some(&ability_name_str))
                && manager.is_definition_active(d, encounter)
                && manager.matches_source_target_filters(
                    &d.trigger,
                    get_entities(encounter),
                    source_id,
                    source_type,
                    source_name,
                    source_npc_id,
                    target_id,
                    target_type,
                    target_name,
                    0,
                )
        })
        .cloned()
        .collect();

    for def in matching {
        manager.start_timer(&def, timestamp, Some(target_id));
    }

    // Check for cancel triggers on damage taken
    manager.cancel_timers_matching(
        |t| matches!(t, TimerTrigger::DamageTaken { abilities, .. } if abilities.iter().any(|s| s.matches(ability_id, Some(&ability_name_str)))),
        &format!("damage taken from {}", ability_name_str)
    );
}

/// Handle time elapsed - check for TimeElapsed triggers
pub(super) fn handle_time_elapsed(
    manager: &mut TimerManager,
    encounter: Option<&CombatEncounter>,
    _timestamp: NaiveDateTime,
) {
    let Some(enc) = encounter else {
        return;
    };

    // Read combat time from encounter (source of truth)
    let new_combat_secs = enc.combat_time_secs;
    let old_combat_secs = enc.prev_combat_time_secs;

    // Only check if time has progressed
    if new_combat_secs <= old_combat_secs {
        return;
    }

    let matching: Vec<_> = manager
        .definitions
        .values()
        .filter(|d| {
            d.matches_time_elapsed(old_combat_secs, new_combat_secs)
                && manager.is_definition_active(d, encounter)
        })
        .cloned()
        .collect();

    for def in matching {
        manager.start_timer(&def, _timestamp, None);
    }

    // Check for cancel triggers on time elapsed
    manager.cancel_timers_matching(
        |t| matches!(t, TimerTrigger::TimeElapsed { secs } if old_combat_secs < *secs && new_combat_secs >= *secs),
        &format!("{:.1}s elapsed", new_combat_secs)
    );
}

/// Handle combat start - start combat-triggered timers
pub(super) fn handle_combat_start(
    manager: &mut TimerManager,
    encounter: Option<&CombatEncounter>,
    timestamp: NaiveDateTime,
) {
    manager.in_combat = true;

    let matching: Vec<_> = manager
        .definitions
        .values()
        .filter(|d| d.triggers_on_combat_start() && manager.is_definition_active(d, encounter))
        .cloned()
        .collect();

    for def in matching {
        manager.start_timer(&def, timestamp, None);
    }
}

/// Clear all combat-scoped timers and encounter context
pub(super) fn clear_combat_timers(manager: &mut TimerManager) {
    manager.in_combat = false;
    manager.active_timers.clear();
    manager.fired_alerts.clear();
    manager.boss_entity_ids.clear();
    // Boss name is now read from encounter.active_boss directly
    manager.clear_boss_npc_class_ids();
}
