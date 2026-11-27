//! Refreshing (and late-creating) effects from `refresh_abilities` casts, heals and hits.

use super::*;

impl EffectTracker {
    /// Refresh any tracked effects that have this action in their refresh_abilities.
    /// For raid frame effects, also creates the effect if it doesn't exist yet
    /// (handles late registration when initial application was missed).
    ///
    /// The `trigger_type` parameter specifies what kind of event triggered this refresh:
    /// - `Activation`: AbilityActivated signal (instant refresh)
    /// - `Heal`: HealingDone signal (refresh after heal lands, for cast-time abilities)
    pub(super) fn refresh_effects_by_action(
        &mut self,
        action_id: i64,
        action_name: IStr,
        source: EntityInfo,
        target: EntityInfo,
        timestamp: NaiveDateTime,
        encounter: Option<&crate::encounter::CombatEncounter>,
        trigger_type: RefreshTrigger,
        // For the Damage trigger: whether this damage event was immune or resisted.
        // Refresh abilities with `ignore_immune_resist` skip these events.
        is_immune_or_resist: bool,
    ) {
        let EntityInfo {
            id: source_id,
            name: source_name,
            ..
        } = source;
        let EntityInfo {
            id: target_id,
            entity_type: target_entity_type,
            name: target_name,
            ..
        } = target;
        // For AoE abilities (target_id == 0), we can't reliably detect which targets
        // were actually hit. Damage events from ongoing DOTs on other targets look
        // identical to first ticks from the new cast. Rather than risk false refreshes
        // on targets that weren't in the AoE, we skip refresh detection entirely.
        // New applications are still tracked via ApplyEffect signals.
        if target_id == 0 {
            return;
        }

        // Use the entity type from the combat log signal rather than the encounter
        // roster, which may be incomplete (players who haven't generated combat events
        // yet won't appear in encounter.players)
        let is_player = target_entity_type == EntityType::Player;

        // Single-target case: refresh effect on specific target
        let action_name_str = crate::context::resolve(action_name);
        let candidates = self
            .definitions
            .find_refreshable_by(action_id as u64, Some(action_name_str));
        if candidates.is_empty() {
            return;
        }

        let local_player_id = self.local_player_id;
        let current_target_id = self.local_target_id();
        let ctx = self.filter_ctx(encounter);

        // In PvP, only classified teammates may claim raid-frame slots.
        let can_register = is_player && Self::can_register_target(local_player_id, source_id, target_id, encounter);

        // Collect matching definitions with all info needed for creation
        struct RefreshableEffect<'a> {
            def_id: DefId,
            def: &'a EffectDefinition,
            duration: Option<Duration>,
            /// Minimum stacks required for this refresh (None = any)
            min_stacks: Option<u8>,
            /// Resolved per-ability flag: defer the refresh to the first damage
            /// event from this ability instead of firing on cast.
            defer_to_first_damage: bool,
            /// Target filter only matched the caster: refresh the caster's
            /// instance, never touch or create one on the cast target.
            self_only: bool,
        }

        let local_discipline = self.local_player_discipline;

        // Ids of single-instance defs refreshable by this ability (both the
        // local and `_others` variants) — the source-agnostic lookup group used
        // below. Collected before filtering: source filters keep only one.
        let single_instance_ids: Vec<DefId> = candidates
            .iter()
            .filter(|(_, d)| d.single_instance_per_target)
            .map(|(id, _)| *id)
            .collect();

        let refreshable_defs: Vec<_> = candidates
            .into_iter()
            .filter(|(_, def)| def.matches_discipline(local_discipline.as_ref()))
            .filter_map(|(def_id, def)| {
                // Only refresh/create effects whose filters match the actual entities
                if !self.source_ok(&ctx, def, source) {
                    return None;
                }
                // Which instance the cast refreshes:
                // - Explicit self-buff (target filter `local_player`): it lives on
                //   the caster, whoever the ability hit (enemy or ally).
                // - Otherwise the hit target's own instance, if it passes the filter.
                // - A friendly ability cast with an NPC targeted lands on the caster
                //   (tab-target fallback): retry the filter against the caster.
                let self_only = if def.target_filter().is_local_player() {
                    if !self.target_ok(&ctx, def, source) {
                        return None;
                    }
                    target_id != source_id
                } else if self.target_ok(&ctx, def, target) {
                    false
                } else if target_id != source_id
                    && target_entity_type != EntityType::Player
                    && self.target_ok(&ctx, def, source)
                {
                    true
                } else {
                    return None;
                };
                Some((def_id, def, self_only))
            })
            .filter_map(|(def_id, def, self_only)| {
                // Find the matching RefreshAbility entry to get conditions
                let refresh_ability = def.find_refresh_ability(action_id as u64, Some(action_name_str))?;

                // Check if trigger type matches
                if refresh_ability.trigger() != trigger_type {
                    return None;
                }

                // Damage trigger: optionally ignore immune/resist hits
                if is_immune_or_resist && refresh_ability.ignore_immune_resist() {
                    return None;
                }

                Some(RefreshableEffect {
                    def_id,
                    def,
                    duration: self.effective_duration(def, timestamp),
                    min_stacks: refresh_ability.min_stacks(),
                    defer_to_first_damage: refresh_ability
                        .refresh_on_first_damage(def.display_targets.contains(&DisplayTarget::DotTracker)),
                    self_only,
                })
            })
            .collect();

        for r in refreshable_defs {
            // A self-only match keys on the caster; the fallback key below then
            // collapses to the primary key and late creation is skipped.
            let target_id = if r.self_only { source_id } else { target_id };

            // First-damage refreshes (non-AoE) defer to the next DamageTaken event.
            // Instead of refreshing immediately on AbilityActivated, set pending state
            // that will be consumed when damage from this ability lands on a target.
            // Whether to defer is resolved per refresh ability (`on_first_damage`),
            // defaulting to true for DotTracker effects and false otherwise.
            // AoE refresh abilities are handled separately by the existing AoE damage
            // correlation path (setup_pending_aoe_refresh / handle_damage_for_aoe_refresh).
            if trigger_type == RefreshTrigger::Activation && r.defer_to_first_damage && !r.self_only {
                self.pending_dot_refresh = Some(PendingDotRefresh {
                    ability_id: action_id,
                    source_id,
                    timestamp,
                    // The game logs a TargetSet on self alongside a fresh application,
                    // so a self snapshot carries no information — fall back to first damage.
                    target_id: current_target_id.filter(|&t| t != 0 && Some(t) != local_player_id),
                });
                continue;
            }

            // Single-instance buffs (Kolto Shell, Trauma Probe): the recast may be
            // silently refreshing any caster's instance, so look up source-agnostically
            // across the def group. When nothing is tracked, the cast may have refreshed
            // an instance whose apply predates the log (healer zoned in earlier) — don't
            // fabricate an instance with wrong credit; just make sure the target is on
            // the raid frames. The true instance is created by the next
            // ApplyEffect (fresh cast) or ModifyCharges (untracked instance).
            if r.def.single_instance_per_target {
                let existing =
                    Self::find_single_instance_effect(&mut self.active_effects, &single_instance_ids, target_id);
                if let Some(effect) = existing.filter(|e| !e.is_removed()) {
                    if let Some(min_stacks) = r.min_stacks
                        && effect.stacks < min_stacks
                    {
                        continue;
                    }
                    // Original caster keeps credit — only the duration refreshes.
                    effect.refresh(timestamp, r.duration);
                }
                if r.def.display_targets.contains(&DisplayTarget::RaidFrames) && can_register {
                    self.new_targets.push(NewTargetInfo {
                        entity_id: target_id,
                        name: target_name,
                    });
                }
                continue;
            }

            let key = EffectKey::for_scope(r.def_id, r.def.refresh_scope, source_id, target_id);
            // Fallback: if the resolved target is an NPC, also try source_id as target.
            // Handles self-cast abilities (e.g. Dark Ward) where target resolution
            // resolves to the combat target but the effect is keyed to the caster.
            // Only for NPC targets — player targets are intentional (e.g. heals).
            let fallback_key = if target_id != source_id && target_entity_type != EntityType::Player {
                Some(EffectKey::for_scope(
                    r.def_id,
                    r.def.refresh_scope,
                    source_id,
                    source_id,
                ))
            } else {
                None
            };

            let matched_key = if self.active_effects.contains_key(&key) {
                Some(key)
            } else {
                fallback_key.filter(|k| self.active_effects.contains_key(k))
            };

            if let Some(effect) = matched_key.and_then(|k| self.active_effects.get_mut(&k)) {
                // Don't resurrect effects that have been authoritatively removed.
                // EffectRemoved is the source of truth — if the game says the effect
                // is gone, refresh abilities cannot bring it back.
                // (timer_expired effects CAN still be refreshed — that's the grace period
                // for when our duration estimate expires before the in-game effect.)
                if effect.is_removed() {
                    continue;
                }

                // Check min_stacks condition if specified
                if let Some(min_stacks) = r.min_stacks
                    && effect.stacks < min_stacks
                {
                    continue; // Skip refresh - not enough stacks
                }

                // Existing effect - refresh duration
                effect.refresh(timestamp, r.duration);

                // Re-register for raid frames (in case user cleared the slot)
                if r.def.display_targets.contains(&DisplayTarget::RaidFrames) && can_register {
                    self.new_targets.push(NewTargetInfo {
                        entity_id: target_id,
                        name: target_name,
                    });
                }
            } else if r.def.display_targets.contains(&DisplayTarget::RaidFrames) && !r.self_only {
                // Don't late-register if min_stacks is required — no existing effect
                // means 0 stacks, which can't satisfy the minimum. Only unconditional
                // refresh abilities (Simple variant, no min_stacks) should late-register.
                if r.min_stacks.is_some() {
                    continue;
                }
                // Raid frame effect doesn't exist - create it (late registration)
                let mut effect = ActiveEffect::from_definition(
                    r.def,
                    action_id as u64,
                    (source_id, target_id),
                    LogNames {
                        source: source_name,
                        target: target_name,
                        ability: action_name,
                        effect: action_name,
                    },
                    timestamp,
                    r.duration,
                );

                if let Some(charges) = r.def.default_charges {
                    effect.set_stacks(charges);
                }

                self.active_effects.insert(key, effect);

                // Queue target for raid frame registration (only players)
                if r.def.display_targets.contains(&DisplayTarget::RaidFrames) && can_register {
                    self.new_targets.push(NewTargetInfo {
                        entity_id: target_id,
                        name: target_name,
                    });
                }
            }
        }
    }
}
