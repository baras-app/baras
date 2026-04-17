//! Signal handler functions for TimerManager
//!
//! Contains all the game signal handling logic extracted from TimerManager.
//! Each function takes `&mut TimerManager` and processes a specific signal type.

use chrono::NaiveDateTime;

use crate::combat_log::EntityType;
use crate::context::IStr;
use crate::dsl::EntityDefinition;
use crate::dsl::TriggerKind;
use crate::encounter::CombatEncounter;

use super::TimerManager;

/// Get the entity roster from the current encounter, or empty slice if none.
fn get_entities(encounter: Option<&CombatEncounter>) -> &[EntityDefinition] {
    static EMPTY: &[EntityDefinition] = &[];
    let Some(enc) = encounter else {
        return EMPTY;
    };
    let Some(idx) = enc.active_boss_idx() else {
        return EMPTY;
    };
    enc.boss_definitions()[idx].entities.as_slice()
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
        .definitions_for_kind(TriggerKind::AbilityCast)
        .iter()
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
        let instance_id = if def.per_target {
            Some(target_id)
        } else {
            None
        };
        manager.start_timer(&def, timestamp, instance_id);
    }

    // Check for cancel triggers on ability cast (entity-filtered)
    manager.cancel_timers_matching_with_source_target(
        get_entities(encounter),
        source_id,
        source_type,
        source_name,
        source_npc_id,
        target_id,
        target_type,
        target_name,
        target_npc_id,
        |t| t.matches_ability(ability_id, Some(ability_name_str)),
    );
    // Check queue_remove_trigger on ability cast (entity-filtered)
    manager.remove_queued_matching_with_source_target(
        get_entities(encounter),
        source_id,
        source_type,
        source_name,
        source_npc_id,
        target_id,
        target_type,
        target_name,
        target_npc_id,
        |t| t.matches_ability(ability_id, Some(ability_name_str)),
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
        .definitions_for_kind(TriggerKind::EffectApplied)
        .iter()
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
        let instance_id = if def.per_target {
            Some(target_id)
        } else {
            None
        };
        manager.start_timer(&def, timestamp, instance_id);
    }

    // Check for cancel triggers on effect applied (entity-filtered)
    manager.cancel_timers_matching_with_source_target(
        get_entities(encounter),
        source_id,
        source_type,
        source_name,
        source_npc_id,
        target_id,
        target_type,
        target_name,
        target_npc_id,
        |t| t.matches_effect_applied(effect_id, Some(effect_name)),
    );
    manager.remove_queued_matching_with_source_target(
        get_entities(encounter),
        source_id,
        source_type,
        source_name,
        source_npc_id,
        target_id,
        target_type,
        target_name,
        target_npc_id,
        |t| t.matches_effect_applied(effect_id, Some(effect_name)),
    );
}

/// Handle effect removed
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
        .definitions_for_kind(TriggerKind::EffectRemoved)
        .iter()
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
        let instance_id = if def.per_target {
            Some(target_id)
        } else {
            None
        };
        manager.start_timer(&def, timestamp, instance_id);
    }

    // Check for cancel triggers on effect removed (entity-filtered)
    manager.cancel_timers_matching_with_source_target(
        get_entities(encounter),
        source_id,
        source_type,
        source_name,
        source_npc_id,
        target_id,
        target_type,
        target_name,
        target_npc_id,
        |t| t.matches_effect_removed(effect_id, Some(effect_name)),
    );
    manager.remove_queued_matching_with_source_target(
        get_entities(encounter),
        source_id,
        source_type,
        source_name,
        source_npc_id,
        target_id,
        target_type,
        target_name,
        target_npc_id,
        |t| t.matches_effect_removed(effect_id, Some(effect_name)),
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
        .definitions_for_kind(TriggerKind::BossHpBelow)
        .iter()
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
    let entities = get_entities(encounter);
    let npc_name_owned = npc_name.to_string();
    manager.cancel_timers_matching_with_entities(entities, |t, ents| {
        t.matches_boss_hp_below(ents, npc_id, &npc_name_owned, previous_hp, current_hp)
    });
}

/// Handle phase change - check for PhaseEntered triggers
pub(super) fn handle_phase_change(
    manager: &mut TimerManager,
    encounter: Option<&CombatEncounter>,
    phase_id: &str,
    timestamp: NaiveDateTime,
) {
    let matching: Vec<_> = manager
        .definitions_for_kind(TriggerKind::PhaseEntered)
        .iter()
        .filter(|d| d.matches_phase_entered(phase_id) && manager.is_definition_active(d, encounter))
        .cloned()
        .collect();

    for def in matching {
        manager.start_timer(&def, timestamp, None);
    }

    // Check for cancel triggers on phase entered
    manager.cancel_timers_matching(|t| t.matches_phase_entered(phase_id));
    manager.remove_queued_matching(|t| t.matches_phase_entered(phase_id));
}

/// Handle phase ended - check for PhaseEnded triggers
pub(super) fn handle_phase_ended(
    manager: &mut TimerManager,
    encounter: Option<&CombatEncounter>,
    phase_id: &str,
    timestamp: NaiveDateTime,
) {
    let matching: Vec<_> = manager
        .definitions_for_kind(TriggerKind::PhaseEnded)
        .iter()
        .filter(|d| d.matches_phase_ended(phase_id) && manager.is_definition_active(d, encounter))
        .cloned()
        .collect();

    for def in matching {
        manager.start_timer(&def, timestamp, None);
    }

    // Check for cancel triggers on phase ended
    manager.cancel_timers_matching(|t| t.matches_phase_ended(phase_id));
    manager.remove_queued_matching(|t| t.matches_phase_ended(phase_id));
}

/// Handle any phase change - check for AnyPhaseChange triggers
///
/// This fires on every phase transition regardless of which phase is entered or exited.
/// Called from the PhaseChanged signal handler alongside PhaseEntered/PhaseEnded handlers.
pub(super) fn handle_any_phase_change(
    manager: &mut TimerManager,
    encounter: Option<&CombatEncounter>,
    timestamp: NaiveDateTime,
) {
    let matching: Vec<_> = manager
        .definitions_for_kind(TriggerKind::AnyPhaseChange)
        .iter()
        .filter(|d| manager.is_definition_active(d, encounter))
        .cloned()
        .collect();

    for def in matching {
        manager.start_timer(&def, timestamp, None);
    }

    // Check for cancel triggers on any phase change
    manager.cancel_timers_matching(|t| t.is_any_phase_change());
    manager.remove_queued_matching(|t| t.is_any_phase_change());
}

/// Handle counter change - check for CounterReaches and CounterChanges triggers
pub(super) fn handle_counter_change(
    manager: &mut TimerManager,
    encounter: Option<&CombatEncounter>,
    counter_id: &str,
    old_value: u32,
    new_value: u32,
    timestamp: NaiveDateTime,
) {
    // CounterReaches: threshold-crossing detection
    let matching: Vec<_> = manager
        .definitions_for_kind(TriggerKind::CounterReaches)
        .iter()
        .filter(|d| {
            d.matches_counter_reaches(counter_id, old_value, new_value)
                && manager.is_definition_active(d, encounter)
        })
        .cloned()
        .collect();

    for def in matching {
        manager.start_timer(&def, timestamp, None);
    }

    // CounterChanges: any change to the specified counter
    let matching_changes: Vec<_> = manager
        .definitions_for_kind(TriggerKind::CounterChanges)
        .iter()
        .filter(|d| {
            d.matches_counter_changes(counter_id) && manager.is_definition_active(d, encounter)
        })
        .cloned()
        .collect();

    for def in matching_changes {
        manager.start_timer(&def, timestamp, None);
    }

    // Check for cancel triggers on counter change
    manager.cancel_timers_matching(|t| t.matches_counter_reaches(counter_id, old_value, new_value));
    manager.cancel_timers_matching(|t| t.matches_counter_changes(counter_id));
    manager.remove_queued_matching(|t| t.matches_counter_reaches(counter_id, old_value, new_value));
    manager.remove_queued_matching(|t| t.matches_counter_changes(counter_id));
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
        .definitions_for_kind(TriggerKind::NpcAppears)
        .iter()
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
    let entities = get_entities(encounter);
    let npc_name_owned = npc_name.to_string();
    manager.cancel_timers_matching_with_entities(entities, |t, ents| {
        t.matches_npc_appears(ents, npc_id, &npc_name_owned)
    });
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
        .definitions_for_kind(TriggerKind::EntityDeath)
        .iter()
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
    let entities = get_entities(encounter);
    let entity_name_owned = entity_name.to_string();
    manager.cancel_timers_matching_with_entities(entities, |t, ents| {
        t.matches_entity_death(ents, npc_id, &entity_name_owned)
    });
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
    let entities = get_entities(encounter);

    let matching: Vec<_> = manager
        .definitions_for_kind(TriggerKind::TargetSet)
        .iter()
        .filter(|d| {
            d.matches_target_set(entities, source_npc_id, Some(source_name_str))
                && manager.is_definition_active(d, encounter)
                && manager.matches_source_target_filters(
                    &d.trigger,
                    entities,
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
    manager.cancel_timers_matching_with_entities(entities, |t, ents| {
        t.matches_target_set(ents, source_npc_id, Some(&source_name_owned))
    });
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
    target_npc_id: i64,
    timestamp: NaiveDateTime,
    defense_type_id: i64,
) {
    let ability_id = ability_id as u64;
    let ability_name_str = crate::context::resolve(ability_name);

    let matching: Vec<_> = manager
        .definitions_for_kind(TriggerKind::DamageTaken)
        .iter()
        .filter(|d| {
            d.matches_damage_taken(ability_id, Some(&ability_name_str), defense_type_id)
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
        let instance_id = if def.per_target {
            Some(target_id)
        } else {
            None
        };
        manager.start_timer(&def, timestamp, instance_id);
    }

    // Check for cancel triggers on damage taken (entity-filtered)
    manager.cancel_timers_matching_with_source_target(
        get_entities(encounter),
        source_id,
        source_type,
        source_name,
        source_npc_id,
        target_id,
        target_type,
        target_name,
        target_npc_id,
        |t| t.matches_damage_taken(ability_id, Some(&ability_name_str), defense_type_id),
    );
    manager.remove_queued_matching_with_source_target(
        get_entities(encounter),
        source_id,
        source_type,
        source_name,
        source_npc_id,
        target_id,
        target_type,
        target_name,
        target_npc_id,
        |t| t.matches_damage_taken(ability_id, Some(&ability_name_str), defense_type_id),
    );
}

/// Handle threat modified - check for ThreatModified triggers
pub(super) fn handle_threat_modified(
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
    threat: f32,
    timestamp: NaiveDateTime,
) {
    let ability_id = ability_id as u64;
    let ability_name_str = crate::context::resolve(ability_name);

    let matching: Vec<_> = manager
        .definitions_for_kind(TriggerKind::ThreatModified)
        .iter()
        .filter(|d| {
            d.matches_threat_modified(ability_id, Some(&ability_name_str), threat)
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
        let instance_id = if def.per_target { Some(target_id) } else { None };
        manager.start_timer(&def, timestamp, instance_id);
    }

    manager.cancel_timers_matching_with_source_target(
        get_entities(encounter),
        source_id, source_type, source_name, source_npc_id,
        target_id, target_type, target_name, target_npc_id,
        |t| t.matches_threat_modified(ability_id, Some(&ability_name_str), threat),
    );
    manager.remove_queued_matching_with_source_target(
        get_entities(encounter),
        source_id, source_type, source_name, source_npc_id,
        target_id, target_type, target_name, target_npc_id,
        |t| t.matches_threat_modified(ability_id, Some(&ability_name_str), threat),
    );
}

/// Handle healing taken - check for HealingTaken triggers
pub(super) fn handle_healing_taken(
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
        .definitions_for_kind(TriggerKind::HealingTaken)
        .iter()
        .filter(|d| {
            d.matches_healing_taken(ability_id, Some(&ability_name_str))
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
        let instance_id = if def.per_target {
            Some(target_id)
        } else {
            None
        };
        manager.start_timer(&def, timestamp, instance_id);
    }

    // Check for cancel triggers on healing taken (entity-filtered)
    manager.cancel_timers_matching_with_source_target(
        get_entities(encounter),
        source_id,
        source_type,
        source_name,
        source_npc_id,
        target_id,
        target_type,
        target_name,
        target_npc_id,
        |t| t.matches_healing_taken(ability_id, Some(&ability_name_str)),
    );
    manager.remove_queued_matching_with_source_target(
        get_entities(encounter),
        source_id,
        source_type,
        source_name,
        source_npc_id,
        target_id,
        target_type,
        target_name,
        target_npc_id,
        |t| t.matches_healing_taken(ability_id, Some(&ability_name_str)),
    );
}

/// Evaluate combat-time-based triggers: CombatStart and TimeElapsed.
///
/// Both are treated uniformly: CombatStart fires at combat_time >= 0,
/// TimeElapsed fires at combat_time >= threshold_secs.
///
/// Called on every tick and signal. Deduplication is handled by `start_timer`
/// which ignores already-active timers. Start timestamps are backdated to
/// `enter_combat_time + threshold` so remaining duration is correct regardless
/// of when definitions loaded or when this is first evaluated.
pub(super) fn handle_combat_time_triggers(
    manager: &mut TimerManager,
    encounter: Option<&CombatEncounter>,
) {
    if !manager.in_combat {
        return;
    }

    let Some(enc) = encounter else {
        return;
    };

    let combat_secs = enc.combat_time_secs;
    let Some(combat_start) = enc.enter_combat_time else {
        return;
    };

    // Find definitions whose combat-time trigger is met (CombatStart or TimeElapsed)
    // Skip definitions already started this combat (prevents re-creation after cancel)
    let matching: Vec<_> = manager
        .definitions_for_kind(TriggerKind::CombatTime)
        .iter()
        .filter(|d| {
            d.trigger.is_combat_time_met(combat_secs)
                && !manager.combat_time_started.contains(&d.id)
                && manager.is_definition_active(d, encounter)
        })
        .cloned()
        .collect();

    for def in matching {
        let threshold = def.trigger.combat_time_threshold().unwrap_or(0.0);
        let start_ts = combat_start + chrono::Duration::milliseconds((threshold * 1000.0) as i64);
        manager.combat_time_started.insert(def.id.clone());
        manager.start_timer(&def, start_ts, None);
    }

    // Cancel triggers based on combat time
    manager.cancel_timers_matching(|t| t.is_combat_time_met(combat_secs));
    manager.remove_queued_matching(|t| t.is_combat_time_met(combat_secs));
}

/// Clear all combat-scoped timers and encounter context
pub(super) fn clear_combat_timers(manager: &mut TimerManager) {
    manager.in_combat = false;
    manager.combat_start_time = None;
    manager.active_timers.clear();
    manager.active_gcd = None;
    manager.fired_alerts.clear();
    manager.boss_entity_ids.clear();
    manager.combat_time_started.clear();
    // Boss name is now read from encounter.active_boss directly
    manager.clear_boss_npc_class_ids();
    // Clear encounter tracking so next encounter triggers fresh initialization
    manager.active_encounter_id = None;
}
