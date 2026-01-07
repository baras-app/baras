//! Challenge tracker - accumulates metrics during boss encounters
//!
//! Lives in encounter/ because it persists with the Encounter for historical data,
//! unlike BossEncounterState which resets on combat end.

use std::collections::HashMap;

use crate::dsl::{
    ChallengeContext, ChallengeDefinition, ChallengeMetric, EntityDefinition, EntityInfo,
};
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
}

impl ChallengeTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Initialize tracker with challenges from a boss definition
    pub fn start(
        &mut self,
        challenges: Vec<ChallengeDefinition>,
        entities: Vec<EntityDefinition>,
        boss_npc_ids: Vec<i64>,
        timestamp: chrono::NaiveDateTime,
    ) {
        self.definitions = challenges;
        self.entities = entities;
        self.boss_npc_ids = boss_npc_ids;
        self.values.clear();
        self.phase_durations.clear();
        self.current_phase_start = None;
        self.total_duration_secs = 0.0;
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

    /// Get current values snapshot with calculated durations
    /// Pass current timestamp for live duration calculation
    /// Only returns challenges that have received at least one matching event
    pub fn snapshot_live(&self, current_time: chrono::NaiveDateTime) -> Vec<ChallengeValue> {
        self.values
            .values()
            .filter(|val| val.first_event_time.is_some()) // Only show challenges with data
            .map(|val| {
                // Calculate duration from activated_time (when challenge context became active)
                // Falls back to first_event_time if activated_time not set
                let duration_secs = val
                    .activated_time
                    .or(val.first_event_time)
                    .map(|start| {
                        let elapsed = current_time.signed_duration_since(start);
                        (elapsed.num_milliseconds() as f32 / 1000.0).max(0.0)
                    })
                    .unwrap_or(0.0);

                ChallengeValue {
                    id: val.id.clone(),
                    name: val.name.clone(),
                    value: val.value,
                    event_count: val.event_count,
                    by_player: val.by_player.clone(),
                    duration_secs,
                    first_event_time: val.first_event_time,
                    activated_time: val.activated_time,
                    // Display settings
                    enabled: val.enabled,
                    color: val.color,
                    columns: val.columns,
                }
            })
            .collect()
    }

    /// Get current values snapshot (uses stored duration - for historical data)
    pub fn snapshot(&self) -> Vec<ChallengeValue> {
        self.values
            .values()
            .map(|val| ChallengeValue {
                id: val.id.clone(),
                name: val.name.clone(),
                value: val.value,
                event_count: val.event_count,
                by_player: val.by_player.clone(),
                duration_secs: val.duration_secs.max(self.total_duration_secs).max(1.0),
                first_event_time: val.first_event_time,
                activated_time: val.activated_time,
                // Display settings
                enabled: val.enabled,
                color: val.color,
                columns: val.columns,
            })
            .collect()
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
        timestamp: chrono::NaiveDateTime,
    ) -> Vec<String> {
        if !self.active || damage == 0 {
            return Vec::new();
        }

        let mut updated = Vec::new();

        for def in &self.definitions {
            let (matches_metric, track_source) = match def.metric {
                ChallengeMetric::Damage => (true, true),
                ChallengeMetric::DamageTaken => (true, false),
                _ => (false, false),
            };

            if !matches_metric {
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
                    val.value += damage;
                    val.event_count += 1;
                    *val.by_player.entry(entity.entity_id).or_insert(0) += damage;
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

    /// Process an effect application (for count metrics)
    pub fn process_effect_applied(
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
            if def.metric != ChallengeMetric::EffectCount {
                continue;
            }

            if def.matches(
                ctx,
                &self.entities,
                Some(source),
                Some(target),
                None,
                Some(effect_id),
            ) && let Some(val) = self.values.get_mut(&def.id)
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

    /// Process a death event
    pub fn process_death(
        &mut self,
        ctx: &ChallengeContext,
        entity: &EntityInfo,
        timestamp: chrono::NaiveDateTime,
    ) -> Vec<String> {
        if !self.active {
            return Vec::new();
        }

        let mut updated = Vec::new();

        for def in &self.definitions {
            if def.metric != ChallengeMetric::Deaths {
                continue;
            }

            if def.matches(ctx, &self.entities, None, Some(entity), None, None)
                && let Some(val) = self.values.get_mut(&def.id)
                && entity.is_player
            {
                if val.first_event_time.is_none() {
                    val.first_event_time = Some(timestamp);
                }
                val.value += 1;
                val.event_count += 1;
                *val.by_player.entry(entity.entity_id).or_insert(0) += 1;
                updated.push(def.id.clone());
            }
        }

        updated
    }

    /// Process a threat event
    pub fn process_threat(
        &mut self,
        ctx: &ChallengeContext,
        source: &EntityInfo,
        target: &EntityInfo,
        threat: i64,
        timestamp: chrono::NaiveDateTime,
    ) -> Vec<String> {
        if !self.active || threat == 0 {
            return Vec::new();
        }

        let mut updated = Vec::new();

        for def in &self.definitions {
            if def.metric != ChallengeMetric::Threat {
                continue;
            }

            if def.matches(ctx, &self.entities, Some(source), Some(target), None, None)
                && let Some(val) = self.values.get_mut(&def.id)
                && source.is_player
            {
                if val.first_event_time.is_none() {
                    val.first_event_time = Some(timestamp);
                }
                val.value += threat;
                val.event_count += 1;
                *val.by_player.entry(source.entity_id).or_insert(0) += threat;
                updated.push(def.id.clone());
            }
        }

        updated
    }
}
