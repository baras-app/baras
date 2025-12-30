//! Entity filter matching
//!
//! Re-exports EntityFilter from baras-types and provides matching logic.
//! The type definition lives in the shared types crate for frontend reuse.

use std::collections::HashSet;

use crate::combat_log::EntityType;
use crate::context::IStr;

// Re-export the type from the shared crate
pub use baras_types::{EntityFilter, EntitySelector};

/// Extension trait for EntityFilter matching logic
///
/// The type definition lives in baras-types for sharing with the frontend,
/// but the matching logic uses core-specific types (EntityType, IStr, etc.)
pub trait EntityFilterMatching {
    /// Check if an entity matches this filter.
    ///
    /// # Arguments
    /// * `entity_id` - Runtime entity ID
    /// * `entity_type` - Player, Companion, or NPC
    /// * `entity_name` - Entity's display name (interned)
    /// * `npc_id` - NPC class/template ID (0 for players/companions)
    /// * `local_player_id` - The local player's entity ID (for LocalPlayer filter)
    /// * `boss_entity_ids` - Set of entity IDs marked as bosses
    fn matches(
        &self,
        entity_id: i64,
        entity_type: EntityType,
        entity_name: IStr,
        npc_id: i64,
        local_player_id: Option<i64>,
        boss_entity_ids: &HashSet<i64>,
    ) -> bool;

    /// Check if an entity matches this filter for challenge conditions.
    ///
    /// This variant uses NPC class IDs (from boss config) instead of runtime
    /// entity IDs. Used for challenge source/target matching where we check
    /// against the configured boss NPC IDs.
    ///
    /// # Arguments
    /// * `is_player` - Whether entity is a player
    /// * `is_local_player` - Whether entity is the local player
    /// * `name` - Entity's display name
    /// * `npc_id` - NPC class/template ID (None for players)
    /// * `boss_npc_ids` - Boss NPC class IDs from encounter config
    fn matches_challenge(
        &self,
        is_player: bool,
        is_local_player: bool,
        name: &str,
        npc_id: Option<i64>,
        boss_npc_ids: &[i64],
    ) -> bool;
}

impl EntityFilterMatching for EntityFilter {
    fn matches(
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
            EntityFilter::AnyCompanion => is_companion,
            EntityFilter::AnyPlayerOrCompanion => is_player || is_companion,

            // NPC filters
            EntityFilter::AnyNpc => is_npc,
            EntityFilter::Boss => is_npc && boss_entity_ids.contains(&entity_id),
            EntityFilter::NpcExceptBoss => is_npc && !boss_entity_ids.contains(&entity_id),

            // Unified selector - matches by ID or name
            EntityFilter::Selector(selectors) => {
                let resolved_name = crate::context::resolve(entity_name);
                selectors.iter().any(|sel| match sel {
                    EntitySelector::Id(id) => is_npc && npc_id == *id,
                    EntitySelector::Name(name) => resolved_name.eq_ignore_ascii_case(name),
                })
            }

            // Any entity
            EntityFilter::Any => true,
        }
    }

    fn matches_challenge(
        &self,
        is_player: bool,
        is_local_player: bool,
        name: &str,
        npc_id: Option<i64>,
        boss_npc_ids: &[i64],
    ) -> bool {
         let is_npc = !is_player;

        match self {
            // Player filters
            EntityFilter::LocalPlayer => is_player && is_local_player,
            EntityFilter::OtherPlayers => is_player && !is_local_player,
            EntityFilter::AnyPlayer => is_player,
            EntityFilter::GroupMembers => is_player,
            EntityFilter::GroupMembersExceptLocal => is_player && !is_local_player,

            // Companion filters - not applicable in challenge context
            EntityFilter::AnyCompanion | EntityFilter::AnyPlayerOrCompanion => false,

            // NPC filters - use NPC class IDs for boss matching
            EntityFilter::AnyNpc => is_npc,
            EntityFilter::Boss => is_npc && npc_id.is_some_and(|id| boss_npc_ids.contains(&id)),
            EntityFilter::NpcExceptBoss => {
                is_npc && npc_id.is_none_or(|id| !boss_npc_ids.contains(&id))
            }

            // Unified selector - matches by ID or name
            EntityFilter::Selector(selectors) => {
                selectors.iter().any(|sel| match sel {
                    EntitySelector::Id(id) => is_npc && npc_id == Some(*id),
                    EntitySelector::Name(n) => name.eq_ignore_ascii_case(n),
                })
            }

            // Any entity
            EntityFilter::Any => true,
        }
    }
}
