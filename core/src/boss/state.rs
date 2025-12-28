//! Runtime encounter state
//!
//! Tracks the current state of a boss encounter during combat:
//! - Active boss and HP percentage
//! - Current phase
//! - Counter values

use chrono::NaiveDateTime;
use std::collections::{HashMap, HashSet};

use super::{CounterCondition, CounterDefinition};
use super::ChallengeContext;

/// Runtime state for a boss encounter
#[derive(Debug, Clone, Default)]
pub struct BossEncounterState {
    /// Currently detected boss (if any)
    pub active_boss: Option<ActiveBoss>,

    /// Current phase ID (e.g., "walker_1", "kephess_2", "burn")
    pub current_phase: Option<String>,

    /// Previous phase ID (for preceded_by checks)
    pub previous_phase: Option<String>,

    /// When the current phase started (for duration display)
    pub phase_started_at: Option<NaiveDateTime>,

    /// Counter values
    pub counters: HashMap<String, u32>,

    /// Boss HP percentage (0.0-100.0) - legacy single-boss tracking
    pub boss_hp_percent: f32,

    /// HP percentages by entity ID (for multi-boss encounters)
    pub hp_by_entity: HashMap<i64, f32>,

    /// HP percentages by NPC ID/class ID (most reliable for boss detection)
    pub hp_by_npc_id: HashMap<i64, f32>,

    /// Raw HP values by NPC ID: (current, max) - for overlay display
    pub hp_raw: HashMap<i64, (i64, i64)>,

    /// First time each NPC was seen (for sorting by encounter order)
    pub first_seen: HashMap<i64, NaiveDateTime>,

    /// HP percentages by boss name (for named HP triggers, fallback)
    pub hp_by_name: HashMap<String, f32>,

    /// When combat started
    pub combat_start: Option<NaiveDateTime>,

    /// Elapsed combat time in seconds
    pub combat_time_secs: f32,

    /// Previous combat time (for TimeElapsed threshold detection)
    pub prev_combat_time_secs: f32,

    /// NPC IDs of kill targets that have died during this encounter
    /// Used to determine if combat should end when all kill targets are dead
    pub dead_kill_targets: HashSet<i64>,
}

/// Information about the currently active boss
#[derive(Debug, Clone)]
pub struct ActiveBoss {
    /// Definition ID (e.g., "apex_vanguard")
    pub definition_id: String,

    /// Display name
    pub name: String,

    /// Entity ID in the combat log
    pub entity_id: i64,

    /// Maximum HP
    pub max_hp: i64,

    /// Current HP
    pub current_hp: i64,
}

impl BossEncounterState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset all state (on combat end or encounter change)
    pub fn reset(&mut self) {
        self.active_boss = None;
        self.current_phase = None;
        self.previous_phase = None;
        self.phase_started_at = None;
        self.counters.clear();
        self.boss_hp_percent = 100.0;
        self.hp_by_entity.clear();
        self.hp_by_npc_id.clear();
        self.hp_raw.clear();
        self.first_seen.clear();
        self.hp_by_name.clear();
        self.combat_start = None;
        self.combat_time_secs = 0.0;
        self.prev_combat_time_secs = 0.0;
        self.dead_kill_targets.clear();
    }

    /// Build a ChallengeContext snapshot for condition matching
    pub fn challenge_context(&self, boss_npc_ids: &[i64]) -> ChallengeContext {
        ChallengeContext {
            current_phase: self.current_phase.clone(),
            counters: self.counters.clone(),
            hp_by_npc_id: self.hp_by_npc_id.clone(),
            boss_npc_ids: boss_npc_ids.to_vec(),
        }
    }

    /// Set the active boss
    pub fn set_boss(&mut self, boss: ActiveBoss) {
        self.boss_hp_percent = boss.hp_percent();
        self.active_boss = Some(boss);
    }

    /// Clear the active boss
    pub fn clear_boss(&mut self) {
        self.active_boss = None;
        self.boss_hp_percent = 100.0;
    }

    /// Update boss HP and return true if HP changed
    pub fn update_boss_hp(&mut self, current: i64, max: i64) -> bool {
        let old_percent = self.boss_hp_percent;

        if max > 0 {
            self.boss_hp_percent = (current as f32 / max as f32) * 100.0;
        }

        if let Some(ref mut boss) = self.active_boss {
            boss.current_hp = current;
            boss.max_hp = max;
        }

        (old_percent - self.boss_hp_percent).abs() > 0.01
    }

    /// Update HP for a specific entity (multi-boss support)
    ///
    /// Tracks HP by:
    /// - `entity_id`: Runtime instance ID (unique per spawn)
    /// - `npc_id`: Class/template ID (same for all instances of that NPC type)
    /// - `name`: Display name (fallback for configs using names)
    ///
    /// Returns `Some((old_hp, new_hp))` if HP changed significantly, `None` otherwise.
    pub fn update_entity_hp(&mut self, entity_id: i64, npc_id: i64, name: &str, current: i64, max: i64, timestamp: NaiveDateTime) -> Option<(f32, f32)> {
        let new_percent = if max > 0 {
            (current as f32 / max as f32) * 100.0
        } else {
            100.0
        };

        let old_percent = self.hp_by_entity.get(&entity_id).copied().unwrap_or(100.0);

        // Track by all identifiers
        self.hp_by_entity.insert(entity_id, new_percent);
        if npc_id != 0 {
            self.hp_by_npc_id.insert(npc_id, new_percent);
            self.hp_raw.insert(npc_id, (current, max));
            // Record first time we saw this NPC (for encounter ordering)
            self.first_seen.entry(npc_id).or_insert(timestamp);
        }
        self.hp_by_name.insert(name.to_string(), new_percent);

        // Also update legacy single-boss tracking if this is the active boss
        if self.active_boss.as_ref().is_some_and(|b| b.entity_id == entity_id) {
            self.boss_hp_percent = new_percent;
            if let Some(ref mut boss) = self.active_boss {
                boss.current_hp = current;
                boss.max_hp = max;
            }
        }

        if (old_percent - new_percent).abs() > 0.01 {
            Some((old_percent, new_percent))
        } else {
            None
        }
    }

    /// Get HP percentage for a specific NPC ID (most reliable)
    pub fn get_npc_hp(&self, npc_id: i64) -> Option<f32> {
        self.hp_by_npc_id.get(&npc_id).copied()
    }

    /// Get HP percentage for a specific boss by name
    pub fn get_boss_hp(&self, name: &str) -> Option<f32> {
        self.hp_by_name.get(name).copied()
    }

    /// Get HP percentage for a specific entity
    pub fn get_entity_hp(&self, entity_id: i64) -> Option<f32> {
        self.hp_by_entity.get(&entity_id).copied()
    }

    /// Get raw HP values (current, max) for a specific NPC ID
    pub fn get_npc_hp_raw(&self, npc_id: i64) -> Option<(i64, i64)> {
        self.hp_raw.get(&npc_id).copied()
    }

    /// Get all raw HP values by NPC ID (for overlay display)
    pub fn all_hp_raw(&self) -> &HashMap<i64, (i64, i64)> {
        &self.hp_raw
    }

    /// Check if a specific boss is below HP threshold
    ///
    /// Priority: NPC ID > name > any tracked boss
    pub fn is_boss_hp_below(&self, npc_id: Option<i64>, name: Option<&str>, threshold: f32) -> bool {
        // First try NPC ID (most reliable)
        if let Some(id) = npc_id
            && let Some(hp) = self.hp_by_npc_id.get(&id) {
                return *hp <= threshold;
        }

        // Fall back to name
        if let Some(boss_name) = name
            && let Some(hp) = self.hp_by_name.get(boss_name) {
                return *hp <= threshold;
        }

        // Fall back to any tracked boss (legacy behavior)
        if npc_id.is_none() && name.is_none() {
            return self.boss_hp_percent <= threshold;
        }

        false
    }

    /// Check if a specific boss is above HP threshold
    ///
    /// Priority: NPC ID > name > any tracked boss
    pub fn is_boss_hp_above(&self, npc_id: Option<i64>, name: Option<&str>, threshold: f32) -> bool {
        // First try NPC ID (most reliable)
        if let Some(id) = npc_id
            && let Some(hp) = self.hp_by_npc_id.get(&id) {
                return *hp >= threshold;
        }

        // Fall back to name
        if let Some(boss_name) = name
            && let Some(hp) = self.hp_by_name.get(boss_name) {
                return *hp >= threshold;
        }

        // Fall back to any tracked boss (legacy behavior)
        if npc_id.is_none() && name.is_none() {
            return self.boss_hp_percent >= threshold;
        }

        false
    }

    /// Set the current phase with timestamp for duration tracking
    pub fn set_phase(&mut self, phase_id: &str, timestamp: NaiveDateTime) {
        self.previous_phase = self.current_phase.take();
        self.current_phase = Some(phase_id.to_string());
        self.phase_started_at = Some(timestamp);
    }

    /// Get the current phase ID
    pub fn phase(&self) -> Option<&str> {
        self.current_phase.as_deref()
    }

    /// Get how long we've been in the current phase (in seconds)
    pub fn phase_duration_secs(&self, current_time: NaiveDateTime) -> f32 {
        self.phase_started_at
            .map(|start| (current_time - start).num_milliseconds() as f32 / 1000.0)
            .unwrap_or(0.0)
    }

    /// Check if currently in a specific phase
    pub fn is_in_phase(&self, phase_id: &str) -> bool {
        self.current_phase.as_deref() == Some(phase_id)
    }

    /// Check if currently in any of the specified phases
    pub fn is_in_any_phase(&self, phase_ids: &[String]) -> bool {
        if phase_ids.is_empty() {
            return true; // Empty list means "all phases"
        }
        if let Some(current) = &self.current_phase {
            phase_ids.iter().any(|p| p == current)
        } else {
            false
        }
    }

    /// Increment a counter and return the new value
    pub fn increment_counter(&mut self, counter_id: &str) -> u32 {
        let count = self.counters.entry(counter_id.to_string()).or_insert(0);
        *count += 1;
        *count
    }

    /// Modify a counter based on definition (supports increment, decrement, or set_value)
    /// Returns (old_value, new_value)
    pub fn modify_counter(&mut self, counter_id: &str, decrement: bool, set_value: Option<u32>) -> (u32, u32) {
        let old_value = self.get_counter(counter_id);
        let new_value = if let Some(val) = set_value {
            val
        } else if decrement {
            old_value.saturating_sub(1)
        } else {
            old_value + 1
        };
        self.counters.insert(counter_id.to_string(), new_value);
        (old_value, new_value)
    }

    /// Decrement a counter (saturates at 0) and return the new value
    pub fn decrement_counter(&mut self, counter_id: &str) -> u32 {
        let count = self.counters.entry(counter_id.to_string()).or_insert(0);
        *count = count.saturating_sub(1);
        *count
    }

    /// Get the current value of a counter
    pub fn get_counter(&self, counter_id: &str) -> u32 {
        self.counters.get(counter_id).copied().unwrap_or(0)
    }

    /// Set a counter to a specific value
    pub fn set_counter(&mut self, counter_id: &str, value: u32) {
        self.counters.insert(counter_id.to_string(), value);
    }

    /// Reset a counter to 0
    pub fn reset_counter(&mut self, counter_id: &str) {
        self.counters.insert(counter_id.to_string(), 0);
    }

    /// Reset multiple counters (to 0)
    pub fn reset_counters(&mut self, counter_ids: &[String]) {
        for id in counter_ids {
            self.counters.insert(id.clone(), 0);
        }
    }

    /// Reset multiple counters to their initial values (using definitions)
    pub fn reset_counters_to_initial(&mut self, counter_ids: &[String], definitions: &[CounterDefinition]) {
        for id in counter_ids {
            let initial = definitions
                .iter()
                .find(|d| d.id == *id)
                .map(|d| d.initial_value)
                .unwrap_or(0);
            self.counters.insert(id.clone(), initial);
        }
    }

    /// Reset all counters
    pub fn reset_all_counters(&mut self) {
        self.counters.clear();
    }

    /// Check a counter condition
    pub fn check_counter_condition(&self, cond: &CounterCondition) -> bool {
        let value = self.get_counter(&cond.counter_id);
        cond.operator.evaluate(value, cond.value)
    }

    /// Start combat timer
    pub fn start_combat(&mut self, timestamp: NaiveDateTime) {
        self.combat_start = Some(timestamp);
        self.combat_time_secs = 0.0;
        self.prev_combat_time_secs = 0.0;
    }

    /// Update combat time and return (old_time, new_time) for threshold checking
    pub fn update_combat_time(&mut self, current_timestamp: NaiveDateTime) -> (f32, f32) {
        let old_time = self.combat_time_secs;
        if let Some(start) = self.combat_start {
            let duration = current_timestamp - start;
            self.combat_time_secs = duration.num_milliseconds() as f32 / 1000.0;
        }
        self.prev_combat_time_secs = old_time;
        (old_time, self.combat_time_secs)
    }

    /// Check if boss is below HP threshold
    pub fn is_hp_below(&self, threshold: f32) -> bool {
        self.boss_hp_percent <= threshold
    }

    /// Check if boss is above HP threshold
    pub fn is_hp_above(&self, threshold: f32) -> bool {
        self.boss_hp_percent >= threshold
    }

    /// Record that a kill target NPC has died
    pub fn mark_kill_target_dead(&mut self, npc_id: i64) {
        self.dead_kill_targets.insert(npc_id);
    }

    /// Check if all required kill targets are dead
    ///
    /// Returns `true` if:
    /// - `kill_target_npc_ids` is non-empty AND
    /// - Every NPC ID in the list has been marked as dead
    ///
    /// Returns `false` if no kill targets are defined (empty list)
    pub fn all_kill_targets_dead(&self, kill_target_npc_ids: &[i64]) -> bool {
        if kill_target_npc_ids.is_empty() {
            return false; // No kill targets defined
        }
        kill_target_npc_ids
            .iter()
            .all(|id| self.dead_kill_targets.contains(id))
    }
}

impl ActiveBoss {
    /// Calculate HP percentage
    pub fn hp_percent(&self) -> f32 {
        if self.max_hp > 0 {
            (self.current_hp as f32 / self.max_hp as f32) * 100.0
        } else {
            100.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boss::ComparisonOp;

    #[test]
    fn test_counter_operations() {
        let mut state = BossEncounterState::new();

        assert_eq!(state.get_counter("test"), 0);
        assert_eq!(state.increment_counter("test"), 1);
        assert_eq!(state.increment_counter("test"), 2);
        assert_eq!(state.get_counter("test"), 2);

        state.reset_counter("test");
        assert_eq!(state.get_counter("test"), 0);
    }

    #[test]
    fn test_phase_checks() {
        let mut state = BossEncounterState::new();

        assert!(!state.is_in_phase("p1"));
        assert!(state.is_in_any_phase(&[])); // Empty = all phases

        state.set_phase("p1", chrono::NaiveDateTime::default());
        assert!(state.is_in_phase("p1"));
        assert!(!state.is_in_phase("p2"));
        assert!(state.is_in_any_phase(&["p1".to_string(), "p2".to_string()]));
        assert!(!state.is_in_any_phase(&["p2".to_string(), "p3".to_string()]));
    }

    #[test]
    fn test_counter_condition() {
        let mut state = BossEncounterState::new();
        state.set_counter("count", 5);

        let cond_eq = CounterCondition {
            counter_id: "count".to_string(),
            operator: ComparisonOp::Eq,
            value: 5,
        };
        assert!(state.check_counter_condition(&cond_eq));

        let cond_gt = CounterCondition {
            counter_id: "count".to_string(),
            operator: ComparisonOp::Gt,
            value: 3,
        };
        assert!(state.check_counter_condition(&cond_gt));

        let cond_lt = CounterCondition {
            counter_id: "count".to_string(),
            operator: ComparisonOp::Lt,
            value: 3,
        };
        assert!(!state.check_counter_condition(&cond_lt));
    }

    #[test]
    fn test_hp_tracking() {
        let mut state = BossEncounterState::new();
        state.set_boss(ActiveBoss {
            definition_id: "test".to_string(),
            name: "Test Boss".to_string(),
            entity_id: 1,
            max_hp: 1000,
            current_hp: 1000,
        });

        assert!((state.boss_hp_percent - 100.0).abs() < 0.01);
        assert!(state.is_hp_above(50.0));

        state.update_boss_hp(500, 1000);
        assert!((state.boss_hp_percent - 50.0).abs() < 0.01);
        assert!(state.is_hp_below(60.0));
        assert!(!state.is_hp_below(40.0));
    }
}
