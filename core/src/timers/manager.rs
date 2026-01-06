//! Timer management handler
//!
//! Manages boss mechanic and ability cooldown timers.
//! Reacts to signals to start, refresh, and expire timers.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use chrono::{Local, NaiveDateTime};

use crate::combat_log::EntityType;
use crate::context::{IStr, resolve};
use crate::dsl::{BossEncounterDefinition, EntityDefinition};
use crate::signal_processor::{GameSignal, SignalHandler};

use super::matching::{is_definition_active, matches_source_target_filters};
use super::signal_handlers;
use super::{ActiveTimer, TimerDefinition, TimerKey, TimerPreferences, TimerTrigger};

/// Maximum age (in minutes) for events to be processed by timers in live mode.
/// Events older than this are skipped since timers are only useful for recent/live events.
/// This is only checked when `live_mode` is true (after initial batch load).
const TIMER_RECENCY_THRESHOLD_MINS: i64 = 5;

// EncounterContext removed: context now read directly from CombatEncounter

/// A fired alert (ephemeral notification, not a countdown timer)
#[derive(Debug, Clone)]
pub struct FiredAlert {
    pub id: String,
    pub name: String,
    pub text: String,
    pub color: Option<[u8; 4]>,
    pub timestamp: NaiveDateTime,
    /// Whether audio is enabled for this alert
    pub audio_enabled: bool,
    /// Optional custom audio file for this alert (relative path)
    pub audio_file: Option<String>,
}

/// Manages ability cooldown and buff timers.
/// Reacts to signals to start, pause, and reset timers.
#[derive(Debug)]
pub struct TimerManager {
    /// Timer definitions indexed by ID (Arc for cheap cloning in signal handlers)
    pub(super) definitions: HashMap<String, Arc<TimerDefinition>>,

    /// User preferences (color, audio, enabled overrides)
    preferences: TimerPreferences,

    /// Currently active timers (countdown timers with duration > 0)
    pub(super) active_timers: HashMap<TimerKey, ActiveTimer>,

    /// Fired alerts (ephemeral notifications, not countdown timers)
    pub(super) fired_alerts: Vec<FiredAlert>,

    /// Timers that expired this tick (for chaining)
    expired_this_tick: Vec<String>,

    /// Timers that started this tick (for counter triggers)
    started_this_tick: Vec<String>,

    /// Whether we're currently in combat
    pub(super) in_combat: bool,

    /// Last known game timestamp
    last_timestamp: Option<NaiveDateTime>,

    /// When true, apply recency threshold to skip old events.
    live_mode: bool,

    // ─── Entity Filter State ─────────────────────────────────────────────────
    /// Local player's entity ID (for LocalPlayer filter)
    pub(super) local_player_id: Option<i64>,

    /// Boss entity IDs currently in combat (for Boss filter)
    /// These are runtime entity IDs (log_id), not NPC class IDs
    pub(super) boss_entity_ids: HashSet<i64>,

    /// Boss NPC class IDs for the active encounter (to detect additional boss entities)
    /// When NPCs with these class IDs are first seen, add their entity_id to boss_entity_ids
    boss_npc_class_ids: HashSet<i64>,
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
            preferences: TimerPreferences::new(),
            active_timers: HashMap::new(),
            fired_alerts: Vec::new(),
            expired_this_tick: Vec::new(),
            started_this_tick: Vec::new(),
            in_combat: false,
            last_timestamp: None,
            live_mode: true, // Default: apply recency threshold (skip old events)
            local_player_id: None,
            boss_entity_ids: HashSet::new(),
            boss_npc_class_ids: HashSet::new(),
        }
    }

    /// Load timer preferences from a file
    pub fn load_preferences(
        &mut self,
        path: &std::path::Path,
    ) -> Result<(), super::PreferencesError> {
        self.preferences = TimerPreferences::load(path)?;
        eprintln!(
            "TimerManager: loaded {} timer preferences",
            self.preferences.timers.len()
        );
        Ok(())
    }

    /// Set timer preferences directly
    pub fn set_preferences(&mut self, preferences: TimerPreferences) {
        self.preferences = preferences;
    }

    /// Get a reference to current preferences
    pub fn preferences(&self) -> &TimerPreferences {
        &self.preferences
    }

    /// Get a mutable reference to preferences (for updating)
    pub fn preferences_mut(&mut self) -> &mut TimerPreferences {
        &mut self.preferences
    }

    /// Clear boss NPC class IDs (called when encounter ends)
    pub(super) fn clear_boss_npc_class_ids(&mut self) {
        self.boss_npc_class_ids.clear();
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
                self.definitions.insert(def.id.clone(), Arc::new(def));
            }
        }
        if duplicate_count > 0 {
            eprintln!(
                "TimerManager: loaded {} enabled definitions ({} duplicates skipped)",
                self.definitions.len(),
                duplicate_count
            );
        } else {
            eprintln!(
                "TimerManager: loaded {} enabled definitions",
                self.definitions.len()
            );
        }

        // Validate timer chain references
        self.validate_timer_chains();
    }

    /// Alias for load_definitions (matches effect tracker API)
    pub fn set_definitions(&mut self, definitions: Vec<TimerDefinition>) {
        self.load_definitions(definitions);
    }

    /// Load boss definitions and extract their timer definitions.
    /// Only the timer definitions are stored - boss definitions are managed by SessionCache.
    pub fn load_boss_definitions(&mut self, bosses: Vec<BossEncounterDefinition>) {
        // Clear existing boss-related timer definitions (keep generic ones)
        // We'll re-add them from the fresh boss definitions
        self.definitions
            .retain(|id, _| !id.contains('_') || id.starts_with("generic_"));

        let mut timer_count = 0;
        let mut duplicate_count = 0;
        let boss_count = bosses.len();

        for boss in bosses {
            // Extract boss timers and convert to TimerDefinition
            for boss_timer in &boss.timers {
                if boss_timer.enabled {
                    let timer_def =
                        boss_timer.to_timer_definition(boss.area_id, &boss.area_name, &boss.name);

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

                    self.definitions
                        .insert(timer_def.id.clone(), Arc::new(timer_def));
                    timer_count += 1;
                }
            }
        }

        if duplicate_count > 0 {
            eprintln!(
                "TimerManager: extracted {} timers from {} bosses ({} DUPLICATES SKIPPED)",
                timer_count, boss_count, duplicate_count
            );
        } else {
            eprintln!(
                "TimerManager: extracted {} timers from {} boss definitions",
                timer_count, boss_count
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
                && !self.definitions.contains_key(chain_to)
            {
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

    /// Tick to process timer expirations based on real time.
    /// Call periodically to update timers even without new signals.
    /// Note: Called without encounter context (for render updates).
    pub fn tick(&mut self) {
        if let Some(ts) = self.last_timestamp {
            self.process_expirations(ts, None);
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

    /// Check all active timers for countdown announcements
    ///
    /// Returns a list of (timer_name, seconds, voice_pack) for each countdown that should be announced.
    /// This mutates the timers to mark countdowns as announced so they won't repeat.
    /// Uses realtime (system Instant) for accurate audio synchronization.
    /// Skips timers with audio_enabled=false.
    pub fn check_all_countdowns(&mut self) -> Vec<(String, u8, String)> {
        self.active_timers
            .values_mut()
            .filter(|timer| timer.audio_enabled)
            .filter_map(|timer| {
                timer
                    .check_countdown()
                    .map(|secs| (timer.name.clone(), secs, timer.countdown_voice.clone()))
            })
            .collect()
    }

    /// Check all active timers for audio offset triggers
    ///
    /// Returns FiredAlerts for timers where remaining time crossed below audio_offset.
    /// This is for "early warning" sounds that play before the timer expires.
    /// Skips timers with audio_enabled=false.
    pub fn check_audio_offsets(&mut self) -> Vec<FiredAlert> {
        let now = Local::now().naive_local();
        self.active_timers
            .values_mut()
            .filter(|timer| timer.audio_enabled)
            .filter_map(|timer| {
                if timer.check_audio_offset() {
                    Some(FiredAlert {
                        id: timer.definition_id.clone(),
                        name: timer.name.clone(),
                        text: timer.name.clone(),
                        color: Some(timer.color),
                        timestamp: now,
                        audio_enabled: true, // Already filtered by audio_enabled
                        audio_file: timer.audio_file.clone(),
                    })
                } else {
                    None
                }
            })
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

    /// Get timer IDs that expired this tick (for counter triggers).
    /// Unlike take_fired_alerts, this returns a clone since the IDs are
    /// also used internally for timer chaining.
    pub fn expired_timer_ids(&self) -> Vec<String> {
        self.expired_this_tick.clone()
    }

    /// Get timer IDs that started this tick (for counter triggers).
    pub fn started_timer_ids(&self) -> Vec<String> {
        self.started_this_tick.clone()
    }

    /// Check if a timer definition is active for current encounter context.
    /// Reads context directly from the encounter (single source of truth).
    /// Also checks preference override for enabled state.
    pub(super) fn is_definition_active(
        &self,
        def: &TimerDefinition,
        encounter: Option<&crate::encounter::CombatEncounter>,
    ) -> bool {
        // Check preference override first - user can disable timers via preferences
        if !self.preferences.is_enabled(def) {
            return false;
        }
        is_definition_active(def, encounter)
    }

    /// Start a timer from a definition
    pub(super) fn start_timer(
        &mut self,
        def: &TimerDefinition,
        timestamp: NaiveDateTime,
        target_id: Option<i64>,
    ) {
        // Apply preference overrides
        let color = self.preferences.get_color(def);
        let audio_enabled = self.preferences.is_audio_enabled(def);
        let audio_file = self.preferences.get_audio_file(def);

        // Alerts are ephemeral notifications, not countdown timers
        if def.is_alert {
            self.fired_alerts.push(FiredAlert {
                id: def.id.clone(),
                name: def.name.clone(),
                text: def.alert_text.clone().unwrap_or_else(|| def.name.clone()),
                color: Some(color),
                timestamp,
                audio_enabled,
                audio_file,
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

        // Build audio config with preference overrides
        let audio_with_prefs = crate::dsl::AudioConfig {
            enabled: audio_enabled,
            file: audio_file,
            offset: def.audio.offset,
            countdown_start: def.audio.countdown_start,
            countdown_voice: def.audio.countdown_voice.clone(),
        };

        // Create new timer
        let timer = ActiveTimer::new(
            def.id.clone(),
            def.name.clone(),
            target_id,
            timestamp,
            Duration::from_secs_f32(def.duration_secs),
            def.repeats,
            color,
            def.triggers_timer.clone(),
            def.show_on_raid_frames,
            def.show_at_secs,
            &audio_with_prefs,
        );

        self.active_timers.insert(key.clone(), timer);

        // Track that this timer started (for counter triggers)
        self.started_this_tick.push(def.id.clone());

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
            self.active_timers.remove(&key);
        }
    }

    /// Cancel active timers whose cancel_trigger matches the given predicate
    pub(super) fn cancel_timers_matching<F>(&mut self, trigger_matches: F, _reason: &str)
    where
        F: Fn(&TimerTrigger) -> bool,
    {
        let keys_to_cancel: Vec<_> = self
            .active_timers
            .iter()
            .filter_map(|(key, timer)| {
                if let Some(def) = self.definitions.get(&timer.definition_id)
                    && let Some(ref cancel_trigger) = def.cancel_trigger
                    && trigger_matches(cancel_trigger)
                {
                    Some(key.clone())
                } else {
                    None
                }
            })
            .collect();

        for key in keys_to_cancel {
            self.active_timers.remove(&key);
        }
    }

    /// Process timer expirations, repeats, and chains
    fn process_expirations(
        &mut self,
        current_time: NaiveDateTime,
        encounter: Option<&crate::encounter::CombatEncounter>,
    ) {
        self.expired_this_tick.clear();
        // Note: started_this_tick is cleared at the START of handle_signal,
        // not here, because timer starts happen BEFORE process_expirations.

        // Find expired timer keys
        let expired_keys: Vec<_> = self
            .active_timers
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
            } else if let Some(timer) = self.active_timers.remove(&key) {
                // Timer exhausted repeats - fire expiration alert if audio is configured
                // Only fire on expiration if audio_offset == 0 (otherwise it already played at offset)
                // Skip if audio_enabled == false
                if timer.audio_enabled && timer.audio_file.is_some() && timer.audio_offset == 0 {
                    self.fired_alerts.push(FiredAlert {
                        id: timer.definition_id.clone(),
                        name: timer.name.clone(),
                        text: timer.name.clone(),
                        color: Some(timer.color),
                        timestamp: current_time,
                        audio_enabled: true, // Already checked above
                        audio_file: timer.audio_file.clone(),
                    });
                }
                // Prepare chain to next timer
                if let Some(next_timer_id) = timer.triggers_timer.clone() {
                    chains_to_start.push((next_timer_id, timer.target_entity_id));
                }
            }
        }

        // Start chained timers (outside the borrow)
        for (next_timer_id, target_id) in chains_to_start {
            if let Some(next_def) = self.definitions.get(&next_timer_id).cloned()
                && self.is_definition_active(&next_def, encounter)
            {
                self.start_timer(&next_def, current_time, target_id);
            }
        }

        // Check for timers triggered by expirations
        let expired_ids = self.expired_this_tick.clone();
        for expired_id in expired_ids {
            let matching: Vec<_> = self
                .definitions
                .values()
                .filter(|d| {
                    d.matches_timer_expires(&expired_id) && self.is_definition_active(d, encounter)
                })
                .cloned()
                .collect();

            for def in matching {
                self.start_timer(&def, current_time, None);
            }
        }
    }

    // ─── Entity Filter Matching (delegates to matching module) ─────────────────

    /// Check if source/target filters pass for a trigger
    pub(super) fn matches_source_target_filters(
        &self,
        trigger: &TimerTrigger,
        entities: &[EntityDefinition],
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
            trigger,
            entities,
            source_id,
            source_type,
            source_name,
            source_npc_id,
            target_id,
            target_type,
            target_name,
            target_npc_id,
            self.local_player_id,
            &self.boss_entity_ids,
        )
    }
}

impl SignalHandler for TimerManager {
    fn handle_signal(
        &mut self,
        signal: &GameSignal,
        encounter: Option<&crate::encounter::CombatEncounter>,
    ) {
        // ─── Context-setting signals: always process (bypass recency filter) ───
        // These establish context for future timer matching, not trigger timers directly.
        match signal {
            GameSignal::PlayerInitialized { entity_id, .. } => {
                self.local_player_id = Some(*entity_id);
                return;
            }
            // AreaEntered: Context is now read from CombatEncounter directly
            GameSignal::AreaEntered { .. } => return,
            _ => {}
        }

        // Skip timer-triggering signals if no definitions loaded
        if self.definitions.is_empty() {
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

        // Clear started_this_tick at the start of each signal processing.
        // Timer starts accumulate during matching below, then we read them after handle_signal.
        self.started_this_tick.clear();

        match signal {
            // Context signals already handled above
            GameSignal::PlayerInitialized { .. } | GameSignal::AreaEntered { .. } => {}

            GameSignal::AbilityActivated {
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
            } => {
                signal_handlers::handle_ability(
                    self,
                    encounter,
                    *ability_id,
                    *ability_name,
                    *source_id,
                    *source_entity_type,
                    *source_name,
                    *source_npc_id,
                    *target_id,
                    *target_entity_type,
                    *target_name,
                    *target_npc_id,
                    *timestamp,
                );
            }

            GameSignal::EffectApplied {
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
                ..
            } => {
                signal_handlers::handle_effect_applied(
                    self,
                    encounter,
                    *effect_id,
                    resolve(*effect_name),
                    *source_id,
                    *source_entity_type,
                    *source_name,
                    *source_npc_id,
                    *target_id,
                    *target_entity_type,
                    *target_name,
                    *target_npc_id,
                    *timestamp,
                );
            }

            GameSignal::EffectRemoved {
                effect_id,
                effect_name,
                source_id,
                source_entity_type,
                source_name,
                target_id,
                target_entity_type,
                target_name,
                timestamp,
                ..
            } => {
                // EffectRemoved doesn't include npc_ids in the game log, pass 0
                signal_handlers::handle_effect_removed(
                    self,
                    encounter,
                    *effect_id,
                    resolve(*effect_name),
                    *source_id,
                    *source_entity_type,
                    *source_name,
                    0,
                    *target_id,
                    *target_entity_type,
                    *target_name,
                    0,
                    *timestamp,
                );
            }

            GameSignal::CombatStarted { timestamp, .. } => {
                signal_handlers::handle_combat_start(self, encounter, *timestamp);
            }

            GameSignal::CombatEnded { .. } => {
                signal_handlers::clear_combat_timers(self);
            }

            GameSignal::EntityDeath {
                npc_id,
                entity_name,
                timestamp,
                ..
            } => {
                signal_handlers::handle_entity_death(
                    self,
                    encounter,
                    *npc_id,
                    entity_name,
                    *timestamp,
                );
            }

            GameSignal::NpcFirstSeen {
                entity_id,
                npc_id,
                entity_name,
                timestamp,
                ..
            } => {
                // Track boss entities for multi-boss fights (e.g., Zorn & Toth)
                if self.boss_npc_class_ids.contains(npc_id)
                    && !self.boss_entity_ids.contains(entity_id)
                {
                    self.boss_entity_ids.insert(*entity_id);
                }
                signal_handlers::handle_npc_first_seen(
                    self,
                    encounter,
                    *npc_id,
                    entity_name,
                    *timestamp,
                );
            }

            // Note: We intentionally DON'T update boss_name from TargetChanged/TargetCleared.
            // The boss encounter context (set by BossEncounterDetected) should persist
            // throughout the fight, regardless of what the player is currently targeting.
            // This ensures timers like "Mighty Leap" work even when the player isn't
            // targeting the boss.
            GameSignal::TargetChanged {
                source_id,
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
                    encounter,
                    *source_id,
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
                timestamp,
            } => {
                signal_handlers::handle_damage_taken(
                    self,
                    encounter,
                    *ability_id,
                    *ability_name,
                    *source_id,
                    *source_entity_type,
                    *source_name,
                    *source_npc_id,
                    *target_id,
                    *target_entity_type,
                    *target_name,
                    *timestamp,
                );
            }

            // ─── Boss Encounter Signals (from EventProcessor) ─────────────────────
            GameSignal::BossEncounterDetected {
                entity_id,
                boss_npc_class_ids,
                timestamp,
                ..
            } => {
                // Boss name is now read from CombatEncounter.active_boss directly

                // Track boss entity ID for source/target "boss" filter
                self.boss_entity_ids.insert(*entity_id);

                // Store boss NPC class IDs from signal (for tracking additional boss entities
                // in multi-boss fights like Zorn & Toth)
                self.boss_npc_class_ids.clear();
                for &class_id in boss_npc_class_ids {
                    self.boss_npc_class_ids.insert(class_id);
                }

                // Start combat-start timers
                signal_handlers::handle_combat_start(self, encounter, *timestamp);
            }

            GameSignal::BossHpChanged {
                npc_id,
                entity_name,
                old_hp_percent,
                new_hp_percent,
                timestamp,
                ..
            } => {
                // Check for HP threshold timer triggers (using pre-computed percentages from signal)
                if (*old_hp_percent - *new_hp_percent).abs() > 0.01 {
                    signal_handlers::handle_boss_hp_change(
                        self,
                        encounter,
                        *npc_id,
                        entity_name,
                        *old_hp_percent,
                        *new_hp_percent,
                        *timestamp,
                    );
                }
            }

            GameSignal::PhaseChanged {
                old_phase,
                new_phase,
                timestamp,
                ..
            } => {
                // Handle the old phase ending first (if any)
                if let Some(ended_phase) = old_phase {
                    signal_handlers::handle_phase_ended(self, encounter, ended_phase, *timestamp);
                }
                // Trigger phase-entered timers
                signal_handlers::handle_phase_change(self, encounter, new_phase, *timestamp);
            }

            GameSignal::CounterChanged {
                counter_id,
                old_value,
                new_value,
                timestamp,
                ..
            } => {
                // Trigger counter-based timers
                signal_handlers::handle_counter_change(
                    self, encounter, counter_id, *old_value, *new_value, *timestamp,
                );
            }

            _ => {}
        }

        // Check for time-elapsed triggers if we're in combat
        if let Some(ts) = self.last_timestamp {
            signal_handlers::handle_time_elapsed(self, encounter, ts);
        }

        // Process expirations after handling signal
        if let Some(ts) = self.last_timestamp {
            self.process_expirations(ts, encounter);
        }
    }

    fn on_encounter_start(&mut self, _encounter_id: u64) {
        // Could reset encounter-specific state here
    }

    fn on_encounter_end(&mut self, _encounter_id: u64) {
        signal_handlers::clear_combat_timers(self);
    }
}
