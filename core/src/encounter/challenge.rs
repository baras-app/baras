//! Challenge tracker - accumulates metrics during boss encounters
//!
//! Lives in encounter/ because it persists with the Encounter for historical data,
//! unlike BossEncounterState which resets on combat end.

use std::collections::HashMap;

use crate::dsl::{
    ChallengeCondition, ChallengeContext, ChallengeDefinition, ChallengeMetric, EntityDefinition,
    EntityInfo,
};
use crate::game_data::Difficulty;
use crate::EntityFilter;
use baras_types::ChallengeColumns;

// ═══════════════════════════════════════════════════════════════════════════
// Challenge Value
// ═══════════════════════════════════════════════════════════════════════════

/// Accumulated value for a challenge
#[derive(Debug, Clone, Default)]
pub struct ChallengeValue {
    /// The challenge definition ID
    pub id: String,

    /// Challenge display name
    pub name: String,

    /// Accumulated numeric value (damage, healing, count, etc.)
    pub value: i64,

    /// Number of events that contributed
    pub event_count: u32,

    /// Per-player breakdown (entity_id → value)
    pub by_player: HashMap<i64, i64>,

    /// Duration in seconds for this challenge (phase-scoped or total)
    pub duration_secs: f32,

    /// When this challenge first received a matching event (for display filtering)
    pub first_event_time: Option<chrono::NaiveDateTime>,

    /// When the challenge context became active (for duration calculation)
    /// Set when phase starts, HP threshold crossed, or encounter starts for unconditional challenges
    pub activated_time: Option<chrono::NaiveDateTime>,

    /// Accumulated duration when non-phase conditions (BossHpRange, Counter) are satisfied
    pub condition_active_secs: f32,

    /// When the non-phase conditions last became active (for live tracking)
    pub condition_active_start: Option<chrono::NaiveDateTime>,

    // ─────────────────────────────────────────────────────────────────────────
    // Display Settings (copied from ChallengeDefinition)
    // ─────────────────────────────────────────────────────────────────────────
    /// Whether this challenge is enabled for overlay display
    pub enabled: bool,

    /// Bar color [r, g, b, a] (None = use overlay default)
    pub color: Option<[u8; 4]>,

    /// Which columns to display
    pub columns: ChallengeColumns,
}

// ═══════════════════════════════════════════════════════════════════════════
// Challenge Tracker
// ═══════════════════════════════════════════════════════════════════════════

/// Tracks challenge metrics during a boss encounter
///
/// Initialized when a boss encounter starts, accumulates values as events
/// are processed, and provides snapshots for overlay/history.
///
/// Lives on Encounter (not BossEncounterState) because challenge data
/// persists with the encounter for historical analysis.
#[derive(Debug, Clone, Default)]
pub struct ChallengeTracker {
    /// Active challenge definitions for this encounter
    definitions: Vec<ChallengeDefinition>,

    /// Accumulated values by challenge ID
    values: HashMap<String, ChallengeValue>,

    /// Entity roster for name → NPC ID resolution
    entities: Vec<EntityDefinition>,

    /// Boss NPC IDs for entity matching
    boss_npc_ids: Vec<i64>,

    /// Whether tracking is active
    active: bool,

    /// Phase durations in seconds (phase_id → duration)
    phase_durations: HashMap<String, f32>,

    /// Current phase and when it started (for duration tracking)
    current_phase_start: Option<(String, chrono::NaiveDateTime)>,

    /// Total encounter duration in seconds (for DPS calculations)
    total_duration_secs: f32,

    /// Pending max stacks per EffectStacks challenge window: challenge_id → (entity_id → max_charges)
    /// Flushed to value on REMOVEEFFECT or encounter end.
    pending_stacks: HashMap<String, HashMap<i64, i32>>,
}

impl ChallengeTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Initialize tracker with challenges from a boss definition.
    ///
    /// Challenges whose `difficulties` list doesn't match `current_difficulty`
    /// are excluded entirely — they will not accumulate events and won't appear
    /// in the overlay or history for this encounter.
    pub fn start(
        &mut self,
        challenges: Vec<ChallengeDefinition>,
        entities: Vec<EntityDefinition>,
        boss_npc_ids: Vec<i64>,
        timestamp: chrono::NaiveDateTime,
        current_difficulty: Option<&Difficulty>,
    ) {
        // Filter out challenges that don't apply to this difficulty
        let challenges: Vec<ChallengeDefinition> = challenges
            .into_iter()
            .filter(|c| {
                c.difficulties.is_empty()
                    || current_difficulty.map_or(true, |d| {
                        c.difficulties.iter().any(|key| d.matches_config_key(key))
                    })
            })
            .collect();
        self.definitions = challenges;
        self.entities = entities;
        self.boss_npc_ids = boss_npc_ids;
        self.values.clear();
        self.phase_durations.clear();
        self.current_phase_start = None;
        self.total_duration_secs = 0.0;
        self.pending_stacks.clear();
        self.active = true;

        // Pre-initialize values for all challenges
        for def in &self.definitions {
            // Challenges without phase conditions are active from encounter start
            let activated_time = if def.has_phase_condition() {
                None // Will be set when the matching phase starts
            } else {
                Some(timestamp)
            };

            self.values.insert(
                def.id.clone(),
                ChallengeValue {
                    id: def.id.clone(),
                    name: def.name.clone(),
                    value: 0,
                    event_count: 0,
                    by_player: HashMap::new(),
                    duration_secs: 0.0, // Calculated in snapshot()
                    first_event_time: None,
                    activated_time,
                    condition_active_secs: 0.0,
                    condition_active_start: None,
                    // Display settings from definition
                    enabled: def.enabled,
                    color: def.color,
                    columns: def.columns,
                },
            );
        }
    }

    /// Stop tracking and return final values
    pub fn stop(&mut self, timestamp: chrono::NaiveDateTime) -> Vec<ChallengeValue> {
        self.end_current_phase(timestamp);
        self.flush_all_pending_stacks();
        self.active = false;
        self.values.values().cloned().collect()
    }

    /// Reset tracker (on combat end)
    pub fn reset(&mut self) {
        self.definitions.clear();
        self.values.clear();
        self.boss_npc_ids.clear();
        self.phase_durations.clear();
        self.current_phase_start = None;
        self.total_duration_secs = 0.0;
        self.pending_stacks.clear();
        self.active = false;
    }

    /// Set the current phase (called on PhaseChanged signal)
    pub fn set_phase(&mut self, phase_id: &str, timestamp: chrono::NaiveDateTime) {
        self.end_current_phase(timestamp);
        self.current_phase_start = Some((phase_id.to_string(), timestamp));

        // Activate challenges that have this phase in their conditions (first time only)
        for def in &self.definitions {
            if let Some(phase_ids) = def.phase_ids()
                && phase_ids.iter().any(|p| p == phase_id)
                && let Some(val) = self.values.get_mut(&def.id)
                && val.activated_time.is_none()
            {
                val.activated_time = Some(timestamp);
            }
        }
    }

    /// End the current phase and record its duration
    fn end_current_phase(&mut self, timestamp: chrono::NaiveDateTime) {
        if let Some((phase_id, start_time)) = self.current_phase_start.take() {
            let duration = timestamp.signed_duration_since(start_time);
            let duration_secs = duration.num_milliseconds() as f32 / 1000.0;
            *self.phase_durations.entry(phase_id).or_insert(0.0) += duration_secs;
        }
    }

    /// Set the total encounter duration
    pub fn set_duration(&mut self, duration_secs: f32) {
        self.total_duration_secs = duration_secs;
    }

    /// Get the duration of a specific phase
    pub fn phase_duration(&self, phase_id: &str) -> f32 {
        self.phase_durations.get(phase_id).copied().unwrap_or(0.0)
    }

    /// Get all phase durations
    pub fn phase_durations(&self) -> &HashMap<String, f32> {
        &self.phase_durations
    }

    /// Finalize the tracker on combat end
    pub fn finalize(&mut self, timestamp: chrono::NaiveDateTime, duration_secs: f32) {
        self.end_current_phase(timestamp);
        self.flush_all_pending_stacks();
        self.total_duration_secs = duration_secs;
    }

    /// Get total encounter duration
    pub fn total_duration(&self) -> f32 {
        self.total_duration_secs
    }

    /// Check if tracker is active
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Calculate the effective duration for a challenge value.
    /// Phase-scoped challenges use cumulative phase time (e.g. burn1 20s + burn2 20s = 40s).
    /// Non-phase challenges use elapsed time from activation.
    fn calculate_duration(&self, val: &ChallengeValue, current_time: chrono::NaiveDateTime) -> f32 {
        // Find the definition to check for phase conditions
        let phase_ids = self
            .definitions
            .iter()
            .find(|d| d.id == val.id)
            .and_then(|d| d.phase_ids());

        if let Some(phase_ids) = phase_ids {
            // Sum accumulated durations for all matching phases
            let mut total: f32 = phase_ids
                .iter()
                .filter_map(|pid| self.phase_durations.get(pid))
                .sum();

            // Add elapsed time if a matching phase is currently running
            if let Some((ref current_id, start_time)) = self.current_phase_start
                && phase_ids.iter().any(|pid| pid == current_id)
            {
                let elapsed = current_time.signed_duration_since(start_time);
                total += (elapsed.num_milliseconds() as f32 / 1000.0).max(0.0);
            }

            total
        } else {
            let def = self.definitions.iter().find(|d| d.id == val.id);
            if def.is_some_and(|d| has_scoping_condition(d)) {
                // Condition-scoped challenge: use accumulated + current active span
                let mut total = val.condition_active_secs;
                if let Some(start) = val.condition_active_start {
                    let elapsed = current_time.signed_duration_since(start);
                    total += (elapsed.num_milliseconds() as f32 / 1000.0).max(0.0);
                }
                total
            } else {
                // Non-phase, non-scoped challenge: elapsed time from activation
                val.activated_time
                    .or(val.first_event_time)
                    .map(|start| {
                        let elapsed = current_time.signed_duration_since(start);
                        (elapsed.num_milliseconds() as f32 / 1000.0).max(0.0)
                    })
                    .unwrap_or(0.0)
            }
        }
    }

    /// Get current values snapshot with calculated durations
    /// Pass current timestamp for live duration calculation
    /// Only returns challenges that have received at least one matching event
    pub fn snapshot_live(&self, current_time: chrono::NaiveDateTime) -> Vec<ChallengeValue> {
        self.values
            .values()
            .filter(|val| val.first_event_time.is_some()) // Only show challenges with data
            .map(|val| {
                let duration_secs = self.calculate_duration(val, current_time);

                // For EffectStacks, include pending (not-yet-flushed) stacks in the live value
                let pending_extra = self
                    .pending_stacks
                    .get(&val.id)
                    .map(|m| m.values().map(|&v| v as i64).sum::<i64>())
                    .unwrap_or(0);

                ChallengeValue {
                    id: val.id.clone(),
                    name: val.name.clone(),
                    value: val.value + pending_extra,
                    event_count: val.event_count,
                    by_player: val.by_player.clone(),
                    duration_secs,
                    first_event_time: val.first_event_time,
                    activated_time: val.activated_time,
                    condition_active_secs: val.condition_active_secs,
                    condition_active_start: val.condition_active_start,
                    // Display settings
                    enabled: val.enabled,
                    color: val.color,
                    columns: val.columns,
                }
            })
            .collect()
    }

    /// Get current values snapshot (uses finalized durations - for historical data)
    pub fn snapshot(&self) -> Vec<ChallengeValue> {
        self.values
            .values()
            .map(|val| {
                // Use phase-aware duration for finalized encounters
                let duration_secs = self.finalized_duration(val);

                ChallengeValue {
                    id: val.id.clone(),
                    name: val.name.clone(),
                    value: val.value,
                    event_count: val.event_count,
                    by_player: val.by_player.clone(),
                    duration_secs,
                    first_event_time: val.first_event_time,
                    activated_time: val.activated_time,
                    condition_active_secs: val.condition_active_secs,
                    condition_active_start: val.condition_active_start,
                    // Display settings
                    enabled: val.enabled,
                    color: val.color,
                    columns: val.columns,
                }
            })
            .collect()
    }

    /// Calculate duration for a finalized (ended) encounter.
    /// Phase-scoped challenges use cumulative phase time.
    /// BossHpRange/Counter-scoped challenges use condition-active time.
    /// Unconditional challenges use total encounter time.
    fn finalized_duration(&self, val: &ChallengeValue) -> f32 {
        let def = self.definitions.iter().find(|d| d.id == val.id);
        let phase_ids = def.and_then(|d| d.phase_ids());

        if let Some(phase_ids) = phase_ids {
            let total: f32 = phase_ids
                .iter()
                .filter_map(|pid| self.phase_durations.get(pid))
                .sum();
            total.max(1.0)
        } else if val.condition_active_secs > 0.0 && def.is_some_and(|d| has_scoping_condition(d))
        {
            // Use condition-scoped duration for BossHpRange/Counter challenges
            val.condition_active_secs.max(1.0)
        } else {
            self.total_duration_secs.max(1.0)
        }
    }

    /// Update condition-active duration tracking for non-phase scoping conditions.
    /// Called after each event to track when BossHpRange/Counter conditions are met.
    pub fn update_condition_tracking(
        &mut self,
        ctx: &ChallengeContext,
        timestamp: chrono::NaiveDateTime,
    ) {
        for def in &self.definitions {
            if def.has_phase_condition() || !has_scoping_condition(def) {
                continue;
            }

            // Check if scoping conditions (BossHpRange, Counter) are currently met
            let conditions_met = def.conditions.iter().all(|c| match c {
                ChallengeCondition::BossHpRange { .. } | ChallengeCondition::Counter { .. } => {
                    c.matches(ctx, &self.entities, None, None, None, None)
                }
                _ => true, // Non-scoping conditions don't affect duration
            });

            let Some(val) = self.values.get_mut(&def.id) else {
                continue;
            };

            if conditions_met {
                // Start tracking if not already active
                if val.condition_active_start.is_none() {
                    val.condition_active_start = Some(timestamp);
                }
            } else if let Some(start) = val.condition_active_start.take() {
                // Accumulate the elapsed active time
                let elapsed = timestamp.signed_duration_since(start);
                val.condition_active_secs +=
                    (elapsed.num_milliseconds() as f32 / 1000.0).max(0.0);
            }
        }
    }

    /// Get the challenge definitions
    pub fn definitions(&self) -> &[ChallengeDefinition] {
        &self.definitions
    }

    /// Get a specific challenge value
    pub fn get_value(&self, challenge_id: &str) -> Option<&ChallengeValue> {
        self.values.get(challenge_id)
    }

    /// Get the boss NPC IDs for context building
    pub fn boss_npc_ids(&self) -> &[i64] {
        &self.boss_npc_ids
    }

    /// Process a damage event
    pub fn process_damage(
        &mut self,
        ctx: &ChallengeContext,
        source: &EntityInfo,
        target: &EntityInfo,
        ability_id: u64,
        damage: i64,
        absorbed: i64,
        timestamp: chrono::NaiveDateTime,
    ) -> Vec<String> {
        if !self.active || (damage == 0 && absorbed == 0) {
            return Vec::new();
        }

        let mut updated = Vec::new();

        for def in &self.definitions {
            let (matches_metric, track_source, value) = match def.metric {
                ChallengeMetric::Damage => (true, true, damage),
                ChallengeMetric::DamageTaken => (true, false, damage),
                ChallengeMetric::DamageAbsorbed => (true, true, absorbed),
                _ => (false, false, 0),
            };

            if !matches_metric || value == 0 {
                continue;
            }

            if def.matches(
                ctx,
                &self.entities,
                Some(source),
                Some(target),
                Some(ability_id),
                None,
            ) && let Some(val) = self.values.get_mut(&def.id)
            {
                let entity = if track_source { source } else { target };
                // Only count player contributions (not companions/NPCs)
                if entity.is_player {
                    // Record first event time for duration calculation
                    if val.first_event_time.is_none() {
                        val.first_event_time = Some(timestamp);
                    }
                    val.value += value;
                    val.event_count += 1;
                    *val.by_player.entry(entity.entity_id).or_insert(0) += value;
                    updated.push(def.id.clone());
                }
            }
        }

        updated
    }

    /// Process a healing event
    pub fn process_healing(
        &mut self,
        ctx: &ChallengeContext,
        source: &EntityInfo,
        target: &EntityInfo,
        ability_id: u64,
        healing: i64,
        effective_healing: i64,
        timestamp: chrono::NaiveDateTime,
    ) -> Vec<String> {
        if !self.active || (healing == 0 && effective_healing == 0) {
            return Vec::new();
        }

        let mut updated = Vec::new();

        for def in &self.definitions {
            let (matches_metric, track_source, value) = match def.metric {
                ChallengeMetric::Healing => (true, true, healing),
                ChallengeMetric::EffectiveHealing => (true, true, effective_healing),
                ChallengeMetric::HealingTaken => (true, false, effective_healing),
                _ => (false, false, 0),
            };

            if !matches_metric || value == 0 {
                continue;
            }

            if def.matches(
                ctx,
                &self.entities,
                Some(source),
                Some(target),
                Some(ability_id),
                None,
            ) && let Some(val) = self.values.get_mut(&def.id)
            {
                let entity = if track_source { source } else { target };
                // Only count player contributions (not companions/NPCs)
                if entity.is_player {
                    if val.first_event_time.is_none() {
                        val.first_event_time = Some(timestamp);
                    }
                    val.value += value;
                    val.event_count += 1;
                    *val.by_player.entry(entity.entity_id).or_insert(0) += value;
                    updated.push(def.id.clone());
                }
            }
        }

        updated
    }

    /// Process an ability interrupt event
    pub fn process_interrupt(
        &mut self,
        ctx: &ChallengeContext,
        source: &EntityInfo,
        ability_id: u64,
        timestamp: chrono::NaiveDateTime,
    ) -> Vec<String> {
        if !self.active {
            return Vec::new();
        }

        let mut updated = Vec::new();

        for def in &self.definitions {
            if def.metric != ChallengeMetric::InterruptCount {
                continue;
            }

            if def.matches(ctx, &self.entities, Some(source), None, Some(ability_id), None)
                && let Some(val) = self.values.get_mut(&def.id)
                && source.is_player
            {
                if val.first_event_time.is_none() {
                    val.first_event_time = Some(timestamp);
                }
                val.value += 1;
                val.event_count += 1;
                *val.by_player.entry(source.entity_id).or_insert(0) += 1;
                updated.push(def.id.clone());
            }
        }

        updated
    }

    /// Process an ability activation (for count metrics)
    pub fn process_ability(
        &mut self,
        ctx: &ChallengeContext,
        source: &EntityInfo,
        target: &EntityInfo,
        ability_id: u64,
        timestamp: chrono::NaiveDateTime,
    ) -> Vec<String> {
        if !self.active {
            return Vec::new();
        }

        let mut updated = Vec::new();

        for def in &self.definitions {
            if def.metric != ChallengeMetric::AbilityCount {
                continue;
            }

            if def.matches(
                ctx,
                &self.entities,
                Some(source),
                Some(target),
                Some(ability_id),
                None,
            ) && let Some(val) = self.values.get_mut(&def.id)
            {
                // Determine tracking entity: if challenge tracks by target (player), use target;
                // otherwise use source (default behavior)
                let entity = if target.is_player && has_player_target_condition(&def.conditions) {
                    target
                } else {
                    source
                };
                if entity.is_player {
                    if val.first_event_time.is_none() {
                        val.first_event_time = Some(timestamp);
                    }
                    val.value += 1;
                    val.event_count += 1;
                    *val.by_player.entry(entity.entity_id).or_insert(0) += 1;
                    updated.push(def.id.clone());
                }
            }
        }

        updated
    }

    /// Process an effect application or charge modification (for count and stacks metrics)
    pub fn process_effect_applied(
        &mut self,
        ctx: &ChallengeContext,
        source: &EntityInfo,
        target: &EntityInfo,
        effect_id: u64,
        charges: i32,
        is_modify: bool,
        timestamp: chrono::NaiveDateTime,
    ) -> Vec<String> {
        if !self.active {
            return Vec::new();
        }

        let mut updated = Vec::new();

        for def in &self.definitions {
            if !matches!(
                def.metric,
                ChallengeMetric::EffectCount | ChallengeMetric::EffectStacks
            ) {
                continue;
            }

            if def.matches(
                ctx,
                &self.entities,
                Some(source),
                Some(target),
                None,
                Some(effect_id),
            ) {
                // Determine tracking entity: if challenge tracks by target (player), use target;
                // otherwise use source (default behavior)
                let entity = if target.is_player && has_player_target_condition(&def.conditions) {
                    target
                } else {
                    source
                };
                if !entity.is_player {
                    continue;
                }

                match def.metric {
                    ChallengeMetric::EffectCount => {
                        if let Some(val) = self.values.get_mut(&def.id) {
                            if val.first_event_time.is_none() {
                                val.first_event_time = Some(timestamp);
                            }
                            val.value += 1;
                            val.event_count += 1;
                            *val.by_player.entry(entity.entity_id).or_insert(0) += 1;
                            updated.push(def.id.clone());
                        }
                    }
                    ChallengeMetric::EffectStacks => {
                        let stacks = if is_modify { charges } else { charges.max(1) };
                        let pending = self
                            .pending_stacks
                            .entry(def.id.clone())
                            .or_default();

                        if is_modify {
                            // Update max for existing window
                            let current = pending.entry(entity.entity_id).or_insert(0);
                            *current = (*current).max(stacks);
                        } else {
                            // New application — flush any existing window first
                            if let Some(prev_max) = pending.remove(&entity.entity_id) {
                                if let Some(val) = self.values.get_mut(&def.id) {
                                    val.value += prev_max as i64;
                                    val.event_count += 1;
                                    *val.by_player.entry(entity.entity_id).or_insert(0) +=
                                        prev_max as i64;
                                }
                            }
                            pending.insert(entity.entity_id, stacks);
                        }

                        if let Some(val) = self.values.get_mut(&def.id)
                            && val.first_event_time.is_none()
                        {
                            val.first_event_time = Some(timestamp);
                        }
                        updated.push(def.id.clone());
                    }
                    _ => {}
                }
            }
        }

        updated
    }

    /// Process an effect removal — flushes pending EffectStacks windows
    pub fn process_effect_removed(
        &mut self,
        ctx: &ChallengeContext,
        source: &EntityInfo,
        target: &EntityInfo,
        effect_id: u64,
        timestamp: chrono::NaiveDateTime,
    ) -> Vec<String> {
        if !self.active {
            return Vec::new();
        }

        let mut updated = Vec::new();

        for def in &self.definitions {
            if def.metric != ChallengeMetric::EffectStacks {
                continue;
            }

            if def.matches(
                ctx,
                &self.entities,
                Some(source),
                Some(target),
                None,
                Some(effect_id),
            ) {
                let entity = if target.is_player && has_player_target_condition(&def.conditions) {
                    target
                } else {
                    source
                };
                if !entity.is_player {
                    continue;
                }

                // Flush the pending max stacks for this entity
                if let Some(pending) = self.pending_stacks.get_mut(&def.id)
                    && let Some(max_stacks) = pending.remove(&entity.entity_id)
                    && let Some(val) = self.values.get_mut(&def.id)
                {
                    val.value += max_stacks as i64;
                    val.event_count += 1;
                    *val.by_player.entry(entity.entity_id).or_insert(0) += max_stacks as i64;
                    if val.first_event_time.is_none() {
                        val.first_event_time = Some(timestamp);
                    }
                    updated.push(def.id.clone());
                }
            }
        }

        updated
    }

    /// Flush all remaining pending stacks (called on encounter end)
    fn flush_all_pending_stacks(&mut self) {
        for (challenge_id, entity_map) in self.pending_stacks.drain() {
            if let Some(val) = self.values.get_mut(&challenge_id) {
                for (entity_id, max_stacks) in entity_map {
                    val.value += max_stacks as i64;
                    val.event_count += 1;
                    *val.by_player.entry(entity_id).or_insert(0) += max_stacks as i64;
                }
            }
        }
    }
}

/// Check if a challenge definition has non-phase scoping conditions (BossHpRange, Counter)
/// that should affect duration calculation.
fn has_scoping_condition(def: &ChallengeDefinition) -> bool {
    def.conditions.iter().any(|c| {
        matches!(
            c,
            ChallengeCondition::BossHpRange { .. } | ChallengeCondition::Counter { .. }
        )
    })
}

/// Check if a challenge has a Target condition that matches players.
/// Used to determine whether to track count metrics by target (received) vs source (used).
fn has_player_target_condition(conditions: &[ChallengeCondition]) -> bool {
    conditions.iter().any(|c| matches!(c,
        ChallengeCondition::Target { matcher }
            if matches!(matcher,
                EntityFilter::AnyPlayer
                | EntityFilter::LocalPlayer
                | EntityFilter::OtherPlayers
                | EntityFilter::AnyPlayerOrCompanion
            )
    ))
}
