//! Runtime encounter state
//!
//! Tracks the current state of a boss encounter during combat:
//! - Active boss and HP percentage
//! - Current phase
//! - Counter values

use chrono::NaiveDateTime;
use std::collections::HashMap;

use super::{ChallengeContext, CounterCondition};

/// Runtime state for a boss encounter
#[derive(Debug, Clone, Default)]
pub struct BossEncounterState {
    /// Currently detected boss (if any)
    pub active_boss: Option<ActiveBoss>,

    /// Current phase ID
    pub current_phase: Option<String>,

    /// Counter values
    pub counters: HashMap<String, u32>,

    /// Boss HP percentage (0.0-100.0) - legacy single-boss tracking
    pub boss_hp_percent: f32,

    /// HP percentages by entity ID (for multi-boss encounters)
    pub hp_by_entity: HashMap<i64, f32>,

    /// HP percentages by NPC ID/class ID (most reliable for boss detection)
    pub hp_by_npc_id: HashMap<i64, f32>,

    /// HP percentages by boss name (for named HP triggers, fallback)
    pub hp_by_name: HashMap<String, f32>,

    /// When combat started
    pub combat_start: Option<NaiveDateTime>,

    /// Elapsed combat time in seconds
    pub combat_time_secs: f32,
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
        self.counters.clear();
        self.boss_hp_percent = 100.0;
        self.hp_by_entity.clear();
        self.hp_by_npc_id.clear();
        self.hp_by_name.clear();
        self.combat_start = None;
        self.combat_time_secs = 0.0;
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
    pub fn update_entity_hp(&mut self, entity_id: i64, npc_id: i64, name: &str, current: i64, max: i64) -> Option<(f32, f32)> {
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

    /// Set the current phase
    pub fn set_phase(&mut self, phase_id: &str) {
        self.current_phase = Some(phase_id.to_string());
    }

    /// Get the current phase ID
    pub fn phase(&self) -> Option<&str> {
        self.current_phase.as_deref()
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

    /// Reset multiple counters
    pub fn reset_counters(&mut self, counter_ids: &[String]) {
        for id in counter_ids {
            self.counters.insert(id.clone(), 0);
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
    }

    /// Update combat time
    pub fn update_combat_time(&mut self, current_timestamp: NaiveDateTime) {
        if let Some(start) = self.combat_start {
            let duration = current_timestamp - start;
            self.combat_time_secs = duration.num_milliseconds() as f32 / 1000.0;
        }
    }

    /// Check if boss is below HP threshold
    pub fn is_hp_below(&self, threshold: f32) -> bool {
        self.boss_hp_percent <= threshold
    }

    /// Check if boss is above HP threshold
    pub fn is_hp_above(&self, threshold: f32) -> bool {
        self.boss_hp_percent >= threshold
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

        state.set_phase("p1");
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
