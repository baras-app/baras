//! Timer definition types
//!
//! Definitions are templates loaded from TOML config files that describe
//! what timers to track and how to display them.

use serde::{Deserialize, Serialize};

use crate::effects::EntityFilter;
use crate::encounters::CounterCondition;
use crate::game_data::Difficulty;

/// What triggers a timer to start
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum TimerTrigger {
    /// Combat starts
    CombatStart,

    /// Specific ability is cast
    AbilityCast {
        /// Ability IDs that trigger this timer
        ability_ids: Vec<u64>,
    },

    /// Effect is applied to someone
    EffectApplied {
        /// Effect IDs that trigger this timer
        effect_ids: Vec<u64>,
    },

    /// Effect is removed from someone
    EffectRemoved {
        /// Effect IDs that trigger this timer
        effect_ids: Vec<u64>,
    },

    /// Another timer expires (chaining)
    TimerExpires {
        /// ID of the timer that triggers this one
        timer_id: String,
    },

    /// Boss HP reaches a threshold
    BossHpThreshold {
        /// HP percentage (0.0 - 100.0)
        hp_percent: f32,
    },

    /// Manually triggered (for testing/debug)
    Manual,

    // ─── Logical Composition ─────────────────────────────────────────────────

    /// All conditions must be met (AND logic)
    AllOf {
        conditions: Vec<TimerTrigger>,
    },

    /// Any condition suffices (OR logic)
    AnyOf {
        conditions: Vec<TimerTrigger>,
    },
}

/// Definition of a timer (loaded from config)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimerDefinition {
    /// Unique identifier for this timer
    pub id: String,

    /// Display name shown in overlays
    pub name: String,

    /// Whether this timer is currently enabled
    #[serde(default = "default_true")]
    pub enabled: bool,

    // ─── Trigger ────────────────────────────────────────────────────────────
    /// What causes this timer to start
    pub trigger: TimerTrigger,

    /// Source filter for trigger events
    #[serde(default)]
    pub source: EntityFilter,

    /// Target filter for trigger events
    #[serde(default)]
    pub target: EntityFilter,

    // ─── Duration ───────────────────────────────────────────────────────────
    /// Timer duration in seconds
    pub duration_secs: f32,

    /// If true, resets duration when triggered again
    #[serde(default)]
    pub can_be_refreshed: bool,

    /// Number of times this repeats after initial trigger (0 = no repeat)
    #[serde(default)]
    pub repeats: u8,

    // ─── Display ────────────────────────────────────────────────────────────
    /// Display color as RGBA
    #[serde(default = "default_timer_color")]
    pub color: [u8; 4],

    /// Show on raid frames instead of timer bar overlay?
    #[serde(default)]
    pub show_on_raid_frames: bool,

    // ─── Alerts ─────────────────────────────────────────────────────────────
    /// Alert when this many seconds remain (None = no alert)
    pub alert_at_secs: Option<f32>,

    /// Custom alert text (None = use timer name)
    pub alert_text: Option<String>,

    /// Audio file to play on alert
    pub audio_file: Option<String>,

    // ─── Chaining ───────────────────────────────────────────────────────────
    /// Timer ID to trigger when this one expires
    pub triggers_timer: Option<String>,

    // ─── Context ────────────────────────────────────────────────────────────
    /// Only active in specific encounters (empty = all)
    #[serde(default)]
    pub encounters: Vec<String>,

    /// Specific boss name (if applicable)
    pub boss: Option<String>,

    /// Active difficulties: "story", "veteran", "master"
    #[serde(default)]
    pub difficulties: Vec<String>,

    // ─── Phase/Counter Conditions (optional) ─────────────────────────────────
    /// Only active during these phases (empty = all phases)
    /// Only applies when fighting a boss with phase definitions
    #[serde(default)]
    pub phases: Vec<String>,

    /// Only active when counter meets condition
    /// Only applies when fighting a boss with counter definitions
    #[serde(default)]
    pub counter_condition: Option<CounterCondition>,
}

// ═══════════════════════════════════════════════════════════════════════════
// Serde Helpers
// ═══════════════════════════════════════════════════════════════════════════

fn default_true() -> bool {
    true
}

fn default_timer_color() -> [u8; 4] {
    [200, 200, 200, 255] // Light grey
}

// ═══════════════════════════════════════════════════════════════════════════
// Config File Structure
// ═══════════════════════════════════════════════════════════════════════════

/// Root structure for timer config files (TOML)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TimerConfig {
    /// Timer definitions in this file
    #[serde(default, rename = "timer")]
    pub timers: Vec<TimerDefinition>,
}

impl TimerDefinition {
    /// Check if this timer matches a given ability ID
    pub fn matches_ability(&self, ability_id: u64) -> bool {
        match &self.trigger {
            TimerTrigger::AbilityCast { ability_ids } => ability_ids.contains(&ability_id),
            _ => false,
        }
    }

    /// Check if this timer matches a given effect ID for apply triggers
    pub fn matches_effect_applied(&self, effect_id: u64) -> bool {
        match &self.trigger {
            TimerTrigger::EffectApplied { effect_ids } => effect_ids.contains(&effect_id),
            _ => false,
        }
    }

    /// Check if this timer matches a given effect ID for remove triggers
    pub fn matches_effect_removed(&self, effect_id: u64) -> bool {
        match &self.trigger {
            TimerTrigger::EffectRemoved { effect_ids } => effect_ids.contains(&effect_id),
            _ => false,
        }
    }

    /// Check if this timer is triggered by another timer expiring
    pub fn matches_timer_expires(&self, timer_id: &str) -> bool {
        match &self.trigger {
            TimerTrigger::TimerExpires { timer_id: trigger_id } => trigger_id == timer_id,
            _ => false,
        }
    }

    /// Check if this timer triggers on combat start
    pub fn triggers_on_combat_start(&self) -> bool {
        matches!(&self.trigger, TimerTrigger::CombatStart)
    }

    /// Check if this timer triggers when boss HP crosses below a threshold
    /// Returns true if timer has a BossHpThreshold trigger and current HP just crossed below it
    pub fn matches_boss_hp_threshold(&self, previous_hp: f32, current_hp: f32) -> bool {
        match &self.trigger {
            TimerTrigger::BossHpThreshold { hp_percent } => {
                // Trigger when HP crosses below threshold (was above, now at or below)
                previous_hp > *hp_percent && current_hp <= *hp_percent
            }
            _ => false,
        }
    }

    /// Check if this timer is active for the current phase/counter state
    pub fn is_active_for_state(&self, state: &crate::encounters::BossEncounterState) -> bool {
        // Check phase filter
        if !self.phases.is_empty() && !state.is_in_any_phase(&self.phases) {
            return false;
        }

        // Check counter condition
        if let Some(ref cond) = self.counter_condition
            && !state.check_counter_condition(cond) {
                return false;
        }

        true
    }

    /// Check if this timer is active for a given encounter context
    pub fn is_active_for_context(&self, encounter: Option<&str>, boss: Option<&str>, difficulty: Option<Difficulty>) -> bool {
        // Check encounter filter
        if !self.encounters.is_empty() {
            if let Some(enc) = encounter {
                if !self.encounters.iter().any(|e| e.eq_ignore_ascii_case(enc)) {
                    return false;
                }
            } else {
                return false; // Timer requires specific encounter but none provided
            }
        }

        // Check boss filter
        if let Some(timer_boss) = &self.boss {
            if let Some(current_boss) = boss {
                if !timer_boss.eq_ignore_ascii_case(current_boss) {
                    return false;
                }
            } else {
                return false; // Timer requires specific boss but none provided
            }
        }

        // Check difficulty filter
        if !self.difficulties.is_empty()
            && let Some(diff) = difficulty {
                // Match if the difficulty's config key matches any in the list
                if !self.difficulties.iter().any(|d| diff.matches_config_key(d)) {
                    return false;
                }
            // If no difficulty provided but timer has filter, allow it (be permissive)
        }

        true
    }
}
