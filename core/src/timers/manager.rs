//! Timer management handler
//!
//! Manages boss mechanic and ability cooldown timers.
//! Reacts to signals to start, refresh, and expire timers.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use chrono::{Local, NaiveDateTime};

use crate::boss::BossEncounterDefinition;
use crate::combat_log::EntityType;
use crate::signal_processor::{GameSignal, SignalHandler};
use crate::game_data::Difficulty;
use crate::context::IStr;

use super::{ActiveTimer, TimerDefinition, TimerKey, TimerTrigger};
use super::matching::{is_definition_active, matches_source_target_filters};
use super::signal_handlers;

/// Maximum age (in minutes) for events to be processed by timers in live mode.
/// Events older than this are skipped since timers are only useful for recent/live events.
/// This is only checked when `live_mode` is true (after initial batch load).
const TIMER_RECENCY_THRESHOLD_MINS: i64 = 5;

/// Current encounter context for filtering timers
#[derive(Debug, Clone, Default)]
pub struct EncounterContext {
    /// Area name from game (for display/logging)
    pub encounter_name: Option<String>,
    /// Area ID from game (primary matching key - more reliable than name)
    pub area_id: Option<i64>,
    pub boss_name: Option<String>,
    pub difficulty: Option<Difficulty>,
}

/// A fired alert (ephemeral notification, not a countdown timer)
#[derive(Debug, Clone)]
pub struct FiredAlert {
    pub id: String,
    pub name: String,
    pub text: String,
    pub color: Option<[u8; 4]>,
    pub timestamp: NaiveDateTime,
}

/// Manages ability cooldown and buff timers.
/// Reacts to signals to start, pause, and reset timers.
#[derive(Debug)]
pub struct TimerManager {
    /// Timer definitions indexed by ID
    pub(super) definitions: HashMap<String, TimerDefinition>,

    /// Currently active timers (countdown timers with duration > 0)
    pub(super) active_timers: HashMap<TimerKey, ActiveTimer>,

    /// Fired alerts (ephemeral notifications, not countdown timers)
    pub(super) fired_alerts: Vec<FiredAlert>,

    /// Timers that expired this tick (for chaining)
    expired_this_tick: Vec<String>,

    /// Current encounter context for filtering
    pub(super) context: EncounterContext,

    /// Whether we're currently in combat
    pub(super) in_combat: bool,

    /// Last known game timestamp
    last_timestamp: Option<NaiveDateTime>,

    /// When true, apply recency threshold to skip old events.
    live_mode: bool,

    // ─── Boss Encounter State (from signals) ───────────────────────────────────
    /// Boss definitions indexed by area name (for timer extraction)
    boss_definitions: HashMap<String, Vec<BossEncounterDefinition>>,

    /// Current phase (from PhaseChanged signals)
    pub(super) current_phase: Option<String>,

    /// Counter values (from CounterChanged signals)
    pub(super) counters: HashMap<String, u32>,

    /// Boss HP by NPC ID (from BossHpChanged signals)
    pub(super) boss_hp_by_npc: HashMap<i64, f32>,

    /// Combat start time (for TimeElapsed triggers)
    pub(super) combat_start_time: Option<NaiveDateTime>,

    /// Last checked combat time in seconds (for crossing detection)
    pub(super) last_combat_secs: f32,

    // ─── Entity Filter State ─────────────────────────────────────────────────
    /// Local player's entity ID (for LocalPlayer filter)
    pub(super) local_player_id: Option<i64>,

    /// Boss entity IDs currently in combat (for Boss filter)
    pub(super) boss_entity_ids: HashSet<i64>,
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
            fired_alerts: Vec::new(),
            expired_this_tick: Vec::new(),
            context: EncounterContext::default(),
            in_combat: false,
            last_timestamp: None,
            live_mode: true, // Default: apply recency threshold (skip old events)
            boss_definitions: HashMap::new(),
            current_phase: None,
            counters: HashMap::new(),
            boss_hp_by_npc: HashMap::new(),
            combat_start_time: None,
            last_combat_secs: 0.0,
            local_player_id: None,
            boss_entity_ids: HashSet::new(),
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
                    let timer_def = boss_timer.to_timer_definition(boss.area_id, &boss.area_name, &boss.name);

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

    /// Enable live mode (apply recency threshold to skip old events).
    /// Call this after the initial batch load to prevent stale log events from triggering timers.
    pub fn set_live_mode(&mut self, enabled: bool) {
        self.live_mode = enabled;
    }

    /// Set the local player's entity ID (for LocalPlayer filter matching).
    /// Call this when the local player is identified during log parsing.
    pub fn set_local_player_id(&mut self, entity_id: i64) {
        self.local_player_id = Some(entity_id);
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

    /// Take all fired alerts, clearing the internal buffer.
    /// Call this after processing signals to capture ephemeral notifications.
    pub fn take_fired_alerts(&mut self) -> Vec<FiredAlert> {
        std::mem::take(&mut self.fired_alerts)
    }

    /// Peek at fired alerts without clearing (for validation/debugging)
    pub fn fired_alerts(&self) -> &[FiredAlert] {
        &self.fired_alerts
    }

    /// Set encounter context for filtering (legacy - prefer using signals)
    pub fn set_context(&mut self, encounter: Option<String>, boss: Option<String>, difficulty: Option<Difficulty>) {
        self.context = EncounterContext {
            encounter_name: encounter,
            area_id: None, // Not available in legacy API
            boss_name: boss,
            difficulty,
        };
    }

    /// Check if a timer definition is active for current context (delegates to matching module)
    pub(super) fn is_definition_active(&self, def: &TimerDefinition) -> bool {
        is_definition_active(def, &self.context, self.current_phase.as_deref(), &self.counters)
    }

    /// Start a timer from a definition
    pub(super) fn start_timer(&mut self, def: &TimerDefinition, timestamp: NaiveDateTime, target_id: Option<i64>) {
        // Alerts are ephemeral notifications, not countdown timers
        if def.is_alert {
            eprintln!("[ALERT] Fired: {} - {}", def.name, def.alert_text.as_deref().unwrap_or(&def.name));
            self.fired_alerts.push(FiredAlert {
                id: def.id.clone(),
                name: def.name.clone(),
                text: def.alert_text.clone().unwrap_or_else(|| def.name.clone()),
                color: Some(def.color),
                timestamp,
            });
            return;
        }

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

        self.active_timers.insert(key.clone(), timer);
        eprintln!("[TIMER] Added to active_timers: {} (key={:?}, total={})", def.name, key, self.active_timers.len());

        // Cancel any timers that have cancel_on_timer pointing to this timer
        self.cancel_timers_on_start(&def.id);
    }

    /// Cancel active timers that have cancel_on_timer matching the started timer ID
    fn cancel_timers_on_start(&mut self, started_timer_id: &str) {
        // Find keys to remove - check for TimerStarted cancel triggers
        let keys_to_cancel: Vec<_> = self.active_timers
            .iter()
            .filter_map(|(key, timer)| {
                if let Some(def) = self.definitions.get(&timer.definition_id)
                    && let Some(ref cancel_trigger) = def.cancel_trigger
                    && matches!(cancel_trigger, TimerTrigger::TimerStarted { timer_id } if timer_id == started_timer_id) {
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

    /// Cancel active timers whose cancel_trigger matches the given predicate
    pub(super) fn cancel_timers_matching<F>(&mut self, trigger_matches: F, reason: &str)
    where
        F: Fn(&TimerTrigger) -> bool,
    {
        let keys_to_cancel: Vec<_> = self.active_timers
            .iter()
            .filter_map(|(key, timer)| {
                if let Some(def) = self.definitions.get(&timer.definition_id)
                    && let Some(ref cancel_trigger) = def.cancel_trigger
                    && trigger_matches(cancel_trigger) {
                        Some(key.clone())
                    } else {
                        None
                    }
            })
            .collect();

        for key in keys_to_cancel {
            if let Some(timer) = self.active_timers.remove(&key) {
                eprintln!("[TIMER] Cancelled '{}' ({})", timer.name, reason);
            }
        }
    }

    /// Process timer expirations, repeats, and chains
    fn process_expirations(&mut self, current_time: NaiveDateTime) {
        self.expired_this_tick.clear();

        // Find expired timer keys
        let expired_keys: Vec<_> = self.active_timers
            .iter()
            .filter(|(_, timer)| timer.has_expired(current_time))
            .map(|(key, _)| key.clone())
            .collect();

        // Collect chain triggers from timers that won't repeat
        let mut chains_to_start: Vec<(String, Option<i64>)> = Vec::new();

        for key in expired_keys {
            // Always record the expiration for TimerExpires triggers
            self.expired_this_tick.push(key.definition_id.clone());

            // Check if timer can repeat
            if let Some(timer) = self.active_timers.get_mut(&key)
                && timer.can_repeat()
            {
                timer.repeat(current_time);
                eprintln!("[TIMER] Repeated '{}' ({}/{})", timer.name, timer.repeat_count, timer.max_repeats);
            } else if let Some(timer) = self.active_timers.remove(&key) {
                // Timer exhausted repeats - remove and prepare chain
                if let Some(next_timer_id) = timer.triggers_timer.clone() {
                    chains_to_start.push((next_timer_id, timer.target_entity_id));
                }
            }
        }

        // Start chained timers (outside the borrow)
        for (next_timer_id, target_id) in chains_to_start {
            if let Some(next_def) = self.definitions.get(&next_timer_id).cloned()
                && self.is_definition_active(&next_def)
            {
                self.start_timer(&next_def, current_time, target_id);
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

    // ─── Entity Filter Matching (delegates to matching module) ─────────────────

    /// Check if source/target filters pass for a timer definition
    pub(super) fn matches_source_target_filters(
        &self,
        def: &TimerDefinition,
        source_id: i64,
        source_type: EntityType,
        source_name: IStr,
        source_npc_id: i64,
        target_id: i64,
        target_type: EntityType,
        target_name: IStr,
        target_npc_id: i64,
    ) -> bool {
        matches_source_target_filters(
            def,
            source_id, source_type, source_name, source_npc_id,
            target_id, target_type, target_name, target_npc_id,
            self.local_player_id, &self.boss_entity_ids,
        )
    }

}

impl SignalHandler for TimerManager {
    fn handle_signal(&mut self, signal: &GameSignal) {
        // Skip if no definitions loaded (historical mode or empty config)
        if self.definitions.is_empty() && self.boss_definitions.is_empty() {
            return;
        }

        // In live mode, skip old events - timers only matter for recent/live events.
        // In historical mode (validation), process all events regardless of age.
        let ts = signal.timestamp();
        if self.live_mode {
            let now = Local::now().naive_local();
            let age_mins = (now - ts).num_minutes();
            if age_mins > TIMER_RECENCY_THRESHOLD_MINS {
                return;
            }
        }
        self.last_timestamp = Some(ts);

        match signal {
            GameSignal::PlayerInitialized { entity_id, .. } => {
                self.local_player_id = Some(*entity_id);
            }

            GameSignal::AbilityActivated {
                ability_id,
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
                signal_handlers::handle_ability(
                    self,
                    *ability_id,
                    *source_id, *source_entity_type, *source_name, *source_npc_id,
                    *target_id, *target_entity_type, *target_name, *target_npc_id,
                    *timestamp,
                );
            }

            GameSignal::EffectApplied {
                effect_id,
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
                signal_handlers::handle_effect_applied(
                    self,
                    *effect_id,
                    *source_id, *source_entity_type, *source_name, *source_npc_id,
                    *target_id, *target_entity_type, *target_name, *target_npc_id,
                    *timestamp,
                );
            }

            GameSignal::EffectRemoved {
                effect_id,
                source_id,
                source_entity_type,
                source_name,
                target_id,
                target_entity_type,
                target_name,
                timestamp,
            } => {
                // EffectRemoved doesn't include npc_ids in the game log, pass 0
                signal_handlers::handle_effect_removed(
                    self,
                    *effect_id,
                    *source_id, *source_entity_type, *source_name, 0,
                    *target_id, *target_entity_type, *target_name, 0,
                    *timestamp,
                );
            }

            GameSignal::CombatStarted { timestamp, .. } => {
                signal_handlers::handle_combat_start(self, *timestamp);
            }

            GameSignal::CombatEnded { .. } => {
                signal_handlers::clear_combat_timers(self);
            }

            GameSignal::EntityDeath { npc_id, entity_name, timestamp, .. } => {
                signal_handlers::handle_entity_death(self, *npc_id, entity_name, *timestamp);
            }

            GameSignal::NpcFirstSeen { npc_id, entity_name, timestamp, .. } => {
                signal_handlers::handle_npc_first_seen(self, *npc_id, entity_name, *timestamp);
            }

            GameSignal::AreaEntered { area_id, area_name, difficulty_name, .. } => {
                // Update encounter context from area (area_id is primary match key)
                self.context.area_id = Some(*area_id);
                self.context.encounter_name = Some(area_name.clone());
                self.context.difficulty = if difficulty_name.is_empty() {
                    None
                } else {
                    Difficulty::from_game_string(difficulty_name)
                };
                eprintln!("[TIMER] Area entered: {} (id: {}, difficulty: {:?})", area_name, area_id, self.context.difficulty);
            }

            // Note: We intentionally DON'T update boss_name from TargetChanged/TargetCleared.
            // The boss encounter context (set by BossEncounterDetected) should persist
            // throughout the fight, regardless of what the player is currently targeting.
            // This ensures timers like "Mighty Leap" work even when the player isn't
            // targeting the boss.
            GameSignal::TargetChanged {
                source_npc_id,
                source_name,
                target_id,
                target_entity_type,
                target_name,
                timestamp,
                ..
            } => {
                // Check for TargetSet triggers (e.g., sphere targeting player)
                signal_handlers::handle_target_set(
                    self,
                    *source_npc_id,
                    *source_name,
                    *target_id,
                    *target_entity_type,
                    *target_name,
                    *timestamp,
                );
            }
            GameSignal::TargetCleared { .. } => {
                // No-op for timer manager
            }

            // ─── Boss Encounter Signals (from EventProcessor) ─────────────────────
            GameSignal::BossEncounterDetected { boss_name, timestamp, .. } => {
                eprintln!("[TIMER] Boss encounter detected: {}", boss_name);
                self.context.boss_name = Some(boss_name.clone());
                // Reset phase and counters for new encounter
                self.current_phase = None;
                self.counters.clear();
                self.boss_hp_by_npc.clear();
                // Start combat-start timers
                signal_handlers::handle_combat_start(self, *timestamp);
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
                    signal_handlers::handle_boss_hp_change(self, *npc_id, entity_name, old_hp, new_hp, *timestamp);
                }
            }

            GameSignal::PhaseChanged { old_phase, new_phase, timestamp, .. } => {
                // Handle the old phase ending first (if any)
                if let Some(ended_phase) = old_phase {
                    eprintln!("[TIMER] Phase ended: {}", ended_phase);
                    signal_handlers::handle_phase_ended(self, ended_phase, *timestamp);
                }

                eprintln!("[TIMER] Phase changed to: {}", new_phase);
                self.current_phase = Some(new_phase.clone());
                // Trigger phase-entered timers
                signal_handlers::handle_phase_change(self, new_phase, *timestamp);
            }

            GameSignal::CounterChanged { counter_id, old_value, new_value, timestamp, .. } => {
                self.counters.insert(counter_id.clone(), *new_value);
                // Trigger counter-based timers
                signal_handlers::handle_counter_change(self, counter_id, *old_value, *new_value, *timestamp);
            }

            _ => {}
        }

        // Check for time-elapsed triggers if we're in combat
        if let Some(ts) = self.last_timestamp {
            signal_handlers::handle_time_elapsed(self, ts);
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
        signal_handlers::clear_combat_timers(self);
    }
}

