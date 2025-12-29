//! Timer matching and filtering utilities
//!
//! Contains entity filter matching and definition context checking.

use std::collections::{HashMap, HashSet};

use crate::combat_log::EntityType;
use crate::context::IStr;

use super::{EncounterContext, TimerDefinition};

/// Check if source/target filters pass for a timer definition
pub(super) fn matches_source_target_filters(
    def: &TimerDefinition,
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
    def.source.matches(source_id, source_type, source_name, source_npc_id, local_player_id, boss_entity_ids)
        && def.target.matches(target_id, target_type, target_name, target_npc_id, local_player_id, boss_entity_ids)
}

/// Check if a timer definition is active for current context
pub(super) fn is_definition_active(
    def: &TimerDefinition,
    context: &EncounterContext,
    current_phase: Option<&str>,
    counters: &HashMap<String, u32>,
) -> bool {
    // First check basic context (encounter, boss, difficulty)
    if !def.enabled || !def.is_active_for_context(
        context.encounter_name.as_deref(),
        context.boss_name.as_deref(),
        context.difficulty,
    ) {
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
