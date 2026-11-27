//! Source/target/discipline filter checks and raid-frame registration rules.

use super::*;

impl EffectTracker {
    /// Whether an effect target may be queued for raid frame registration.
    ///
    /// The game only lands beneficial effects on teammates, so a cast from the
    /// local player — or, in PvP, from any player classified friendly — proves
    /// the target is a teammate. In PvP a cast where neither endpoint is known
    /// friendly must not register: an enemy healer's Kolto Shell would pull
    /// enemies onto the raid frames.
    pub(super) fn can_register_target(
        local_player_id: Option<i64>,
        source_id: i64,
        target_id: i64,
        encounter: Option<&CombatEncounter>,
    ) -> bool {
        if local_player_id == Some(source_id) {
            return true;
        }
        let Some(enc) = encounter.filter(|e| e.is_pvp()) else {
            return true;
        };
        let local = local_player_id.unwrap_or(0);
        enc.pvp_factions.faction_of(source_id, local) == Some(PvpFaction::Friendly)
            || enc.pvp_factions.faction_of(target_id, local) == Some(PvpFaction::Friendly)
    }

    /// Check a source/target discipline filter against an entity.
    /// Empty filter = no constraint. Non-empty: the entity must be a player
    /// with a known discipline matching one of the entries — NPCs, companions,
    /// and players whose discipline hasn't been seen yet never match.
    pub(super) fn matches_entity_disciplines(
        &self,
        filters: &[DisciplineFilter],
        entity_id: i64,
        entity_type: EntityType,
    ) -> bool {
        if filters.is_empty() {
            return true;
        }
        entity_type == EntityType::Player
            && self
                .player_disciplines
                .get(&entity_id)
                .is_some_and(|d| filters.iter().any(|f| f.matches(*d)))
    }

    /// Local player's current target (TargetChanged cache)
    pub(super) fn local_target_id(&self) -> Option<i64> {
        self.local_player_id
            .and_then(|id| self.current_targets.get(&id).map(|(tid, _, _)| *tid))
    }

    pub(super) fn filter_ctx<'a>(&self, encounter: Option<&'a CombatEncounter>) -> FilterCtx<'a> {
        FilterCtx::new(encounter, self.local_player_id, self.local_target_id())
    }

    /// Source filter and source discipline filter
    pub(super) fn source_ok(&self, ctx: &FilterCtx, def: &EffectDefinition, e: EntityInfo) -> bool {
        ctx.matches(def.source_filter(), e)
            && self.matches_entity_disciplines(&def.source_disciplines, e.id, e.entity_type)
    }

    /// Target filter and target discipline filter
    pub(super) fn target_ok(&self, ctx: &FilterCtx, def: &EffectDefinition, e: EntityInfo) -> bool {
        ctx.matches(def.target_filter(), e)
            && self.matches_entity_disciplines(&def.target_disciplines, e.id, e.entity_type)
    }

    /// Local discipline scope plus source and target filters
    pub(super) fn matches_filters(
        &self,
        ctx: &FilterCtx,
        def: &EffectDefinition,
        source: EntityInfo,
        target: EntityInfo,
    ) -> bool {
        def.matches_discipline(self.local_player_discipline.as_ref())
            && self.source_ok(ctx, def, source)
            && self.target_ok(ctx, def, target)
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Modifier Evaluation
    // ═══════════════════════════════════════════════════════════════════════════
}
