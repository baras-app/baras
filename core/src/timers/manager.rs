//! Timer management handler
//!
//! Manages boss mechanic and ability cooldown timers.
//! Reacts to signals to start, refresh, and expire timers.

use std::collections::HashMap;
use std::time::Duration;

use chrono::NaiveDateTime;

use crate::encounters::{BossDefinition, BossEncounterState};
use crate::events::{GameSignal, SignalHandler};
use crate::game_data::Difficulty;

use super::{ActiveTimer, TimerDefinition, TimerKey};

/// Current encounter context for filtering timers
#[derive(Debug, Clone, Default)]
pub struct EncounterContext {
    pub encounter_name: Option<String>,
    pub boss_name: Option<String>,
    pub difficulty: Option<Difficulty>,
}

/// Manages ability cooldown and buff timers.
/// Reacts to signals to start, pause, and reset timers.
#[derive(Debug)]
pub struct TimerManager {
    /// Timer definitions indexed by ID
    definitions: HashMap<String, TimerDefinition>,

    /// Currently active timers
    active_timers: HashMap<TimerKey, ActiveTimer>,

    /// Timers that expired this tick (for chaining)
    expired_this_tick: Vec<String>,

    /// Current encounter context for filtering
    context: EncounterContext,

    /// Whether we're currently in combat
    in_combat: bool,

    /// Last known game timestamp
    last_timestamp: Option<NaiveDateTime>,

    // ─── Boss Encounter State ──────────────────────────────────────────────────
    /// Boss definitions indexed by area name (area -> [bosses in that area])
    boss_definitions: HashMap<String, Vec<BossDefinition>>,

    /// Current boss encounter runtime state
    encounter_state: BossEncounterState,

    /// Active boss definition (if fighting a known boss)
    active_boss_def: Option<BossDefinition>,
}

impl Default for TimerManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TimerManager {
    pub fn new() -> Self {
        Self {
            definitions: HashMap::new(),
            active_timers: HashMap::new(),
            expired_this_tick: Vec::new(),
            context: EncounterContext::default(),
            in_combat: false,
            last_timestamp: None,
            boss_definitions: HashMap::new(),
            encounter_state: BossEncounterState::default(),
            active_boss_def: None,
        }
    }

    /// Load timer definitions
    pub fn load_definitions(&mut self, definitions: Vec<TimerDefinition>) {
        self.definitions.clear();
        let mut duplicate_count = 0;
        for def in definitions {
            if def.enabled {
                if let Some(existing) = self.definitions.get(&def.id) {
                    eprintln!(
                        "[TIMER WARNING] Duplicate timer ID '{}' found! First: '{}', Duplicate: '{}'. Keeping first.",
                        def.id, existing.name, def.name
                    );
                    duplicate_count += 1;
                    continue;
                }
                self.definitions.insert(def.id.clone(), def);
            }
        }
        if duplicate_count > 0 {
            eprintln!("TimerManager: loaded {} enabled definitions ({} duplicates skipped)",
                self.definitions.len(), duplicate_count);
        } else {
            eprintln!("TimerManager: loaded {} enabled definitions", self.definitions.len());
        }

        // Validate timer chain references
        self.validate_timer_chains();
    }

    /// Alias for load_definitions (matches effect tracker API)
    pub fn set_definitions(&mut self, definitions: Vec<TimerDefinition>) {
        self.load_definitions(definitions);
    }

    /// Load boss definitions (indexed by area name for quick lookup)
    /// Also extracts boss timers into the main definitions map
    pub fn load_boss_definitions(&mut self, bosses: Vec<BossDefinition>) {
        self.boss_definitions.clear();

        // Clear existing boss-related timer definitions (keep generic ones)
        // We'll re-add them from the fresh boss definitions
        self.definitions.retain(|id, _| !id.contains('_') || id.starts_with("generic_"));

        let mut timer_count = 0;
        let mut duplicate_count = 0;
        for boss in bosses {
            // Extract boss timers and convert to TimerDefinition
            for boss_timer in &boss.timers {
                if boss_timer.enabled {
                    let timer_def = convert_boss_timer_to_definition(boss_timer, &boss);

                    // Check for duplicate ID - warn and skip instead of silent overwrite
                    if let Some(existing) = self.definitions.get(&timer_def.id) {
                        eprintln!(
                            "[TIMER WARNING] Duplicate timer ID '{}' found! \
                            First: '{}' ({}), Duplicate: '{}' ({}). \
                            Keeping first, ignoring duplicate.",
                            timer_def.id,
                            existing.name,
                            existing.boss.as_deref().unwrap_or("unknown"),
                            timer_def.name,
                            boss.name
                        );
                        duplicate_count += 1;
                        continue;
                    }

                    self.definitions.insert(timer_def.id.clone(), timer_def);
                    timer_count += 1;
                }
            }

            self.boss_definitions
                .entry(boss.area_name.clone())
                .or_default()
                .push(boss);
        }

        let boss_count: usize = self.boss_definitions.values().map(|v| v.len()).sum();
        if duplicate_count > 0 {
            eprintln!(
                "TimerManager: loaded {} bosses, {} timers ({} DUPLICATES SKIPPED - check your timer definitions!)",
                boss_count, timer_count, duplicate_count
            );
        } else {
            eprintln!(
                "TimerManager: loaded {} bosses across {} areas, {} boss timers",
                boss_count,
                self.boss_definitions.len(),
                timer_count
            );
        }

        // Validate timer chain references
        self.validate_timer_chains();
    }

    /// Validate that all timer chain references (triggers_timer/chains_to) point to existing timers
    fn validate_timer_chains(&self) {
        let mut broken_chains = Vec::new();

        for (id, def) in &self.definitions {
            if let Some(ref chain_to) = def.triggers_timer {
                if !self.definitions.contains_key(chain_to) {
                    broken_chains.push((id.clone(), chain_to.clone()));
                }
            }
        }

        if !broken_chains.is_empty() {
            eprintln!(
                "[TIMER WARNING] {} broken timer chain reference(s) found:",
                broken_chains.len()
            );
            for (timer_id, missing_ref) in &broken_chains {
                eprintln!(
                    "  - Timer '{}' chains to '{}' which does not exist",
                    timer_id, missing_ref
                );
            }
        }
    }

    /// Get current encounter state (for external queries)
    pub fn encounter_state(&self) -> &BossEncounterState {
        &self.encounter_state
    }

    /// Tick to process timer expirations based on real time.
    /// Call periodically to update timers even without new signals.
    pub fn tick(&mut self) {
        if let Some(ts) = self.last_timestamp {
            self.process_expirations(ts);
        }
    }

    /// Get all currently active timers (for overlay rendering)
    pub fn active_timers(&self) -> Vec<&ActiveTimer> {
        self.active_timers.values().collect()
    }

    /// Get active timers as owned data (for sending to overlay)
    pub fn active_timers_snapshot(&self, current_time: NaiveDateTime) -> Vec<ActiveTimer> {
        self.active_timers
            .values()
            .filter(|t| !t.has_expired(current_time))
            .cloned()
            .collect()
    }

    /// Set encounter context for filtering
    pub fn set_context(&mut self, encounter: Option<String>, boss: Option<String>, difficulty: Option<Difficulty>) {
        self.context = EncounterContext {
            encounter_name: encounter,
            boss_name: boss,
            difficulty,
        };
    }

    /// Check if a timer definition is active for current context
    fn is_definition_active(&self, def: &TimerDefinition) -> bool {
        // First check basic context (encounter, boss, difficulty)
        if !def.enabled || !def.is_active_for_context(
            self.context.encounter_name.as_deref(),
            self.context.boss_name.as_deref(),
            self.context.difficulty,
        ) {
            return false;
        }

        // Then check phase/counter conditions if specified
        def.is_active_for_state(&self.encounter_state)
    }

    /// Start a timer from a definition
    fn start_timer(&mut self, def: &TimerDefinition, timestamp: NaiveDateTime, target_id: Option<i64>) {
        let key = TimerKey::new(&def.id, target_id);

        // Check if timer already exists and can be refreshed
        if let Some(existing) = self.active_timers.get_mut(&key) {
            if def.can_be_refreshed {
                existing.refresh(timestamp);
                return;
            }
            // Timer exists and can't be refreshed - ignore
            return;
        }

        // Create new timer
        let timer = ActiveTimer::new(
            def.id.clone(),
            def.name.clone(),
            target_id,
            timestamp,
            Duration::from_secs_f32(def.duration_secs),
            def.repeats,
            def.color,
            def.triggers_timer.clone(),
            def.show_on_raid_frames,
        );

        self.active_timers.insert(key, timer);
    }

    /// Process timer expirations and chains
    fn process_expirations(&mut self, current_time: NaiveDateTime) {
        self.expired_this_tick.clear();

        // Find expired timers
        let expired_keys: Vec<_> = self.active_timers
            .iter()
            .filter(|(_, timer)| timer.has_expired(current_time))
            .map(|(key, timer)| (key.clone(), timer.triggers_timer.clone()))
            .collect();

        // Remove expired timers and collect chain triggers
        for (key, chain_timer_id) in expired_keys {
            if let Some(timer) = self.active_timers.remove(&key) {
                self.expired_this_tick.push(timer.definition_id.clone());

                // Handle chaining
                if let Some(next_timer_id) = chain_timer_id
                    && let Some(next_def) = self.definitions.get(&next_timer_id).cloned()
                        && self.is_definition_active(&next_def) {
                            self.start_timer(&next_def, current_time, timer.target_entity_id);
                }
            }
        }

        // Check for timers triggered by expirations
        let expired_ids = self.expired_this_tick.clone();
        for expired_id in expired_ids {
            let matching: Vec<_> = self.definitions
                .values()
                .filter(|d| d.matches_timer_expires(&expired_id) && self.is_definition_active(d))
                .cloned()
                .collect();

            for def in matching {
                self.start_timer(&def, current_time, None);
            }
        }
    }

    /// Handle ability activation
    fn handle_ability(&mut self, ability_id: i64, _source_id: i64, target_id: i64, timestamp: NaiveDateTime) {
        // Convert i64 to u64 for matching (game IDs are always positive)
        let ability_id = ability_id as u64;

        let matching: Vec<_> = self.definitions
            .values()
            .filter(|d| {
                let matches_ability = d.matches_ability(ability_id);
                let is_active = self.is_definition_active(d);
                if matches_ability && !is_active {
                    let diff_str = self.context.difficulty.map(|d| d.config_key()).unwrap_or("none");
                    eprintln!("[TIMER] Ability {} matches timer '{}' but context filter failed (enc={:?}, boss={:?}, diff={})",
                        ability_id, d.name, self.context.encounter_name, self.context.boss_name, diff_str);
                }
                matches_ability && is_active
            })
            .cloned()
            .collect();

        for def in matching {
            eprintln!("[TIMER] Starting timer '{}' (ability {})", def.name, ability_id);
            self.start_timer(&def, timestamp, Some(target_id));
        }
    }

    /// Handle effect applied
    fn handle_effect_applied(&mut self, effect_id: i64, _source_id: i64, target_id: i64, timestamp: NaiveDateTime) {
        // Convert i64 to u64 for matching (game IDs are always positive)
        let effect_id = effect_id as u64;

        let matching: Vec<_> = self.definitions
            .values()
            .filter(|d| d.matches_effect_applied(effect_id) && self.is_definition_active(d))
            .cloned()
            .collect();

        for def in matching {
            self.start_timer(&def, timestamp, Some(target_id));
        }
    }

    /// Handle effect removed
    fn handle_effect_removed(&mut self, effect_id: i64, target_id: i64, timestamp: NaiveDateTime) {
        // Convert i64 to u64 for matching (game IDs are always positive)
        let effect_id = effect_id as u64;

        let matching: Vec<_> = self.definitions
            .values()
            .filter(|d| d.matches_effect_removed(effect_id) && self.is_definition_active(d))
            .cloned()
            .collect();

        for def in matching {
            self.start_timer(&def, timestamp, Some(target_id));
        }
    }

    /// Handle boss HP change - check for HP threshold triggers
    fn handle_boss_hp_change(&mut self, previous_hp: f32, current_hp: f32, timestamp: NaiveDateTime) {
        let matching: Vec<_> = self.definitions
            .values()
            .filter(|d| d.matches_boss_hp_threshold(previous_hp, current_hp) && self.is_definition_active(d))
            .cloned()
            .collect();

        for def in matching {
            eprintln!("[TIMER] Starting HP threshold timer '{}' (HP crossed below {}%)",
                def.name,
                match &def.trigger {
                    super::TimerTrigger::BossHpThreshold { hp_percent } => *hp_percent,
                    _ => 0.0,
                }
            );
            self.start_timer(&def, timestamp, None);
        }
    }

    /// Handle combat start
    fn handle_combat_start(&mut self, timestamp: NaiveDateTime) {
        self.in_combat = true;
        self.encounter_state.start_combat(timestamp);

        // If we have an active boss, set initial phase
        if let Some(ref boss_def) = self.active_boss_def {
            if let Some(initial_phase) = boss_def.initial_phase() {
                self.encounter_state.set_phase(&initial_phase.id);
                eprintln!("[TIMER] Boss fight started, initial phase: {}", initial_phase.name);
            }
        }

        let matching: Vec<_> = self.definitions
            .values()
            .filter(|d| d.triggers_on_combat_start() && self.is_definition_active(d))
            .cloned()
            .collect();

        for def in matching {
            self.start_timer(&def, timestamp, None);
        }
    }

    /// Clear all combat-scoped timers
    fn clear_combat_timers(&mut self) {
        self.in_combat = false;
        self.active_timers.clear();
        self.encounter_state.reset();
        self.active_boss_def = None;
    }

    /// Detect boss from NPC name and activate boss definition
    fn detect_boss(&mut self, npc_name: &str) {
        // If we have an area, search only that area first
        if let Some(ref area_name) = self.context.encounter_name {
            if let Some(bosses) = self.boss_definitions.get(area_name) {
                if let Some(boss_def) = bosses.iter().find(|b| b.matches_npc_name(npc_name)) {
                    eprintln!("[TIMER] Boss detected by name: {} ({}) in {}", boss_def.name, boss_def.id, area_name);
                    self.active_boss_def = Some(boss_def.clone());
                    self.context.boss_name = Some(boss_def.name.clone());
                    return;
                }
            }
        }

        // No area set or boss not found in current area - search ALL areas
        for (area_name, bosses) in &self.boss_definitions {
            if let Some(boss_def) = bosses.iter().find(|b| b.matches_npc_name(npc_name)) {
                eprintln!("[TIMER] Boss detected by name: {} ({}) - inferred area: {}",
                    boss_def.name, boss_def.id, area_name);
                self.active_boss_def = Some(boss_def.clone());
                self.context.boss_name = Some(boss_def.name.clone());
                self.context.encounter_name = Some(area_name.clone());
                return;
            }
        }
    }

    /// Detect boss from NPC ID (more reliable than name)
    fn detect_boss_by_npc_id(&mut self, npc_id: i64) {
        // If we have an area, search only that area first
        if let Some(ref area_name) = self.context.encounter_name {
            if let Some(bosses) = self.boss_definitions.get(area_name) {
                if let Some(boss_def) = bosses.iter().find(|b| b.matches_npc_id(npc_id)) {
                    eprintln!("[TIMER] Boss detected by NPC ID {}: {} ({}) in {}",
                        npc_id, boss_def.name, boss_def.id, area_name);
                    self.active_boss_def = Some(boss_def.clone());
                    self.context.boss_name = Some(boss_def.name.clone());
                    return;
                }
            }
        }

        // No area set or boss not found in current area - search ALL areas
        for (area_name, bosses) in &self.boss_definitions {
            if let Some(boss_def) = bosses.iter().find(|b| b.matches_npc_id(npc_id)) {
                eprintln!("[TIMER] Boss detected by NPC ID {}: {} ({}) - inferred area: {}",
                    npc_id, boss_def.name, boss_def.id, area_name);
                self.active_boss_def = Some(boss_def.clone());
                self.context.boss_name = Some(boss_def.name.clone());
                self.context.encounter_name = Some(area_name.clone());
                return;
            }
        }
    }

    /// Update boss HP and check for phase transitions
    pub fn update_boss_hp(&mut self, current_hp: i64, max_hp: i64, timestamp: NaiveDateTime) {
        let hp_changed = self.encounter_state.update_boss_hp(current_hp, max_hp);

        if hp_changed {
            self.check_phase_transitions(timestamp);
        }
    }

    /// Check for HP-triggered phase transitions
    fn check_phase_transitions(&mut self, _timestamp: NaiveDateTime) {
        let Some(ref boss_def) = self.active_boss_def else {
            return;
        };

        let current_phase = self.encounter_state.current_phase.clone();

        // Check each phase for HP threshold triggers
        for phase in &boss_def.phases {
            // Skip if we're already in this phase
            if current_phase.as_deref() == Some(&phase.id) {
                continue;
            }

            let should_transition = match &phase.trigger {
                crate::encounters::PhaseTrigger::BossHpBelow { hp_percent, npc_id, boss_name } => {
                    // Priority: NPC ID > name > any boss
                    self.encounter_state.is_boss_hp_below(*npc_id, boss_name.as_deref(), *hp_percent)
                }
                crate::encounters::PhaseTrigger::BossHpAbove { hp_percent, npc_id, boss_name } => {
                    self.encounter_state.is_boss_hp_above(*npc_id, boss_name.as_deref(), *hp_percent)
                }
                _ => false,
            };

            if should_transition {
                let hp_display = match &phase.trigger {
                    crate::encounters::PhaseTrigger::BossHpBelow { boss_name, .. } |
                    crate::encounters::PhaseTrigger::BossHpAbove { boss_name, .. } => {
                        boss_name.as_ref()
                            .and_then(|n| self.encounter_state.get_boss_hp(n))
                            .unwrap_or(self.encounter_state.boss_hp_percent)
                    }
                    _ => self.encounter_state.boss_hp_percent,
                };

                eprintln!("[TIMER] Phase transition: {:?} -> {} (HP: {:.1}%)",
                    current_phase, phase.name, hp_display);

                // Reset counters specified for this phase
                self.encounter_state.reset_counters(&phase.resets_counters);

                // Set new phase
                self.encounter_state.set_phase(&phase.id);

                // Start any timers triggered by phase entry
                // TODO: Implement PhaseEntered trigger for boss timers
                break;
            }
        }
    }

    /// Pause timers for a dead entity
    fn pause_entity_timers(&mut self, _entity_id: i64) {
        // For now, we don't pause - just let them expire naturally
        // Could implement pause/resume logic if needed
    }
}

impl SignalHandler for TimerManager {
    fn handle_signal(&mut self, signal: &GameSignal) {
        // Update timestamp from signal
        if let Some(ts) = signal_timestamp(signal) {
            self.last_timestamp = Some(ts);
        }

        match signal {
            GameSignal::AbilityActivated {
                ability_id,
                source_id,
                target_id,
                timestamp,
                ..
            } => {
                self.handle_ability(*ability_id, *source_id, *target_id, *timestamp);
            }

            GameSignal::EffectApplied {
                effect_id,
                source_id,
                target_id,
                timestamp,
                ..
            } => {
                self.handle_effect_applied(*effect_id, *source_id, *target_id, *timestamp);
            }

            GameSignal::EffectRemoved {
                effect_id,
                target_id,
                timestamp,
                ..
            } => {
                self.handle_effect_removed(*effect_id, *target_id, *timestamp);
            }

            GameSignal::CombatStarted { timestamp, .. } => {
                self.handle_combat_start(*timestamp);
            }

            GameSignal::CombatEnded { .. } => {
                self.clear_combat_timers();
            }

            GameSignal::EntityDeath { entity_id, .. } => {
                self.pause_entity_timers(*entity_id);
            }

            GameSignal::AreaEntered { area_name, difficulty_name, .. } => {
                // Update encounter context from area
                self.context.encounter_name = Some(area_name.clone());
                self.context.difficulty = if difficulty_name.is_empty() {
                    None
                } else {
                    Difficulty::from_game_string(difficulty_name)
                };
                eprintln!("[TIMER] Area entered: {} (difficulty: {:?})", area_name, self.context.difficulty);
            }

            GameSignal::TargetChanged { target_name, target_npc_id, target_entity_type, .. } => {
                // Update boss context when targeting an NPC (bosses are NPCs)
                if matches!(target_entity_type, crate::EntityType::Npc) {
                    let name = crate::context::resolve(*target_name);
                    eprintln!("[TIMER] Target changed to NPC: {} (ID: {})", name, target_npc_id);
                    self.context.boss_name = Some(name.to_string());

                    // Try to detect if this is a known boss (prefer NPC ID over name)
                    if *target_npc_id != 0 {
                        self.detect_boss_by_npc_id(*target_npc_id);
                    }
                    // Fall back to name detection if NPC ID didn't match
                    if self.active_boss_def.is_none() {
                        self.detect_boss(name);
                    }
                }
            }

            GameSignal::TargetCleared { .. } => {
                // Clear boss context when target is cleared
                self.context.boss_name = None;
            }

            GameSignal::BossHpChanged { entity_id, npc_id, entity_name, current_hp, max_hp, timestamp } => {
                // Update per-entity HP tracking (by entity ID, NPC ID, and name)
                // Returns Some((old_hp, new_hp)) if HP changed significantly
                let hp_change = self.encounter_state.update_entity_hp(
                    *entity_id,
                    *npc_id,
                    entity_name,
                    *current_hp,
                    *max_hp,
                );

                // Try to detect boss by NPC ID if not already detected
                if self.active_boss_def.is_none() && *npc_id != 0 {
                    self.detect_boss_by_npc_id(*npc_id);
                }

                if let Some((old_hp, new_hp)) = hp_change {
                    // Check for HP threshold timer triggers
                    self.handle_boss_hp_change(old_hp, new_hp, *timestamp);
                    // Check for phase transitions
                    self.check_phase_transitions(*timestamp);
                }
            }

            // Informational signals - no action needed in TimerManager
            GameSignal::PhaseChanged { .. } | GameSignal::CounterChanged { .. } => {}

            _ => {}
        }

        // Process expirations after handling signal
        if let Some(ts) = self.last_timestamp {
            self.process_expirations(ts);
        }
    }

    fn on_encounter_start(&mut self, _encounter_id: u64) {
        // Could reset encounter-specific state here
    }

    fn on_encounter_end(&mut self, _encounter_id: u64) {
        self.clear_combat_timers();
    }
}

/// Extract timestamp from a signal (if available)
fn signal_timestamp(signal: &GameSignal) -> Option<NaiveDateTime> {
    match signal {
        GameSignal::CombatStarted { timestamp, .. } => Some(*timestamp),
        GameSignal::CombatEnded { timestamp, .. } => Some(*timestamp),
        GameSignal::EntityDeath { timestamp, .. } => Some(*timestamp),
        GameSignal::EntityRevived { timestamp, .. } => Some(*timestamp),
        GameSignal::EffectApplied { timestamp, .. } => Some(*timestamp),
        GameSignal::EffectRemoved { timestamp, .. } => Some(*timestamp),
        GameSignal::EffectChargesChanged { timestamp, .. } => Some(*timestamp),
        GameSignal::AbilityActivated { timestamp, .. } => Some(*timestamp),
        GameSignal::TargetChanged { timestamp, .. } => Some(*timestamp),
        GameSignal::TargetCleared { timestamp, .. } => Some(*timestamp),
        GameSignal::AreaEntered { timestamp, .. } => Some(*timestamp),
        GameSignal::PlayerInitialized { timestamp, .. } => Some(*timestamp),
        GameSignal::DisciplineChanged { timestamp, .. } => Some(*timestamp),
        GameSignal::BossHpChanged { timestamp, .. } => Some(*timestamp),
        GameSignal::PhaseChanged { timestamp, .. } => Some(*timestamp),
        GameSignal::CounterChanged { timestamp, .. } => Some(*timestamp),
    }
}

/// Convert a BossTimerDefinition to a TimerDefinition
/// This bridges the boss-specific timer format to the generic timer system
fn convert_boss_timer_to_definition(
    boss_timer: &crate::encounters::BossTimerDefinition,
    boss: &BossDefinition,
) -> TimerDefinition {
    use crate::encounters::BossTimerTrigger;

    // Convert trigger type
    let trigger = match &boss_timer.trigger {
        BossTimerTrigger::CombatStart => super::TimerTrigger::CombatStart,
        BossTimerTrigger::AbilityCast { ability_ids } => {
            super::TimerTrigger::AbilityCast { ability_ids: ability_ids.clone() }
        }
        BossTimerTrigger::EffectApplied { effect_ids } => {
            super::TimerTrigger::EffectApplied { effect_ids: effect_ids.clone() }
        }
        BossTimerTrigger::EffectRemoved { effect_ids } => {
            super::TimerTrigger::EffectRemoved { effect_ids: effect_ids.clone() }
        }
        BossTimerTrigger::TimerExpires { timer_id } => {
            super::TimerTrigger::TimerExpires { timer_id: timer_id.clone() }
        }
        BossTimerTrigger::PhaseEntered { .. } => {
            // Phase-entered triggers need special handling - not yet implemented
            super::TimerTrigger::Manual
        }
        BossTimerTrigger::BossHpBelow { hp_percent, .. } => {
            super::TimerTrigger::BossHpThreshold { hp_percent: *hp_percent }
        }
        BossTimerTrigger::AllOf { conditions } => {
            super::TimerTrigger::AllOf {
                conditions: conditions.iter().map(|c| convert_boss_trigger(c)).collect()
            }
        }
        BossTimerTrigger::AnyOf { conditions } => {
            super::TimerTrigger::AnyOf {
                conditions: conditions.iter().map(|c| convert_boss_trigger(c)).collect()
            }
        }
    };

    TimerDefinition {
        id: boss_timer.id.clone(),
        name: boss_timer.name.clone(),
        enabled: boss_timer.enabled,
        trigger,
        source: Default::default(),
        target: Default::default(),
        duration_secs: boss_timer.duration_secs,
        can_be_refreshed: boss_timer.can_be_refreshed,
        repeats: boss_timer.repeats,
        color: boss_timer.color,
        show_on_raid_frames: boss_timer.show_on_raid_frames,
        alert_at_secs: boss_timer.alert_at_secs,
        alert_text: None,
        audio_file: None,
        triggers_timer: boss_timer.chains_to.clone(),
        // Context: tie to this boss's area and name
        encounters: vec![boss.area_name.clone()],
        boss: Some(boss.name.clone()),
        difficulties: boss_timer.difficulties.clone(),
        phases: boss_timer.phases.clone(),
        counter_condition: boss_timer.counter_condition.clone(),
    }
}

/// Convert a single BossTimerTrigger to TimerTrigger (for nested conditions)
fn convert_boss_trigger(trigger: &crate::encounters::BossTimerTrigger) -> super::TimerTrigger {
    use crate::encounters::BossTimerTrigger;

    match trigger {
        BossTimerTrigger::CombatStart => super::TimerTrigger::CombatStart,
        BossTimerTrigger::AbilityCast { ability_ids } => {
            super::TimerTrigger::AbilityCast { ability_ids: ability_ids.clone() }
        }
        BossTimerTrigger::EffectApplied { effect_ids } => {
            super::TimerTrigger::EffectApplied { effect_ids: effect_ids.clone() }
        }
        BossTimerTrigger::EffectRemoved { effect_ids } => {
            super::TimerTrigger::EffectRemoved { effect_ids: effect_ids.clone() }
        }
        BossTimerTrigger::TimerExpires { timer_id } => {
            super::TimerTrigger::TimerExpires { timer_id: timer_id.clone() }
        }
        BossTimerTrigger::PhaseEntered { .. } => super::TimerTrigger::Manual,
        BossTimerTrigger::BossHpBelow { hp_percent, .. } => {
            super::TimerTrigger::BossHpThreshold { hp_percent: *hp_percent }
        }
        BossTimerTrigger::AllOf { conditions } => {
            super::TimerTrigger::AllOf {
                conditions: conditions.iter().map(|c| convert_boss_trigger(c)).collect()
            }
        }
        BossTimerTrigger::AnyOf { conditions } => {
            super::TimerTrigger::AnyOf {
                conditions: conditions.iter().map(|c| convert_boss_trigger(c)).collect()
            }
        }
    }
}
