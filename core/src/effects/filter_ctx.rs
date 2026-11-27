//! Per-signal context for matching definition source/target filters.

use std::collections::HashSet;
use std::sync::Arc;

use crate::combat_log::EntityType;
use crate::context::IStr;
use crate::dsl::{EntityDefinition, EntityFilter, EntityFilterMatching};
use crate::encounter::{CombatEncounter, PvpFactionMap};

/// An entity from a signal, as filters see it
#[derive(Debug, Clone, Copy)]
pub(super) struct EntityInfo {
    pub id: i64,
    /// NPC class/template ID (0 for players/companions)
    pub npc_id: i64,
    pub entity_type: EntityType,
    pub name: IStr,
}

/// Encounter state every filter check needs, gathered once per signal
pub(super) struct FilterCtx<'a> {
    entities: &'a [EntityDefinition],
    boss_ids: Arc<HashSet<i64>>,
    pvp_factions: Option<&'a PvpFactionMap>,
    local_player_id: Option<i64>,
    current_target_id: Option<i64>,
}

impl<'a> FilterCtx<'a> {
    pub fn new(
        encounter: Option<&'a CombatEncounter>,
        local_player_id: Option<i64>,
        current_target_id: Option<i64>,
    ) -> Self {
        Self {
            entities: Self::entities(encounter),
            boss_ids: encounter.map(|e| e.boss_entity_ids()).unwrap_or_default(),
            pvp_factions: encounter.and_then(|e| e.pvp_faction_context()),
            local_player_id,
            current_target_id,
        }
    }

    /// Entity roster of the active boss, or empty
    fn entities(encounter: Option<&'a CombatEncounter>) -> &'a [EntityDefinition] {
        encounter
            .and_then(|enc| {
                // get(): the index can be stale after boss definitions reload
                enc.boss_definitions().get(enc.active_boss_idx()?)
            })
            .map_or(&[], |def| def.entities.as_slice())
    }

    pub fn matches(&self, filter: &EntityFilter, e: EntityInfo) -> bool {
        filter.is_any()
            || filter.matches(
                self.entities,
                e.id,
                e.entity_type,
                e.name,
                e.npc_id,
                self.local_player_id,
                self.current_target_id,
                &self.boss_ids,
                self.pvp_factions,
            )
    }
}
