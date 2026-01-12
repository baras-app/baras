use crate::combat_log::{CombatEvent, EntityType};
use crate::context::resolve;
use crate::encounter::EncounterState;
use crate::encounter::combat::ActiveBoss;
use crate::encounter::entity_info::PlayerInfo;
use crate::game_data::{correct_apply_charges, effect_id, effect_type_id};
use crate::signal_processor::signal::GameSignal;
use crate::state::cache::SessionCache;

use super::{challenge, combat_state, counter, phase};

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
    /// Returns the event back along with signals to avoid cloning.
    pub fn process_event(
        &mut self,
        event: CombatEvent,
        cache: &mut SessionCache,
    ) -> (Vec<GameSignal>, CombatEvent) {
        let mut signals = Vec::new();

        // ═══════════════════════════════════════════════════════════════════════
        // PHASE 1: Global Event Handlers (state-independent)
        // ═══════════════════════════════════════════════════════════════════════

        // 1a. Player/discipline tracking
        signals.extend(self.handle_discipline_event(&event, cache));

        // 1b. Entity lifecycle (death/revive)
        signals.extend(self.handle_entity_lifecycle(&event, cache));

        // 1c. Area transitions
        signals.extend(self.handle_area_transition(&event, cache));

        // 1d. NPC first seen tracking (for ANY NPC, not just bosses)
        signals.extend(self.handle_npc_first_seen(&event, cache));

        // 1e. Boss encounter detection
        signals.extend(self.handle_boss_detection(&event, cache));

        // 1f. Boss HP tracking and phase transitions
        signals.extend(self.handle_boss_hp_and_phases(&event, cache));

        // 1e. NPC Target Tracking
        signals.extend(self.handle_target_changed(&event, cache));

        // ═══════════════════════════════════════════════════════════════════════
        // PHASE 2: Signal Emission (pure transformation)
        // ═══════════════════════════════════════════════════════════════════════

        signals.extend(self.emit_effect_signals(&event));
        signals.extend(self.emit_action_signals(&event));
        signals.extend(self.emit_damage_signals(&event));

        // Check if current phase's end_trigger fired (emits PhaseEndTriggered signal)
        signals.extend(phase::check_phase_end_triggers(&event, cache, &signals));

        // Check for counter increments based on events and signals
        // IMPORTANT: This must happen BEFORE phase transitions so counter_conditions
        // see the updated values (e.g., fs_burn needs counter=4 after 4th shield phase)
        signals.extend(counter::check_counter_increments(&event, cache, &signals));

        // Check for ability/effect-based phase transitions (can now match PhaseEnded)
        signals.extend(phase::check_ability_phase_transitions(
            &event, cache, &signals,
        ));

        // Check for entity-based phase transitions (EntityFirstSeen, EntityDeath, PhaseEnded)
        signals.extend(phase::check_entity_phase_transitions(
            cache,
            &signals,
            event.timestamp,
        ));

        // Update combat time and check for TimeElapsed phase transitions
        signals.extend(phase::check_time_phase_transitions(cache, event.timestamp));

        // Process challenge metrics (accumulates values, polled with combat data)
        challenge::process_challenge_events(&event, cache);

        // ═══════════════════════════════════════════════════════════════════════
        // PHASE 3: Combat State Machine
        // ═══════════════════════════════════════════════════════════════════════

        signals.extend(combat_state::advance_combat_state(
            &event,
            cache,
            self.post_combat_threshold_ms,
        ));

        (signals, event)
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

    fn register_player_discipline(&self, event: &CombatEvent, cache: &mut SessionCache) {
        // Only register actual players, not companions
        if event.source_entity.entity_type != EntityType::Player {
            return;
        }

        let player_info = PlayerInfo {
            id: event.source_entity.log_id,
            name: event.source_entity.name,
            class_id: event.effect.effect_id,
            class_name: resolve(event.effect.effect_name).to_string(),
            discipline_id: event.effect.discipline_id,
            discipline_name: resolve(event.effect.discipline_name).to_string(),
            is_dead: false,
            death_time: None,
            current_target_id: 0,
        };

        // Upsert into session-level player discipline registry (source of truth)
        cache
            .player_disciplines
            .insert(event.source_entity.log_id, player_info);
    }

    fn update_area_from_event(&self, event: &CombatEvent, cache: &mut SessionCache) {
        cache.current_area.area_name = resolve(event.effect.effect_name).to_string();
        cache.current_area.area_id = event.effect.effect_id;
        // Only update difficulty if we get a valid ID (game sends 0 first, then real value)
        if event.effect.difficulty_id != 0 {
            cache.current_area.difficulty_id = event.effect.difficulty_id;
            cache.current_area.difficulty_name = resolve(event.effect.difficulty_name).to_string();
        }
        cache.current_area.entered_at = Some(event.timestamp);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Phase 1: Global Event Handlers
    // ═══════════════════════════════════════════════════════════════════════════

    /// Handle DisciplineChanged events for player initialization and role detection.
    fn handle_discipline_event(
        &self,
        event: &CombatEvent,
        cache: &mut SessionCache,
    ) -> Vec<GameSignal> {
        if event.effect.type_id != effect_type_id::DISCIPLINECHANGED {
            return Vec::new();
        }

        let mut signals = Vec::new();

        // Initialize or update primary player
        if !cache.player_initialized || event.source_entity.log_id == cache.player.id {
            self.update_primary_player(event, cache);
            if cache.player_initialized {
                signals.push(GameSignal::PlayerInitialized {
                    entity_id: cache.player.id,
                    timestamp: event.timestamp,
                });
            }
        }

        // Register player discipline in session-level registry
        self.register_player_discipline(event, cache);

        // Emit DisciplineChanged for ALL players (used for raid frame role detection)
        if event.effect.discipline_id != 0 {
            signals.push(GameSignal::DisciplineChanged {
                entity_id: event.source_entity.log_id,
                class_id: event.effect.effect_id,
                discipline_id: event.effect.discipline_id,
                timestamp: event.timestamp,
            });
        }

        signals
    }

    /// Handle Death and Revive events.
    fn handle_entity_lifecycle(
        &self,
        event: &CombatEvent,
        cache: &mut SessionCache,
    ) -> Vec<GameSignal> {
        let mut signals = Vec::new();

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
                npc_id: event.target_entity.class_id,
                entity_name: resolve(event.target_entity.name).to_string(),
                timestamp: event.timestamp,
            });
        } else if event.effect.effect_id == effect_id::REVIVED {
            if let Some(enc) = cache.current_encounter_mut() {
                enc.set_entity_alive(event.source_entity.log_id, &event.source_entity.entity_type);
                enc.check_all_players_dead();
            }
            signals.push(GameSignal::EntityRevived {
                entity_id: event.source_entity.log_id,
                entity_type: event.source_entity.entity_type,
                npc_id: event.source_entity.class_id,
                timestamp: event.timestamp,
            });
        }

        signals
    }

    /// Handle AreaEntered events.
    fn handle_area_transition(
        &self,
        event: &CombatEvent,
        cache: &mut SessionCache,
    ) -> Vec<GameSignal> {
        if event.effect.type_id != effect_type_id::AREAENTERED {
            return Vec::new();
        }

        self.update_area_from_event(event, cache);

        // Also update the current encounter's area/difficulty
        // (fixes timers with difficulty filters when AreaEntered fires mid-session)
        if let Some(enc) = cache.current_encounter_mut() {
            // Use difficulty_id (language-independent) instead of parsing localized strings
            // Note: Game sends two AreaEntered events - first with difficulty_id=0, then with real value
            // Only update difficulty if we get a valid ID (non-zero)
            if event.effect.difficulty_id != 0 {
                enc.set_difficulty(crate::game_data::Difficulty::from_difficulty_id(
                    event.effect.difficulty_id,
                ));
            }
            let area_id = if event.effect.effect_id != 0 {
                Some(event.effect.effect_id)
            } else {
                None
            };
            let area_name = Some(resolve(event.effect.effect_name).to_string());
            enc.set_area(area_id, area_name);
        }

        vec![GameSignal::AreaEntered {
            area_id: event.effect.effect_id,
            area_name: resolve(event.effect.effect_name).to_string(),
            difficulty_id: event.effect.difficulty_id,
            difficulty_name: resolve(event.effect.difficulty_name).to_string(),
            timestamp: event.timestamp,
        }]
    }

    /// Emit NpcFirstSeen for any NPC instance encountered for the first time.
    /// Tracks by log_id (instance), so each spawn of the same NPC type fires the signal.
    /// The signal includes npc_id (class_id) so timers can match on NPC type.
    fn handle_npc_first_seen(
        &self,
        event: &CombatEvent,
        cache: &mut SessionCache,
    ) -> Vec<GameSignal> {
        let mut signals = Vec::new();

        for entity in [&event.source_entity, &event.target_entity] {
            // Only track NPCs with valid IDs
            if entity.entity_type != EntityType::Npc || entity.class_id == 0 || entity.log_id == 0 {
                continue;
            }

            // Track by log_id (instance) so each spawn is detected
            // Signal includes npc_id (class_id) for timer matching
            if cache.seen_npc_instances.insert(entity.log_id) {
                signals.push(GameSignal::NpcFirstSeen {
                    entity_id: entity.log_id, // Unique instance
                    npc_id: entity.class_id,  // NPC type for timer matching
                    entity_name: resolve(entity.name).to_string(),
                    timestamp: event.timestamp,
                });
            }
        }

        signals
    }

    /// Detect boss encounters based on NPC class IDs.
    /// When a known boss NPC is first seen in combat, activates the encounter.
    fn handle_boss_detection(
        &self,
        event: &CombatEvent,
        cache: &mut SessionCache,
    ) -> Vec<GameSignal> {
        // Already tracking a boss encounter
        let Some(enc) = cache.current_encounter() else {
            return Vec::new();
        };
        if enc.active_boss_idx().is_some() {
            return Vec::new();
        }

        // Only detect bosses when actually in combat
        if enc.state != EncounterState::InCombat {
            return Vec::new();
        }

        // No boss definitions loaded for this area
        if enc.boss_definitions().is_empty() {
            return Vec::new();
        }

        // Check source and target entities for boss NPC match
        let entities_to_check = [&event.source_entity, &event.target_entity];

        for entity in entities_to_check {
            if entity.entity_type != EntityType::Npc || entity.class_id == 0 {
                continue;
            }

            // Try to detect boss encounter from this NPC
            if let Some(idx) = cache.detect_boss_encounter(entity.class_id) {
                // Get the encounter mutably and extract data from definition
                let enc = cache.current_encounter_mut().unwrap();
                let def = &enc.boss_definitions()[idx];
                let challenges = def.challenges.clone();
                let counters = def.counters.clone();
                let entities = def.entities.clone();
                let npc_ids: Vec<i64> = def.boss_npc_ids().collect();
                let def_id = def.id.clone();
                let boss_name = def.name.clone();
                let initial_phase = def.initial_phase().cloned();

                // Set active boss for timer context (HP will be updated later)
                enc.set_boss(ActiveBoss {
                    definition_id: def_id.clone(),
                    name: boss_name.clone(),
                    entity_id: entity.log_id,
                    max_hp: 0,
                    current_hp: 0,
                });

                // Start challenge tracker (combat already started via EnterCombat)
                enc.challenge_tracker
                    .start(challenges, entities, npc_ids.clone(), event.timestamp);
                if let Some(ref initial) = initial_phase {
                    enc.challenge_tracker
                        .set_phase(&initial.id, event.timestamp);
                }

                let mut signals = vec![GameSignal::BossEncounterDetected {
                    definition_id: def_id.clone(),
                    boss_name,
                    definition_idx: idx,
                    entity_id: entity.log_id,
                    npc_id: entity.class_id,
                    boss_npc_class_ids: npc_ids,
                    timestamp: event.timestamp,
                }];

                // Activate initial phase (CombatStart trigger)
                if let Some(ref initial) = initial_phase {
                    enc.set_phase(&initial.id, event.timestamp);
                    enc.reset_counters_to_initial(&initial.resets_counters, &counters);

                    signals.push(GameSignal::PhaseChanged {
                        boss_id: def_id,
                        old_phase: None,
                        new_phase: initial.id.clone(),
                        timestamp: event.timestamp,
                    });
                }

                return signals;
            }
        }

        Vec::new()
    }

    /// Track boss HP changes and evaluate phase transitions.
    fn handle_boss_hp_and_phases(
        &self,
        event: &CombatEvent,
        cache: &mut SessionCache,
    ) -> Vec<GameSignal> {
        // No active boss encounter
        let Some(enc) = cache.current_encounter() else {
            return Vec::new();
        };
        let Some(def_idx) = enc.active_boss_idx() else {
            return Vec::new();
        };

        let mut signals = Vec::new();

        // Update HP for entities that are boss NPCs
        for entity in [&event.source_entity, &event.target_entity] {
            if entity.entity_type != EntityType::Npc || entity.class_id == 0 {
                continue;
            }

            // Check if this NPC is part of the active boss encounter
            let enc = cache.current_encounter().unwrap();
            let def = &enc.boss_definitions()[def_idx];
            if !def.matches_npc_id(entity.class_id) {
                continue;
            }

            let (current_hp, max_hp) = (entity.health.0, entity.health.1);
            if max_hp <= 0 {
                continue;
            }

            // Update boss state and check if HP changed
            let enc = cache.current_encounter_mut().unwrap();
            if let Some((old_hp, new_hp)) = enc.update_entity_hp(entity.log_id, current_hp, max_hp)
            {
                signals.push(GameSignal::BossHpChanged {
                    entity_id: entity.log_id,
                    npc_id: entity.class_id,
                    entity_name: resolve(entity.name).to_string(),
                    current_hp,
                    max_hp,
                    old_hp_percent: old_hp,
                    new_hp_percent: new_hp,
                    timestamp: event.timestamp,
                });

                // Check for HP-based phase transitions
                signals.extend(phase::check_hp_phase_transitions(
                    cache,
                    old_hp,
                    new_hp,
                    entity.class_id,
                    resolve(entity.name),
                    event.timestamp,
                ));
            }
        }

        signals
    }

    fn handle_target_changed(
        &self,
        event: &CombatEvent,
        cache: &mut SessionCache,
    ) -> Vec<GameSignal> {
        let mut signals = Vec::new();

        match event.effect.effect_id {
            effect_id::TARGETSET => {
                signals.push(GameSignal::TargetChanged {
                    source_id: event.source_entity.log_id,
                    source_npc_id: event.source_entity.class_id,
                    source_name: event.source_entity.name,
                    target_id: event.target_entity.log_id,
                    target_name: event.target_entity.name,
                    target_npc_id: event.target_entity.class_id,
                    target_entity_type: event.target_entity.entity_type,
                    timestamp: event.timestamp,
                });
                if let Some(enc) = cache.current_encounter_mut() {
                    // Ensure entity is tracked before setting target
                    enc.track_event_entities(event);
                    enc.set_entity_target(event.source_entity.log_id, event.target_entity.log_id);
                }
            }
            effect_id::TARGETCLEARED => {
                signals.push(GameSignal::TargetCleared {
                    source_id: event.source_entity.log_id,
                    timestamp: event.timestamp,
                });
                if let Some(enc) = cache.current_encounter_mut() {
                    // Ensure entity is tracked before clearing target
                    enc.track_event_entities(event);
                    enc.clear_entity_target(event.source_entity.log_id);
                }
            }
            _ => {}
        }
        signals
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Phase 2: Signal Emission (pure transformation, no state changes)
    // ═══════════════════════════════════════════════════════════════════════════

    /// Emit signals for effect application/removal/charge changes.
    /// Pure transformation - no encounter state modification.
    fn emit_effect_signals(&self, event: &CombatEvent) -> Vec<GameSignal> {
        match event.effect.type_id {
            effect_type_id::APPLYEFFECT => {
                if event.target_entity.entity_type == EntityType::Empty {
                    return Vec::new();
                }
                let charges = if event.details.charges > 0 {
                    Some(correct_apply_charges(
                        event.effect.effect_id,
                        event.details.charges as u8,
                    ))
                } else {
                    None
                };
                vec![GameSignal::EffectApplied {
                    effect_id: event.effect.effect_id,
                    effect_name: event.effect.effect_name,
                    action_id: event.action.action_id,
                    action_name: event.action.name,
                    source_id: event.source_entity.log_id,
                    source_name: event.source_entity.name,
                    source_entity_type: event.source_entity.entity_type,
                    source_npc_id: event.source_entity.class_id,
                    target_id: event.target_entity.log_id,
                    target_name: event.target_entity.name,
                    target_entity_type: event.target_entity.entity_type,
                    target_npc_id: event.target_entity.class_id,
                    timestamp: event.timestamp,
                    charges,
                }]
            }
            effect_type_id::REMOVEEFFECT => {
                if event.source_entity.entity_type == EntityType::Empty {
                    return Vec::new();
                }
                vec![GameSignal::EffectRemoved {
                    effect_id: event.effect.effect_id,
                    effect_name: event.effect.effect_name,
                    source_id: event.source_entity.log_id,
                    source_entity_type: event.source_entity.entity_type,
                    source_name: event.source_entity.name,
                    target_id: event.target_entity.log_id,
                    target_entity_type: event.target_entity.entity_type,
                    target_name: event.target_entity.name,
                    timestamp: event.timestamp,
                }]
            }
            effect_type_id::MODIFYCHARGES => {
                if event.target_entity.entity_type == EntityType::Empty {
                    return Vec::new();
                }
                vec![GameSignal::EffectChargesChanged {
                    effect_id: event.effect.effect_id,
                    effect_name: event.effect.effect_name,
                    action_id: event.action.action_id,
                    action_name: event.action.name,
                    target_id: event.target_entity.log_id,
                    timestamp: event.timestamp,
                    charges: event.details.charges as u8,
                }]
            }
            _ => Vec::new(),
        }
    }

    /// Emit signals for ability activations and target changes.
    /// Pure transformation - no encounter state modification.
    fn emit_action_signals(&self, event: &CombatEvent) -> Vec<GameSignal> {
        let mut signals = Vec::new();
        let effect_id = event.effect.effect_id;

        // Ability activation
        if effect_id == effect_id::ABILITYACTIVATE {
            signals.push(GameSignal::AbilityActivated {
                ability_id: event.action.action_id,
                ability_name: event.action.name,
                source_id: event.source_entity.log_id,
                source_entity_type: event.source_entity.entity_type,
                source_name: event.source_entity.name,
                source_npc_id: event.source_entity.class_id,
                target_id: event.target_entity.log_id,
                target_entity_type: event.target_entity.entity_type,
                target_name: event.target_entity.name,
                target_npc_id: event.target_entity.class_id,
                timestamp: event.timestamp,
            });
        }
        signals
    }

    /// Emit signals for damage events (tank buster detection, raid-wide damage, etc.).
    /// Pure transformation - no encounter state modification.
    fn emit_damage_signals(&self, event: &CombatEvent) -> Vec<GameSignal> {
        // Only emit for damage during APPLYEFFECT
        if event.effect.type_id != effect_type_id::APPLYEFFECT
            || event.effect.effect_id != effect_id::DAMAGE
        {
            return Vec::new();
        }

        // Ensure we have valid source and target
        if event.source_entity.entity_type == EntityType::Empty
            || event.target_entity.entity_type == EntityType::Empty
        {
            return Vec::new();
        }

        vec![GameSignal::DamageTaken {
            ability_id: event.action.action_id,
            ability_name: event.action.name,
            source_id: event.source_entity.log_id,
            source_entity_type: event.source_entity.entity_type,
            source_name: event.source_entity.name,
            source_npc_id: event.source_entity.class_id,
            target_id: event.target_entity.log_id,
            target_entity_type: event.target_entity.entity_type,
            target_name: event.target_entity.name,
            timestamp: event.timestamp,
        }]
    }
}
