//! Timer matching and filtering utilities
//!
//! Contains entity filter matching and definition context checking.

use std::collections::{HashMap, HashSet};

use crate::combat_log::EntityType;
use crate::effects::EntityFilter;
use crate::context::IStr;

use super::{EncounterContext, TimerDefinition};

/// Check if an entity matches an EntityFilter
pub(super) fn matches_entity_filter(
    filter: &EntityFilter,
    entity_id: i64,
    entity_type: EntityType,
    entity_name: IStr,
    local_player_id: Option<i64>,
    boss_entity_ids: &HashSet<i64>,
) -> bool {
    let is_local = local_player_id == Some(entity_id);
    let is_player = matches!(entity_type, EntityType::Player);
    let is_companion = matches!(entity_type, EntityType::Companion);
    let is_npc = matches!(entity_type, EntityType::Npc);

    match filter {
        // Player filters
        EntityFilter::LocalPlayer => is_local && is_player,
        EntityFilter::OtherPlayers => !is_local && is_player,
        EntityFilter::AnyPlayer => is_player,
        EntityFilter::GroupMembers => is_player,
        EntityFilter::GroupMembersExceptLocal => !is_local && is_player,

        // Companion filters
        EntityFilter::LocalCompanion => is_companion,
        EntityFilter::OtherCompanions => is_companion && !is_local,
        EntityFilter::AnyCompanion => is_companion,
        EntityFilter::LocalPlayerOrCompanion => (is_local && is_player) || is_companion,
        EntityFilter::AnyPlayerOrCompanion => is_player || is_companion,

        // NPC filters
        EntityFilter::AnyNpc => is_npc,
        EntityFilter::Boss => is_npc && boss_entity_ids.contains(&entity_id),
        EntityFilter::NpcExceptBoss => is_npc && !boss_entity_ids.contains(&entity_id),

        // Specific entity by name
        EntityFilter::Specific(name) => {
            let resolved_name = crate::context::resolve(entity_name);
            resolved_name.eq_ignore_ascii_case(name)
        }

        // Specific NPC by class/template ID - would need npc_id passed in
        EntityFilter::SpecificNpc(_npc_id) => false, // TODO: needs npc_id

        // Any entity
        EntityFilter::Any => true,
    }
}

/// Check if source/target filters pass for a timer definition
pub(super) fn matches_source_target_filters(
    def: &TimerDefinition,
    source_id: i64,
    source_type: EntityType,
    source_name: IStr,
    target_id: i64,
    target_type: EntityType,
    target_name: IStr,
    local_player_id: Option<i64>,
    boss_entity_ids: &HashSet<i64>,
) -> bool {
    matches_entity_filter(&def.source, source_id, source_type, source_name, local_player_id, boss_entity_ids)
        && matches_entity_filter(&def.target, target_id, target_type, target_name, local_player_id, boss_entity_ids)
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
