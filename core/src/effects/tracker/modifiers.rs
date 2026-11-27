//! Duration modifiers: ModifyCharges-driven and signal-driven (`evaluate_modifiers`).

use super::*;

impl EffectTracker {
    /// Handle charges changed signal
    pub(super) fn handle_charges_changed(
        &mut self,
        effect_id: i64,
        effect_name: IStr,
        _action_id: i64,
        _action_name: IStr,
        source_id: i64,
        source_entity_type: EntityType,
        source_name: IStr,
        source_npc_id: i64,
        target_id: i64,
        target_entity_type: EntityType,
        target_name: IStr,
        target_npc_id: i64,
        timestamp: NaiveDateTime,
        charges: u8,
        encounter: Option<&crate::encounter::CombatEncounter>,
    ) {
        // Stack count changed on a local-player alacrity buff
        if self.local_player_id == Some(target_id) {
            self.alacrity_buffs.on_charges_changed(effect_id, charges, timestamp);
        }

        // Entity info for filter matching (single-instance creation below)
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

        // Find matching definitions (by ID or name)
        let effect_name_str = crate::context::resolve(effect_name);
        let matching_defs: Vec<_> = self
            .definitions
            .find_matching(effect_id as u64, Some(effect_name_str))
            .into_iter()
            .collect();

        // Source-agnostic lookup group for single-instance buffs (see
        // handle_effect_applied) — a merged instance keeps the original
        // caster's key, which may not match this signal's source.
        let single_instance_ids: Vec<DefId> = matching_defs
            .iter()
            .filter(|(_, d)| d.single_instance_per_target)
            .map(|(id, _)| *id)
            .collect();

        for (def_id, def) in matching_defs {
            let key = EffectKey::for_scope(def_id, def.refresh_scope, source_id, target_id);

            // Calculate duration before borrowing active_effects mutably.
            // Needed for the blanket is_refreshed_on_modify refresh AND for
            // refill_duration modifiers (which refill without is_refreshed_on_modify).
            let needs_duration = def.is_refreshed_on_modify || def.modifiers.iter().any(|m| m.refill_duration);
            let duration = if needs_duration {
                self.effective_duration(def, timestamp)
            } else {
                None
            };
            let icds: Vec<Option<f32>> = def.modifiers.iter().map(|m| self.effective_icd(m, timestamp)).collect();

            if let Some(effect) = if self.active_effects.contains_key(&key) {
                self.active_effects.get_mut(&key)
            } else if def.single_instance_per_target {
                Self::find_single_instance_effect(&mut self.active_effects, &single_instance_ids, target_id)
            } else {
                None
            } {
                let old_stacks = effect.stacks;
                effect.set_stacks(charges);

                // Refresh duration on ModifyCharges if is_refreshed_on_modify is set.
                if def.is_refreshed_on_modify
                    && let Some(dur) = duration
                {
                    effect.refresh_duration(timestamp, dur);
                }

                // Evaluate SelfChargesChanged modifiers
                if !def.modifiers.is_empty() {
                    let modifier_count = def.modifiers.len();
                    effect.ensure_modifier_icd(modifier_count);

                    for (mod_idx, modifier) in def.modifiers.iter().enumerate() {
                        if !crate::effects::modifier_match::matches_self_charges(&modifier.trigger, old_stacks, charges)
                        {
                            continue;
                        }
                        let adj = ModifierAdjustment::new(modifier, mod_idx, timestamp, 1.0, icds[mod_idx]);
                        if !effect.modifier_icd_ready(mod_idx, adj.icd_secs, adj.icd_from_application, timestamp) {
                            continue;
                        }
                        effect.modifier_last_proc[mod_idx] = Some(timestamp);
                        effect.apply_modifier(&adj, duration, true);
                        if adj.cancel {
                            break;
                        }
                    }
                }
            } else if def.single_instance_per_target
                && def.is_effect_applied_trigger()
                && !def.is_alert
                && self.matches_filters(&ctx, def, source_info, target_info)
            {
                // Nothing tracked on this target: the original apply predates the
                // log (e.g. the local player zoned in after another healer applied).
                // ModifyCharges carries the true caster as source, so create the
                // instance here with correct credit. Real remaining duration is
                // unknown — start a full one and let refreshes/RemoveEffect
                // correct it. matches_filters picks the local vs `_others` def,
                // so the pair creates at most one instance.
                let duration = self.effective_duration(def, timestamp);
                let mut effect = ActiveEffect::from_definition(
                    def,
                    effect_id as u64,
                    (source_id, target_id),
                    LogNames {
                        source: source_name,
                        target: target_name,
                        ability: effect_name,
                        effect: effect_name,
                    },
                    timestamp,
                    duration,
                );
                effect.set_stacks(charges);
                self.active_effects.insert(key, effect);
            }
        }
    }

    pub(super) fn evaluate_modifiers(&mut self, signals: &[GameSignal]) {
        if !self.definitions.has_modifiers() {
            return;
        }
        // Seed last_seen_charges from EffectApplied so first ChargesChanged has a baseline
        for s in signals {
            if let GameSignal::EffectApplied {
                effect_id,
                target_id,
                charges,
                ..
            } = s
                && let Some(c) = charges
                && *c > 0
            {
                self.last_seen_charges.entry((*effect_id, *target_id)).or_insert(*c);
            }
        }

        // Snapshot old charges for direction comparison, then update to new values
        let mut charge_snapshots: Vec<((i64, i64), u8, u8)> = Vec::new();
        for s in signals {
            if let GameSignal::EffectChargesChanged {
                effect_id,
                target_id,
                charges,
                ..
            } = s
            {
                let key = (*effect_id, *target_id);
                let old = self.last_seen_charges.get(&key).copied().unwrap_or(*charges);
                charge_snapshots.push((key, old, *charges));
                self.last_seen_charges.insert(key, *charges);
            }
        }

        // Collect modifications to apply (avoids borrow conflict on self)
        let mut adjustments: Vec<(EffectKey, ModifierAdjustment)> = Vec::new();

        for (key, effect) in &self.active_effects {
            // timer_expired is an interpolated estimate — a log event can still
            // prove the effect alive (checked per-hit below against log time).
            if effect.is_removed() {
                continue;
            }
            let def = self.definitions.def(key.def);
            if def.modifiers.is_empty() {
                continue;
            }

            for (mod_idx, modifier) in def.modifiers.iter().enumerate() {
                // One hit per matching signal, stamped with that signal's log time
                let mut hits: Vec<crate::effects::modifier_match::Hit> = Vec::new();
                crate::effects::modifier_match::collect_hits(
                    &modifier.trigger,
                    modifier.requires_crit,
                    effect.target_entity_id,
                    signals,
                    &charge_snapshots,
                    &mut hits,
                );

                for hit in hits {
                    let icd = self.effective_icd(modifier, hit.at);
                    adjustments.push((*key, ModifierAdjustment::new(modifier, mod_idx, hit.at, hit.scale, icd)));
                }
            }
        }

        // Apply collected adjustments
        for (key, adj) in adjustments {
            // Pre-compute effective duration for refill before mutable borrow
            let refill_dur = if adj.refill_duration {
                self.effective_duration(self.definitions.def(key.def), adj.at)
            } else {
                None
            };

            let Some(effect) = self.active_effects.get_mut(&key) else {
                continue;
            };
            // Removed by an earlier hit in this batch (cancel), or genuinely
            // expired in log time before this hit — nothing to modify.
            if effect.is_removed() || (effect.is_timer_expired() && effect.expires_at.is_some_and(|e| e < adj.at)) {
                continue;
            }
            let modifier_count = self.definitions.def(key.def).modifiers.len();
            effect.ensure_modifier_icd(modifier_count);

            if !effect.modifier_icd_ready(adj.mod_idx, adj.icd_secs, adj.icd_from_application, adj.at) {
                continue;
            }

            effect.modifier_last_proc[adj.mod_idx] = Some(adj.at);
            effect.apply_modifier(&adj, refill_dur, true);
        }
    }
}
