use crate::combat_log::{CombatEvent, EntityType};
use crate::context::resolve;
use crate::events::signal::GameSignal;
use crate::state::cache::SessionCache;
use crate::encounter::EncounterState;
use crate::encounter::entity_info::PlayerInfo;
use crate::game_data::{effect_id, effect_type_id, correct_apply_charges};
use chrono::NaiveDateTime;

// Combat state machine constants
const COMBAT_TIMEOUT_SECONDS: i64 = 120;
const POST_COMBAT_THRESHOLD_MS: i64 = 5000;

/// Processes combat events, routes them to encounters, and emits signals.
/// This is the state machine that manages combat lifecycle.
pub struct EventProcessor {
    /// Grace period for trailing damage after combat ends
    post_combat_threshold_ms: i64,
}

impl Default for EventProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl EventProcessor {
    pub fn new() -> Self {
        Self {
            post_combat_threshold_ms: POST_COMBAT_THRESHOLD_MS,
        }
    }

    /// Process an incoming event.
    /// Updates the cache and returns signals for cross-cutting concerns.
    pub fn process_event(
        &mut self,
        event: CombatEvent,
        cache: &mut SessionCache,
    ) -> Vec<GameSignal> {
        let mut signals = Vec::new();

        // 1. Handle player initialization on DisciplineChanged
        if event.effect.type_id == effect_type_id::DISCIPLINECHANGED {
            if !cache.player_initialized || event.source_entity.log_id == cache.player.id {
                self.update_primary_player(&event, cache);
                if cache.player_initialized {
                    signals.push(GameSignal::PlayerInitialized {
                        entity_id: cache.player.id,
                        timestamp: event.timestamp,
                    });
                }
            }
            self.add_player_to_encounter(&event, cache);

            // Emit DisciplineChanged for ALL players (used for raid frame role detection)
            if event.effect.discipline_id != 0 {
                signals.push(GameSignal::DisciplineChanged {
                    entity_id: event.source_entity.log_id,
                    discipline_id: event.effect.discipline_id,
                    timestamp: event.timestamp,
                });
            }
        }

        // 2. Handle death/revive
        if event.effect.effect_id == effect_id::DEATH {
            if let Some(enc) = cache.current_encounter_mut() {
                enc.set_entity_death(
                    event.target_entity.log_id,
                    &event.target_entity.entity_type,
                    event.timestamp,
                );
                enc.check_all_players_dead();
            }
            signals.push(GameSignal::EntityDeath {
                entity_id: event.target_entity.log_id,
                entity_type: event.target_entity.entity_type,
                timestamp: event.timestamp,
            });
        } else if event.effect.effect_id == effect_id::REVIVED {
            if let Some(enc) = cache.current_encounter_mut() {
                enc.set_entity_alive(
                    event.source_entity.log_id,
                    &event.source_entity.entity_type,
                );
                enc.check_all_players_dead();
            }
            signals.push(GameSignal::EntityRevived {
                entity_id: event.source_entity.log_id,
                entity_type: event.source_entity.entity_type,
                timestamp: event.timestamp,
            });
        }

        // 3. Handle area transitions
        if event.effect.type_id == effect_type_id::AREAENTERED {
            self.update_area_from_event(&event, cache);
            signals.push(GameSignal::AreaEntered {
                area_id: event.effect.effect_id,
                area_name: resolve(event.effect.effect_name).to_string(),
                difficulty_id: event.effect.difficulty_id,
                difficulty_name: resolve(event.effect.difficulty_name).to_string(),
                timestamp: event.timestamp,
            });
        }

        // 4. Route event to encounter and collect additional signals
        let routing_signals = self.route_event_to_encounter(event, cache);
        signals.extend(routing_signals);

        signals
    }

    fn update_primary_player(&self, event: &CombatEvent, cache: &mut SessionCache) {
        if !cache.player_initialized {
            cache.player.name = event.source_entity.name;
            cache.player.id = event.source_entity.log_id;
            cache.player_initialized = true;
        }
        cache.player.class_name = resolve(event.effect.effect_name).to_string();
        cache.player.class_id = event.effect.effect_id;
        cache.player.discipline_id = event.effect.discipline_id;
        cache.player.discipline_name = resolve(event.effect.discipline_name).to_string();
    }

    fn add_player_to_encounter(&self, event: &CombatEvent, cache: &mut SessionCache) {
        let Some(enc) = cache.current_encounter_mut() else {
            return;
        };

        enc.players
            .entry(event.source_entity.log_id)
            .or_insert(PlayerInfo {
                id: event.source_entity.log_id,
                name: event.source_entity.name,
                class_id: event.effect.effect_id,
                class_name: resolve(event.effect.effect_name).to_string(),
                discipline_id: event.effect.discipline_id,
                discipline_name: resolve(event.effect.discipline_name).to_string(),
                is_dead: false,
                death_time: None,
            });
    }

    fn update_area_from_event(&self, event: &CombatEvent, cache: &mut SessionCache) {
        cache.current_area.area_name = resolve(event.effect.effect_name).to_string();
        cache.current_area.area_id = event.effect.effect_id;
        cache.current_area.difficulty_id = event.effect.difficulty_id;
        cache.current_area.difficulty_name = resolve(event.effect.difficulty_name).to_string();
        cache.current_area.entered_at = Some(event.timestamp);
    }

    fn route_event_to_encounter(
        &mut self,
        event: CombatEvent,
        cache: &mut SessionCache,
    ) -> Vec<GameSignal> {
        let mut signals = Vec::new();

        let effect_id = event.effect.effect_id;
        let effect_type_id = event.effect.type_id;
        let timestamp = event.timestamp;

        let current_state = cache
            .current_encounter()
            .map(|e| e.state.clone())
            .unwrap_or_default();

        // Handle effect application/removal/charges (doesn't change combat state)
        match event.effect.type_id {
            effect_type_id::APPLYEFFECT => {
                if event.target_entity.entity_type == EntityType::Empty {
                    return signals;
                }
                if let Some(enc) = cache.current_encounter_mut() {
                    enc.apply_effect(&event);
                }
                let charges = if event.details.charges > 0 {
                    Some(correct_apply_charges(event.effect.effect_id, event.details.charges as u8))
                } else {
                    None
                };
                signals.push(GameSignal::EffectApplied {
                    effect_id: event.effect.effect_id,
                    action_id: event.action.action_id,
                    source_id: event.source_entity.log_id,
                    target_id: event.target_entity.log_id,
                    target_name: event.target_entity.name,
                    target_entity_type: event.target_entity.entity_type,
                    timestamp: event.timestamp,
                    charges,
                });
            }
            effect_type_id::REMOVEEFFECT => {
                if event.source_entity.entity_type == EntityType::Empty {
                    return signals;
                }
                if let Some(enc) = cache.current_encounter_mut() {
                    enc.remove_effect(&event);
                }
                signals.push(GameSignal::EffectRemoved {
                    effect_id: event.effect.effect_id,
                    source_id: event.source_entity.log_id,
                    target_id: event.target_entity.log_id,
                    timestamp: event.timestamp,
                });
            }
            effect_type_id::MODIFYCHARGES => {
                if event.target_entity.entity_type == EntityType::Empty {
                    return signals;
                }
                signals.push(GameSignal::EffectChargesChanged {
                    effect_id: event.effect.effect_id,
                    action_id: event.action.action_id,
                    target_id: event.target_entity.log_id,
                    timestamp: event.timestamp,
                    charges: event.details.charges as u8,
                });
            }
            _ => {}
        }

        // Emit ability activation signal
        if effect_id == effect_id::ABILITYACTIVATE {
            signals.push(GameSignal::AbilityActivated {
                ability_id: event.action.action_id,
                source_id: event.source_entity.log_id,
                target_id: event.target_entity.log_id,
                target_name: event.target_entity.name,
                target_entity_type: event.target_entity.entity_type,
                timestamp: event.timestamp,
            });
        }

        // Emit target change signals
        if effect_id == effect_id::TARGETSET {
            signals.push(GameSignal::TargetChanged {
                source_id: event.source_entity.log_id,
                target_id: event.target_entity.log_id,
                target_name: event.target_entity.name,
                target_entity_type: event.target_entity.entity_type,
                timestamp: event.timestamp,
            });
        } else if effect_id == effect_id::TARGETCLEARED {
            signals.push(GameSignal::TargetCleared {
                source_id: event.source_entity.log_id,
                timestamp: event.timestamp,
            });
        }

        match current_state {
            EncounterState::NotStarted => {
                signals.extend(self.handle_not_started(event, cache, effect_id, timestamp));
            }
            EncounterState::InCombat => {
                signals.extend(self.handle_in_combat(
                    event,
                    cache,
                    effect_id,
                    effect_type_id,
                    timestamp,
                ));
            }
            EncounterState::PostCombat { exit_time } => {
                signals.extend(self.handle_post_combat(event, cache, effect_id, timestamp, exit_time));
            }
        }

        signals
    }

    fn handle_not_started(
        &mut self,
        event: CombatEvent,
        cache: &mut SessionCache,
        effect_id: i64,
        timestamp: NaiveDateTime,
    ) -> Vec<GameSignal> {
        let mut signals = Vec::new();

        if effect_id == effect_id::ENTERCOMBAT {
            if let Some(enc) = cache.current_encounter_mut() {
                enc.state = EncounterState::InCombat;
                enc.enter_combat_time = Some(timestamp);
                enc.track_event_entities(&event);
                enc.accumulate_data(&event);
                enc.events.push(event);

                signals.push(GameSignal::CombatStarted {
                    timestamp,
                    encounter_id: enc.id,
                });
            }
        } else {
            // Buffer non-damage events for the upcoming encounter
            if let Some(enc) = cache.current_encounter_mut() {
                enc.accumulate_data(&event);
                enc.events.push(event);
            }
        }

        signals
    }

    fn handle_in_combat(
        &mut self,
        event: CombatEvent,
        cache: &mut SessionCache,
        effect_id: i64,
        effect_type_id: i64,
        timestamp: NaiveDateTime,
    ) -> Vec<GameSignal> {
        let mut signals = Vec::new();

        // Check for combat timeout
        if let Some(enc) = cache.current_encounter() && let Some(last_activity) = enc.last_combat_activity_time {
                let elapsed = timestamp.signed_duration_since(last_activity).num_seconds();
                if elapsed >= COMBAT_TIMEOUT_SECONDS {
                    let encounter_id = enc.id;
                    // End combat at last_activity_time
                    if let Some(enc) = cache.current_encounter_mut() {
                        enc.flush_pending_absorptions();
                        enc.exit_combat_time = Some(last_activity);
                        enc.state = EncounterState::PostCombat {
                            exit_time: last_activity,
                        };
                    }

                    signals.push(GameSignal::CombatEnded {
                        timestamp: last_activity,
                        encounter_id,
                    });

                    cache.push_new_encounter();
                    // Re-process this event in the new encounter
                    signals.extend(self.route_event_to_encounter(event, cache));
                    return signals;
                }

        }

        let all_players_dead = cache
            .current_encounter()
            .map(|e| e.all_players_dead)
            .unwrap_or(false);

        if effect_id == effect_id::ENTERCOMBAT {
            // Unexpected EnterCombat while in combat - terminate and restart
            let encounter_id = cache.current_encounter().map(|e| e.id).unwrap_or(0);
            if let Some(enc) = cache.current_encounter_mut() {
                enc.flush_pending_absorptions();
                enc.exit_combat_time = Some(timestamp);
                enc.state = EncounterState::PostCombat {
                    exit_time: timestamp,
                };
            }

            signals.push(GameSignal::CombatEnded {
                timestamp,
                encounter_id,
            });

            cache.push_new_encounter();
            signals.extend(self.route_event_to_encounter(event, cache));
        } else if effect_id == effect_id::EXITCOMBAT || all_players_dead {
            let encounter_id = cache.current_encounter().map(|e| e.id).unwrap_or(0);
            if let Some(enc) = cache.current_encounter_mut() {
                enc.flush_pending_absorptions();
                enc.exit_combat_time = Some(timestamp);
                enc.state = EncounterState::PostCombat {
                    exit_time: timestamp,
                };
                enc.events.push(event);
            }

            signals.push(GameSignal::CombatEnded {
                timestamp,
                encounter_id,
            });
        } else if effect_type_id == effect_type_id::AREAENTERED {
            let encounter_id = cache.current_encounter().map(|e| e.id).unwrap_or(0);
            if let Some(enc) = cache.current_encounter_mut() {
                enc.flush_pending_absorptions();
                enc.exit_combat_time = Some(timestamp);
                enc.state = EncounterState::PostCombat {
                    exit_time: timestamp,
                };
            }

            signals.push(GameSignal::CombatEnded {
                timestamp,
                encounter_id,
            });

            cache.push_new_encounter();
        } else {
            // Normal combat event
            if let Some(enc) = cache.current_encounter_mut() {
                enc.track_event_entities(&event);
                enc.accumulate_data(&event);
                enc.events.push(event);
                if effect_id == effect_id::DAMAGE || effect_id == effect_id::HEAL {
                    enc.last_combat_activity_time = Some(timestamp);
                }
            }
        }

        signals
    }

    fn handle_post_combat(
        &mut self,
        event: CombatEvent,
        cache: &mut SessionCache,
        effect_id: i64,
        timestamp: NaiveDateTime,
        exit_time: NaiveDateTime,
    ) -> Vec<GameSignal> {
        let mut signals = Vec::new();

        if effect_id == effect_id::ENTERCOMBAT {
            // New combat starting
            let new_encounter_id = cache.push_new_encounter();
            if let Some(enc) = cache.current_encounter_mut() {
                enc.state = EncounterState::InCombat;
                enc.enter_combat_time = Some(timestamp);
                enc.accumulate_data(&event);
                enc.events.push(event);
            }

            signals.push(GameSignal::CombatStarted {
                timestamp,
                encounter_id: new_encounter_id,
            });
        } else if effect_id == effect_id::DAMAGE {
            let elapsed = timestamp
                .signed_duration_since(exit_time)
                .num_milliseconds();
            if elapsed <= self.post_combat_threshold_ms {
                // Trailing damage - assign to ending encounter
                if let Some(enc) = cache.current_encounter_mut() {
                    enc.track_event_entities(&event);
                    enc.accumulate_data(&event);
                    enc.events.push(event);
                }
            } else {
                // Beyond grace period - discard and start fresh
                cache.push_new_encounter();
            }
        } else {
            // Non-damage event - goes to next encounter
            cache.push_new_encounter();
            if let Some(enc) = cache.current_encounter_mut() {
                enc.accumulate_data(&event);
                enc.events.push(event);
            }
        }

        signals
    }
}
