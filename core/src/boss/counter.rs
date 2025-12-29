//! Counter definitions for boss encounters
//!
//! Counters track occurrences during a fight (e.g., add spawns, ability casts).

use serde::{Deserialize, Serialize};

/// A counter that tracks occurrences during a boss fight
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CounterDefinition {
    /// Counter identifier (e.g., "bull_count")
    pub id: String,

    /// What increments this counter
    pub increment_on: CounterTrigger,

    /// When to reset to initial_value (default: combat_end)
    /// Uses the same trigger types as increment_on for consistency
    #[serde(default)]
    pub reset_on: CounterTrigger,

    /// Starting value (and value after reset)
    #[serde(default)]
    pub initial_value: u32,

    /// Optional: decrement instead of increment (for countdown patterns)
    #[serde(default)]
    pub decrement: bool,

    /// Optional: set to specific value instead of increment/decrement
    #[serde(default)]
    pub set_value: Option<u32>,
}

/// Events that increment or modify a counter
/// Used for both `increment_on` and `reset_on` triggers
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum CounterTrigger {
    /// Combat starts (useful for reset_on to reset at fight start)
    CombatStart,

    /// Combat ends (default reset behavior)
    CombatEnd,

    /// Ability is cast
    AbilityCast {
        #[serde(default)]
        ability_ids: Vec<u64>,
        /// Optional source filter (entity name from roster)
        #[serde(default)]
        source: Option<String>,
    },

    /// Effect/buff is applied
    EffectApplied {
        #[serde(default)]
        effect_ids: Vec<u64>,
        /// Optional target filter ("local_player" or entity name)
        #[serde(default)]
        target: Option<String>,
    },

    /// Effect/buff is removed
    EffectRemoved {
        #[serde(default)]
        effect_ids: Vec<u64>,
        /// Optional target filter ("local_player" or entity name)
        #[serde(default)]
        target: Option<String>,
    },

    /// Timer expires
    TimerExpires {
        timer_id: String,
    },

    /// Timer starts (for cancellation patterns)
    TimerStarts {
        timer_id: String,
    },

    /// Phase is entered
    PhaseEntered {
        phase_id: String,
    },

    /// Phase ends
    PhaseEnded {
        phase_id: String,
    },

    /// Any phase change occurs
    AnyPhaseChange,

    /// NPC is first seen (add spawn)
    EntityFirstSeen {
        /// Entity reference from roster (preferred)
        #[serde(default)]
        entity: Option<String>,
        /// NPC ID (legacy/fallback)
        #[serde(default)]
        npc_id: Option<i64>,
        /// Entity name fallback (runtime matching)
        #[serde(default)]
        entity_name: Option<String>,
    },

    /// Entity dies
    EntityDeath {
        /// Entity reference from roster (preferred)
        #[serde(default)]
        entity: Option<String>,
        /// NPC ID (legacy/fallback)
        #[serde(default)]
        npc_id: Option<i64>,
        /// Entity name fallback (runtime matching)
        #[serde(default)]
        entity_name: Option<String>,
    },

    /// Counter reaches a specific value (for chained counter logic)
    CounterReaches {
        counter_id: String,
        value: u32,
    },

    /// HP threshold crossed (for HP-based counter triggers)
    BossHpBelow {
        hp_percent: f32,
        #[serde(default)]
        entity: Option<String>,
        #[serde(default)]
        boss_name: Option<String>,
    },

    /// Never triggers (use for counters that should never auto-reset)
    Never,
}

impl Default for CounterTrigger {
    fn default() -> Self {
        CounterTrigger::CombatEnd
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Counter Conditions (shared with timers)
// ═══════════════════════════════════════════════════════════════════════════

/// Condition for counter-based timer/phase activation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CounterCondition {
    /// Counter to check
    pub counter_id: String,

    /// Comparison operator
    #[serde(default)]
    pub operator: ComparisonOp,

    /// Value to compare against
    pub value: u32,
}

/// Comparison operators for counter conditions
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonOp {
    #[default]
    Eq,
    Lt,
    Gt,
    Lte,
    Gte,
    Ne,
}

impl ComparisonOp {
    pub fn evaluate(&self, left: u32, right: u32) -> bool {
        match self {
            ComparisonOp::Eq => left == right,
            ComparisonOp::Lt => left < right,
            ComparisonOp::Gt => left > right,
            ComparisonOp::Lte => left <= right,
            ComparisonOp::Gte => left >= right,
            ComparisonOp::Ne => left != right,
        }
    }
}
