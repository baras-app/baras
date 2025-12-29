//! Entity filter matching
//!
//! Defines filters for matching entities by type, role, or identity.
//! Used by both effects and timers for source/target filtering.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::combat_log::EntityType;
use crate::context::IStr;

/// Filter for matching entities (used for both source and target)
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityFilter {
    /// The local player only
    #[default]
    LocalPlayer,
    /// Local player's companion
    LocalCompanion,
    /// Local player OR their companion
    LocalPlayerOrCompanion,
    /// Other players (not local)
    OtherPlayers,
    /// Other players' companions
    OtherCompanions,
    /// Any player (including local)
    AnyPlayer,
    /// Any companion (any player's)
    AnyCompanion,
    /// Any player or companion
    AnyPlayerOrCompanion,
    /// Group members (players in the local player's group)
    GroupMembers,
    /// Group members except local player
    GroupMembersExceptLocal,
    /// Boss NPCs specifically
    Boss,
    /// Non-boss NPCs (trash mobs)
    NpcExceptBoss,
    /// Any NPC (boss or trash)
    AnyNpc,
    /// Specific entity by name
    Specific(String),
    /// Specific NPC by class/template ID
    SpecificNpc(i64),
    /// Any entity whatsoever
    Any,
}

impl EntityFilter {
    /// Check if an entity matches this filter.
    ///
    /// # Arguments
    /// * `entity_id` - Runtime entity ID
    /// * `entity_type` - Player, Companion, or NPC
    /// * `entity_name` - Entity's display name (interned)
    /// * `npc_id` - NPC class/template ID (0 for players/companions)
    /// * `local_player_id` - The local player's entity ID (for LocalPlayer filter)
    /// * `boss_entity_ids` - Set of entity IDs marked as bosses
    pub fn matches(
        &self,
        entity_id: i64,
        entity_type: EntityType,
        entity_name: IStr,
        npc_id: i64,
        local_player_id: Option<i64>,
        boss_entity_ids: &HashSet<i64>,
    ) -> bool {
        let is_local = local_player_id == Some(entity_id);
        let is_player = matches!(entity_type, EntityType::Player);
        let is_companion = matches!(entity_type, EntityType::Companion);
        let is_npc = matches!(entity_type, EntityType::Npc);

        match self {
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

            // Specific NPC by class/template ID
            EntityFilter::SpecificNpc(filter_npc_id) => is_npc && npc_id == *filter_npc_id,

            // Any entity
            EntityFilter::Any => true,
        }
    }
}
