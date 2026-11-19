//! Entity filter for effect and timer source/target matching

use crate::EntitySelector;
use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// Entity Filter
// ─────────────────────────────────────────────────────────────────────────────

/// Filter for matching entities (used for both source and target filtering).
///
/// Shared between core (for timer/effect matching) and frontend (for UI editing).
/// The actual matching logic lives in core since it requires runtime types.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityFilter {
    /// The local player only
    LocalPlayer,
    /// Other players (not local player)
    #[serde(alias = "group_members_except_local")]
    OtherPlayers,
    /// Any player (including local)
    #[serde(alias = "group_members")]
    AnyPlayer,
    /// Any friendly player (PvP: local player's team; PvE: same as AnyPlayer)
    AnyFriendlyPlayer,
    /// Any enemy player (PvP: opposing team; PvE: never matches)
    AnyEnemyPlayer,
    /// Any companion (any player's)
    AnyCompanion,
    /// Any player or companion
    AnyPlayerOrCompanion,
    /// Any entity except local player (players, companions, NPCs)
    AnyExceptLocal,
    /// The local player's current target
    CurrentTarget,
    /// Boss NPCs specifically
    Boss,
    /// Non-boss NPCs (trash mobs / adds)
    NpcExceptBoss,
    /// Any NPC (boss or trash)
    AnyNpc,
    /// Specific entities by selector (IDs, names, or roster aliases)
    Selector(Vec<EntitySelector>),
    /// Any entity whatsoever
    #[default]
    Any,
}

impl EntityFilter {
    /// Get a user-friendly label for this filter
    pub fn label(&self) -> &'static str {
        match self {
            Self::LocalPlayer => "Local Player",
            Self::OtherPlayers => "Other Players",
            Self::AnyPlayer => "Any Player",
            Self::AnyFriendlyPlayer => "Any Friendly Player (PvP)",
            Self::AnyEnemyPlayer => "Any Enemy Player (PvP)",
            Self::AnyCompanion => "Any Companion",
            Self::AnyPlayerOrCompanion => "Any Player or Companion",
            Self::AnyExceptLocal => "Any Except Local",
            Self::CurrentTarget => "Current Target",
            Self::Boss => "Boss",
            Self::NpcExceptBoss => "Adds (Non-Boss)",
            Self::AnyNpc => "Any NPC",
            Self::Selector(_) => "Specific Selector",
            Self::Any => "Any",
        }
    }

    /// Default for trigger source/target (any entity)
    pub fn default_any() -> Self {
        Self::Any
    }

    /// Returns true if this filter matches anything (no restriction)
    pub fn is_any(&self) -> bool {
        matches!(self, Self::Any)
    }

    /// Returns true if this is the LocalPlayer filter
    pub fn is_local_player(&self) -> bool {
        matches!(self, Self::LocalPlayer)
    }

    /// Returns true if this is the Boss filter
    pub fn is_boss(&self) -> bool {
        matches!(self, Self::Boss)
    }

    /// Check if this filter matches a specific NPC by class ID
    pub fn matches_npc_id(&self, npc_id: i64) -> bool {
        match self {
            Self::Selector(selectors) => selectors
                .iter()
                .any(|s| matches!(s, EntitySelector::Id(id) if *id == npc_id)),
            Self::AnyNpc | Self::Boss | Self::NpcExceptBoss | Self::Any => true,
            _ => false,
        }
    }

    /// Check if this filter matches by name (case insensitive)
    pub fn matches_name(&self, name: &str) -> bool {
        match self {
            Self::Selector(selectors) => selectors
                .iter()
                .any(|s| matches!(s, EntitySelector::Name(n) if n.eq_ignore_ascii_case(name))),
            Self::Any => true,
            _ => false,
        }
    }

    /// Common options for source/target dropdowns (challenges)
    pub fn common_options() -> &'static [EntityFilter] {
        &[
            Self::Boss,
            Self::NpcExceptBoss,
            Self::AnyNpc,
            Self::AnyPlayer,
            Self::LocalPlayer,
            Self::Any,
        ]
    }

    /// Get the snake_case type name for serialization
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::LocalPlayer => "local_player",
            Self::OtherPlayers => "other_players",
            Self::AnyPlayer => "any_player",
            Self::AnyFriendlyPlayer => "any_friendly_player",
            Self::AnyEnemyPlayer => "any_enemy_player",
            Self::AnyCompanion => "any_companion",
            Self::AnyPlayerOrCompanion => "any_player_or_companion",
            Self::AnyExceptLocal => "any_except_local",
            Self::CurrentTarget => "current_target",
            Self::Boss => "boss",
            Self::NpcExceptBoss => "npc_except_boss",
            Self::AnyNpc => "any_npc",
            Self::Selector(_) => "selector",
            Self::Any => "any",
        }
    }

    /// All filters for source field (timers/effects/triggers)
    pub fn source_options() -> &'static [EntityFilter] {
        &[
            Self::Any,
            Self::LocalPlayer,
            Self::OtherPlayers,
            Self::AnyPlayer,
            Self::AnyFriendlyPlayer,
            Self::AnyEnemyPlayer,
            Self::AnyCompanion,
            Self::AnyPlayerOrCompanion,
            Self::AnyExceptLocal,
            Self::CurrentTarget,
            Self::Boss,
            Self::NpcExceptBoss,
            Self::AnyNpc,
        ]
    }

    /// All filters for target field (timers/effects/triggers)
    pub fn target_options() -> &'static [EntityFilter] {
        &[
            Self::Any,
            Self::LocalPlayer,
            Self::OtherPlayers,
            Self::AnyPlayer,
            Self::AnyFriendlyPlayer,
            Self::AnyEnemyPlayer,
            Self::AnyCompanion,
            Self::AnyPlayerOrCompanion,
            Self::AnyExceptLocal,
            Self::CurrentTarget,
            Self::Boss,
            Self::NpcExceptBoss,
            Self::AnyNpc,
        ]
    }
}
