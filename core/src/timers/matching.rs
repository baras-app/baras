//! Timer matching and filtering utilities
//!
//! Contains entity filter matching and definition context checking.

use std::collections::{HashMap, HashSet};

use crate::combat_log::EntityType;
use crate::context::IStr;
use crate::entity_filter::{EntityFilter, EntityFilterMatching};
use crate::triggers::Trigger;

use super::{EncounterContext, TimerDefinition};

/// Check if source/target filters pass for a trigger
pub(super) fn matches_source_target_filters(
    trigger: &Trigger,
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
        if !source_filter.matches(source_id, source_type, source_name, source_npc_id, local_player_id, boss_entity_ids) {
            return false;
        }
    }

    // Check target filter if present (None = any, passes)
    if let Some(target_filter) = trigger.target_filter() {
        if !target_filter.matches(target_id, target_type, target_name, target_npc_id, local_player_id, boss_entity_ids) {
            return false;
        }
    }

    true
}

/// Check if a timer definition is active for current context
pub(super) fn is_definition_active(
    def: &TimerDefinition,
    context: &EncounterContext,
    current_phase: Option<&str>,
    counters: &HashMap<String, u32>,
) -> bool {
    // First check basic context (area, boss, difficulty)
    if !def.enabled || !def.is_active_for_context(
        context.area_id,
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
