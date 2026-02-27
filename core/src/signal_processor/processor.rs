use crate::combat_log::{CombatEvent, EntityType};
use crate::context::resolve;
use crate::encounter::combat::ActiveBoss;
use crate::encounter::entity_info::PlayerInfo;
use crate::encounter::EncounterState;
use crate::game_data::{
    correct_apply_charges, effect_id, effect_type_id, BATTLE_REZ_ABILITY_IDS,
    SCRIPTED_REVIVE_EFFECT_IDS,
};
use crate::signal_processor::signal::GameSignal;
use crate::state::cache::SessionCache;

use super::{challenge, combat_state, counter, phase};

/// Processes combat events, routes them to encounters, and emits signals.
/// This is the state machine that manages combat lifecycle.
pub struct EventProcessor;

impl Default for EventProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl EventProcessor {
    pub fn new() -> Self {
        Self
    }

    /// Process an incoming event.
    /// Updates the cache and returns signals for cross-cutting concerns.
    /// Returns the event back along with signals to avoid cloning.
    /// The bool indicates whether the event was accumulated (for parquet filtering).
    pub fn process_event(
        &mut self,
        event: CombatEvent,
        cache: &mut SessionCache,
    ) -> (Vec<GameSignal>, CombatEvent, bool) {
        let mut signals = Vec::new();

        // ═══════════════════════════════════════════════════════════════════════
        // PHASE 1: Global Event Handlers (state-independent)
        // ═══════════════════════════════════════════════════════════════════════

        // 1a. Player/discipline tracking
        signals.extend(self.handle_discipline_event(&event, cache));

        // 1b. Entity lifecycle (death/revive)
        signals.extend(self.handle_entity_lifecycle(&event, cache));

        // 1b'. Synthetic death from 0 HP observation (handles missing death events
        // in 16-man and other content where SWTOR doesn't log entity death events)
        signals.extend(self.handle_zero_hp_deaths(&event, cache));

        // 1c. Area transitions
        signals.extend(self.handle_area_transition(&event, cache));

        // Detect missing area: if no AreaEntered has been seen after the first event,
        // the log file is incomplete (e.g., crash/disconnect caused SWTOR to resume
        // logging without emitting an initial AreaEntered).
        if !cache.missing_area
            && cache.current_area.area_id == 0
            && event.effect.type_id != effect_type_id::AREAENTERED
        {
            cache.missing_area = true;
        }

        // 1d. NPC first seen tracking (for ANY NPC, not just bosses)
        signals.extend(self.handle_npc_first_seen(&event, cache));

        // 1e. Boss encounter detection
        signals.extend(self.handle_boss_detection(&event, cache));

        // 1f. Boss HP tracking and phase transitions
        signals.extend(self.handle_boss_hp_and_phases(&event, cache));

        // 1g. NPC Target Tracking
        signals.extend(self.handle_target_changed(&event, cache));

        // 1h. Battle rez tracking (must run after target tracking and entity lifecycle)
        self.handle_battle_rez_tracking(&event, cache);

        // ═══════════════════════════════════════════════════════════════════════
        // PHASE 2: Signal Emission (pure transformation)
        // ═══════════════════════════════════════════════════════════════════════

        signals.extend(self.emit_effect_signals(&event));
        signals.extend(self.emit_action_signals(&event));
        signals.extend(self.emit_damage_signals(&event));
        signals.extend(self.emit_healing_signals(&event));

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
        // Skip during grace window to prevent inflating combat_time_secs/phase durations
        if !cache.is_in_grace_window() {
            signals.extend(phase::check_time_phase_transitions(cache, event.timestamp));
        }

        // Victory trigger detection (for special encounters like Coratanni)
        // Must happen after signals are emitted to support HP-based victory triggers
        self.handle_victory_trigger(&event, &signals, cache);

        // Process challenge metrics (accumulates values, polled with combat data)
        // Skip during grace window to prevent accumulating metrics after combat ends
        if !cache.is_in_grace_window() {
            challenge::process_challenge_events(&event, cache);
        }

        // ═══════════════════════════════════════════════════════════════════════
        // PHASE 3: Combat State Machine
        // ═══════════════════════════════════════════════════════════════════════

        let (combat_signals, was_accumulated) = combat_state::advance_combat_state(&event, cache);
        signals.extend(combat_signals);

        (signals, event, was_accumulated)
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
            received_revive_immunity: false,
            current_target_id: 0,
            last_seen_at: Some(event.timestamp),
        };

        // Upsert into session-level player discipline registry (source of truth)
        cache
            .player_disciplines
            .insert(event.source_entity.log_id, player_info);
    }

    fn update_area_from_event(&self, event: &CombatEvent, cache: &mut SessionCache) {
        let area_changed = event.effect.effect_id != cache.current_area.area_id;
        cache.current_area.area_name = resolve(event.effect.effect_name).to_string();
        cache.current_area.area_id = event.effect.effect_id;
        // Update difficulty: game sends 0 for non-instanced areas and as an initial
        // placeholder before the real value arrives. Only skip the update when staying
        // in the same area (placeholder case); on area change, 0 means open world.
        if event.effect.difficulty_id != 0 {
            cache.current_area.difficulty_id = event.effect.difficulty_id;
            cache.current_area.difficulty_name = resolve(event.effect.difficulty_name).to_string();
        } else if area_changed {
            // New area with no difficulty = open world, clear stale instance difficulty
            cache.current_area.difficulty_id = 0;
            cache.current_area.difficulty_name.clear();
        }
        if area_changed {
            cache.current_area.generation += 1;
        }
        cache.current_area.entered_at = Some(event.timestamp);
        cache.current_area.entered_at_line = Some(event.line_number);
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
            // Check if entity was already marked dead (e.g., synthetic death from 0 HP
            // observation fired on a previous event). If so, skip signal emission to
            // prevent duplicate EntityDeath signals reaching timers, counters, etc.
            let already_dead = cache
                .current_encounter()
                .map(|enc| match event.target_entity.entity_type {
                    EntityType::Player => enc
                        .players
                        .get(&event.target_entity.log_id)
                        .map_or(false, |p| p.is_dead),
                    EntityType::Npc | EntityType::Companion => enc
                        .npcs
                        .get(&event.target_entity.log_id)
                        .map_or(false, |n| n.is_dead),
                    _ => false,
                })
                .unwrap_or(false);

            // Always update state (idempotent) — ensures death_time gets the
            // authoritative timestamp from the real death event
            if let Some(enc) = cache.current_encounter_mut() {
                enc.set_entity_death(
                    event.target_entity.log_id,
                    &event.target_entity.entity_type,
                    event.timestamp,
                );
                enc.check_all_players_dead();
            }

            // Only emit signal if not already dead (prevents duplicate signals)
            if !already_dead {
                signals.push(GameSignal::EntityDeath {
                    entity_id: event.target_entity.log_id,
                    entity_type: event.target_entity.entity_type,
                    npc_id: event.target_entity.class_id,
                    entity_name: resolve(event.target_entity.name).to_string(),
                    timestamp: event.timestamp,
                });
            }
        } else if event.effect.effect_id == effect_id::REVIVED {
            // Check if this is a battle rez (pending) or OOC revive for the local player
            let is_local_player = event.source_entity.log_id == cache.player.id;
            if is_local_player {
                let battle_rez_pending = cache
                    .current_encounter()
                    .map_or(false, |enc| enc.battle_rez_pending);

                if let Some(enc) = cache.current_encounter_mut() {
                    if battle_rez_pending {
                        // Battle rez landed — clear pending flag, combat continues
                        enc.battle_rez_pending = false;
                        tracing::debug!(
                            "[BATTLE_REZ] Battle rez landed on local player at {}",
                            event.timestamp
                        );
                    } else if enc.state == EncounterState::InCombat {
                        // No battle rez pending — this is an OOC revive (medcenter/probe)
                        enc.local_player_ooc_revive_time = Some(event.timestamp);
                        tracing::info!(
                            "[BATTLE_REZ] OOC revive detected for local player at {}, will end combat",
                            event.timestamp
                        );
                    }
                }
            }

            if let Some(enc) = cache.current_encounter_mut() {
                // Don't process revives after a definitive wipe (all players dead)
                // This prevents post-wipe UI revives from resetting is_dead flags
                if !enc.all_players_dead {
                    enc.set_entity_alive(
                        event.source_entity.log_id,
                        &event.source_entity.entity_type,
                    );
                    enc.check_all_players_dead();
                }
            }
            signals.push(GameSignal::EntityRevived {
                entity_id: event.source_entity.log_id,
                entity_type: event.source_entity.entity_type,
                npc_id: event.source_entity.class_id,
                timestamp: event.timestamp,
            });
        } else if event.effect.effect_id == effect_id::RECENTLY_REVIVED
            && event.effect.type_id == effect_type_id::APPLYEFFECT
            && event.source_entity.entity_type == EntityType::Player
        {
            // Player received the revive immunity buff (medcenter/probe revive)
            // Mark them as permanently dead for this encounter
            let is_local_player = event.source_entity.log_id == cache.player.id;
            tracing::debug!(
                "[ENCOUNTER] RECENTLY_REVIVED at {} - player_id: {}, is_local_player: {}",
                event.timestamp,
                event.source_entity.log_id,
                is_local_player
            );
            // Only track revive immunity during active combat
            // Prevents stale timestamps from previous encounters bleeding into new ones
            // (e.g., medcenter revive after a wipe shouldn't affect the next pull)
            if let Some(enc) = cache.current_encounter_mut() {
                if enc.state == EncounterState::InCombat {
                    enc.set_player_revive_immunity(event.source_entity.log_id);

                    // Track timestamp for local player (soft-timeout wipe detection)
                    if is_local_player {
                        enc.local_player_revive_immunity_time = Some(event.timestamp);
                        // Player chose medcenter over a pending battle rez — clear the flag
                        // so the revive immunity fallback handles it correctly
                        enc.battle_rez_pending = false;
                    }
                }
            }
        }

        signals
    }

    /// Detect entity deaths from 0 HP when no explicit death event was logged.
    ///
    /// SWTOR sometimes fails to log death events, particularly in 16-man content.
    /// This checks every entity on every event: if an entity has 0 current HP with
    /// a valid max HP and is tracked but not yet marked dead, we emit a synthetic
    /// `EntityDeath` signal. The `is_dead` flag acts as a latch — once set on the
    /// first 0 HP observation, all subsequent events with that entity at 0 HP are
    /// no-ops, preventing repeated signal firing.
    fn handle_zero_hp_deaths(
        &self,
        event: &CombatEvent,
        cache: &mut SessionCache,
    ) -> Vec<GameSignal> {
        let mut signals = Vec::new();

        for entity in [&event.source_entity, &event.target_entity] {
            // Must have 0 current HP with a valid max HP (not an empty/uninitialized entity)
            if entity.health.0 != 0 || entity.health.1 <= 0 {
                continue;
            }

            // Must have a valid entity ID
            if entity.log_id == 0 {
                continue;
            }

            match entity.entity_type {
                EntityType::Npc | EntityType::Companion => {
                    let Some(enc) = cache.current_encounter_mut() else {
                        continue;
                    };
                    let Some(npc) = enc.npcs.get(&entity.log_id) else {
                        continue;
                    };
                    if npc.is_dead {
                        continue;
                    }

                    let npc_id = npc.class_id;
                    let entity_name = resolve(npc.name).to_string();

                    enc.set_entity_death(entity.log_id, &entity.entity_type, event.timestamp);

                    signals.push(GameSignal::EntityDeath {
                        entity_id: entity.log_id,
                        entity_type: entity.entity_type,
                        npc_id,
                        entity_name,
                        timestamp: event.timestamp,
                    });
                }
                EntityType::Player => {
                    let Some(enc) = cache.current_encounter_mut() else {
                        continue;
                    };
                    let Some(player) = enc.players.get(&entity.log_id) else {
                        continue;
                    };
                    if player.is_dead {
                        continue;
                    }

                    let entity_name = resolve(player.name).to_string();

                    enc.set_entity_death(entity.log_id, &entity.entity_type, event.timestamp);
                    enc.check_all_players_dead();

                    signals.push(GameSignal::EntityDeath {
                        entity_id: entity.log_id,
                        entity_type: entity.entity_type,
                        npc_id: 0,
                        entity_name,
                        timestamp: event.timestamp,
                    });
                }
                _ => {}
            }
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

        // Detect character mismatch: if a different player enters an area after the
        // local character was already established, the log file contains data from
        // multiple logins (e.g., hibernation caused a second login to append here).
        if cache.player_initialized
            && event.source_entity.log_id != 0
            && event.source_entity.log_id != cache.player.id
        {
            cache.character_mismatch = true;
        }

        self.update_area_from_event(event, cache);

        // Also update the current encounter's area/difficulty
        // BUT only if the encounter hasn't started combat yet.
        // If combat is active, the encounter should keep its original area info
        // (e.g., when player uses ReturnToMedCenter, we don't want to overwrite
        // the raid area with the medcenter/fleet area)
        if let Some(enc) = cache.current_encounter_mut() {
            if enc.state == EncounterState::NotStarted {
                if event.effect.difficulty_id != 0 {
                    let difficulty = crate::game_data::Difficulty::from_difficulty_id(
                        event.effect.difficulty_id,
                    );
                    let difficulty_id = Some(event.effect.difficulty_id);
                    let difficulty_name = Some(resolve(event.effect.difficulty_name).to_string());
                    enc.set_difficulty_info(difficulty, difficulty_id, difficulty_name);
                } else {
                    // No difficulty = open world, clear any stale instance difficulty
                    enc.set_difficulty_info(None, None, None);
                }
                let area_id = if event.effect.effect_id != 0 {
                    Some(event.effect.effect_id)
                } else {
                    None
                };
                let area_name = Some(resolve(event.effect.effect_name).to_string());
                let area_entered_line = Some(event.line_number);
                enc.set_area(area_id, area_name, area_entered_line);
            }
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
        // Gather state from immutable borrow first
        let (has_active_boss, in_combat, has_definitions, registered_npcs) = {
            let Some(enc) = cache.current_encounter() else {
                return Vec::new();
            };
            (
                enc.active_boss_idx().is_some(),
                enc.state == EncounterState::InCombat,
                !enc.boss_definitions().is_empty(),
                // Collect registered NPC log_ids for the check below
                enc.npcs.keys().copied().collect::<Vec<_>>(),
            )
        };

        // Already tracking a boss encounter
        if has_active_boss {
            return Vec::new();
        }

        // Only detect bosses when actually in combat
        if !in_combat {
            return Vec::new();
        }

        // No boss definitions loaded for this area
        if !has_definitions {
            return Vec::new();
        }

        // Check source and target entities for boss NPC match
        // Only consider NPCs that are actually registered in the encounter (engaged in combat),
        // not just appearing in events (e.g., from targeting without engagement)
        let entities_to_check = [&event.source_entity, &event.target_entity];

        for entity in entities_to_check {
            if entity.entity_type != EntityType::Npc || entity.class_id == 0 {
                continue;
            }

            // Skip NPCs not registered in the encounter (not actually engaged)
            if !registered_npcs.contains(&entity.log_id) {
                continue;
            }

            // Try to detect boss encounter from this NPC
            if let Some(idx) = cache.detect_boss_encounter(entity.class_id) {
                // Get the encounter mutably and extract data from definition
                let Some(enc) = cache.current_encounter_mut() else {
                    tracing::error!(
                        "BUG: encounter missing after detect_boss_encounter in handle_boss_detection"
                    );
                    continue;
                };
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
            let Some(enc) = cache.current_encounter() else {
                tracing::error!("BUG: encounter missing in handle_boss_hp_and_phases loop");
                continue;
            };
            let def = &enc.boss_definitions()[def_idx];
            if !def.matches_npc_id(entity.class_id) {
                continue;
            }

            let (current_hp, max_hp) = (entity.health.0, entity.health.1);
            if max_hp <= 0 {
                continue;
            }

            // Update boss state and check if HP changed
            let Some(enc) = cache.current_encounter_mut() else {
                tracing::error!("BUG: encounter missing in handle_boss_hp_and_phases loop (mut)");
                continue;
            };
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

    /// Check for victory trigger on special encounters (e.g., Coratanni, Terror from Beyond).
    /// Updates encounter.victory_triggered when the trigger fires.
    /// Supports all trigger types including ability casts, HP thresholds, and composite triggers.
    /// Optionally filtered by difficulty (e.g., Trandoshan Squad only has victory trigger on Master).
    fn handle_victory_trigger(
        &self,
        event: &CombatEvent,
        signals: &[GameSignal],
        cache: &mut SessionCache,
    ) {
        let Some(enc) = cache.current_encounter() else {
            return;
        };

        let Some(idx) = enc.active_boss_idx() else {
            return;
        };

        let boss_def = &enc.boss_definitions()[idx];

        if !boss_def.has_victory_trigger {
            return;
        }

        // Check if victory trigger applies to current difficulty
        if !boss_def.victory_trigger_difficulties.is_empty() {
            let difficulty_matches = enc
                .difficulty
                .as_ref()
                .map(|d| {
                    boss_def
                        .victory_trigger_difficulties
                        .iter()
                        .any(|vd| d.matches_config_key(vd))
                })
                .unwrap_or(true); // Default to true if difficulty unknown

            if !difficulty_matches {
                return; // Victory trigger doesn't apply to this difficulty
            }
        }

        // Check victory conditions (state guards) first
        if !boss_def.victory_conditions.is_empty()
            && !enc.evaluate_conditions(&boss_def.victory_conditions)
        {
            return; // Victory conditions not met
        }

        // Build filter context for source/target checking
        let boss_ids = enc.boss_entity_ids();
        let local_player_id = Some(cache.player.id).filter(|&id| id != 0);
        let current_target_id = local_player_id.and_then(|pid| enc.local_player_target_id(pid));
        let filter_ctx = super::trigger_eval::FilterContext {
            entities: &boss_def.entities,
            local_player_id,
            current_target_id,
            boss_entity_ids: &boss_ids,
        };

        // Check if trigger matches (needs immutable borrow)
        let trigger_fired = if let Some(ref trigger) = boss_def.victory_trigger {
            super::trigger_eval::check_event_trigger(trigger, event, Some(&filter_ctx))
                || super::trigger_eval::check_signal_trigger(trigger, signals, &filter_ctx)
        } else {
            false
        };

        // Update state if triggered (needs mutable borrow)
        if trigger_fired {
            if let Some(enc) = cache.current_encounter_mut() {
                enc.victory_triggered = true;
                enc.victory_triggered_at = Some(event.timestamp);
            }
        }
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
                    source_entity_type: event.source_entity.entity_type,
                    source_npc_id: event.source_entity.class_id,
                    source_name: event.source_entity.name,
                    target_id: event.target_entity.log_id,
                    target_entity_type: event.target_entity.entity_type,
                    target_name: event.target_entity.name,
                    target_npc_id: event.target_entity.class_id,
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

    /// Track battle rez casts targeting the local player.
    ///
    /// Sets `battle_rez_pending` when a battle rez is activated targeting the local player,
    /// and clears it on interrupt/cancel. The REVIVED handler in `handle_entity_lifecycle`
    /// checks this flag to distinguish battle rez (combat continues) from OOC revive
    /// (combat should end).
    fn handle_battle_rez_tracking(&self, event: &CombatEvent, cache: &mut SessionCache) {
        let eid = event.effect.effect_id;

        // A) Battle rez activated — check if caster is targeting local player
        if eid == effect_id::ABILITYACTIVATE
            && BATTLE_REZ_ABILITY_IDS.contains(&event.action.action_id)
        {
            if let Some(enc) = cache.current_encounter() {
                let targets_local = enc
                    .get_current_target(event.source_entity.log_id)
                    .is_some_and(|tid| tid == cache.player.id);
                if targets_local {
                    if let Some(enc) = cache.current_encounter_mut() {
                        enc.battle_rez_pending = true;
                        tracing::debug!(
                            "[BATTLE_REZ] Pending battle rez on local player at {}",
                            event.timestamp
                        );
                    }
                }
            }
            return;
        }

        // B) Battle rez interrupted or cancelled — clear pending flag
        if (eid == effect_id::ABILITYINTERRUPT || eid == effect_id::ABILITYCANCEL)
            && BATTLE_REZ_ABILITY_IDS.contains(&event.action.action_id)
        {
            if let Some(enc) = cache.current_encounter_mut() {
                if enc.battle_rez_pending {
                    enc.battle_rez_pending = false;
                    tracing::debug!(
                        "[BATTLE_REZ] Battle rez cancelled/interrupted at {}",
                        event.timestamp
                    );
                }
            }
            return;
        }

        // C) Scripted revive effect applied to local player (e.g., Boon of the Spirit on Revan)
        if event.effect.type_id == effect_type_id::APPLYEFFECT
            && SCRIPTED_REVIVE_EFFECT_IDS.contains(&eid)
            && event.source_entity.log_id == cache.player.id
        {
            if let Some(enc) = cache.current_encounter_mut() {
                enc.battle_rez_pending = true;
                tracing::debug!(
                    "[BATTLE_REZ] Scripted revive effect on local player at {}",
                    event.timestamp
                );
            }
        }
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
                    source_npc_id: event.source_entity.class_id,
                    target_id: event.target_entity.log_id,
                    target_entity_type: event.target_entity.entity_type,
                    target_name: event.target_entity.name,
                    target_npc_id: event.target_entity.class_id,
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
            target_npc_id: event.target_entity.class_id,
            timestamp: event.timestamp,
        }]
    }

    /// Emit signals for healing events (for effect refresh on heal completion).
    /// Pure transformation - no encounter state modification.
    fn emit_healing_signals(&self, event: &CombatEvent) -> Vec<GameSignal> {
        // Only emit for heals during APPLYEFFECT
        if event.effect.type_id != effect_type_id::APPLYEFFECT
            || event.effect.effect_id != effect_id::HEAL
        {
            return Vec::new();
        }

        // Ensure we have valid source and target
        if event.source_entity.entity_type == EntityType::Empty
            || event.target_entity.entity_type == EntityType::Empty
        {
            return Vec::new();
        }

        vec![GameSignal::HealingDone {
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
        }]
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Victory Trigger Checking
// ═══════════════════════════════════════════════════════════════════════════

// Victory trigger evaluation now uses the unified trigger_eval functions.
// See the call site in handle_victory_trigger() above.
