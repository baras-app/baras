use crate::combat_log::{CombatEvent, EntityType};
use crate::context::resolve;
use crate::dsl::triggers::EntitySelectorExt;
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
        let mut signals = Vec::with_capacity(8);

        // ═══════════════════════════════════════════════════════════════════════
        // PHASE 1: Global Event Handlers (state-independent)
        // ═══════════════════════════════════════════════════════════════════════

        // 1a. Player/discipline tracking
        self.handle_discipline_event(&event, cache, &mut signals);

        // 1b. Entity lifecycle (death/revive)
        self.handle_entity_lifecycle(&event, cache, &mut signals);

        // 1b'. Synthetic death from 0 HP observation (handles missing death events
        // in 16-man and other content where SWTOR doesn't log entity death events)
        self.handle_zero_hp_deaths(&event, cache, &mut signals);

        // 1c. Area transitions
        self.handle_area_transition(&event, cache, &mut signals);

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
        self.handle_npc_first_seen(&event, cache, &mut signals);

        // 1e. Boss encounter detection
        self.handle_boss_detection(&event, cache, &mut signals);

        // 1f. Boss HP tracking and phase transitions
        self.handle_boss_hp_and_phases(&event, cache, &mut signals);

        // 1g. NPC Target Tracking
        self.handle_target_changed(&event, cache, &mut signals);

        // 1h. Battle rez tracking (must run after target tracking and entity lifecycle)
        self.handle_battle_rez_tracking(&event, cache);

        // ═══════════════════════════════════════════════════════════════════════
        // PHASE 2: Signal Emission (pure transformation)
        // ═══════════════════════════════════════════════════════════════════════

        self.emit_effect_signals(&event, &mut signals);
        self.emit_action_signals(&event, cache, &mut signals);
        self.emit_damage_signals(&event, &mut signals);
        self.emit_healing_signals(&event, &mut signals);
        self.emit_threat_signals(&event, &mut signals);

        // Effect stack counters: update per-entity stack state and aggregate
        // Must run after effect signals are emitted, before the counter↔phase loop.
        signals.extend(counter::check_effect_stack_counters(
            cache,
            &signals,
            event.timestamp,
        ));

        // Check if current phase's end_trigger fired (emits PhaseEndTriggered signal)
        signals.extend(phase::check_phase_end_triggers(&event, cache, &signals));

        // ── Counter ↔ Phase fixed-point loop ────────────────────────────────
        //
        // Counters and phases can trigger each other:
        //   - Counter increments can satisfy counter_conditions on phases
        //   - Phase transitions produce PhaseChanged/PhaseEndTriggered signals
        //     that counters react to (PhaseEntered, PhaseEnded, AnyPhaseChange)
        //   - Counter changes produce CounterChanged signals that other counters
        //     or phases may react to (CounterReaches)
        //
        // We evaluate in a loop until no new signals are produced (fixed-point).
        // The watermark tracks which signals have already been processed so each
        // iteration only evaluates NEW signals — preventing double-counting.
        //
        // First iteration: evaluate counters against the raw event + all signals
        // so far (event-based triggers like AbilityCast only fire here).
        // Subsequent iterations: evaluate counters against new signals only.
        //
        // Phases are naturally idempotent (current_phase == target is skipped).
        // Counters are safe because each iteration only sees new signals via watermark.
        //
        // Termination: bounded by number of phases (each can enter at most once)
        // plus number of counters. Safety cap prevents infinite loops from
        // circular definitions.

        const MAX_ITERATIONS: usize = 20;
        let skip_time_phases = cache.is_in_grace_window();

        // First iteration: event-based counter evaluation
        signals.extend(counter::check_counter_increments(&event, cache, &signals));

        for iteration in 0..MAX_ITERATIONS {
            let watermark = signals.len();

            // Phases: check all transition types against current signal state
            signals.extend(phase::check_ability_phase_transitions(
                &event, cache, &signals,
            ));
            signals.extend(phase::check_entity_phase_transitions(
                cache,
                &signals,
                event.timestamp,
            ));
            if !skip_time_phases {
                signals.extend(phase::check_time_phase_transitions(cache, event.timestamp));
            }

            // Check if current phase's end_trigger fired from new signals
            signals.extend(phase::check_phase_end_triggers(&event, cache, &signals));

            // Did anything new get produced?
            if signals.len() == watermark {
                break; // Fixed-point reached
            }

            // Evaluate counters against only the NEW signals from this iteration
            let new_counter_signals = counter::check_counter_signal_triggers(
                cache,
                &signals[watermark..],
                event.timestamp,
            );
            signals.extend(new_counter_signals);

            // If still nothing new after counter eval, we're done
            if signals.len() == watermark {
                break;
            }

            if iteration == MAX_ITERATIONS - 1 {
                tracing::warn!(
                    "Counter/phase fixed-point loop hit safety cap ({MAX_ITERATIONS} iterations). \
                     Possible circular trigger definition."
                );
            }
        }

        // Boss shield activation/deactivation/depletion.
        // Runs after the full fixed-point loop so phase/counter signals are visible.
        self.check_shield_triggers(&signals, cache);

        // Victory trigger detection (for special encounters like Coratanni)
        // Must happen after signals are emitted to support HP-based victory triggers
        self.handle_victory_trigger(&event, &signals, cache);

        // Process challenge metrics (accumulates values, polled with combat data)
        // Grace-window events are still accumulated to parquet (combat_state::handle_post_combat)
        // and belong to this encounter, so the challenge tracker must see them too — otherwise
        // trailing damage (overkill, DoT ticks landing at 0 HP) shows up in the data explorer
        // but not in challenge totals.
        challenge::process_challenge_events(&event, cache);

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
        out: &mut Vec<GameSignal>,
    ) {
        if event.effect.type_id != effect_type_id::DISCIPLINECHANGED {
            return;
        }

        // Initialize or update primary player
        if !cache.player_initialized || event.source_entity.log_id == cache.player.id {
            self.update_primary_player(event, cache);
            if cache.player_initialized {
                out.push(GameSignal::PlayerInitialized {
                    entity_id: cache.player.id,
                    timestamp: event.timestamp,
                });
            }
        }

        // Register player discipline in session-level registry
        self.register_player_discipline(event, cache);

        // Emit DisciplineChanged for ALL players (used for raid frame role detection)
        if event.effect.discipline_id != 0 {
            out.push(GameSignal::DisciplineChanged {
                entity_id: event.source_entity.log_id,
                class_id: event.effect.effect_id,
                discipline_id: event.effect.discipline_id,
                timestamp: event.timestamp,
            });
        }
    }

    /// Handle Death and Revive events.
    fn handle_entity_lifecycle(
        &self,
        event: &CombatEvent,
        cache: &mut SessionCache,
        out: &mut Vec<GameSignal>,
    ) {
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
                out.push(GameSignal::EntityDeath {
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
            out.push(GameSignal::EntityRevived {
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
        out: &mut Vec<GameSignal>,
    ) {
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

                    out.push(GameSignal::EntityDeath {
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

                    out.push(GameSignal::EntityDeath {
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
    }

    /// Handle AreaEntered events.
    fn handle_area_transition(
        &self,
        event: &CombatEvent,
        cache: &mut SessionCache,
        out: &mut Vec<GameSignal>,
    ) {
        if event.effect.type_id != effect_type_id::AREAENTERED {
            return;
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
        let local_id = cache.player.id;
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

                // Evict stale players that leaked in via TargetSet/TargetCleared
                // between encounters — they'll be re-discovered once combat starts.
                enc.players.retain(|&id, _| id == local_id);
            }
        }

        out.push(GameSignal::AreaEntered {
            area_id: event.effect.effect_id,
            area_name: resolve(event.effect.effect_name).to_string(),
            difficulty_id: event.effect.difficulty_id,
            difficulty_name: resolve(event.effect.difficulty_name).to_string(),
            timestamp: event.timestamp,
        });
    }

    /// Emit NpcFirstSeen for any NPC instance encountered for the first time.
    /// Tracks by log_id (instance), so each spawn of the same NPC type fires the signal.
    /// The signal includes npc_id (class_id) so timers can match on NPC type.
    fn handle_npc_first_seen(
        &self,
        event: &CombatEvent,
        cache: &mut SessionCache,
        out: &mut Vec<GameSignal>,
    ) {
        for entity in [&event.source_entity, &event.target_entity] {
            // Only track NPCs with valid IDs
            if entity.entity_type != EntityType::Npc || entity.class_id == 0 || entity.log_id == 0 {
                continue;
            }

            // Track by log_id (instance) so each spawn is detected
            // Signal includes npc_id (class_id) for timer matching
            if cache.seen_npc_instances.insert(entity.log_id) {
                out.push(GameSignal::NpcFirstSeen {
                    entity_id: entity.log_id, // Unique instance
                    npc_id: entity.class_id,  // NPC type for timer matching
                    entity_name: resolve(entity.name).to_string(),
                    timestamp: event.timestamp,
                });
            }
        }
    }

    /// Detect boss encounters based on NPC class IDs.
    /// When a known boss NPC is first seen in combat, activates the encounter.
    fn handle_boss_detection(
        &self,
        event: &CombatEvent,
        cache: &mut SessionCache,
        out: &mut Vec<GameSignal>,
    ) {
        // Gather state from immutable borrow first
        let (has_active_boss, in_combat, has_definitions, registered_npcs) = {
            let Some(enc) = cache.current_encounter() else {
                return;
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
            return;
        }

        // Only detect bosses when actually in combat
        if !in_combat {
            return;
        }

        // No boss definitions loaded for this area
        if !has_definitions {
            return;
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
                // Pass current difficulty so difficulty-gated challenges are excluded.
                let current_difficulty = enc.difficulty.as_ref();
                enc.challenge_tracker.start(
                    challenges,
                    entities,
                    npc_ids.clone(),
                    event.timestamp,
                    current_difficulty,
                );
                if let Some(ref initial) = initial_phase {
                    enc.challenge_tracker
                        .set_phase(&initial.id, event.timestamp);
                }

                out.push(GameSignal::BossEncounterDetected {
                    definition_id: def_id.clone(),
                    boss_name,
                    definition_idx: idx,
                    entity_id: entity.log_id,
                    npc_id: entity.class_id,
                    boss_npc_class_ids: npc_ids,
                    timestamp: event.timestamp,
                });

                // Activate initial phase (CombatStart trigger)
                if let Some(ref initial) = initial_phase {
                    enc.set_phase(&initial.id, event.timestamp);
                    enc.reset_counters_to_initial(&initial.resets_counters, &counters);

                    out.push(GameSignal::PhaseChanged {
                        boss_id: def_id,
                        old_phase: None,
                        new_phase: initial.id.clone(),
                        timestamp: event.timestamp,
                    });
                }

                return;
            }
        }
    }

    /// Track boss HP changes and evaluate phase transitions.
    fn handle_boss_hp_and_phases(
        &self,
        event: &CombatEvent,
        cache: &mut SessionCache,
        out: &mut Vec<GameSignal>,
    ) {
        // No active boss encounter
        let Some(enc) = cache.current_encounter() else {
            return;
        };
        let Some(def_idx) = enc.active_boss_idx() else {
            return;
        };

        // Update HP for entities that are boss NPCs.
        // Prefer the target entity's HP snapshot (more accurate for the entity being acted upon).
        // Only fall back to source entity HP if the NPC has no prior HP recorded (first sighting).
        for (entity, is_target) in [(&event.target_entity, true), (&event.source_entity, false)] {
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

            // Source entity HP is only used as a fallback when the NPC has no prior HP.
            // Once we have a target-derived HP reading, ignore source snapshots (often stale).
            if !is_target && enc.npc_has_hp(entity.log_id) {
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
                out.push(GameSignal::BossHpChanged {
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
                out.extend(phase::check_hp_phase_transitions(
                    cache,
                    old_hp,
                    new_hp,
                    entity.class_id,
                    resolve(entity.name),
                    event.timestamp,
                ));
            }
        }
    }

    /// Evaluate shield start/end triggers against the emitted signal vec and update
    /// boss shield state (activate, deactivate, deplete).
    ///
    /// This runs after Phase 2 signal emission so that all `GameSignal` variants —
    /// including `PhaseChanged`, `CounterChanged`, `TimerExpires`, etc. — are
    /// available for `start_trigger` / `end_trigger` matching.
    fn check_shield_triggers(&self, signals: &[GameSignal], cache: &mut SessionCache) {
        let Some(enc) = cache.current_encounter() else {
            return;
        };
        let Some(def_idx) = enc.active_boss_idx() else {
            return;
        };
        let difficulty = enc.difficulty;
        let boss_def = &enc.boss_definitions()[def_idx];
        let entities = boss_def.entities.as_slice();

        // Collect all shield state changes before mutating.
        // Each item is either:
        //   ShieldChange::Activate(npc_log_id, entity_name, shield_idx, effective_total)
        //   ShieldChange::Deactivate(npc_log_id, entity_name, shield_idx)
        //   ShieldChange::Absorb(npc_log_id, amount)
        enum ShieldChange {
            Activate(i64, String, usize, i64),
            Deactivate(i64, String, usize),
            Absorb(i64, i64),
        }

        // Snapshot active shield keys before mutation so end triggers can scan them.
        let active_shield_keys: Vec<(i64, String, usize)> = cache
            .current_encounter()
            .map(|enc| enc.boss_shields.keys().cloned().collect())
            .unwrap_or_default();

        let mut changes: Vec<ShieldChange> = Vec::new();

        for signal in signals {
            match signal {
                // ── Damage absorption: deplete shields on the specific NPC instance ──
                GameSignal::DamageTaken {
                    target_id,
                    target_npc_id,
                    target_entity_type,
                    absorbed,
                    ..
                } if *absorbed > 0
                    && *target_npc_id != 0
                    && *target_entity_type == EntityType::Npc =>
                {
                    changes.push(ShieldChange::Absorb(*target_id, *absorbed as i64));
                }

                _ => {
                    for entity in entities {
                        for (shield_idx, shield) in entity.shields.iter().enumerate() {
                            // ── Start trigger: activate on the specific NPC instance ──
                            if shield_signal_matches(&shield.start_trigger, signal, entities) {
                                if let Some(log_id) = signal_npc_log_id(signal) {
                                    let total = shield.effective_total(difficulty);
                                    changes.push(ShieldChange::Activate(
                                        log_id,
                                        entity.name.clone(),
                                        shield_idx,
                                        total,
                                    ));
                                }
                            }

                            // ── End trigger: deactivate active instances of this shield ──
                            // Scoped to the entity definition whose end_trigger matched,
                            // so "Melee Add : slot 0" and "Ranged Add : slot 0" are independent
                            // and one firing its end trigger does not deactivate the other.
                            // Phase/counter/non-NPC triggers still correctly deactivate all
                            // instances of the matching entity's shields because those trigger
                            // types iterate all entities in the outer loop independently.
                            if shield_signal_matches(&shield.end_trigger, signal, entities) {
                                for (log_id, entity_name, idx) in &active_shield_keys {
                                    if *idx == shield_idx && entity_name == &entity.name {
                                        changes.push(ShieldChange::Deactivate(
                                            *log_id,
                                            entity_name.clone(),
                                            *idx,
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Apply all collected changes
        if changes.is_empty() {
            return;
        }
        let Some(enc) = cache.current_encounter_mut() else {
            return;
        };
        for change in changes {
            match change {
                ShieldChange::Activate(npc_id, entity_name, idx, total) => {
                    enc.activate_shield(npc_id, &entity_name, idx, total);
                }
                ShieldChange::Deactivate(npc_id, entity_name, idx) => {
                    enc.deactivate_shield(npc_id, &entity_name, idx);
                }
                ShieldChange::Absorb(npc_id, amount) => {
                    enc.absorb_shield_damage(npc_id, amount);
                }
            }
        }
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
        out: &mut Vec<GameSignal>,
    ) {
        match event.effect.effect_id {
            effect_id::TARGETSET => {
                out.push(GameSignal::TargetChanged {
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
                out.push(GameSignal::TargetCleared {
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

        // B) Battle rez interrupted or canceled — clear pending flag
        if (eid == effect_id::ABILITYINTERRUPT || eid == effect_id::ABILITYCANCEL)
            && BATTLE_REZ_ABILITY_IDS.contains(&event.action.action_id)
        {
            if let Some(enc) = cache.current_encounter_mut() {
                if enc.battle_rez_pending {
                    enc.battle_rez_pending = false;
                    tracing::debug!(
                        "[BATTLE_REZ] Battle rez canceled/interrupted at {}",
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
    fn emit_effect_signals(&self, event: &CombatEvent, out: &mut Vec<GameSignal>) {
        match event.effect.type_id {
            effect_type_id::APPLYEFFECT => {
                if event.target_entity.entity_type == EntityType::Empty {
                    return;
                }
                let charges = if event.details.charges > 0 {
                    Some(correct_apply_charges(
                        event.effect.effect_id,
                        event.details.charges as u8,
                    ))
                } else {
                    None
                };
                out.push(GameSignal::EffectApplied {
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
                });
            }
            effect_type_id::REMOVEEFFECT => {
                if event.target_entity.entity_type == EntityType::Empty {
                    return;
                }
                out.push(GameSignal::EffectRemoved {
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
                });
            }
            effect_type_id::MODIFYCHARGES => {
                if event.target_entity.entity_type == EntityType::Empty {
                    return;
                }
                out.push(GameSignal::EffectChargesChanged {
                    effect_id: event.effect.effect_id,
                    effect_name: event.effect.effect_name,
                    action_id: event.action.action_id,
                    action_name: event.action.name,
                    source_id: event.source_entity.log_id,
                    source_entity_type: event.source_entity.entity_type,
                    source_name: event.source_entity.name,
                    source_npc_id: event.source_entity.class_id,
                    target_id: event.target_entity.log_id,
                    target_entity_type: event.target_entity.entity_type,
                    target_name: event.target_entity.name,
                    target_npc_id: event.target_entity.class_id,
                    timestamp: event.timestamp,
                    charges: event.details.charges as u8,
                });
            }
            _ => {}
        }
    }

    /// Emit signals for ability activations and target changes.
    /// Resolves the actual target from encounter state when the combat log
    /// reports self-targeting (which SWTOR does for most AbilityActivate events).
    fn emit_action_signals(
        &self,
        event: &CombatEvent,
        cache: &SessionCache,
        out: &mut Vec<GameSignal>,
    ) {
        let eid = event.effect.effect_id;

        // Ability activation
        if eid == effect_id::ABILITYACTIVATE {
            let source_id = event.source_entity.log_id;
            let raw_target_id = event.target_entity.log_id;

            // Resolve actual target: SWTOR reports self as target for most abilities.
            // When target == source or target == 0, look up the caster's real target
            // from the encounter's tracked target state.
            let is_self_or_empty = raw_target_id == 0 || raw_target_id == source_id;
            let (target_id, target_entity_type, target_name, target_npc_id) = if is_self_or_empty {
                if let Some(resolved_id) = cache
                    .current_encounter()
                    .and_then(|e| e.get_current_target(source_id))
                {
                    let enc = cache.current_encounter().unwrap();
                    if let Some(player) = enc.players.get(&resolved_id) {
                        (resolved_id, EntityType::Player, player.name, 0)
                    } else if let Some(npc) = enc.npcs.get(&resolved_id) {
                        (resolved_id, npc.entity_type, npc.name, npc.class_id)
                    } else {
                        // Target exists but not in our roster — use source info as fallback
                        (
                            source_id,
                            event.source_entity.entity_type,
                            event.source_entity.name,
                            event.source_entity.class_id,
                        )
                    }
                } else {
                    // No encounter or no target tracked — default to self
                    (
                        source_id,
                        event.source_entity.entity_type,
                        event.source_entity.name,
                        event.source_entity.class_id,
                    )
                }
            } else {
                (
                    raw_target_id,
                    event.target_entity.entity_type,
                    event.target_entity.name,
                    event.target_entity.class_id,
                )
            };

            out.push(GameSignal::AbilityActivated {
                ability_id: event.action.action_id,
                ability_name: event.action.name,
                source_id,
                source_entity_type: event.source_entity.entity_type,
                source_name: event.source_entity.name,
                source_npc_id: event.source_entity.class_id,
                target_id,
                target_entity_type,
                target_name,
                target_npc_id,
                timestamp: event.timestamp,
            });
        }
    }

    /// Emit signals for damage events (tank buster detection, raid-wide damage, etc.).
    /// Pure transformation - no encounter state modification.
    fn emit_damage_signals(&self, event: &CombatEvent, out: &mut Vec<GameSignal>) {
        // Only emit for damage during APPLYEFFECT
        if event.effect.type_id != effect_type_id::APPLYEFFECT
            || event.effect.effect_id != effect_id::DAMAGE
        {
            return;
        }

        // Ensure we have valid source and target
        if event.source_entity.entity_type == EntityType::Empty
            || event.target_entity.entity_type == EntityType::Empty
        {
            return;
        }

        out.push(GameSignal::DamageTaken {
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
            absorbed: event.details.dmg_absorbed,
            defense_type_id: event.details.defense_type_id,
        });
    }

    /// Emit signals for healing events (for effect refresh on heal completion).
    /// Pure transformation - no encounter state modification.
    fn emit_healing_signals(&self, event: &CombatEvent, out: &mut Vec<GameSignal>) {
        // Only emit for heals during APPLYEFFECT
        if event.effect.type_id != effect_type_id::APPLYEFFECT
            || event.effect.effect_id != effect_id::HEAL
        {
            return;
        }

        // Ensure we have valid source and target
        if event.source_entity.entity_type == EntityType::Empty
            || event.target_entity.entity_type == EntityType::Empty
        {
            return;
        }

        out.push(GameSignal::HealingDone {
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

    /// Emit signals for threat modification events (MODIFYTHREAT and TAUNT).
    /// Pure transformation - no encounter state modification.
    fn emit_threat_signals(&self, event: &CombatEvent, out: &mut Vec<GameSignal>) {
        // MODIFYTHREAT and TAUNT are EVENT type events distinguished by effect_id
        if event.effect.type_id != effect_type_id::EVENT {
            return;
        }
        if event.effect.effect_id != effect_id::MODIFYTHREAT
            && event.effect.effect_id != effect_id::TAUNT
        {
            return;
        }

        if event.source_entity.entity_type == EntityType::Empty
            || event.target_entity.entity_type == EntityType::Empty
        {
            return;
        }

        out.push(GameSignal::ThreatModified {
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
}

// ═══════════════════════════════════════════════════════════════════════════
// Victory Trigger Checking
// ═══════════════════════════════════════════════════════════════════════════

// Victory trigger evaluation now uses the unified trigger_eval functions.
// See the call site in handle_victory_trigger() above.

// ═══════════════════════════════════════════════════════════════════════════
// Shield Signal Matching
// ═══════════════════════════════════════════════════════════════════════════

/// Extract the NPC instance log_id from a signal — the specific NPC this signal is "about".
///
/// Used by the shield system to key activation/deactivation against the correct NPC instance
/// rather than the class/template ID, so multiple NPCs of the same type have independent state.
///
/// Returns `None` for signals that aren't tied to a specific NPC instance (e.g. phase/counter changes).
fn signal_npc_log_id(signal: &GameSignal) -> Option<i64> {
    match signal {
        // Effect signals: the NPC is the target
        GameSignal::EffectApplied {
            target_id,
            target_entity_type,
            ..
        }
        | GameSignal::EffectRemoved {
            target_id,
            target_entity_type,
            ..
        }
        | GameSignal::EffectChargesChanged {
            target_id,
            target_entity_type,
            ..
        } if *target_entity_type == EntityType::Npc => Some(*target_id),
        // Ability/damage/heal: source NPC (the one casting/dealing)
        GameSignal::AbilityActivated {
            source_id,
            source_entity_type,
            ..
        }
        | GameSignal::DamageTaken {
            source_id,
            source_entity_type,
            ..
        }
        | GameSignal::HealingDone {
            source_id,
            source_entity_type,
            ..
        } if *source_entity_type == EntityType::Npc => Some(*source_id),
        // NPC lifecycle
        GameSignal::NpcFirstSeen { entity_id, .. } => Some(*entity_id),
        GameSignal::EntityDeath {
            entity_id,
            entity_type,
            ..
        } if *entity_type == EntityType::Npc => Some(*entity_id),
        // BossHpChanged: the boss NPC
        GameSignal::BossHpChanged { entity_id, .. } => Some(*entity_id),
        // Phase/counter/timer/combat signals are not instance-specific
        _ => None,
    }
}

/// Check whether a shield `Trigger` matches a given `GameSignal`.
///
/// This is the shield-specific counterpart to the timer signal dispatch in
/// `timers/signal_handlers.rs`. It evaluates the full `Trigger` enum against
/// the emitted signal vec, making all signal-producing variants available.
///
/// `CombatStart` and `TimeElapsed` are intentionally excluded — they produce no
/// signal and are caught by `validate_triggers()` at config load time.
fn shield_signal_matches(
    trigger: &crate::dsl::Trigger,
    signal: &GameSignal,
    entities: &[crate::dsl::EntityDefinition],
) -> bool {
    use crate::context::resolve;
    use crate::dsl::Trigger;

    match trigger {
        Trigger::AnyOf { conditions } => conditions
            .iter()
            .any(|c| shield_signal_matches(c, signal, entities)),

        Trigger::EffectApplied { effects, .. } => {
            if let GameSignal::EffectApplied {
                effect_id,
                effect_name,
                ..
            } = signal
            {
                let name = resolve(*effect_name);
                !effects.is_empty()
                    && effects
                        .iter()
                        .any(|s| s.matches(*effect_id as u64, Some(name)))
            } else {
                false
            }
        }

        Trigger::EffectRemoved { effects, .. } => {
            if let GameSignal::EffectRemoved {
                effect_id,
                effect_name,
                ..
            } = signal
            {
                let name = resolve(*effect_name);
                !effects.is_empty()
                    && effects
                        .iter()
                        .any(|s| s.matches(*effect_id as u64, Some(name)))
            } else {
                false
            }
        }

        Trigger::AbilityCast { abilities, .. } => {
            if let GameSignal::AbilityActivated {
                ability_id,
                ability_name,
                ..
            } = signal
            {
                let name = resolve(*ability_name);
                !abilities.is_empty()
                    && abilities
                        .iter()
                        .any(|s| s.matches(*ability_id as u64, Some(name)))
            } else {
                false
            }
        }

        Trigger::DamageTaken { .. } => {
            if let GameSignal::DamageTaken {
                ability_id,
                ability_name,
                defense_type_id,
                ..
            } = signal
            {
                let name = resolve(*ability_name);
                trigger.matches_damage_taken(*ability_id as u64, Some(name), *defense_type_id)
            } else {
                false
            }
        }

        Trigger::HealingTaken { abilities, .. } => {
            if let GameSignal::HealingDone {
                ability_id,
                ability_name,
                ..
            } = signal
            {
                let name = resolve(*ability_name);
                !abilities.is_empty()
                    && abilities
                        .iter()
                        .any(|s| s.matches(*ability_id as u64, Some(name)))
            } else {
                false
            }
        }

        Trigger::ThreatModified { .. } => {
            if let GameSignal::ThreatModified {
                ability_id,
                ability_name,
                ..
            } = signal
            {
                let name = resolve(*ability_name);
                trigger.matches_threat_modified(*ability_id as u64, Some(name))
            } else {
                false
            }
        }

        Trigger::NpcAppears { selector } => {
            if let GameSignal::NpcFirstSeen {
                npc_id,
                entity_name,
                ..
            } = signal
            {
                !selector.is_empty()
                    && selector.matches_with_roster(entities, *npc_id, Some(entity_name.as_str()))
            } else {
                false
            }
        }

        Trigger::EntityDeath { selector } => {
            if let GameSignal::EntityDeath {
                npc_id,
                entity_name,
                ..
            } = signal
            {
                if selector.is_empty() {
                    return true;
                }
                selector.matches_with_roster(entities, *npc_id, Some(entity_name.as_str()))
            } else {
                false
            }
        }

        Trigger::BossHpBelow {
            hp_percent,
            selector,
        } => {
            if let GameSignal::BossHpChanged {
                npc_id,
                entity_name,
                old_hp_percent,
                new_hp_percent,
                ..
            } = signal
            {
                let crossed =
                    *old_hp_percent > *hp_percent && *new_hp_percent <= *hp_percent + 0.01;
                if !crossed {
                    return false;
                }
                if selector.is_empty() {
                    return true;
                }
                selector.matches_with_roster(entities, *npc_id, Some(entity_name.as_str()))
            } else {
                false
            }
        }

        Trigger::BossHpAbove {
            hp_percent,
            selector,
        } => {
            if let GameSignal::BossHpChanged {
                npc_id,
                entity_name,
                old_hp_percent,
                new_hp_percent,
                ..
            } = signal
            {
                let crossed =
                    *old_hp_percent < *hp_percent && *new_hp_percent >= *hp_percent - 0.01;
                if !crossed {
                    return false;
                }
                if selector.is_empty() {
                    return true;
                }
                selector.matches_with_roster(entities, *npc_id, Some(entity_name.as_str()))
            } else {
                false
            }
        }

        Trigger::PhaseEntered { phase_id } => {
            matches!(signal, GameSignal::PhaseChanged { new_phase, .. } if new_phase == phase_id)
        }

        Trigger::PhaseEnded { phase_id } => {
            matches!(signal, GameSignal::PhaseEndTriggered { phase_id: pid, .. } if pid == phase_id)
        }

        Trigger::AnyPhaseChange => {
            matches!(
                signal,
                GameSignal::PhaseChanged { .. } | GameSignal::PhaseEndTriggered { .. }
            )
        }

        Trigger::CounterReaches { counter_id, value } => {
            matches!(
                signal,
                GameSignal::CounterChanged { counter_id: cid, old_value, new_value, .. }
                    if cid == counter_id && new_value == value && old_value != value
            )
        }

        Trigger::CounterChanges { counter_id } => {
            matches!(signal, GameSignal::CounterChanged { counter_id: cid, .. } if cid == counter_id)
        }

        Trigger::TimerExpires { timer_id } => {
            // TimerExpires/Started/Canceled have no corresponding GameSignal currently;
            // they are only evaluated in the timer system. Always false for shields.
            let _ = timer_id;
            false
        }

        Trigger::TimerStarted { timer_id } => {
            let _ = timer_id;
            false
        }

        Trigger::TimerCanceled { timer_id } => {
            let _ = timer_id;
            false
        }

        Trigger::TargetSet { selector, .. } => {
            if let GameSignal::TargetChanged {
                source_npc_id,
                source_name,
                ..
            } = signal
            {
                if selector.is_empty() {
                    return false;
                }
                let name = resolve(*source_name);
                selector.matches_with_roster(entities, *source_npc_id, Some(name))
            } else {
                false
            }
        }

        Trigger::CombatStart | Trigger::TimeElapsed { .. } => false,
        Trigger::CombatEnd => matches!(signal, GameSignal::CombatEnded { .. }),
        Trigger::Manual | Trigger::Never => false,
    }
}
