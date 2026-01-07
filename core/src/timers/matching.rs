//! Timer matching and filtering utilities
//!
//! Contains entity filter matching and definition context checking.

use hashbrown::HashMap;
use std::collections::HashSet;

use crate::combat_log::EntityType;
use crate::context::IStr;
use crate::dsl::EntityDefinition;
use crate::dsl::EntityFilterMatching;
use crate::dsl::Trigger;
use crate::encounter::CombatEncounter;

use super::TimerDefinition;

/// Check if source/target filters pass for a trigger
pub(super) fn matches_source_target_filters(
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
    boss_entity_ids: &HashSet<i64>,
) -> bool {
    // Check source filter if present (None = any, passes)
    if let Some(source_filter) = trigger.source_filter() {
        if !source_filter.matches(
            entities,
            source_id,
            source_type,
            source_name,
            source_npc_id,
            local_player_id,
            boss_entity_ids,
        ) {
            return false;
        }
    }

    // Check target filter if present (None = any, passes)
    if let Some(target_filter) = trigger.target_filter() {
        if !target_filter.matches(
            entities,
            target_id,
            target_type,
            target_name,
            target_npc_id,
            local_player_id,
            boss_entity_ids,
        ) {
            return false;
        }
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
    let (area_id, area_name, boss_name, difficulty, current_phase, counters) = match encounter {
        Some(enc) => (
            enc.area_id,
            enc.area_name.as_deref(),
            enc.active_boss.as_ref().map(|b| b.name.as_str()),
            enc.difficulty,
            enc.current_phase.as_deref(),
            &enc.counters,
        ),
        None => (None, None, None, None, None, &*EMPTY_COUNTERS),
    };

    // First check basic context (area, boss, difficulty)
    if !def.enabled || !def.is_active_for_context(area_id, area_name, boss_name, difficulty) {
        return false;
    }

    // Check phase filter
    if !def.phases.is_empty() {
        if let Some(current) = current_phase {
            if !def.phases.iter().any(|p| p == current) {
                return false;
            }
        } else {
            return false; // Timer requires phase but none active
        }
    }

    // Check counter condition
    if let Some(ref cond) = def.counter_condition {
        let value = counters.get(&cond.counter_id).copied().unwrap_or(0);
        if !cond.operator.evaluate(value, cond.value) {
            return false;
        }
    }

    true
}

/// Empty counters for when no encounter is available
static EMPTY_COUNTERS: std::sync::LazyLock<HashMap<String, u32>> =
    std::sync::LazyLock::new(HashMap::new);
