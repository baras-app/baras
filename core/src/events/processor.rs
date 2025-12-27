use crate::boss::EntityInfo;
use crate::combat_log::{CombatEvent, EntityType};
use crate::context::resolve;
use crate::events::signal::GameSignal;
use crate::state::cache::SessionCache;
use crate::encounter::EncounterState;
use crate::encounter::entity_info::PlayerInfo;
use crate::game_data::{effect_id, effect_type_id, correct_apply_charges};
use crate::boss::PhaseTrigger;
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

        // ═══════════════════════════════════════════════════════════════════════
        // PHASE 2: Signal Emission (pure transformation)
        // ═══════════════════════════════════════════════════════════════════════

        signals.extend(self.emit_effect_signals(&event));
        signals.extend(self.emit_action_signals(&event));

        // Check if current phase's end_trigger fired (emits PhaseEndTriggered signal)
        signals.extend(self.check_phase_end_triggers(&event, cache, &signals));

        // Check for ability/effect-based phase transitions (can now match PhaseEnded)
        signals.extend(self.check_ability_phase_transitions(&event, cache, &signals));

        // Check for entity-based phase transitions (EntityFirstSeen, EntityDeath, PhaseEnded)
        signals.extend(self.check_entity_phase_transitions(cache, &signals, event.timestamp));

        // Check for counter increments based on events and signals
        signals.extend(self.check_counter_increments(&event, cache, &signals));

        // Process challenge metrics (accumulates values, polled with combat data)
        self.process_challenge_events(&event, cache);

        // ═══════════════════════════════════════════════════════════════════════
        // PHASE 3: Combat State Machine
        // ═══════════════════════════════════════════════════════════════════════

        signals.extend(self.advance_combat_state(event, cache));

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

    // ═══════════════════════════════════════════════════════════════════════════
    // Phase 1: Global Event Handlers
    // ═══════════════════════════════════════════════════════════════════════════

    /// Handle DisciplineChanged events for player initialization and role detection.
    fn handle_discipline_event(&self, event: &CombatEvent, cache: &mut SessionCache) -> Vec<GameSignal> {
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

        // Track player in encounter
        self.add_player_to_encounter(event, cache);

        // Emit DisciplineChanged for ALL players (used for raid frame role detection)
        if event.effect.discipline_id != 0 {
            signals.push(GameSignal::DisciplineChanged {
                entity_id: event.source_entity.log_id,
                discipline_id: event.effect.discipline_id,
                timestamp: event.timestamp,
            });
        }

        signals
    }

    /// Handle Death and Revive events.
    fn handle_entity_lifecycle(&self, event: &CombatEvent, cache: &mut SessionCache) -> Vec<GameSignal> {
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
                enc.set_entity_alive(
                    event.source_entity.log_id,
                    &event.source_entity.entity_type,
                );
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
    fn handle_area_transition(&self, event: &CombatEvent, cache: &mut SessionCache) -> Vec<GameSignal> {
        if event.effect.type_id != effect_type_id::AREAENTERED {
            return Vec::new();
        }

        self.update_area_from_event(event, cache);

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
    fn handle_npc_first_seen(&self, event: &CombatEvent, cache: &mut SessionCache) -> Vec<GameSignal> {
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
                    entity_id: entity.log_id,      // Unique instance
                    npc_id: entity.class_id,       // NPC type for timer matching
                    entity_name: resolve(entity.name).to_string(),
                    timestamp: event.timestamp,
                });
            }
        }

        signals
    }

    /// Detect boss encounters based on NPC class IDs.
    /// When a known boss NPC is first seen in combat, activates the encounter.
    fn handle_boss_detection(&self, event: &CombatEvent, cache: &mut SessionCache) -> Vec<GameSignal> {
        // Already tracking a boss encounter
        if cache.active_boss_idx.is_some() {
            return Vec::new();
        }

        // No boss definitions loaded for this area
        if cache.boss_definitions.is_empty() {
            return Vec::new();
        }

        // Check source and target entities for boss NPC match
        let entities_to_check = [&event.source_entity, &event.target_entity];

        for entity in entities_to_check {
            // Only check NPCs
            if entity.entity_type != EntityType::Npc || entity.class_id == 0 {
                continue;
            }

            // Try to detect boss encounter from this NPC
            if let Some(idx) = cache.detect_boss_encounter(entity.class_id) {
                // Clone data from definition before taking mutable borrows
                let def = &cache.boss_definitions[idx];
                let challenges = def.challenges.clone();
                let npc_ids: Vec<i64> = def.boss_npc_ids().collect();
                let def_id = def.id.clone();
                let boss_name = def.name.clone();
                let initial_phase = def.initial_phase().cloned();

                // Start combat timer in boss state
                cache.boss_state.start_combat(event.timestamp);

                // Start challenge tracker on the encounter (persists with encounter, not boss state)
                // Also set initial phase on challenge tracker for duration tracking
                if let Some(enc) = cache.current_encounter_mut() {
                    enc.challenge_tracker.start(challenges, npc_ids);
                    if let Some(ref initial) = initial_phase {
                        enc.challenge_tracker.set_phase(&initial.id, event.timestamp);
                    }
                }

                let mut signals = vec![GameSignal::BossEncounterDetected {
                    definition_id: def_id.clone(),
                    boss_name,
                    definition_idx: idx,
                    entity_id: entity.log_id,
                    npc_id: entity.class_id,
                    timestamp: event.timestamp,
                }];

                // Activate initial phase (CombatStart trigger)
                if let Some(ref initial) = initial_phase {
                    cache.boss_state.set_phase(&initial.id, event.timestamp);
                    cache.boss_state.reset_counters(&initial.resets_counters);

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
    fn handle_boss_hp_and_phases(&self, event: &CombatEvent, cache: &mut SessionCache) -> Vec<GameSignal> {
        // No active boss encounter
        let Some(def_idx) = cache.active_boss_idx else {
            return Vec::new();
        };

        let mut signals = Vec::new();

        // Update HP for entities that are boss NPCs
        for entity in [&event.source_entity, &event.target_entity] {
            if entity.entity_type != EntityType::Npc || entity.class_id == 0 {
                continue;
            }

            // Check if this NPC is part of the active boss encounter (boss entity)
            let def = &cache.boss_definitions[def_idx];
            if !def.matches_npc_id(entity.class_id) {
                continue;
            }

            let (current_hp, max_hp) = (entity.health.0 as i64, entity.health.1 as i64);
            if max_hp <= 0 {
                continue;
            }

            // Update boss state and check if HP changed
            if let Some((old_hp, new_hp)) = cache.boss_state.update_entity_hp(
                entity.log_id,
                entity.class_id,
                resolve(entity.name),
                current_hp,
                max_hp,
                event.timestamp,
            ) {
                // NpcFirstSeen is now emitted globally in handle_npc_first_seen
                signals.push(GameSignal::BossHpChanged {
                    entity_id: entity.log_id,
                    npc_id: entity.class_id,
                    entity_name: resolve(entity.name).to_string(),
                    current_hp,
                    max_hp,
                    timestamp: event.timestamp,
                });

                // Check for HP-based phase transitions
                signals.extend(self.check_hp_phase_transitions(cache, old_hp, new_hp, entity.class_id, event.timestamp));
            }
        }

        signals
    }

    /// Check for phase transitions based on HP changes.
    fn check_hp_phase_transitions(
        &self,
        cache: &mut SessionCache,
        old_hp: f32,
        new_hp: f32,
        npc_id: i64,
        timestamp: chrono::NaiveDateTime,
    ) -> Vec<GameSignal> {
        let Some(def_idx) = cache.active_boss_idx else {
            return Vec::new();
        };

        let def = &cache.boss_definitions[def_idx];
        let current_phase = cache.boss_state.current_phase.clone();
        let previous_phase = cache.boss_state.previous_phase.clone();

        for phase in &def.phases {
            // Don't re-enter the current phase
            if current_phase.as_ref() == Some(&phase.id) {
                continue;
            }

            // Check preceded_by guard (e.g., walker_2 requires prior phase to be kephess_1)
            // Use current_phase if set, otherwise fall back to previous_phase for transition periods
            if let Some(ref required) = phase.preceded_by {
                let last_phase = current_phase.as_ref().or(previous_phase.as_ref());
                if last_phase != Some(required) {
                    continue;
                }
            }

            if self.check_hp_trigger(&phase.start_trigger, old_hp, new_hp, npc_id, &cache.boss_state) {
                let old_phase = cache.boss_state.current_phase.clone();
                let new_phase_id = phase.id.clone();
                let boss_id = def.id.clone();
                let resets = phase.resets_counters.clone();

                cache.boss_state.set_phase(&new_phase_id, timestamp);
                cache.boss_state.reset_counters(&resets);

                // Update challenge tracker phase for duration tracking
                if let Some(enc) = cache.current_encounter_mut() {
                    enc.challenge_tracker.set_phase(&new_phase_id, timestamp);
                }

                return vec![GameSignal::PhaseChanged {
                    boss_id,
                    old_phase,
                    new_phase: new_phase_id,
                    timestamp,
                }];
            }
        }

        Vec::new()
    }

    /// Check if an HP-based phase trigger is satisfied.
    fn check_hp_trigger(
        &self,
        trigger: &crate::boss::PhaseTrigger,
        old_hp: f32,
        new_hp: f32,
        npc_id: i64,
        state: &crate::boss::BossEncounterState,
    ) -> bool {
        use crate::boss::PhaseTrigger;

        match trigger {
            PhaseTrigger::BossHpBelow { hp_percent, npc_id: trigger_npc, boss_name, .. } => {
                // Check if we crossed below the threshold
                let crossed = old_hp > *hp_percent && new_hp <= *hp_percent;
                if !crossed {
                    return false;
                }

                // TODO: Support entity reference resolution
                // For now, entity references need to be resolved at load time

                // Check NPC filter
                if let Some(required_npc) = trigger_npc {
                    return npc_id == *required_npc;
                }

                // Check name filter (fallback)
                if let Some(name) = boss_name {
                    return state.hp_by_name.contains_key(name);
                }

                // No filter - any boss crossing threshold triggers
                true
            }
            PhaseTrigger::BossHpAbove { hp_percent, npc_id: trigger_npc, boss_name, .. } => {
                // Check if we crossed above the threshold
                let crossed = old_hp < *hp_percent && new_hp >= *hp_percent;
                if !crossed {
                    return false;
                }

                // TODO: Support entity reference resolution

                // Check NPC filter
                if let Some(required_npc) = trigger_npc {
                    return npc_id == *required_npc;
                }

                // Check name filter (fallback)
                if let Some(name) = boss_name {
                    return state.hp_by_name.contains_key(name);
                }

                true
            }
            PhaseTrigger::AnyOf { conditions } => {
                conditions.iter().any(|c| self.check_hp_trigger(c, old_hp, new_hp, npc_id, state))
            }
            _ => false, // Other triggers not HP-related
        }
    }

    /// Check if the current phase's end_trigger fired.
    /// Emits PhaseEndTriggered signal which other phases can use as a start_trigger.
    fn check_phase_end_triggers(&self, event: &CombatEvent, cache: &SessionCache, current_signals: &[GameSignal]) -> Vec<GameSignal> {
        let Some(def_idx) = cache.active_boss_idx else {
            return Vec::new();
        };
        let Some(current_phase_id) = &cache.boss_state.current_phase else {
            return Vec::new();
        };

        let def = &cache.boss_definitions[def_idx];

        // Find the current phase definition
        let Some(phase) = def.phases.iter().find(|p| &p.id == current_phase_id) else {
            return Vec::new();
        };

        // Check if end_trigger is defined and matches
        let Some(ref end_trigger) = phase.end_trigger else {
            return Vec::new();
        };

        // Check ability/effect triggers
        if self.check_ability_trigger(end_trigger, event) {
            return vec![GameSignal::PhaseEndTriggered {
                phase_id: current_phase_id.clone(),
                timestamp: event.timestamp,
            }];
        }

        // Check signal-based triggers (EntityFirstSeen, EntityDeath)
        if self.check_signal_phase_trigger(end_trigger, current_signals) {
            return vec![GameSignal::PhaseEndTriggered {
                phase_id: current_phase_id.clone(),
                timestamp: event.timestamp,
            }];
        }

        Vec::new()
    }

    /// Check for phase transitions based on ability/effect events.
    fn check_ability_phase_transitions(&self, event: &CombatEvent, cache: &mut SessionCache, current_signals: &[GameSignal]) -> Vec<GameSignal> {
        let Some(def_idx) = cache.active_boss_idx else {
            return Vec::new();
        };

        let def = &cache.boss_definitions[def_idx];
        let current_phase = cache.boss_state.current_phase.clone();
        let previous_phase = cache.boss_state.previous_phase.clone();

        for phase in &def.phases {
            // Don't re-enter the current phase
            if current_phase.as_ref() == Some(&phase.id) {
                continue;
            }

            // Check preceded_by guard (e.g., walker_2 requires prior phase to be kephess_1)
            // Use current_phase if set, otherwise fall back to previous_phase for transition periods
            if let Some(ref required) = phase.preceded_by {
                let last_phase = current_phase.as_ref().or(previous_phase.as_ref());
                if last_phase != Some(required) {
                    continue;
                }
            }

            // Check ability/effect triggers OR signal-based triggers (including PhaseEnded)
            let trigger_matched = self.check_ability_trigger(&phase.start_trigger, event)
                || self.check_signal_phase_trigger(&phase.start_trigger, current_signals);

            if trigger_matched {
                let old_phase = cache.boss_state.current_phase.clone();
                let new_phase_id = phase.id.clone();
                let boss_id = def.id.clone();
                let resets = phase.resets_counters.clone();

                cache.boss_state.set_phase(&new_phase_id, event.timestamp);
                cache.boss_state.reset_counters(&resets);

                // Update challenge tracker phase for duration tracking
                if let Some(enc) = cache.current_encounter_mut() {
                    enc.challenge_tracker.set_phase(&new_phase_id, event.timestamp);
                }

                return vec![GameSignal::PhaseChanged {
                    boss_id,
                    old_phase,
                    new_phase: new_phase_id,
                    timestamp: event.timestamp,
                }];
            }
        }

        Vec::new()
    }

    /// Check if an ability/effect-based phase trigger is satisfied.
    fn check_ability_trigger(&self, trigger: &crate::boss::PhaseTrigger, event: &CombatEvent) -> bool {
        use crate::boss::PhaseTrigger;

        match trigger {
            PhaseTrigger::AbilityCast { ability_ids } => {
                // Check if this is an ability activation event with matching ID
                if event.effect.effect_id != effect_id::ABILITYACTIVATE {
                    return false;
                }
                ability_ids.contains(&(event.action.action_id as u64))
            }
            PhaseTrigger::EffectApplied { effect_ids } => {
                if event.effect.type_id != effect_type_id::APPLYEFFECT {
                    return false;
                }
                effect_ids.contains(&(event.effect.effect_id as u64))
            }
            PhaseTrigger::EffectRemoved { effect_ids } => {
                if event.effect.type_id != effect_type_id::REMOVEEFFECT {
                    return false;
                }
                effect_ids.contains(&(event.effect.effect_id as u64))
            }
            PhaseTrigger::AnyOf { conditions } => {
                conditions.iter().any(|c| self.check_ability_trigger(c, event))
            }
            _ => false, // HP, timer, counter, time triggers handled elsewhere
        }
    }

    /// Check if a signal-based phase trigger is satisfied (EntityFirstSeen, EntityDeath).
    /// TODO: Support entity reference resolution (requires passing in definition)
    fn check_signal_phase_trigger(&self, trigger: &crate::boss::PhaseTrigger, signals: &[GameSignal]) -> bool {

        match trigger {
            PhaseTrigger::EntityFirstSeen { npc_id, entity_name, .. } => {
                // TODO: Support entity reference resolution
                signals.iter().any(|s| {
                    if let GameSignal::NpcFirstSeen { npc_id: sig_npc_id, entity_name: sig_name, .. } = s {
                        // Check NPC ID filter (preferred)
                        if let Some(required_id) = npc_id {
                            return sig_npc_id == required_id;
                        }
                        // Check name filter (fallback)
                        if let Some(required_name) = entity_name {
                            return sig_name.contains(required_name);
                        }
                        // No filter specified
                        false
                    } else {
                        false
                    }
                })
            }
            PhaseTrigger::EntityDeath { npc_id, entity_name, .. } => {
                signals.iter().any(|s| {
                    if let GameSignal::EntityDeath { npc_id: sig_npc_id, entity_name: sig_name, .. } = s {
                        // Check NPC ID filter
                        if let Some(required_id) = npc_id
                           && sig_npc_id != required_id {
                                return false;

                        }
                        // Check name filter
                        if let Some(required_name) = entity_name
                            && !required_name.eq_ignore_ascii_case(sig_name) {
                                return false;
                        }
                        true
                    } else {
                        false
                    }
                })
            }
            PhaseTrigger::PhaseEnded { phase_id, phase_ids } => {
                signals.iter().any(|s| {
                    if let GameSignal::PhaseEndTriggered { phase_id: sig_phase_id, .. } = s {
                        // Check single phase_id
                        if let Some(required) = phase_id {
                            if sig_phase_id == required {
                                return true;
                            }
                        }
                        // Check phase_ids list
                        if phase_ids.iter().any(|p| p == sig_phase_id) {
                            return true;
                        }
                        false
                    } else {
                        false
                    }
                })
            }
            PhaseTrigger::AnyOf { conditions } => {
                conditions.iter().any(|c| self.check_signal_phase_trigger(c, signals))
            }
            _ => false,
        }
    }

    /// Check for phase transitions based on entity signals (EntityFirstSeen, EntityDeath).
    fn check_entity_phase_transitions(&self, cache: &mut SessionCache, current_signals: &[GameSignal], timestamp: chrono::NaiveDateTime) -> Vec<GameSignal> {
        let Some(def_idx) = cache.active_boss_idx else {
            return Vec::new();
        };

        // Clone what we need before mutable borrow
        let phases: Vec<_> = cache.boss_definitions[def_idx].phases.clone();
        let boss_id = cache.boss_definitions[def_idx].id.clone();
        let current_phase = cache.boss_state.current_phase.clone();
        let previous_phase = cache.boss_state.previous_phase.clone();

        let mut signals = Vec::new();

        for phase in &phases {
            // Don't re-enter the current phase
            if current_phase.as_ref() == Some(&phase.id) {
                continue;
            }

            // Check preceded_by guard (e.g., walker_2 requires prior phase to be kephess_1)
            // Use current_phase if set, otherwise fall back to previous_phase for transition periods
            if let Some(ref required) = phase.preceded_by {
                let last_phase = current_phase.as_ref().or(previous_phase.as_ref());
                if last_phase != Some(required) {
                    continue;
                }
            }

            if self.check_signal_phase_trigger(&phase.start_trigger, current_signals) {
                let old_phase = cache.boss_state.current_phase.clone();
                let new_phase_id = phase.id.clone();
                let resets = phase.resets_counters.clone();

                cache.boss_state.set_phase(&new_phase_id, timestamp);
                cache.boss_state.reset_counters(&resets);

                // Update challenge tracker phase for duration tracking
                if let Some(enc) = cache.current_encounter_mut() {
                    enc.challenge_tracker.set_phase(&new_phase_id, timestamp);
                }

                signals.push(GameSignal::PhaseChanged {
                    boss_id: boss_id.clone(),
                    old_phase,
                    new_phase: new_phase_id,
                    timestamp,
                });

                break; // Only one phase transition per event
            }
        }

        signals
    }

    /// Check for counter increments based on events.
    fn check_counter_increments(
        &self,
        event: &CombatEvent,
        cache: &mut SessionCache,
        current_signals: &[GameSignal],
    ) -> Vec<GameSignal> {
        let Some(def_idx) = cache.active_boss_idx else {
            return Vec::new();
        };

        let def = &cache.boss_definitions[def_idx];
        let mut signals = Vec::new();

        for counter in &def.counters {
            if self.check_counter_trigger(&counter.increment_on, event, current_signals) {
                let old_value = cache.boss_state.get_counter(&counter.id);
                let new_value = cache.boss_state.increment_counter(&counter.id);

                signals.push(GameSignal::CounterChanged {
                    counter_id: counter.id.clone(),
                    old_value,
                    new_value,
                    timestamp: event.timestamp,
                });
            }
        }

        signals
    }

    /// Check if a counter trigger is satisfied.
    fn check_counter_trigger(
        &self,
        trigger: &crate::boss::CounterTrigger,
        event: &CombatEvent,
        current_signals: &[GameSignal],
    ) -> bool {
        use crate::boss::CounterTrigger;

        match trigger {
            CounterTrigger::AbilityCast { ability_ids } => {
                if event.effect.effect_id != effect_id::ABILITYACTIVATE {
                    return false;
                }
                ability_ids.contains(&(event.action.action_id as u64))
            }
            CounterTrigger::EffectApplied { effect_ids } => {
                if event.effect.type_id != effect_type_id::APPLYEFFECT {
                    return false;
                }
                effect_ids.contains(&(event.effect.effect_id as u64))
            }
            CounterTrigger::PhaseEntered { phase_id } => {
                // Check if we emitted a PhaseChanged signal with this phase
                current_signals.iter().any(|s| {
                    matches!(s, GameSignal::PhaseChanged { new_phase, .. } if new_phase == phase_id)
                })
            }
            CounterTrigger::TimerExpires { .. } => {
                // Timer expiration handled by TimerManager
                false
            }
            CounterTrigger::EntityFirstSeen { npc_id, entity_name, .. } => {
                // Check if we emitted an NpcFirstSeen signal for this NPC
                // TODO: Support entity reference resolution
                current_signals.iter().any(|s| {
                    if let GameSignal::NpcFirstSeen { npc_id: sig_npc_id, entity_name: sig_name, .. } = s {
                        // Check NPC ID filter (preferred)
                        if let Some(required_id) = npc_id {
                            return sig_npc_id == required_id;
                        }
                        // Check name filter (fallback)
                        if let Some(required_name) = entity_name {
                            return sig_name.contains(required_name);
                        }
                        false
                    } else {
                        false
                    }
                })
            }
            CounterTrigger::EntityDeath { npc_id, entity_name, .. } => {
                // Check if we emitted an EntityDeath signal matching the filter
                current_signals.iter().any(|s| {
                    if let GameSignal::EntityDeath { npc_id: sig_npc_id, entity_name: sig_name, .. } = s {
                        // Check NPC ID filter
                        if let Some(required_id) = npc_id
                            && sig_npc_id != required_id {
                                return false;
                        }
                        // Check name filter
                        if let Some(required_name) = entity_name
                            && !required_name.eq_ignore_ascii_case(sig_name) {
                                return false;
                        }
                        true
                    } else {
                        false
                    }
                })
            }
        }
    }

    /// Process events through the challenge tracker to accumulate metrics
    /// Challenge data is polled with other combat metrics, not pushed via signals
    fn process_challenge_events(&self, event: &CombatEvent, cache: &mut SessionCache) {
        // Get boss_npc_ids from encounter's tracker (need to extract before mutable borrow)
        let boss_npc_ids = match cache.current_encounter() {
            Some(enc) if enc.challenge_tracker.is_active() => {
                enc.challenge_tracker.boss_npc_ids().to_vec()
            }
            _ => return, // No active challenge tracking
        };

        // Build context from current boss state (phase, counters, HP)
        let ctx = cache.boss_state.challenge_context(&boss_npc_ids);

        // Get local player ID for local_player matching
        let local_player_id = cache.player.id;

        // Convert entities to EntityInfo
        let source = self.entity_to_info(&event.source_entity, local_player_id);
        let target = self.entity_to_info(&event.target_entity, local_player_id);

        // Get mutable access to the encounter's tracker
        let Some(enc) = cache.current_encounter_mut() else { return };
        let tracker = &mut enc.challenge_tracker;

        // Process based on event type - just accumulate, no signals needed
        match event.effect.effect_id {
            effect_id::DAMAGE => {
                let damage = event.details.dmg_effective as i64;
                tracker.process_damage(
                    &ctx,
                    &source,
                    &target,
                    event.action.action_id as u64,
                    damage,
                );
            }
            effect_id::HEAL => {
                let healing = event.details.heal_effective as i64;
                tracker.process_healing(
                    &ctx,
                    &source,
                    &target,
                    event.action.action_id as u64,
                    healing,
                );
            }
            effect_id::ABILITYACTIVATE => {
                tracker.process_ability(
                    &ctx,
                    &source,
                    &target,
                    event.action.action_id as u64,
                );
            }
            effect_id::DEATH => {
                tracker.process_death(&ctx, &target);
            }
            _ => {
                if event.effect.type_id == effect_type_id::APPLYEFFECT {
                    tracker.process_effect_applied(
                        &ctx,
                        &source,
                        &target,
                        event.effect.effect_id as u64,
                    );
                }
            }
        }
    }

    /// Convert a combat log Entity to EntityInfo for challenge matching
    fn entity_to_info(&self, entity: &crate::combat_log::Entity, local_player_id: i64) -> EntityInfo {
        match entity.entity_type {
            EntityType::Player => EntityInfo {
                entity_id: entity.log_id,
                name: resolve(entity.name).to_string(),
                is_player: true,
                is_local_player: entity.log_id == local_player_id,
                npc_id: None,
            },
            EntityType::Npc | EntityType::Companion => EntityInfo {
                entity_id: entity.log_id,
                name: resolve(entity.name).to_string(),
                is_player: false,
                is_local_player: false,
                npc_id: Some(entity.class_id),
            },
            _ => EntityInfo::default(),
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
                    Some(correct_apply_charges(event.effect.effect_id, event.details.charges as u8))
                } else {
                    None
                };
                vec![GameSignal::EffectApplied {
                    effect_id: event.effect.effect_id,
                    action_id: event.action.action_id,
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
                    source_id: event.source_entity.log_id,
                    target_id: event.target_entity.log_id,
                    timestamp: event.timestamp,
                }]
            }
            effect_type_id::MODIFYCHARGES => {
                if event.target_entity.entity_type == EntityType::Empty {
                    return Vec::new();
                }
                vec![GameSignal::EffectChargesChanged {
                    effect_id: event.effect.effect_id,
                    action_id: event.action.action_id,
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
                source_id: event.source_entity.log_id,
                source_npc_id: event.source_entity.class_id,
                target_id: event.target_entity.log_id,
                target_name: event.target_entity.name,
                target_entity_type: event.target_entity.entity_type,
                target_npc_id: event.target_entity.class_id,
                timestamp: event.timestamp,
            });
        }

        // Target changes
        if effect_id == effect_id::TARGETSET {
            signals.push(GameSignal::TargetChanged {
                source_id: event.source_entity.log_id,
                target_id: event.target_entity.log_id,
                target_name: event.target_entity.name,
                target_npc_id: event.target_entity.class_id,
                target_entity_type: event.target_entity.entity_type,
                timestamp: event.timestamp,
            });
        } else if effect_id == effect_id::TARGETCLEARED {
            signals.push(GameSignal::TargetCleared {
                source_id: event.source_entity.log_id,
                timestamp: event.timestamp,
            });
        }

        signals
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Phase 3: Combat State Machine
    // ═══════════════════════════════════════════════════════════════════════════

    /// Advance the combat state machine and emit CombatStarted/CombatEnded signals.
    /// This is the critical encounter lifecycle handler.
    fn advance_combat_state(
        &mut self,
        event: CombatEvent,
        cache: &mut SessionCache,
    ) -> Vec<GameSignal> {
        // Track effect applications/removals in the encounter (for shield absorption calculation).
        // Must run before accumulate_data() so shield effects are present when damage is processed.
        self.track_encounter_effects(&event, cache);

        let effect_id = event.effect.effect_id;
        let effect_type_id = event.effect.type_id;
        let timestamp = event.timestamp;

        let current_state = cache
            .current_encounter()
            .map(|e| e.state.clone())
            .unwrap_or_default();

        match current_state {
            EncounterState::NotStarted => {
                self.handle_not_started(event, cache, effect_id, timestamp)
            }
            EncounterState::InCombat => {
                self.handle_in_combat(event, cache, effect_id, effect_type_id, timestamp)
            }
            EncounterState::PostCombat { exit_time } => {
                self.handle_post_combat(event, cache, effect_id, timestamp, exit_time)
            }
        }
    }

    /// Track effect applications/removals in the encounter for shield absorption calculation.
    fn track_encounter_effects(&self, event: &CombatEvent, cache: &mut SessionCache) {
        let Some(enc) = cache.current_encounter_mut() else { return };

        match event.effect.type_id {
            effect_type_id::APPLYEFFECT if event.target_entity.entity_type != EntityType::Empty => {
                enc.apply_effect(event);
            }
            effect_type_id::REMOVEEFFECT if event.source_entity.entity_type != EntityType::Empty => {
                enc.remove_effect(event);
            }
            _ => {}
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Combat State Handlers (handle_not_started, handle_in_combat, handle_post_combat)
    // These contain the critical state machine logic - modify with extreme care!
    // ═══════════════════════════════════════════════════════════════════════════

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
                        // Finalize challenge tracker with phase duration and total time
                        let duration = enc.duration_seconds().unwrap_or(0) as f32;
                        enc.challenge_tracker.finalize(last_activity, duration);
                    }

                    signals.push(GameSignal::CombatEnded {
                        timestamp: last_activity,
                        encounter_id,
                    });

                    cache.push_new_encounter();
                    // Re-process this event in the new encounter's state machine
                    signals.extend(self.advance_combat_state(event, cache));
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
                let duration = enc.duration_seconds().unwrap_or(0) as f32;
                enc.challenge_tracker.finalize(timestamp, duration);
            }

            signals.push(GameSignal::CombatEnded {
                timestamp,
                encounter_id,
            });

            cache.push_new_encounter();
            signals.extend(self.advance_combat_state(event, cache));
        } else if effect_id == effect_id::EXITCOMBAT || all_players_dead {
            let encounter_id = cache.current_encounter().map(|e| e.id).unwrap_or(0);
            if let Some(enc) = cache.current_encounter_mut() {
                enc.flush_pending_absorptions();
                enc.exit_combat_time = Some(timestamp);
                enc.state = EncounterState::PostCombat {
                    exit_time: timestamp,
                };
                enc.events.push(event);
                let duration = enc.duration_seconds().unwrap_or(0) as f32;
                enc.challenge_tracker.finalize(timestamp, duration);
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
                let duration = enc.duration_seconds().unwrap_or(0) as f32;
                enc.challenge_tracker.finalize(timestamp, duration);
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
