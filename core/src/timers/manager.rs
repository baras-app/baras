//! Timer management handler
//!
//! Manages boss mechanic and ability cooldown timers.
//! Reacts to signals to start, refresh, and expire timers.

use std::collections::HashMap;
use std::time::Duration;

use chrono::{Local, NaiveDateTime};

use crate::boss::BossEncounterDefinition;
use crate::events::{GameSignal, SignalHandler};
use crate::game_data::Difficulty;

use super::{ActiveTimer, TimerDefinition, TimerKey};

/// Maximum age (in minutes) for events to be processed by timers.
/// Events older than this are skipped since timers are only useful for recent/live events.
const TIMER_RECENCY_THRESHOLD_MINS: i64 = 5;

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

    // ─── Boss Encounter State (from signals) ───────────────────────────────────
    /// Boss definitions indexed by area name (for timer extraction)
    boss_definitions: HashMap<String, Vec<BossEncounterDefinition>>,

    /// Active boss definition ID (set by BossEncounterDetected signal)
    active_boss_id: Option<String>,

    /// Current phase (from PhaseChanged signals)
    current_phase: Option<String>,

    /// Counter values (from CounterChanged signals)
    counters: HashMap<String, u32>,

    /// Boss HP by NPC ID (from BossHpChanged signals)
    boss_hp_by_npc: HashMap<i64, f32>,

    /// Combat start time (for TimeElapsed triggers)
    combat_start_time: Option<NaiveDateTime>,

    /// Last checked combat time in seconds (for crossing detection)
    last_combat_secs: f32,
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
            active_boss_id: None,
            current_phase: None,
            counters: HashMap::new(),
            boss_hp_by_npc: HashMap::new(),
            combat_start_time: None,
            last_combat_secs: 0.0,
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
    pub fn load_boss_definitions(&mut self, bosses: Vec<BossEncounterDefinition>) {
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
            if let Some(ref chain_to) = def.triggers_timer
                && !self.definitions.contains_key(chain_to) {
                    broken_chains.push((id.clone(), chain_to.clone()));
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

    /// Get the current boss phase (if any)
    pub fn current_phase(&self) -> Option<&str> {
        self.current_phase.as_deref()
    }

    /// Get the current value of a counter
    pub fn counter_value(&self, counter_id: &str) -> u32 {
        self.counters.get(counter_id).copied().unwrap_or(0)
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

        // Check phase filter
        if !def.phases.is_empty() {
            if let Some(ref current) = self.current_phase {
                if !def.phases.iter().any(|p| p == current) {
                    return false;
                }
            } else {
                return false; // Timer requires phase but none active
            }
        }

        // Check counter condition
        if let Some(ref cond) = def.counter_condition {
            let value = self.counters.get(&cond.counter_id).copied().unwrap_or(0);
            if !cond.operator.evaluate(value, cond.value) {
                return false;
            }
        }

        true
    }

    /// Start a timer from a definition
    fn start_timer(&mut self, def: &TimerDefinition, timestamp: NaiveDateTime, target_id: Option<i64>) {
        let key = TimerKey::new(&def.id, target_id);

        // Check if timer already exists and can be refreshed
        if let Some(existing) = self.active_timers.get_mut(&key) {
            if def.can_be_refreshed {
                existing.refresh(timestamp);
                // Still need to cancel timers that depend on this one
                self.cancel_timers_on_start(&def.id);
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

        // Cancel any timers that have cancel_on_timer pointing to this timer
        self.cancel_timers_on_start(&def.id);
    }

    /// Cancel active timers that have cancel_on_timer matching the started timer ID
    fn cancel_timers_on_start(&mut self, started_timer_id: &str) {
        // Find keys to remove
        let keys_to_cancel: Vec<_> = self.active_timers
            .iter()
            .filter_map(|(key, timer)| {
                // Look up the definition to check cancel_on_timer
                if let Some(def) = self.definitions.get(&timer.definition_id)
                    && let Some(ref cancel_on) = def.cancel_on_timer
                    && cancel_on == started_timer_id {
                        Some(key.clone())
                    } else {
                        None
                    }
            })
            .collect();

        // Remove cancelled timers
        for key in keys_to_cancel {
            if let Some(timer) = self.active_timers.remove(&key) {
                eprintln!("[TIMER] Cancelled '{}' because '{}' started", timer.name, started_timer_id);
            }
        }
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
    fn handle_boss_hp_change(&mut self, npc_id: i64, npc_name: &str, previous_hp: f32, current_hp: f32, timestamp: NaiveDateTime) {
        let matching: Vec<_> = self.definitions
            .values()
            .filter(|d| d.matches_boss_hp_threshold(npc_id, Some(npc_name), previous_hp, current_hp) && self.is_definition_active(d))
            .cloned()
            .collect();

        for def in matching {
            eprintln!("[TIMER] Starting HP threshold timer '{}' (HP crossed below {}% for {})",
                def.name,
                match &def.trigger {
                    super::TimerTrigger::BossHpThreshold { hp_percent, .. } => *hp_percent,
                    _ => 0.0,
                },
                npc_name
            );
            self.start_timer(&def, timestamp, None);
        }
    }

    /// Handle phase change - check for PhaseEntered triggers
    fn handle_phase_change(&mut self, phase_id: &str, timestamp: NaiveDateTime) {
        let matching: Vec<_> = self.definitions
            .values()
            .filter(|d| d.matches_phase_entered(phase_id) && self.is_definition_active(d))
            .cloned()
            .collect();

        for def in matching {
            eprintln!("[TIMER] Starting phase-triggered timer '{}' (phase: {})", def.name, phase_id);
            self.start_timer(&def, timestamp, None);
        }
    }

    /// Handle phase ended - check for PhaseEnded triggers
    fn handle_phase_ended(&mut self, phase_id: &str, timestamp: NaiveDateTime) {
        let matching: Vec<_> = self.definitions
            .values()
            .filter(|d| d.matches_phase_ended(phase_id) && self.is_definition_active(d))
            .cloned()
            .collect();

        for def in matching {
            eprintln!("[TIMER] Starting phase-ended timer '{}' (phase {} ended)", def.name, phase_id);
            self.start_timer(&def, timestamp, None);
        }
    }

    /// Handle counter change - check for CounterReaches triggers
    fn handle_counter_change(&mut self, counter_id: &str, old_value: u32, new_value: u32, timestamp: NaiveDateTime) {
        let matching: Vec<_> = self.definitions
            .values()
            .filter(|d| d.matches_counter_reaches(counter_id, old_value, new_value) && self.is_definition_active(d))
            .cloned()
            .collect();

        for def in matching {
            eprintln!("[TIMER] Starting counter-triggered timer '{}' (counter {} reached {})",
                def.name, counter_id, new_value);
            self.start_timer(&def, timestamp, None);
        }
    }

    /// Handle NPC first seen - check for EntityFirstSeen triggers
    fn handle_npc_first_seen(&mut self, npc_id: i64, npc_name: &str, timestamp: NaiveDateTime) {
        let matching: Vec<_> = self.definitions
            .values()
            .filter(|d| d.matches_entity_first_seen(npc_id) && self.is_definition_active(d))
            .cloned()
            .collect();

        for def in matching {
            eprintln!("[TIMER] Starting first-seen timer '{}' (NPC {} spawned)", def.name, npc_name);
            self.start_timer(&def, timestamp, None);
        }
    }

    /// Handle entity death - check for EntityDeath triggers
    fn handle_entity_death(&mut self, npc_id: i64, entity_name: &str, timestamp: NaiveDateTime) {
        let matching: Vec<_> = self.definitions
            .values()
            .filter(|d| d.matches_entity_death(npc_id, Some(entity_name)) && self.is_definition_active(d))
            .cloned()
            .collect();

        for def in matching {
            eprintln!("[TIMER] Starting death-triggered timer '{}' ({} died)", def.name, entity_name);
            self.start_timer(&def, timestamp, None);
        }
    }

    /// Handle time elapsed - check for TimeElapsed triggers
    fn handle_time_elapsed(&mut self, timestamp: NaiveDateTime) {
        let Some(start_time) = self.combat_start_time else {
            return;
        };

        let new_combat_secs = (timestamp - start_time).num_milliseconds() as f32 / 1000.0;
        let old_combat_secs = self.last_combat_secs;

        // Only check if time has progressed
        if new_combat_secs <= old_combat_secs {
            return;
        }

        let matching: Vec<_> = self.definitions
            .values()
            .filter(|d| d.matches_time_elapsed(old_combat_secs, new_combat_secs) && self.is_definition_active(d))
            .cloned()
            .collect();

        for def in matching {
            eprintln!("[TIMER] Starting time-triggered timer '{}' ({:.1}s into combat)", def.name, new_combat_secs);
            self.start_timer(&def, timestamp, None);
        }

        self.last_combat_secs = new_combat_secs;
    }

    /// Handle combat start - start combat-triggered timers
    fn handle_combat_start(&mut self, timestamp: NaiveDateTime) {
        self.in_combat = true;
        self.combat_start_time = Some(timestamp);
        self.last_combat_secs = 0.0;

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
        self.active_boss_id = None;
        self.current_phase = None;
        self.counters.clear();
        self.boss_hp_by_npc.clear();
        self.combat_start_time = None;
        self.last_combat_secs = 0.0;
    }
}

impl SignalHandler for TimerManager {
    fn handle_signal(&mut self, signal: &GameSignal) {
        // Skip if no definitions loaded (historical mode or empty config)
        if self.definitions.is_empty() && self.boss_definitions.is_empty() {
            return;
        }

        // Skip old events - timers only matter for recent/live events
        let ts = signal.timestamp();
        let now = Local::now().naive_local();
        let age_mins = (now - ts).num_minutes();
        if age_mins > TIMER_RECENCY_THRESHOLD_MINS {
            return;
        }
        self.last_timestamp = Some(ts);

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

            GameSignal::EntityDeath { npc_id, entity_name, timestamp, .. } => {
                self.handle_entity_death(*npc_id, entity_name, *timestamp);
            }

            GameSignal::NpcFirstSeen { npc_id, entity_name, timestamp, .. } => {
                self.handle_npc_first_seen(*npc_id, entity_name, *timestamp);
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

            GameSignal::TargetChanged { target_name, target_entity_type, .. } => {
                // Update boss context when targeting an NPC
                if matches!(target_entity_type, crate::EntityType::Npc) {
                    let name = crate::context::resolve(*target_name);
                    self.context.boss_name = Some(name.to_string());
                }
            }

            GameSignal::TargetCleared { .. } => {
                self.context.boss_name = None;
            }

            // ─── Boss Encounter Signals (from EventProcessor) ─────────────────────
            GameSignal::BossEncounterDetected { definition_id, boss_name, timestamp, .. } => {
                eprintln!("[TIMER] Boss encounter detected: {} ({})", boss_name, definition_id);
                self.active_boss_id = Some(definition_id.clone());
                self.context.boss_name = Some(boss_name.clone());
                // Reset phase and counters for new encounter
                self.current_phase = None;
                self.counters.clear();
                self.boss_hp_by_npc.clear();
                // Start combat-start timers
                self.handle_combat_start(*timestamp);
            }

            GameSignal::BossHpChanged { npc_id, entity_name, current_hp, max_hp, timestamp, .. } => {
                let new_hp = if *max_hp > 0 {
                    (*current_hp as f32 / *max_hp as f32) * 100.0
                } else {
                    100.0
                };
                let old_hp = self.boss_hp_by_npc.get(npc_id).copied().unwrap_or(100.0);
                self.boss_hp_by_npc.insert(*npc_id, new_hp);

                // Check for HP threshold timer triggers
                if (old_hp - new_hp).abs() > 0.01 {
                    self.handle_boss_hp_change(*npc_id, entity_name, old_hp, new_hp, *timestamp);
                }
            }

            GameSignal::PhaseChanged { old_phase, new_phase, timestamp, .. } => {
                // Handle the old phase ending first (if any)
                if let Some(ended_phase) = old_phase {
                    eprintln!("[TIMER] Phase ended: {}", ended_phase);
                    self.handle_phase_ended(ended_phase, *timestamp);
                }

                eprintln!("[TIMER] Phase changed to: {}", new_phase);
                self.current_phase = Some(new_phase.clone());
                // Trigger phase-entered timers
                self.handle_phase_change(new_phase, *timestamp);
            }

            GameSignal::CounterChanged { counter_id, old_value, new_value, timestamp, .. } => {
                self.counters.insert(counter_id.clone(), *new_value);
                // Trigger counter-based timers
                self.handle_counter_change(counter_id, *old_value, *new_value, *timestamp);
            }

            _ => {}
        }

        // Check for time-elapsed triggers if we're in combat
        if let Some(ts) = self.last_timestamp {
            self.handle_time_elapsed(ts);
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

/// Convert a BossTimerDefinition to a TimerDefinition
/// Adds boss context (area, name, difficulties) to the timer
fn convert_boss_timer_to_definition(
    boss_timer: &crate::boss::BossTimerDefinition,
    boss: &BossEncounterDefinition,
) -> TimerDefinition {
    TimerDefinition {
        id: boss_timer.id.clone(),
        name: boss_timer.name.clone(),
        enabled: boss_timer.enabled,
        trigger: boss_timer.trigger.clone(),
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
        cancel_on_timer: boss_timer.cancel_on_timer.clone(),
        // Context: tie to this boss's area and name
        encounters: vec![boss.area_name.clone()],
        boss: Some(boss.name.clone()),
        difficulties: boss_timer.difficulties.clone(),
        phases: boss_timer.phases.clone(),
        counter_condition: boss_timer.counter_condition.clone(),
    }
}
