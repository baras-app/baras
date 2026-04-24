//! Timer management handler
//!
//! Manages boss mechanic and ability cooldown timers.
//! Reacts to signals to start, refresh, and expire timers.

use tracing;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::NaiveDateTime;

use crate::combat_log::EntityType;
use crate::context::{IStr, resolve};
use crate::dsl::{BossEncounterDefinition, EntityDefinition};
use crate::game_data::{Discipline, Role};
use crate::signal_processor::{GameSignal, SignalHandler};

use super::matching::{
    is_definition_active, is_definition_active_with_snapshot, matches_source_target_filters,
};
use super::signal_handlers;
use super::{ActiveTimer, TimerDefinition, TimerKey, TimerPreferences, TimerTrigger};

use crate::dsl::TriggerKind;

// EncounterContext removed: context now read directly from CombatEncounter

/// A fired alert (ephemeral notification, not a countdown timer)
#[derive(Debug, Clone)]
pub struct FiredAlert {
    pub id: String,
    pub name: String,
    pub text: String,
    pub color: Option<[u8; 4]>,
    pub timestamp: NaiveDateTime,
    /// Whether this alert should display text in the overlay
    pub alert_text_enabled: bool,
    /// Whether audio is enabled for this alert
    pub audio_enabled: bool,
    /// Optional custom audio file for this alert (relative path)
    pub audio_file: Option<String>,
    /// Optional ability ID for icon display in the alerts overlay
    pub icon_ability_id: Option<u64>,
}

/// Manages ability cooldown and buff timers.
/// Reacts to signals to start, pause, and reset timers.
#[derive(Debug)]
pub struct TimerManager {
    /// Timer definitions indexed by ID (Arc for cheap cloning in signal handlers)
    pub(super) definitions: HashMap<String, Arc<TimerDefinition>>,

    /// Per-trigger-kind index: maps each [`TriggerKind`] to the definitions
    /// whose `trigger` (or nested `AnyOf` conditions) can produce that kind.
    /// Rebuilt when definitions change, avoids O(n) full scans per signal.
    trigger_index: HashMap<TriggerKind, Vec<Arc<TimerDefinition>>>,

    /// User preferences (color, audio, enabled overrides)
    preferences: TimerPreferences,

    /// Currently active timers (countdown timers with duration > 0)
    pub(super) active_timers: HashMap<TimerKey, ActiveTimer>,

    /// Active GCD countdown (single slot, replaced on re-fire per D-03)
    pub(super) active_gcd: Option<super::ActiveGcd>,

    /// Fired alerts (ephemeral notifications, not countdown timers)
    pub(super) fired_alerts: Vec<FiredAlert>,

    /// Timers that expired during the current signal (cleared per-signal for
    /// correct chain-trigger logic inside `process_expirations`).
    expired_this_tick: Vec<String>,

    /// Timers that started during the current signal (cleared per-signal).
    started_this_tick: Vec<String>,

    /// Timers that were canceled during the current signal (cleared per-signal).
    canceled_this_tick: Vec<String>,

    /// Timers that transitioned to queued/READY during the current signal.
    queued_this_tick: Vec<String>,

    /// Timers removed from queued state by queue_remove_trigger during the current signal.
    removed_queued_this_tick: Vec<String>,

    /// Batch-level accumulation of expired timer IDs across all signals in a
    /// `handle_signals` or `tick` call. Read by the timer feedback loop.
    batch_expired: Vec<String>,

    /// Batch-level accumulation of started timer IDs.
    batch_started: Vec<String>,

    /// Batch-level accumulation of canceled timer IDs.
    batch_canceled: Vec<String>,

    /// Batch-level accumulation of timers that transitioned to queued/READY state.
    batch_queued: Vec<String>,

    /// Batch-level accumulation of timers removed from queued state.
    batch_removed_queued: Vec<String>,

    /// Whether we're currently in combat
    pub(super) in_combat: bool,

    /// Combat start timestamp (for calculating elapsed time in alerts)
    pub(super) combat_start_time: Option<NaiveDateTime>,

    /// Game-time anchor: the highest game time we've seen (monotonic).
    /// Updated via `advance_game_time_anchor()` which ensures this never
    /// moves backward.
    last_timestamp: Option<NaiveDateTime>,

    /// Monotonic instant when `last_timestamp` was last anchored.
    /// Together with `last_timestamp`, forms a game-time anchor for interpolation.
    last_timestamp_instant: Option<Instant>,

    // ─── Entity Filter State ─────────────────────────────────────────────────
    /// Local player's entity ID (for LocalPlayer filter)
    pub(super) local_player_id: Option<i64>,

    /// Local player's current target entity ID (for CurrentTarget filter)
    pub(super) current_target_id: Option<i64>,

    /// Local player's current role (for role-scoped timer filtering)
    current_role: Option<Role>,

    /// Boss entity IDs currently in combat (for Boss filter)
    /// These are runtime entity IDs (log_id), not NPC class IDs
    pub(super) boss_entity_ids: HashSet<i64>,

    /// Boss NPC class IDs for the active encounter (to detect additional boss entities)
    /// When NPCs with these class IDs are first seen, add their entity_id to boss_entity_ids
    boss_npc_class_ids: HashSet<i64>,

    /// Timer definition IDs that have already been started by combat-time triggers
    /// (CombatStart / TimeElapsed) this combat. Prevents re-creation after cancellation.
    /// Cleared on combat end and encounter change.
    pub(super) combat_time_started: HashSet<String>,

    // ─── Encounter-scoped State (for lazy re-initialization) ─────────────────
    /// Current encounter ID being tracked (for detecting encounter changes)
    /// When this doesn't match the signal's encounter, we reset timer state.
    pub(super) active_encounter_id: Option<u64>,

    /// Fingerprint of loaded definitions (hash of definition IDs + count)
    /// Used to detect when definitions actually change vs. redundant reloads.
    definitions_fingerprint: u64,
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
            trigger_index: HashMap::new(),
            preferences: TimerPreferences::new(),
            active_timers: HashMap::new(),
            active_gcd: None,
            fired_alerts: Vec::new(),
            expired_this_tick: Vec::new(),
            started_this_tick: Vec::new(),
            canceled_this_tick: Vec::new(),
            queued_this_tick: Vec::new(),
            removed_queued_this_tick: Vec::new(),
            batch_expired: Vec::new(),
            batch_started: Vec::new(),
            batch_canceled: Vec::new(),
            batch_queued: Vec::new(),
            batch_removed_queued: Vec::new(),
            in_combat: false,
            combat_start_time: None,
            last_timestamp: None,
            last_timestamp_instant: None,
            local_player_id: None,
            current_target_id: None,
            current_role: None,
            boss_entity_ids: HashSet::new(),
            boss_npc_class_ids: HashSet::new(),
            combat_time_started: HashSet::new(),
            active_encounter_id: None,
            definitions_fingerprint: 0,
        }
    }

    /// Load timer preferences from a file
    pub fn load_preferences(
        &mut self,
        path: &std::path::Path,
    ) -> Result<(), super::PreferencesError> {
        self.preferences = TimerPreferences::load(path)?;
        tracing::debug!(
            count = self.preferences.timers.len(),
            "Loaded timer preferences"
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

    /// Get the current active GCD (if any)
    pub fn active_gcd(&self) -> Option<&super::ActiveGcd> {
        self.active_gcd.as_ref()
    }

    /// Get a mutable reference to preferences (for updating)
    pub fn preferences_mut(&mut self) -> &mut TimerPreferences {
        &mut self.preferences
    }

    /// Clear boss NPC class IDs (called when encounter ends)
    pub(super) fn clear_boss_npc_class_ids(&mut self) {
        self.boss_npc_class_ids.clear();
    }

    /// Clear the definitions fingerprint, forcing the next load to actually reload.
    /// Call this before load_boss_definitions when user explicitly triggers a reload.
    pub fn invalidate_definitions_cache(&mut self) {
        self.definitions_fingerprint = 0;
    }

    /// Compute a fingerprint for a set of boss definitions.
    /// Used to detect if definitions actually changed vs. redundant reloads.
    fn compute_boss_fingerprint(bosses: &[BossEncounterDefinition]) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();

        // Hash boss count and each boss's area_id + name + timer count + timer IDs
        bosses.len().hash(&mut hasher);
        for boss in bosses {
            boss.area_id.hash(&mut hasher);
            boss.name.hash(&mut hasher);
            boss.timers.len().hash(&mut hasher);
            for timer in &boss.timers {
                timer.id.hash(&mut hasher);
                timer.enabled.hash(&mut hasher);
            }
        }

        hasher.finish()
    }

    /// Format alert text for display
    fn format_alert_text(&self, text: &str, _timestamp: NaiveDateTime) -> String {
        text.to_string()
    }

    /// Load timer definitions
    pub fn load_definitions(&mut self, definitions: Vec<TimerDefinition>) {
        self.definitions.clear();
        let mut duplicate_count = 0;
        for def in definitions {
            if def.enabled {
                if let Some(existing) = self.definitions.get(&def.id) {
                    tracing::warn!(
                        timer_id = %def.id,
                        first_name = %existing.name,
                        duplicate_name = %def.name,
                        "Duplicate timer ID, keeping first"
                    );
                    duplicate_count += 1;
                    continue;
                }
                self.definitions.insert(def.id.clone(), Arc::new(def));
            }
        }
        if duplicate_count > 0 {
            tracing::info!(
                count = self.definitions.len(),
                duplicates_skipped = duplicate_count,
                "Loaded enabled timer definitions"
            );
        } else {
            tracing::info!(
                count = self.definitions.len(),
                "Loaded enabled timer definitions"
            );
        }

        // Validate timer chain references
        self.validate_timer_chains();

        // Rebuild per-trigger-kind index
        self.rebuild_trigger_index();
    }

    /// Alias for load_definitions (matches effect tracker API)
    pub fn set_definitions(&mut self, definitions: Vec<TimerDefinition>) {
        self.load_definitions(definitions);
    }

    /// Load boss definitions and extract their timer definitions.
    /// Only the timer definitions are stored - boss definitions are managed by SessionCache.
    ///
    /// Returns true if definitions were actually loaded (changed), false if skipped (same fingerprint).
    pub fn load_boss_definitions(&mut self, bosses: Vec<BossEncounterDefinition>) -> bool {
        // Check fingerprint to avoid redundant reloads
        let new_fingerprint = Self::compute_boss_fingerprint(&bosses);
        if new_fingerprint == self.definitions_fingerprint && !self.definitions.is_empty() {
            // Definitions haven't changed - skip reload
            return false;
        }

        // Clear existing boss-related timer definitions (keep generic ones)
        // We'll re-add them from the fresh boss definitions
        self.definitions
            .retain(|id, _| !id.contains('_') || id.starts_with("generic_"));

        let mut timer_count = 0;
        let mut duplicate_count = 0;
        let boss_count = bosses.len();

        for boss in bosses {
            // Skip entirely disabled boss definitions
            if !boss.enabled {
                tracing::debug!(boss_id = %boss.id, "Skipping disabled boss definition");
                continue;
            }
            // Extract boss timers and convert to TimerDefinition
            for boss_timer in &boss.timers {
                if boss_timer.enabled {
                    let timer_def =
                        boss_timer.to_timer_definition(boss.area_id, &boss.area_name, &boss.name, &boss.id);

                    // Check for duplicate ID - warn and skip instead of silent overwrite
                    if let Some(existing) = self.definitions.get(&timer_def.id) {
                        tracing::warn!(
                            timer_id = %timer_def.id,
                            first_name = %existing.name,
                            first_boss = %existing.boss.as_deref().unwrap_or("unknown"),
                            duplicate_name = %timer_def.name,
                            duplicate_boss = %boss.name,
                            "Duplicate timer ID, keeping first"
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

        // Update fingerprint
        self.definitions_fingerprint = new_fingerprint;

        if duplicate_count > 0 {
            tracing::info!(
                timer_count,
                boss_count,
                duplicates_skipped = duplicate_count,
                "Extracted timers from boss definitions"
            );
        } else {
            tracing::info!(
                timer_count,
                boss_count,
                "Extracted timers from boss definitions"
            );
        }

        // Validate timer chain references
        self.validate_timer_chains();

        // Rebuild per-trigger-kind index
        self.rebuild_trigger_index();

        // No special mid-combat handling needed: combat_start and time_elapsed
        // triggers are evaluated continuously by handle_combat_time_triggers
        // on every tick/event. When definitions load mid-combat, the next
        // tick will pick them up and backdate start timestamps correctly.

        true
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
            tracing::warn!(
                count = broken_chains.len(),
                "Broken timer chain references found"
            );
            for (timer_id, missing_ref) in &broken_chains {
                tracing::warn!(
                    timer_id = %timer_id,
                    chains_to = %missing_ref,
                    "Timer chains to non-existent target"
                );
            }
        }
    }

    /// Rebuild the per-trigger-kind index from current definitions.
    ///
    /// Called automatically after `load_definitions` / `load_boss_definitions`.
    fn rebuild_trigger_index(&mut self) {
        self.trigger_index.clear();
        let mut kinds_buf = Vec::new();
        let mut seen = HashSet::new();
        for def in self.definitions.values() {
            kinds_buf.clear();
            seen.clear();
            def.trigger.collect_kinds(&mut kinds_buf);
            for &kind in &kinds_buf {
                // Deduplicate kinds (AnyOf can produce repeats)
                if seen.insert(kind) {
                    self.trigger_index
                        .entry(kind)
                        .or_default()
                        .push(Arc::clone(def));
                }
            }
        }
    }

    /// Return definitions that could match the given trigger kind.
    ///
    /// Falls back to an empty slice when no definitions are registered for
    /// that kind, avoiding allocation.
    pub(super) fn definitions_for_kind(&self, kind: TriggerKind) -> &[Arc<TimerDefinition>] {
        static EMPTY: &[Arc<TimerDefinition>] = &[];
        self.trigger_index.get(&kind).map_or(EMPTY, |v| v.as_slice())
    }

    /// Tick to process timer expirations and time-elapsed triggers.
    /// Call periodically to update timers even without new signals.
    /// Pass the current encounter context to allow timer restarts.
    ///
    /// Uses interpolated game time (game clock + monotonic elapsed) to determine
    /// which timers have expired, avoiding cross-clock comparison with the system clock.
    pub fn tick(&mut self, encounter: Option<&crate::encounter::CombatEncounter>) {
        if let Some(ts) = self.last_timestamp {
            // Clear per-signal and batch vectors (tick is a standalone entry point)
            self.started_this_tick.clear();
            self.canceled_this_tick.clear();
            self.expired_this_tick.clear();
            self.queued_this_tick.clear();
            self.removed_queued_this_tick.clear();
            self.batch_expired.clear();
            self.batch_started.clear();
            self.batch_canceled.clear();
            self.batch_queued.clear();
            self.batch_removed_queued.clear();

            // Evaluate combat-time triggers (CombatStart + TimeElapsed) so they
            // fire even during idle periods (no combat events arriving).
            signal_handlers::handle_combat_time_triggers(self, encounter);

            // Use interpolated game time to check expirations
            let interp_time = self.interpolated_game_time().unwrap_or(ts);

            // Prune expired GCD
            if let Some(ref gcd) = self.active_gcd {
                if gcd.has_expired(interp_time) {
                    self.active_gcd = None;
                }
            }

            self.process_expirations(interp_time, encounter);

            // Process cancellation chains (timer_canceled triggers)
            for timer_id in self.canceled_this_tick.clone() {
                self.start_timers_on_cancel(&timer_id, ts, encounter);
                self.cancel_timers_on_cancel(&timer_id);
            }

            // Populate batch vectors so accessors return tick's events
            self.batch_expired.extend(self.expired_this_tick.drain(..));
            self.batch_started.extend(self.started_this_tick.drain(..));
            self.batch_canceled.extend(self.canceled_this_tick.drain(..));
            self.batch_queued.extend(self.queued_this_tick.drain(..));
            self.batch_removed_queued.extend(self.removed_queued_this_tick.drain(..));
        }
    }

    /// Get all currently active timers (for overlay rendering)
    pub fn active_timers(&self) -> Vec<&ActiveTimer> {
        self.active_timers.values().collect()
    }

    /// Look up a timer definition's display name by its ID.
    pub fn definition_name(&self, id: &str) -> Option<&str> {
        self.definitions.get(id).map(|def| def.name.as_str())
    }

    /// Look up a timer definition's duration in seconds by its ID.
    pub fn definition_duration(&self, id: &str) -> Option<f32> {
        self.definitions.get(id).map(|def| def.duration_secs)
    }

    /// Compute an interpolated game time for smooth display between log events.
    ///
    /// Takes the last game timestamp we received and advances it by the wall time
    /// elapsed since we received it. This stays in SWTOR's clock domain (no cross-clock
    /// comparison) and provides smooth countdown between log events.
    ///
    /// Returns `None` if no game timestamp has been received yet.
    pub fn interpolated_game_time(&self) -> Option<NaiveDateTime> {
        let game_time = self.last_timestamp?;
        let received_at = self.last_timestamp_instant?;
        let elapsed = received_at.elapsed();
        Some(game_time + chrono::Duration::milliseconds(elapsed.as_millis() as i64))
    }

    /// Advance the game-time anchor to at least `event_timestamp`.
    ///
    /// Uses a monotonic high-water-mark: the new anchor is
    /// `max(event_timestamp, current_interpolated_time)`. This prevents
    /// interpolated game time from jumping backward when a batch of events
    /// arrives, and naturally absorbs processing latency.
    fn advance_game_time_anchor(&mut self, event_timestamp: NaiveDateTime) {
        let now = Instant::now();
        let anchor_time = match (self.last_timestamp, self.last_timestamp_instant) {
            (Some(gt), Some(inst)) => {
                let interp = gt + chrono::Duration::milliseconds(inst.elapsed().as_millis() as i64);
                if event_timestamp > interp { event_timestamp } else { interp }
            }
            _ => event_timestamp,
        };
        self.last_timestamp = Some(anchor_time);
        self.last_timestamp_instant = Some(now);
    }

    /// Build a snapshot of timer remaining seconds keyed by definition_id.
    /// For per-target timers with multiple instances, uses the maximum remaining time
    /// (any instance active = most time remaining wins).
    /// Used to populate CombatEncounter.timer_remaining for condition evaluation.
    /// Returns a hashbrown::HashMap to match CombatEncounter's field type.
    ///
    /// Uses interpolated game time for accurate remaining values that account for
    /// processing delay without comparing SWTOR's clock to the system clock.
    pub fn timer_remaining_snapshot(&self) -> hashbrown::HashMap<String, f32> {
        let game_time = self.interpolated_game_time();
        let mut snapshot = hashbrown::HashMap::new();
        if let Some(game_time) = game_time {
            for timer in self.active_timers.values() {
                let remaining = timer.remaining_secs(game_time);
                if remaining > 0.0 {
                    let entry = snapshot
                        .entry(timer.definition_id.clone())
                        .or_insert(0.0f32);
                    if remaining > *entry {
                        *entry = remaining;
                    }
                }
            }
        }
        snapshot
    }

    /// Build a timer remaining snapshot using an explicit game time.
    /// Use this for replay/validation where the caller controls the clock.
    pub fn timer_remaining_snapshot_at(
        &self,
        game_time: NaiveDateTime,
    ) -> hashbrown::HashMap<String, f32> {
        let mut snapshot = hashbrown::HashMap::new();
        for timer in self.active_timers.values() {
            let remaining = timer.remaining_secs(game_time);
            if remaining > 0.0 {
                let entry = snapshot
                    .entry(timer.definition_id.clone())
                    .or_insert(0.0f32);
                if remaining > *entry {
                    *entry = remaining;
                }
            }
        }
        snapshot
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
    /// Uses interpolated game time for accurate audio synchronization without clock skew.
    /// Skips timers with audio_enabled=false.
    pub fn check_all_countdowns(&mut self) -> Vec<(String, u8, String)> {
        let Some(interp_time) = self.interpolated_game_time() else {
            return Vec::new();
        };
        self.active_timers
            .values_mut()
            .filter(|timer| !timer.role_hidden && timer.audio_enabled)
            .filter_map(|timer| {
                let remaining = timer.remaining_secs(interp_time);
                timer
                    .check_countdown(remaining)
                    .map(|secs| (timer.name.clone(), secs, timer.countdown_voice.clone()))
            })
            .collect()
    }

    /// Check all active timers for audio offset triggers
    ///
    /// Returns FiredAlerts for timers where remaining time crossed below audio_offset.
    /// This is for "early warning" sounds that play before the timer expires.
    /// Uses interpolated game time for accurate timing without clock skew.
    /// Skips timers with audio_enabled=false.
    pub fn check_audio_offsets(&mut self) -> Vec<FiredAlert> {
        let Some(interp_time) = self.interpolated_game_time() else {
            return Vec::new();
        };

        // Collect timer data first (can't call format_alert_text while iterating mutably)
        let triggered: Vec<_> = self
            .active_timers
            .values_mut()
            .filter_map(|timer| {
                let remaining = timer.remaining_secs(interp_time);
                if !timer.role_hidden && timer.audio_enabled && timer.check_audio_offset(remaining) {
                    Some((
                        timer.definition_id.clone(),
                        timer.name.clone(),
                        timer.color,
                        timer.audio_file.clone(),
                        timer.icon_ability_id,
                    ))
                } else {
                    None
                }
            })
            .collect();

        // Now format with elapsed time
        triggered
            .into_iter()
            .map(|(id, name, color, audio_file, icon_ability_id)| {
                let text = self.format_alert_text(&name, interp_time);
                FiredAlert {
                    id,
                    name,
                    text,
                    color: Some(color),
                    timestamp: interp_time,
                    alert_text_enabled: false,
                    audio_enabled: true,
                    audio_file,
                    icon_ability_id,
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

    /// Get timer IDs that expired during the last `handle_signal` call.
    /// For per-signal consumers (e.g. validation tool).
    pub fn expired_timer_ids(&self) -> &[String] {
        &self.expired_this_tick
    }

    /// Get timer IDs that started during the last `handle_signal` call.
    pub fn started_timer_ids(&self) -> &[String] {
        &self.started_this_tick
    }

    /// Get timer IDs that were canceled during the last `handle_signal` call.
    pub fn canceled_timer_ids(&self) -> &[String] {
        &self.canceled_this_tick
    }

    /// Get timer IDs that expired across the entire `handle_signals` batch.
    /// Used by the timer feedback loop in `parser.rs`.
    pub fn batch_expired_timer_ids(&self) -> &[String] {
        &self.batch_expired
    }

    /// Get timer IDs that started across the entire `handle_signals` batch.
    pub fn batch_started_timer_ids(&self) -> &[String] {
        &self.batch_started
    }

    /// Get timer IDs that were canceled across the entire `handle_signals` batch.
    pub fn batch_canceled_timer_ids(&self) -> &[String] {
        &self.batch_canceled
    }

    /// Get timer IDs that transitioned to queued/READY state across the batch.
    pub fn batch_queued_timer_ids(&self) -> &[String] {
        &self.batch_queued
    }

    /// Get timer IDs that were cleared from queued state via queue_remove_trigger.
    pub fn batch_removed_queued_timer_ids(&self) -> &[String] {
        &self.batch_removed_queued
    }

    /// Check if a timer definition is active for current encounter context.
    /// Reads context directly from the encounter (single source of truth).
    /// Also checks preference override for enabled state.
    ///
    /// When the definition uses `TimerTimeRemaining` conditions, a fresh
    /// snapshot is computed from `active_timers` so conditions see current
    /// timer state rather than the potentially stale snapshot cached on the
    /// encounter from before signal dispatch.
    pub(super) fn is_definition_active(
        &self,
        def: &TimerDefinition,
        encounter: Option<&crate::encounter::CombatEncounter>,
    ) -> bool {
        // Check preference override first - user can disable timers via preferences
        if !self.preferences.is_enabled(def) {
            return false;
        }

        // If the definition uses TimerTimeRemaining conditions, compute a
        // fresh snapshot from active_timers instead of relying on the
        // encounter's cached snapshot (which may be stale mid-processing).
        if def.conditions.iter().any(|c| c.uses_timer_time_remaining()) {
            let now = self.last_timestamp.unwrap_or_else(|| chrono::Local::now().naive_local());
            let snapshot = self.timer_remaining_snapshot_at(now);
            is_definition_active_with_snapshot(def, encounter, &snapshot)
        } else {
            is_definition_active(def, encounter)
        }
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
        let role_hidden = !self.preferences.is_role_visible(def, self.current_role);

        // Determine if we should fire an alert on start
        let should_alert_on_start = !role_hidden
            && (def.is_alert || matches!(def.alert_on, baras_types::AlertTrigger::OnApply));

        // Fire start alert if needed (instant alerts always fire, or alert_on == OnApply)
        if should_alert_on_start {
            let raw_text = def.alert_text.clone().unwrap_or_else(|| def.name.clone());
            let text = self.format_alert_text(&raw_text, timestamp);
            // For instant alerts, audio fires with the alert since there's no timer lifecycle.
            // For regular timers, audio fires independently via offset/countdown/expiration,
            // so we don't attach it here — AlertOn only controls the text alert overlay.
            let (alert_audio_enabled, alert_audio_file) = if def.is_alert {
                (audio_enabled, audio_file.clone())
            } else {
                (false, None)
            };
            self.fired_alerts.push(FiredAlert {
                id: def.id.clone(),
                name: def.name.clone(),
                text,
                color: Some(color),
                timestamp,
                alert_text_enabled: true,
                audio_enabled: alert_audio_enabled,
                audio_file: alert_audio_file,
                icon_ability_id: def.icon_ability_id,
            });
        }

        // Instant alerts are ephemeral - no countdown timer created
        if def.is_alert {
            self.started_this_tick.push(def.id.clone());
            self.cancel_timers_on_start(&def.id);
            return;
        }

        let key = TimerKey::new(&def.id, target_id);

        // Check if timer already exists
        let existing_is_queued = self.active_timers.get(&key).map(|t| t.is_queued);
        match existing_is_queued {
            Some(true) => {
                // Queued/ready timer: always restart fresh when trigger fires again.
                // Remove the queued entry and fall through to create a new countdown.
                self.active_timers.remove(&key);
            }
            Some(false) => {
                // Running timer: refresh if allowed, otherwise ignore
                if def.can_be_refreshed {
                    let existing = self.active_timers.get_mut(&key).unwrap();
                    existing.refresh(timestamp);
                    if let Some(secs) = def.gcd_secs {
                        self.active_gcd = Some(super::ActiveGcd::new(timestamp, secs));
                    }
                    self.cancel_timers_on_start(&def.id);
                }
                return;
            }
            None => {} // Timer doesn't exist yet — fall through to create
        }

        // Build audio config with preference overrides
        let audio_with_prefs = crate::dsl::AudioConfig {
            enabled: audio_enabled,
            file: audio_file,
            offset: def.audio.offset,
            countdown_start: def.audio.countdown_start,
            countdown_voice: def.audio.countdown_voice.clone(),
            alert_text: def.audio.alert_text.clone(),
        };

        // Create new timer
        let alert_on_expire = matches!(def.alert_on, baras_types::AlertTrigger::OnExpire);
        let timer = ActiveTimer::new(
            def.id.clone(),
            def.name.clone(),
            target_id,
            timestamp,
            Duration::from_secs_f32(def.duration_secs),
            def.repeats,
            color,
            def.icon_ability_id,
            def.triggers_timer.clone(),
            def.show_on_raid_frames,
            def.show_at_secs,
            &audio_with_prefs,
            def.display_target,
            alert_on_expire,
            def.alert_text.clone(),
            role_hidden,
            def.queue_on_expire,
            def.queue_priority,
            def.queue_blocking_timers.clone(),
            def.queue_countdown_bar,
            def.queue_hide_from_next,
        );
        self.active_timers.insert(key, timer);

        // Create GCD entry if this timer has gcd_secs configured (per D-02)
        // Replace policy: any existing GCD is overwritten (per D-03)
        if let Some(secs) = def.gcd_secs {
            self.active_gcd = Some(super::ActiveGcd::new(timestamp, secs));
        }

        // Track that this timer started (for counter triggers)
        self.started_this_tick.push(def.id.clone());

        // Cancel any timers that have cancel_on_timer pointing to this timer
        self.cancel_timers_on_start(&def.id);
    }

    /// Cancel active timers that have cancel_on_timer matching the started timer ID
    fn cancel_timers_on_start(&mut self, started_timer_id: &str) {
        // Collect keys to cancel - we need the full key for HashMap::remove
        let keys_to_cancel: Vec<_> = self.active_timers
            .iter()
            .filter_map(|(key, timer)| {
                if let Some(def) = self.definitions.get(&timer.definition_id)
                    && let Some(ref cancel_trigger) = def.cancel_trigger
                    && cancel_trigger.matches_timer_started(started_timer_id) {
                        Some(key.clone())
                    } else {
                        None
                    }
            })
            .collect();

        // Track cancellations and remove timers
        for key in keys_to_cancel {
            self.active_timers.remove(&key);
            // Move key.definition_id into canceled_this_tick (avoids extra clone)
            self.canceled_this_tick.push(key.definition_id);
        }

        // Also remove queued entries whose queue_remove_trigger matches timer started
        let keys_to_remove: Vec<_> = self.active_timers
            .iter()
            .filter_map(|(key, timer)| {
                if timer.is_queued
                    && let Some(def) = self.definitions.get(&timer.definition_id)
                    && let Some(ref remove_trigger) = def.queue_remove_trigger
                    && remove_trigger.matches_timer_started(started_timer_id)
                {
                    Some(key.clone())
                } else {
                    None
                }
            })
            .collect();

        for key in keys_to_remove {
            self.active_timers.remove(&key);
            self.canceled_this_tick.push(key.definition_id);
        }
    }

    /// Cancel active timers whose cancel_trigger matches the given predicate
    pub(super) fn cancel_timers_matching<F>(&mut self, trigger_matches: F)
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

        // Track cancellations and remove timers
        for key in keys_to_cancel {
            self.active_timers.remove(&key);
            // Move key.definition_id into canceled_this_tick (avoids extra clone)
            self.canceled_this_tick.push(key.definition_id);
        }
    }

    /// Cancel active timers whose cancel_trigger matches the given predicate (with entity roster)
    pub(super) fn cancel_timers_matching_with_entities<F>(
        &mut self,
        entities: &[crate::dsl::EntityDefinition],
        trigger_matches: F,
    ) where
        F: Fn(&TimerTrigger, &[crate::dsl::EntityDefinition]) -> bool,
    {
        let keys_to_cancel: Vec<_> = self
            .active_timers
            .iter()
            .filter_map(|(key, timer)| {
                if let Some(def) = self.definitions.get(&timer.definition_id)
                    && let Some(ref cancel_trigger) = def.cancel_trigger
                    && trigger_matches(cancel_trigger, entities)
                {
                    Some(key.clone())
                } else {
                    None
                }
            })
            .collect();

        // Track cancellations and remove timers
        for key in keys_to_cancel {
            self.active_timers.remove(&key);
            // Move key.definition_id into canceled_this_tick (avoids extra clone)
            self.canceled_this_tick.push(key.definition_id);
        }
    }

    /// Cancel active timers whose cancel_trigger matches the given predicate,
    /// also enforcing source/target entity filters on the cancel trigger.
    ///
    /// This is the entity-aware counterpart to `cancel_timers_matching`. It should
    /// be used for event-driven cancels (effect applied/removed, ability cast,
    /// damage/healing taken) so that a cancel trigger with source/target filters
    /// only fires when the correct entities are involved — preventing a shield or
    /// buff removal on entity A from inadvertently cancelling a timer scoped to
    /// entity B.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn cancel_timers_matching_with_source_target<F>(
        &mut self,
        entities: &[crate::dsl::EntityDefinition],
        source_id: i64,
        source_type: crate::combat_log::EntityType,
        source_name: crate::context::IStr,
        source_npc_id: i64,
        target_id: i64,
        target_type: crate::combat_log::EntityType,
        target_name: crate::context::IStr,
        target_npc_id: i64,
        trigger_matches: F,
    ) where
        F: Fn(&TimerTrigger) -> bool,
    {
        let local_player_id = self.local_player_id;
        let current_target_id = self.current_target_id;
        let boss_entity_ids = &self.boss_entity_ids;

        let keys_to_cancel: Vec<_> = self
            .active_timers
            .iter()
            .filter_map(|(key, timer)| {
                let def = self.definitions.get(&timer.definition_id)?;
                let cancel_trigger = def.cancel_trigger.as_ref()?;
                if trigger_matches(cancel_trigger)
                    && matches_source_target_filters(
                        cancel_trigger,
                        entities,
                        source_id,
                        source_type,
                        source_name,
                        source_npc_id,
                        target_id,
                        target_type,
                        target_name,
                        target_npc_id,
                        local_player_id,
                        current_target_id,
                        boss_entity_ids,
                    )
                {
                    Some(key.clone())
                } else {
                    None
                }
            })
            .collect();

        for key in keys_to_cancel {
            self.active_timers.remove(&key);
            self.canceled_this_tick.push(key.definition_id);
        }
    }

    /// Remove queued/ready timers whose queue_remove_trigger matches the given predicate
    pub(super) fn remove_queued_matching<F>(&mut self, trigger_matches: F)
    where
        F: Fn(&TimerTrigger) -> bool,
    {
        let keys: Vec<_> = self
            .active_timers
            .iter()
            .filter_map(|(key, timer)| {
                if timer.is_queued
                    && let Some(def) = self.definitions.get(&timer.definition_id)
                    && let Some(ref remove_trigger) = def.queue_remove_trigger
                    && trigger_matches(remove_trigger)
                {
                    Some(key.clone())
                } else {
                    None
                }
            })
            .collect();

        for key in keys {
            self.active_timers.remove(&key);
            self.canceled_this_tick.push(key.definition_id.clone());
            self.removed_queued_this_tick.push(key.definition_id);
        }
    }

    /// Remove queued/ready timers whose queue_remove_trigger matches, with entity filters
    #[allow(clippy::too_many_arguments)]
    pub(super) fn remove_queued_matching_with_source_target<F>(
        &mut self,
        entities: &[crate::dsl::EntityDefinition],
        source_id: i64,
        source_type: crate::combat_log::EntityType,
        source_name: crate::context::IStr,
        source_npc_id: i64,
        target_id: i64,
        target_type: crate::combat_log::EntityType,
        target_name: crate::context::IStr,
        target_npc_id: i64,
        trigger_matches: F,
    ) where
        F: Fn(&TimerTrigger) -> bool,
    {
        let local_player_id = self.local_player_id;
        let current_target_id = self.current_target_id;
        let boss_entity_ids = &self.boss_entity_ids;

        let keys: Vec<_> = self
            .active_timers
            .iter()
            .filter_map(|(key, timer)| {
                if !timer.is_queued {
                    return None;
                }
                let def = self.definitions.get(&timer.definition_id)?;
                let remove_trigger = def.queue_remove_trigger.as_ref()?;
                if trigger_matches(remove_trigger)
                    && matches_source_target_filters(
                        remove_trigger,
                        entities,
                        source_id,
                        source_type,
                        source_name,
                        source_npc_id,
                        target_id,
                        target_type,
                        target_name,
                        target_npc_id,
                        local_player_id,
                        current_target_id,
                        boss_entity_ids,
                    )
                {
                    Some(key.clone())
                } else {
                    None
                }
            })
            .collect();

        for key in keys {
            self.active_timers.remove(&key);
            self.canceled_this_tick.push(key.definition_id);
        }
    }

    /// Process timer expirations, repeats, and chains
    fn process_expirations(
        &mut self,
        current_time: NaiveDateTime,
        encounter: Option<&crate::encounter::CombatEncounter>,
    ) {
        self.expired_this_tick.clear();

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
            if let Some(timer) = self.active_timers.get_mut(&key) {
                // Skip already-queued timers — they stay indefinitely until combat end
                if timer.is_queued {
                    continue;
                }

                // Check if timer can repeat
                if timer.can_repeat() {
                    timer.repeat(current_time);
                    self.expired_this_tick.push(key.definition_id);
                    continue;
                }

                // Check if timer should hold as queued (per D-07)
                // Fire alerts and chains on the transition tick BEFORE marking queued
                if timer.queue_on_expire {
                    self.expired_this_tick.push(key.definition_id.clone());
                    self.queued_this_tick.push(key.definition_id.clone());

                    let should_fire_audio = !timer.role_hidden && timer.audio_enabled && timer.audio_file.is_some() && timer.audio_offset == 0;
                    let should_fire_expire_alert = !timer.role_hidden && timer.alert_on_expire;
                    if should_fire_audio || should_fire_expire_alert {
                        let text = timer.alert_text.clone().unwrap_or_else(|| timer.name.clone());
                        self.fired_alerts.push(FiredAlert {
                            id: timer.definition_id.clone(),
                            name: timer.name.clone(),
                            text,
                            color: Some(timer.color),
                            timestamp: current_time,
                            alert_text_enabled: should_fire_expire_alert,
                            audio_enabled: should_fire_audio,
                            audio_file: timer.audio_file.clone(),
                            icon_ability_id: timer.icon_ability_id,
                        });
                    }

                    // Fire chain on the transition tick
                    if let Some(next_timer_id) = timer.triggers_timer.clone() {
                        chains_to_start.push((next_timer_id, timer.target_entity_id));
                    }

                    // NOW mark as queued — timer stays in active_timers indefinitely
                    timer.is_queued = true;
                    continue;
                }
            }

            // Timer is fully expired — remove it, fire alerts, chain
            if let Some(mut timer) = self.active_timers.remove(&key) {
                // Record expiration (move from key since we're done with it)
                self.expired_this_tick.push(key.definition_id);
                // Fire expiration alert if:
                // 1. Audio is configured with offset=0 (play sound on expire), OR
                // 2. alert_on_expire is true (alert text notification on expire)
                let has_chain = timer.triggers_timer.is_some();
                let should_fire_audio = !timer.role_hidden && timer.audio_enabled && timer.audio_file.is_some() && timer.audio_offset == 0;
                let should_fire_expire_alert = !timer.role_hidden && timer.alert_on_expire;

                if should_fire_audio || should_fire_expire_alert {
                    let raw_text = timer.alert_text.as_deref().unwrap_or(&timer.name);
                    let text = self.format_alert_text(raw_text, current_time);
                    // Move fields from timer since we own it and are done with it (unless chaining)
                    let (id, name, audio_file) = if has_chain {
                        // Need to clone since timer is still used for chain
                        (
                            timer.definition_id.clone(),
                            timer.name.clone(),
                            timer.audio_file.clone(),
                        )
                    } else {
                        // Can move since timer is not used after this
                        (
                            std::mem::take(&mut timer.definition_id),
                            std::mem::take(&mut timer.name),
                            timer.audio_file.take(),
                        )
                    };
                    self.fired_alerts.push(FiredAlert {
                        id,
                        name,
                        text,
                        color: Some(timer.color),
                        timestamp: current_time,
                        alert_text_enabled: should_fire_expire_alert,
                        audio_enabled: should_fire_audio,
                        audio_file,
                        icon_ability_id: timer.icon_ability_id,
                    });
                }
                // Prepare chain to next timer (take ownership of triggers_timer)
                if let Some(next_timer_id) = std::mem::take(&mut timer.triggers_timer) {
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
        for expired_id in &expired_ids {
            let matching: Vec<_> = self
                .definitions_for_kind(TriggerKind::TimerExpires)
                .iter()
                .filter(|d| {
                    d.matches_timer_expires(expired_id)
                        && self.is_definition_active(d, encounter)
                })
                .cloned()
                .collect();

            for def in matching {
                self.start_timer(&def, current_time, None);
            }
        }

        // Cancel timers with TimerExpires cancel triggers
        for expired_id in &expired_ids {
            self.cancel_timers_on_expire(expired_id);
        }

    }

    /// Cancel active timers that have cancel_trigger matching the expired timer ID
    fn cancel_timers_on_expire(&mut self, expired_timer_id: &str) {
        let keys_to_cancel: Vec<_> = self
            .active_timers
            .iter()
            .filter_map(|(key, timer)| {
                if let Some(def) = self.definitions.get(&timer.definition_id)
                    && let Some(ref cancel_trigger) = def.cancel_trigger
                    && cancel_trigger.matches_timer_expires(expired_timer_id)
                {
                    Some(key.clone())
                } else {
                    None
                }
            })
            .collect();

        // Track cancellations and remove timers
        for key in keys_to_cancel {
            self.active_timers.remove(&key);
            self.canceled_this_tick.push(key.definition_id);
        }

        // Also remove queued entries whose queue_remove_trigger matches timer expired
        let keys_to_remove: Vec<_> = self
            .active_timers
            .iter()
            .filter_map(|(key, timer)| {
                if timer.is_queued
                    && let Some(def) = self.definitions.get(&timer.definition_id)
                    && let Some(ref remove_trigger) = def.queue_remove_trigger
                    && remove_trigger.matches_timer_expires(expired_timer_id)
                {
                    Some(key.clone())
                } else {
                    None
                }
            })
            .collect();

        for key in keys_to_remove {
            self.active_timers.remove(&key);
            self.canceled_this_tick.push(key.definition_id);
        }
    }

    pub fn start_timers_on_cancel(
        &mut self,
        canceled_timer_id: &str,
        current_time: NaiveDateTime,
        encounter: Option<&crate::encounter::CombatEncounter>,
    ) {
        let keys_to_start: Vec<_> = self
            .definitions_for_kind(TriggerKind::TimerCanceled)
            .iter()
            .filter(|d| {
                d.matches_timer_canceled(canceled_timer_id)
                    && self.is_definition_active(d, encounter)
            })
            .cloned()
            .collect();

        for key in keys_to_start {
            self.start_timer(&key, current_time, None);
        }
    }

    /// Cancel active timers (and clear queued entries) whose triggers match the canceled timer ID.
    fn cancel_timers_on_cancel(&mut self, canceled_timer_id: &str) {
        let keys_to_cancel: Vec<_> = self
            .active_timers
            .iter()
            .filter_map(|(key, timer)| {
                if let Some(def) = self.definitions.get(&timer.definition_id)
                    && let Some(ref cancel_trigger) = def.cancel_trigger
                    && cancel_trigger.matches_timer_canceled(canceled_timer_id)
                {
                    Some(key.clone())
                } else {
                    None
                }
            })
            .collect();

        for key in keys_to_cancel {
            self.active_timers.remove(&key);
            self.canceled_this_tick.push(key.definition_id);
        }

        // Also remove queued entries whose queue_remove_trigger matches timer canceled
        let keys_to_remove: Vec<_> = self
            .active_timers
            .iter()
            .filter_map(|(key, timer)| {
                if timer.is_queued
                    && let Some(def) = self.definitions.get(&timer.definition_id)
                    && let Some(ref remove_trigger) = def.queue_remove_trigger
                    && remove_trigger.matches_timer_canceled(canceled_timer_id)
                {
                    Some(key.clone())
                } else {
                    None
                }
            })
            .collect();

        for key in keys_to_remove {
            self.active_timers.remove(&key);
            self.canceled_this_tick.push(key.definition_id);
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
            self.current_target_id,
            &self.boss_entity_ids,
        )
    }
}

impl SignalHandler for TimerManager {
    /// Override handle_signals to accumulate batch-level timer event vectors.
    ///
    /// Per-signal vectors (`expired_this_tick`, `started_this_tick`, `canceled_this_tick`)
    /// are cleared at the start of each `handle_signal` / `process_expirations` call
    /// so that chain-trigger logic within a single signal sees only that signal's events.
    ///
    /// After each signal, we drain the per-signal vectors into the batch-level vectors
    /// (`batch_expired`, `batch_started`, `batch_canceled`) so the timer feedback loop
    /// in `parser.rs` can see ALL timer events from the entire batch.
    fn handle_signals(
        &mut self,
        signals: &[GameSignal],
        encounter: Option<&crate::encounter::CombatEncounter>,
    ) {
        self.batch_expired.clear();
        self.batch_started.clear();
        self.batch_canceled.clear();
        self.batch_queued.clear();
        self.batch_removed_queued.clear();
        for signal in signals {
            self.handle_signal(signal, encounter);
            // Accumulate per-signal events into batch-level vectors
            self.batch_expired.extend(self.expired_this_tick.drain(..));
            self.batch_started.extend(self.started_this_tick.drain(..));
            self.batch_canceled.extend(self.canceled_this_tick.drain(..));
            self.batch_queued.extend(self.queued_this_tick.drain(..));
            self.batch_removed_queued.extend(self.removed_queued_this_tick.drain(..));
        }
    }

    fn handle_signal(
        &mut self,
        signal: &GameSignal,
        encounter: Option<&crate::encounter::CombatEncounter>,
    ) {
        // ─── Context-setting signals: always process (bypass recency filter) ───
        // These establish context for future timer matching, not trigger timers directly.
        // IMPORTANT: Boss/combat context must be set even if definitions aren't loaded yet,
        // otherwise a race between definition loading and combat start will break timers.
        match signal {
            GameSignal::PlayerInitialized { entity_id, .. } => {
                self.local_player_id = Some(*entity_id);
                return;
            }
            GameSignal::DisciplineChanged {
                entity_id,
                discipline_id,
                ..
            } => {
                if self.local_player_id == Some(*entity_id) {
                    self.current_role =
                        Discipline::from_guid(*discipline_id).map(|d| d.role());
                }
                return;
            }
            // AreaEntered: Context is now read from CombatEncounter directly
            GameSignal::AreaEntered { .. } => return,

            // CombatEnded: Clear combat state even if definitions not loaded
            GameSignal::CombatEnded { .. } => {
                signal_handlers::clear_combat_timers(self);
                return;
            }

            _ => {}
        }

        // Skip timer-triggering signals if no definitions loaded
        if self.definitions.is_empty() {
            return;
        }

        let ts = signal.timestamp();
        self.advance_game_time_anchor(ts);

        // ─── Encounter change detection ────────────────────────────────────────
        // If the encounter ID changed, reset timer state. Combat-time triggers
        // (combat_start, time_elapsed) will be picked up naturally by
        // handle_combat_time_triggers on the next evaluation.
        if let Some(enc) = encounter {
            let current_enc_id = enc.id;
            if self.active_encounter_id != Some(current_enc_id) {
                self.active_timers.clear();
                self.fired_alerts.clear();
                self.combat_time_started.clear();
                self.active_encounter_id = Some(current_enc_id);
            }
        }

        // Clear per-signal tracking vectors. Batch-level accumulation happens in
        // handle_signals() after each handle_signal() call.
        self.started_this_tick.clear();
        self.canceled_this_tick.clear();

        match signal {
            // Context signals already handled above
            GameSignal::PlayerInitialized { .. } | GameSignal::AreaEntered { .. } => {}

            // BossEncounterDetected: set boss context, then fall through to
            // handle_combat_time_triggers below so combat_start timers fire immediately.
            GameSignal::BossEncounterDetected {
                entity_id,
                boss_npc_class_ids,
                timestamp,
                ..
            } => {
                self.boss_entity_ids.insert(*entity_id);
                self.boss_npc_class_ids.clear();
                for &class_id in boss_npc_class_ids {
                    self.boss_npc_class_ids.insert(class_id);
                }
                let combat_start = encounter
                    .and_then(|e| e.enter_combat_time)
                    .unwrap_or(*timestamp);
                self.in_combat = true;
                self.combat_start_time = Some(combat_start);
            }

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
                source_npc_id,
                target_id,
                target_entity_type,
                target_name,
                target_npc_id,
                timestamp,
            } => {
                signal_handlers::handle_effect_removed(
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

            GameSignal::CombatStarted { timestamp, .. } => {
                // Just set combat state. Combat-start timers are evaluated
                // continuously by handle_combat_time_triggers below.
                self.in_combat = true;
                self.combat_start_time = Some(*timestamp);
            }

            // CombatEnded handled in early context-setting section above
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
                // Track local player's current target for CurrentTarget filter
                if self.local_player_id == Some(*source_id) {
                    self.current_target_id = Some(*target_id);
                }

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
            GameSignal::TargetCleared { source_id, .. } => {
                // Clear local player's current target if they cleared their target
                if self.local_player_id == Some(*source_id) {
                    self.current_target_id = None;
                }
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
                    *target_npc_id,
                    *timestamp,
                    *defense_type_id,
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
            } => {
                signal_handlers::handle_healing_taken(
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

            GameSignal::ThreatModified {
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
                threat,
                timestamp,
            } => {
                signal_handlers::handle_threat_modified(
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
                    *threat,
                    *timestamp,
                );
            }

            // ─── Boss Encounter Signals (from EventProcessor) ─────────────────────
            // BossEncounterDetected handled in early context-setting section above
            GameSignal::BossHpChanged {
                npc_id,
                entity_name,
                old_hp_percent,
                new_hp_percent,
                timestamp,
                ..
            } => {
                // Check for HP threshold timer triggers (using pre-computed percentages from signal)
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

            GameSignal::PhaseChanged {
                new_phase,
                timestamp,
                ..
            } => {
                // Trigger phase-entered timers
                signal_handlers::handle_phase_change(self, encounter, new_phase, *timestamp);
                // Trigger any-phase-change timers (start + cancel)
                signal_handlers::handle_any_phase_change(self, encounter, *timestamp);
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

            GameSignal::PhaseEndTriggered {
                phase_id,
                timestamp,
            } => {
                // Phase's end_trigger fired (may be before actual phase transition)
                signal_handlers::handle_phase_ended(self, encounter, phase_id, *timestamp);
            }

            _ => {}
        }

        // Evaluate combat-time triggers (CombatStart + TimeElapsed)
        signal_handlers::handle_combat_time_triggers(self, encounter);

        // Process expirations after handling signal
        if let Some(ts) = self.last_timestamp {
            self.process_expirations(ts, encounter);
        }
        // Process Cancellation Triggers
        if let Some(ts) = self.last_timestamp {
            for timer_id in self.canceled_this_tick.clone() {
                self.start_timers_on_cancel(&timer_id, ts, encounter);
                self.cancel_timers_on_cancel(&timer_id);
            }
        }

    }

    fn on_encounter_start(&mut self, _encounter_id: u64) {
        // Could reset encounter-specific state here
    }

    fn on_encounter_end(&mut self, _encounter_id: u64) {
        signal_handlers::clear_combat_timers(self);
    }
}
