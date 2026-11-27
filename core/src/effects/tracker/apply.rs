//! Effect applied/removed signals and housekeeping wipes (death, combat end, zone change).

use super::*;

impl EffectTracker {
    /// Handle effect application signal
    pub(super) fn handle_effect_applied(
        &mut self,
        effect_id: i64,
        effect_name: IStr,
        action_id: i64,
        _action_name: IStr,
        source_id: i64,
        source_name: IStr,
        source_entity_type: EntityType,
        source_npc_id: i64,
        target_id: i64,
        target_name: IStr,
        target_entity_type: EntityType,
        target_npc_id: i64,
        timestamp: NaiveDateTime,
        charges: Option<u8>,
        encounter: Option<&crate::encounter::CombatEncounter>,
    ) {
        // Note: GC is handled by tick() - don't duplicate here to reduce work per signal

        let local_player_id = self.local_player_id;

        // Track alacrity buffs landing on the local player (affects future durations)
        if local_player_id == Some(target_id) {
            self.alacrity_buffs.on_applied(effect_id, charges, timestamp);
        }

        // Build entity info for filter matching
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

        // Resolve effect name for matching
        let effect_name_str = crate::context::resolve(effect_name);

        // Find matching definitions (only those that trigger on EffectApplied)
        let all_matches = self.definitions.find_matching(effect_id as u64, Some(effect_name_str));

        // Ids of single-instance defs sharing this game effect (e.g. kolto_shell
        // + kolto_shell_others) — the source-agnostic lookup group used below.
        // Collected before filtering: source filters keep only one of the pair.
        let single_instance_ids: Vec<DefId> = all_matches
            .iter()
            .filter(|(_, d)| d.single_instance_per_target)
            .map(|(id, _)| *id)
            .collect();

        let matching_defs: Vec<_> = all_matches
            .into_iter()
            .filter(|(_, def)| def.is_effect_applied_trigger())
            .filter(|(_, def)| self.matches_filters(&ctx, def, source_info, target_info))
            .collect();

        let is_from_local = local_player_id == Some(source_id);
        let mut should_register = false;
        let mut pending_alerts: Vec<FiredAlert> = Vec::new();

        for (def_id, def) in matching_defs {
            // Instant alerts: fire and skip — no ActiveEffect created
            if def.is_alert {
                pending_alerts.push(alerts::instant(
                    def,
                    timestamp,
                    source_name,
                    target_name,
                    charges.unwrap_or(1),
                ));
                continue;
            }

            let key = EffectKey::for_scope(def_id, def.refresh_scope, source_id, target_id);

            let duration = self.effective_duration(def, timestamp);

            // Single-instance buffs (Kolto Shell, Trauma Probe): the game keeps one
            // instance per target across all sources — a second caster's apply
            // refreshes the existing instance and the original caster keeps credit.
            // Source-agnostic lookup across the def group, in both directions
            // (local over other's AND other's over local).
            if def.single_instance_per_target
                && let Some(existing) =
                    Self::find_single_instance_effect(&mut self.active_effects, &single_instance_ids, target_id)
                && !existing.is_removed()
            {
                existing.refresh(timestamp, duration);
                if let Some(c) = charges {
                    existing.set_stacks(c);
                }
                // Register the target even when the cast came from another
                // player — in PvE anyone receiving this buff is a group member;
                // in PvP only once classified as a teammate.
                if def.display_targets.contains(&DisplayTarget::RaidFrames)
                    && target_entity_type == EntityType::Player
                    && Self::can_register_target(local_player_id, source_id, target_id, encounter)
                {
                    self.new_targets.push(NewTargetInfo {
                        entity_id: target_id,
                        name: target_name,
                    });
                }
                continue;
            }

            if let Some(existing) = self.active_effects.get_mut(&key) {
                if def.display_targets.contains(&DisplayTarget::DotTracker) {
                    if existing.is_removed() {
                        // EffectRemoved arrived before EffectApplied (reverse ordering).
                        // Revive the effect — EffectApplied is authoritative for new applications.
                        existing.refresh(timestamp, duration);
                        Self::clear_pending_dot_refresh(&mut self.pending_dot_refresh, action_id, target_id);
                    } else {
                        // Don't refresh the duration here — wait for a DamageTaken
                        // event to confirm a real hit (handle_damage_for_dot_refresh).
                        // Touch last_refreshed_at so the 1-second stale-removal guard
                        // covers the incoming EffectRemoved for the old DOT instance.
                        existing.last_refreshed_at = timestamp;
                        if let Some(c) = charges {
                            existing.set_stacks(c);
                        }
                    }
                    continue;
                }

                // Skip duplicate log lines (same timestamp) to avoid corrupting timing
                if existing.last_refreshed_at == timestamp {
                    if let Some(c) = charges {
                        existing.set_stacks(c);
                    }
                    continue;
                }

                // Ignore refreshes: skip retrigger only while still in base duration.
                // Once the cooldown enters the ready state it has effectively expired,
                // so a new trigger is a fresh activation, not a refresh.
                if def.ignore_refreshes && existing.is_in_base_duration(timestamp) {
                    continue;
                }

                existing.refresh(timestamp, duration);
                if let Some(c) = charges {
                    existing.set_stacks(c);
                }
                should_register |= def.display_targets.contains(&DisplayTarget::RaidFrames);

                // Collect alert for effect refresh if configured
                pending_alerts.extend(alerts::on_apply(
                    def,
                    timestamp,
                    source_name,
                    target_name,
                    charges.unwrap_or(1),
                ));
            } else {
                // A logged application is authoritative: drop any deferred refresh for
                // this cast so a bystander's tick can't claim it afterwards.
                if def.display_targets.contains(&DisplayTarget::DotTracker) {
                    Self::clear_pending_dot_refresh(&mut self.pending_dot_refresh, action_id, target_id);
                }
                // Create new effect
                let mut effect = ActiveEffect::from_definition(
                    def,
                    effect_id as u64,
                    (source_id, source_name),
                    (target_id, target_name),
                    timestamp,
                    duration,
                );

                if let Some(c) = charges {
                    effect.set_stacks(c);
                }

                self.active_effects.insert(key, effect);
                should_register |= def.display_targets.contains(&DisplayTarget::RaidFrames);

                // Collect alert for effect start if configured
                pending_alerts.extend(alerts::on_apply(
                    def,
                    timestamp,
                    source_name,
                    target_name,
                    charges.unwrap_or(1),
                ));
            }
        }

        // Queue collected alerts
        self.fired_alerts.extend(pending_alerts);

        // Queue target for raid frame registration only when a raid-frame effect
        // was created or refreshed — a tracked cooldown/buff (e.g. a stim
        // re-applied at zone-in) must not claim a slot. Only players belong on
        // raid frames (not companions or NPCs)
        if should_register && is_from_local && target_entity_type == EntityType::Player {
            self.new_targets.push(NewTargetInfo {
                entity_id: target_id,
                name: target_name,
            });
        }
    }

    /// Handle effect removal signal
    pub(super) fn handle_effect_removed(
        &mut self,
        effect_id: i64,
        effect_name: IStr,
        source_id: i64,
        source_entity_type: EntityType,
        source_name: IStr,
        source_npc_id: i64,
        target_id: i64,
        target_entity_type: EntityType,
        target_name: IStr,
        target_npc_id: i64,
        timestamp: NaiveDateTime,
        encounter: Option<&crate::encounter::CombatEncounter>,
    ) {
        let local_player_id = self.local_player_id;

        // Alacrity buff fell off the local player
        if local_player_id == Some(target_id) {
            self.alacrity_buffs.on_removed(effect_id);
        }

        // Build entity info for filter matching
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

        // Resolve effect name for matching
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

        for &(def_id, def) in &matching_defs {
            let key = EffectKey::for_scope(def_id, def.refresh_scope, source_id, target_id);

            if def.is_effect_applied_trigger() {
                // Mark existing effect as removed (normal behavior)
                // Skip if ignore_effect_removed OR cooldowns (cooldowns always use timer-based expiry)
                let is_cooldown = def.display_targets.contains(&DisplayTarget::Cooldowns)
                    || def.display_targets.contains(&DisplayTarget::CooldownsB);
                if def.ignore_effect_removed || is_cooldown {
                    continue;
                }
                let matched = if self.active_effects.contains_key(&key) {
                    self.active_effects.get_mut(&key)
                } else if def.single_instance_per_target {
                    Self::find_single_instance_effect(&mut self.active_effects, &single_instance_ids, target_id)
                } else {
                    None
                };
                if let Some(effect) = matched {
                    // Only honor removal if it occurred well AFTER the last refresh.
                    // DOT reapplication sends ApplyEffect then RemoveEffect - sometimes
                    // the RemoveEffect arrives up to ~1 second later (for the old DOT instance).
                    // Use a 1 second window to ignore stale RemoveEffect signals.
                    let since_refresh_ms = timestamp
                        .signed_duration_since(effect.last_refreshed_at)
                        .num_milliseconds();
                    if since_refresh_ms > 1000 {
                        effect.confirm_removed();
                    }
                }
            } else if def.is_effect_removed_trigger() && self.matches_filters(&ctx, def, source_info, target_info) {
                // Instant alerts: fire and skip — no ActiveEffect created
                if def.is_alert {
                    self.fired_alerts
                        .push(alerts::instant(def, timestamp, source_name, target_name, 1));
                    continue;
                }

                // Create new effect when the game effect is removed (cooldown tracking)
                let duration = self.effective_duration(def, timestamp);
                let effect = ActiveEffect::from_definition(
                    def,
                    effect_id as u64,
                    (source_id, source_name),
                    (target_id, target_name),
                    timestamp,
                    duration,
                );
                self.active_effects.insert(key, effect);

                // Fire OnApply alert for new EffectRemoved-triggered effect
                self.fired_alerts
                    .extend(alerts::on_apply(def, timestamp, source_name, target_name, 1));
            }
        }
    }

    /// Handle entity death - clear effects unless persist_past_death
    pub(super) fn handle_entity_death(&mut self, entity_id: i64) {
        for (key, effect) in self.active_effects.iter_mut() {
            if effect.target_entity_id != entity_id {
                continue;
            }
            if !self.definitions.def(key.def).persist_past_death {
                effect.discard();
            }
        }
    }

    /// Handle combat end - optionally clear combat-only effects
    pub(super) fn handle_combat_ended(&mut self) {
        // Clear pending refresh state
        self.pending_aoe_refresh = None;
        self.aoe_collecting = None;
        self.pending_dot_refresh = None;

        // Discard effects that don't track outside combat
        for (key, effect) in self.active_effects.iter_mut() {
            if !self.definitions.def(key.def).track_outside_combat {
                effect.discard();
            }
        }
    }

    /// Handle area change (zone transition) - clear all active effects
    pub(super) fn handle_area_change(&mut self) {
        // Clear pending refresh state
        self.pending_aoe_refresh = None;
        self.aoe_collecting = None;
        self.pending_dot_refresh = None;
        self.alacrity_buffs.clear();

        for effect in self.active_effects.values_mut() {
            effect.discard();
        }
    }
}
