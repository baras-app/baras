//! `SignalHandler` dispatch: routes each `GameSignal` to its handler.

use super::*;

impl SignalHandler for EffectTracker {
    fn handle_signals(&mut self, signals: &[GameSignal], encounter: Option<&crate::encounter::CombatEncounter>) {
        for signal in signals {
            self.handle_signal(signal, encounter);
        }
        // Drop alacrity buffs that outlived their duration (missed remove events).
        // Log time, not the interpolated anchor — see `effective_duration`.
        if let Some(last) = signals.last() {
            self.alacrity_buffs.prune_expired(last.timestamp());
        }
        // Evaluate modifiers on active effects against this batch of signals
        self.evaluate_modifiers(signals);
        // Only finalize AoE collection if we're past the collection window (10ms).
        // This ensures secondary targets have time to arrive across multiple batches,
        // while still finalizing promptly once the window has elapsed.
        if let Some(ref collecting) = self.aoe_collecting
            && let Some(current_time) = self.current_game_time
        {
            let elapsed_ms = (current_time - collecting.anchor_timestamp).num_milliseconds();
            if elapsed_ms > 10 {
                self.finalize_aoe_refresh();
            }
        }
    }

    fn handle_signal(&mut self, signal: &GameSignal, encounter: Option<&crate::encounter::CombatEncounter>) {
        self.advance_game_time_anchor(signal.timestamp());
        match signal {
            GameSignal::EffectApplied {
                effect_id,
                effect_name,
                action_id,
                action_name,
                source_id,
                source_name,
                source_entity_type,
                source_npc_id,
                target_id,
                target_name,
                target_entity_type,
                target_npc_id,
                timestamp,
                charges,
            } => {
                self.handle_effect_applied(
                    *effect_id,
                    *effect_name,
                    *action_id,
                    *action_name,
                    *source_id,
                    *source_name,
                    *source_entity_type,
                    *source_npc_id,
                    *target_id,
                    *target_name,
                    *target_entity_type,
                    *target_npc_id,
                    *timestamp,
                    *charges,
                    encounter,
                );
            }
            GameSignal::EffectRemoved {
                effect_id,
                effect_name,
                source_id,
                source_entity_type,
                source_name,
                source_npc_id,
                target_id,
                target_entity_type,
                target_name,
                target_npc_id,
                timestamp,
            } => {
                self.handle_effect_removed(
                    *effect_id,
                    *effect_name,
                    *source_id,
                    *source_entity_type,
                    *source_name,
                    *source_npc_id,
                    *target_id,
                    *target_entity_type,
                    *target_name,
                    *target_npc_id,
                    *timestamp,
                    encounter,
                );
            }
            GameSignal::EffectChargesChanged {
                effect_id,
                effect_name,
                action_id,
                action_name,
                source_id,
                source_entity_type,
                source_name,
                source_npc_id,
                target_id,
                target_entity_type,
                target_name,
                target_npc_id,
                timestamp,
                charges,
            } => {
                self.handle_charges_changed(
                    *effect_id,
                    *effect_name,
                    *action_id,
                    *action_name,
                    *source_id,
                    *source_entity_type,
                    *source_name,
                    *source_npc_id,
                    *target_id,
                    *target_entity_type,
                    *target_name,
                    *target_npc_id,
                    *timestamp,
                    *charges,
                    encounter,
                );
            }
            GameSignal::EntityDeath { entity_id, .. } => {
                self.handle_entity_death(*entity_id);
            }
            GameSignal::CombatEnded { .. } => {
                self.handle_combat_ended();
            }
            GameSignal::AreaEntered { area_id, .. }
                // Instanced areas fire AreaEntered twice (plain + difficulty)
                // around the game's effect restatement — only wipe on an
                // actual area change.
                if self.last_area_id != Some(*area_id) => {
                    self.last_area_id = Some(*area_id);
                    self.handle_area_change();
                }
            GameSignal::DisciplineChanged {
                entity_id,
                discipline_id,
                ..
            } => {
                // Track per-player disciplines for source/target discipline filters
                let discipline = Discipline::from_guid(*discipline_id);
                if let Some(d) = discipline {
                    self.player_disciplines.insert(*entity_id, d);
                } else {
                    self.player_disciplines.remove(entity_id);
                }
                // Track local player's discipline for discipline-scoped effects
                if self.local_player_id == Some(*entity_id) {
                    self.local_player_discipline = discipline;
                }
            }
            GameSignal::AbilityActivated {
                ability_id,
                ability_name,
                source_id,
                source_name,
                source_entity_type,
                source_npc_id,
                target_id,
                target_name,
                target_entity_type,
                target_npc_id,
                timestamp,
            } => {
                let source = EntityInfo {
                    id: *source_id,
                    npc_id: *source_npc_id,
                    entity_type: *source_entity_type,
                    name: *source_name,
                };
                // Resolve actual target: the signal already has a resolved target from
                // encounter state, but our local current_targets cache may be more
                // up-to-date (works outside combat). Apply additional resolution if
                // the signal still reports self-targeting. A target resolved from
                // either cache has no known NPC class id.
                let is_self_or_empty = *target_id == 0 || *target_id == *source_id;
                let target = if !is_self_or_empty {
                    EntityInfo {
                        id: *target_id,
                        npc_id: *target_npc_id,
                        entity_type: *target_entity_type,
                        name: *target_name,
                    }
                } else if let Some((id, name, entity_type)) = self.current_targets.get(source_id).copied() {
                    EntityInfo {
                        id,
                        npc_id: 0,
                        entity_type,
                        name,
                    }
                } else if let Some(id) = encounter.and_then(|e| e.get_current_target(*source_id)) {
                    let player = encounter.and_then(|e| e.players.get(&id));
                    EntityInfo {
                        id,
                        npc_id: 0,
                        entity_type: if player.is_some() {
                            EntityType::Player
                        } else {
                            EntityType::Npc
                        },
                        name: player.map(|p| p.name).unwrap_or(*source_name),
                    }
                } else {
                    source
                };

                // Handle AbilityCast-triggered effects (procs, cooldowns)
                self.handle_ability_cast(
                    *ability_id,
                    *ability_name,
                    *source_id,
                    *source_name,
                    *source_entity_type,
                    *source_npc_id,
                    target.id,
                    target.name,
                    target.entity_type,
                    target.npc_id,
                    *timestamp,
                    encounter,
                );

                self.refresh_effects_by_action(
                    *ability_id,
                    *ability_name,
                    source,
                    target,
                    *timestamp,
                    encounter,
                    RefreshTrigger::Activation,
                    false,
                );

                // For AoE abilities, set up pending state for damage correlation
                self.setup_pending_aoe_refresh(*ability_id, *ability_name, *source_id, *timestamp, target.id);
            }
            GameSignal::DamageTaken {
                ability_id,
                ability_name,
                source_id,
                source_entity_type,
                source_name,
                source_npc_id,
                target_id,
                target_entity_type,
                target_name,
                target_npc_id,
                timestamp,
                defense_type_id,
                ..
            } => {
                // AoE refresh damage correlation
                self.handle_damage_for_aoe_refresh(*ability_id, *ability_name, *target_id, *timestamp);
                // Single-target DotTracker refresh damage confirmation
                self.handle_damage_for_dot_refresh(*ability_id, *ability_name, *target_id, *timestamp);
                // Refresh abilities configured with the Damage trigger
                let is_immune_or_resist = *defense_type_id
                    == crate::game_data::defense_type::IMMUNE
                    || *defense_type_id == crate::game_data::defense_type::RESIST;
                self.refresh_effects_by_action(
                    *ability_id,
                    *ability_name,
                    EntityInfo {
                        id: *source_id,
                        npc_id: *source_npc_id,
                        entity_type: *source_entity_type,
                        name: *source_name,
                    },
                    EntityInfo {
                        id: *target_id,
                        npc_id: *target_npc_id,
                        entity_type: *target_entity_type,
                        name: *target_name,
                    },
                    *timestamp,
                    encounter,
                    RefreshTrigger::Damage,
                    is_immune_or_resist,
                );
                // DamageTaken trigger matching for effects tracker
                self.handle_ability_event_trigger(
                    *ability_id,
                    *ability_name,
                    *source_id,
                    *source_name,
                    *source_entity_type,
                    *source_npc_id,
                    *target_id,
                    *target_name,
                    *target_entity_type,
                    *target_npc_id,
                    *timestamp,
                    encounter,
                    EffectDefinition::is_damage_taken_trigger,
                );
            }
            GameSignal::HealingDone {
                ability_id,
                ability_name,
                source_id,
                source_entity_type,
                source_name,
                source_npc_id,
                target_id,
                target_entity_type,
                target_name,
                target_npc_id,
                timestamp,
                ..
            } => {
                // Refresh effects on heal completion
                self.refresh_effects_by_action(
                    *ability_id,
                    *ability_name,
                    EntityInfo {
                        id: *source_id,
                        npc_id: *source_npc_id,
                        entity_type: *source_entity_type,
                        name: *source_name,
                    },
                    EntityInfo {
                        id: *target_id,
                        npc_id: *target_npc_id,
                        entity_type: *target_entity_type,
                        name: *target_name,
                    },
                    *timestamp,
                    encounter,
                    RefreshTrigger::Heal,
                    false,
                );
                // HealingTaken trigger matching for effects tracker
                self.handle_ability_event_trigger(
                    *ability_id,
                    *ability_name,
                    *source_id,
                    *source_name,
                    *source_entity_type,
                    *source_npc_id,
                    *target_id,
                    *target_name,
                    *target_entity_type,
                    *target_npc_id,
                    *timestamp,
                    encounter,
                    EffectDefinition::is_healing_taken_trigger,
                );
            }
            GameSignal::TargetChanged {
                source_id,
                target_id,
                target_entity_type,
                target_name,
                ..
            } => {
                // Cache target ID, name, and entity type for fallback
                self.current_targets
                    .insert(*source_id, (*target_id, *target_name, *target_entity_type));
            }
            GameSignal::TargetCleared { source_id, .. } => {
                self.current_targets.remove(source_id);
            }
            // Boss entity IDs are now read from encounter.hp_by_entity in matches_filters
            _ => {}
        }
    }
}
