//! AbilityCast, DamageTaken and HealingTaken triggered effects.

use super::*;

impl EffectTracker {
    /// Handle ability cast for AbilityCast-triggered effects (procs, cooldowns)
    pub(super) fn handle_ability_cast(
        &mut self,
        ability_id: i64,
        ability_name: IStr,
        source_id: i64,
        source_name: IStr,
        source_entity_type: EntityType,
        source_npc_id: i64,
        target_id: i64,
        target_name: IStr,
        target_entity_type: EntityType,
        target_npc_id: i64,
        timestamp: NaiveDateTime,
        encounter: Option<&crate::encounter::CombatEncounter>,
    ) {
        let ability_name_str = crate::context::resolve(ability_name);

        // Find definitions with AbilityCast triggers that match this ability
        let matching_defs: Vec<_> = self
            .definitions
            .find_ability_cast_matching(ability_id as u64, Some(ability_name_str));

        if matching_defs.is_empty() {
            return;
        }

        // Build entity info for source filter matching
        let source_info = EntityInfo {
            id: source_id,
            npc_id: source_npc_id,
            entity_type: source_entity_type,
            name: source_name,
        };

        let ctx = self.filter_ctx(encounter);
        let target_info = EntityInfo {
            id: target_id,
            npc_id: target_npc_id,
            entity_type: target_entity_type,
            name: target_name,
        };
        for (def_id, def) in matching_defs {
            // Only process AbilityCast triggers here (index also contains DamageTaken/HealingTaken)
            if !def.is_ability_cast_trigger() {
                continue;
            }

            // Check discipline filter
            if !def.matches_discipline(self.local_player_discipline.as_ref()) {
                continue;
            }

            if !self.source_ok(&ctx, def, source_info) {
                continue;
            }

            // Instant alerts: fire and skip — no ActiveEffect created
            if def.is_alert {
                self.fired_alerts
                    .push(alerts::instant(def, timestamp, source_name, target_name, 1));
                continue;
            }

            // Validate resolved target against the definition's target filter
            if !self.target_ok(&ctx, def, target_info) {
                continue;
            }

            // AbilityCast trigger matched — track the effect on the caster (source)
            let key = EffectKey::for_scope(def_id, def.refresh_scope, source_id, source_id);

            let duration = self.effective_duration(def, timestamp);

            if let Some(existing) = self.active_effects.get_mut(&key) {
                // Ignore refreshes: skip retrigger only while still in base duration.
                // Once the cooldown enters the ready state it has effectively expired,
                // so a new trigger is a fresh activation, not a refresh.
                if def.ignore_refreshes && existing.is_in_base_duration(timestamp) {
                    continue;
                }

                existing.refresh(timestamp, duration);

                // Recast on a different target: update the displayed target
                if existing.target_name != target_name {
                    existing.target_name = target_name;
                    existing.set_display_template(def.display_text());
                }

                // Fire OnApply alert on refresh
                self.fired_alerts
                    .extend(alerts::on_apply(def, timestamp, source_name, target_name, 1));
            } else {
                // Tracked on the caster (keying, death cleanup); only the
                // displayed name is the target resolved at cast time
                let effect = ActiveEffect::from_definition(
                    def,
                    ability_id as u64,
                    (source_id, source_name),
                    (source_id, target_name),
                    timestamp,
                    duration,
                );
                self.active_effects.insert(key, effect);

                // Fire OnApply alert for new effect
                self.fired_alerts
                    .extend(alerts::on_apply(def, timestamp, source_name, target_name, 1));
            }
        }
    }

    /// Handle damage/healing taken trigger - creates a simple timed effect.
    /// No refresh or charge logic, just starts (or restarts) the timer on each event.
    pub(super) fn handle_ability_event_trigger(
        &mut self,
        ability_id: i64,
        ability_name: IStr,
        source_id: i64,
        source_name: IStr,
        source_entity_type: EntityType,
        source_npc_id: i64,
        target_id: i64,
        target_name: IStr,
        target_entity_type: EntityType,
        target_npc_id: i64,
        timestamp: NaiveDateTime,
        encounter: Option<&crate::encounter::CombatEncounter>,
        trigger_check: fn(&EffectDefinition) -> bool,
    ) {
        let ability_name_str = crate::context::resolve(ability_name);

        let matching_defs: Vec<_> = self
            .definitions
            .find_ability_cast_matching(ability_id as u64, Some(ability_name_str));

        if matching_defs.is_empty() {
            return;
        }

        let source_info = EntityInfo {
            id: source_id,
            npc_id: source_npc_id,
            entity_type: source_entity_type,
            name: source_name,
        };
        let target_info = EntityInfo {
            id: target_id,
            npc_id: target_npc_id,
            entity_type: target_entity_type,
            name: target_name,
        };
        let ctx = self.filter_ctx(encounter);

        for (def_id, def) in matching_defs {
            if !trigger_check(def) {
                continue;
            }
            if !self.matches_filters(&ctx, def, source_info, target_info) {
                continue;
            }

            // Instant alerts: fire and skip — no ActiveEffect created
            if def.is_alert {
                self.fired_alerts
                    .push(alerts::instant(def, timestamp, source_name, target_name, 1));
                continue;
            }

            let key = EffectKey::for_scope(def_id, def.refresh_scope, source_id, target_id);
            let duration = self.effective_duration(def, timestamp);

            if let Some(existing) = self.active_effects.get_mut(&key) {
                // Ignore refreshes: skip retrigger only while still in base duration.
                // Once the cooldown enters the ready state it has effectively expired,
                // so a new trigger is a fresh activation, not a refresh.
                if def.ignore_refreshes && existing.is_in_base_duration(timestamp) {
                    continue;
                }

                existing.refresh(timestamp, duration);

                // Fire OnApply alert on refresh
                self.fired_alerts
                    .extend(alerts::on_apply(def, timestamp, source_name, target_name, 1));
            } else {
                let effect = ActiveEffect::from_definition(
                    def,
                    ability_id as u64,
                    (source_id, source_name),
                    (target_id, target_name),
                    timestamp,
                    duration,
                );
                self.active_effects.insert(key, effect);

                // Fire OnApply alert for new effect
                self.fired_alerts
                    .extend(alerts::on_apply(def, timestamp, source_name, target_name, 1));
            }
        }
    }
}
