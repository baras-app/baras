//! Timer matching and filtering utilities
//!
//! Contains entity filter matching and definition context checking.

use std::collections::HashSet;

use crate::combat_log::{EntityType, Position};
use crate::context::IStr;
use crate::dsl::EntityDefinition;
use crate::dsl::EntityFilterMatching;
use crate::dsl::Trigger;
use crate::dsl::triggers::matches_position_constraints;
use crate::encounter::CombatEncounter;

use super::TimerDefinition;

/// Check if source/target filters pass for a trigger.
/// Used by timers, phases, victory triggers, and counters.
/// `source_pos`/`target_pos` are the entities' positions for position
/// constraint checks; pass `None` when unknown (constraints then fail).
pub fn matches_source_target_filters(
    trigger: &Trigger,
    entities: &[EntityDefinition],
    source_id: i64,
    source_type: EntityType,
    source_name: IStr,
    source_npc_id: i64,
    target_id: i64,
    target_type: EntityType,
    target_name: IStr,
    target_npc_id: i64,
    local_player_id: Option<i64>,
    current_target_id: Option<i64>,
    boss_entity_ids: &HashSet<i64>,
    pvp_factions: Option<&crate::encounter::PvpFactionMap>,
    source_pos: Option<Position>,
    target_pos: Option<Position>,
) -> bool {
    // Position constraints (empty list passes)
    if !matches_position_constraints(trigger.position_constraints(), source_pos, target_pos) {
        return false;
    }

    // Check source filter if present (None = any, passes)
    if let Some(source_filter) = trigger.source_filter()
        && !source_filter.matches(
            entities,
            source_id,
            source_type,
            source_name,
            source_npc_id,
            local_player_id,
            current_target_id,
            boss_entity_ids,
            pvp_factions,
        ) {
            return false;
        }

    // Check target filter if present (None = any, passes)
    if let Some(target_filter) = trigger.target_filter()
        && !target_filter.matches(
            entities,
            target_id,
            target_type,
            target_name,
            target_npc_id,
            local_player_id,
            current_target_id,
            boss_entity_ids,
            pvp_factions,
        ) {
            return false;
        }

    true
}

/// Check if a timer definition is active for current encounter context.
/// Reads context directly from the encounter (single source of truth).
pub(super) fn is_definition_active(
    def: &TimerDefinition,
    encounter: Option<&CombatEncounter>,
) -> bool {
    // Extract context from encounter
    let (area_id, area_name, boss_name, boss_def_id, difficulty) = match encounter {
        Some(enc) => (
            enc.area_id,
            enc.area_name.as_deref(),
            enc.active_boss.as_ref().map(|b| b.name.as_str()),
            enc.active_boss.as_ref().map(|b| b.definition_id.as_str()),
            enc.difficulty,
        ),
        None => (None, None, None, None, None),
    };

    // First check basic context (area, boss, difficulty)
    if !def.enabled
        || !def.is_active_for_context(area_id, area_name, boss_name, boss_def_id, difficulty)
    {
        return false;
    }

    // Check conditions (new unified system + legacy phases/counter_condition)
    if let Some(enc) = encounter {
        if !enc.evaluate_merged_conditions(
            &def.conditions,
            &def.phases,
            def.counter_condition.as_ref(),
        ) {
            return false;
        }
    } else {
        // No encounter context — fail if any conditions are specified
        if !def.conditions.is_empty() || !def.phases.is_empty() || def.counter_condition.is_some() {
            return false;
        }
    }

    true
}

/// Check if a timer definition is active, using a fresh timer_remaining snapshot.
///
/// Identical to `is_definition_active` except `TimerTimeRemaining` conditions
/// are evaluated against the provided `timer_snapshot` instead of the encounter's
/// cached `timer_remaining`. This prevents stale-snapshot race conditions when
/// timers expire mid-processing and trigger other timers whose conditions
/// depend on the just-expired timer's state.
pub(super) fn is_definition_active_with_snapshot(
    def: &TimerDefinition,
    encounter: Option<&CombatEncounter>,
    timer_snapshot: &hashbrown::HashMap<String, f32>,
) -> bool {
    let (area_id, area_name, boss_name, boss_def_id, difficulty) = match encounter {
        Some(enc) => (
            enc.area_id,
            enc.area_name.as_deref(),
            enc.active_boss.as_ref().map(|b| b.name.as_str()),
            enc.active_boss.as_ref().map(|b| b.definition_id.as_str()),
            enc.difficulty,
        ),
        None => (None, None, None, None, None),
    };

    if !def.enabled
        || !def.is_active_for_context(area_id, area_name, boss_name, boss_def_id, difficulty)
    {
        return false;
    }

    if let Some(enc) = encounter {
        if !enc.evaluate_merged_conditions_with_timer_snapshot(
            &def.conditions,
            &def.phases,
            def.counter_condition.as_ref(),
            timer_snapshot,
        ) {
            return false;
        }
    } else {
        if !def.conditions.is_empty() || !def.phases.is_empty() || def.counter_condition.is_some() {
            return false;
        }
    }

    true
}
