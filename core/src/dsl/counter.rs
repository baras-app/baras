//! Counter definitions for boss encounters
//!
//! Counters track occurrences during a fight (e.g., add spawns, ability casts).
//! Counters can also automatically track effect stacks on entities.

use serde::{Deserialize, Serialize};

use super::triggers::Trigger;
pub use baras_types::{EffectStackConfig, StackAggregation};

// Re-export Trigger as CounterTrigger for backward compatibility during migration
pub use super::triggers::Trigger as CounterTrigger;

/// A counter that tracks occurrences during a boss fight
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CounterDefinition {
    /// Counter identifier (auto-generated from name if empty)
    pub id: String,

    /// Display name (used for ID generation, must be unique within encounter)
    pub name: String,

    /// Whether this counter is active (disabled counters are skipped at runtime)
    #[serde(default = "crate::serde_defaults::default_true")]
    pub enabled: bool,

    /// Optional in-game display text (defaults to name, then id)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_text: Option<String>,

    /// Free-text note shown in the editor only; never read at runtime
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,

    /// What increments this counter
    pub increment_on: Trigger,

    /// What decrements this counter (optional, for countdown patterns)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decrement_on: Option<Trigger>,

    /// When to reset to initial_value (default: combat_end)
    /// Uses the same trigger types as increment_on for consistency
    #[serde(default = "default_reset_trigger")]
    pub reset_on: Trigger,

    /// Starting value (and value after reset)
    #[serde(default)]
    pub initial_value: u32,

    /// Optional: decrement instead of increment (for countdown patterns)
    /// DEPRECATED: Use decrement_on instead for more flexibility
    #[serde(default)]
    pub decrement: bool,

    /// Optional: set to specific value instead of increment/decrement
    #[serde(default)]
    pub set_value: Option<u32>,

    /// If set, this counter automatically tracks the stack count of the
    /// specified effect instead of using increment_on/decrement_on triggers.
    /// The counter value reflects the current (aggregated) stack count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track_effect_stacks: Option<EffectStackConfig>,
}

fn default_reset_trigger() -> Trigger {
    Trigger::CombatEnd
}

// ═══════════════════════════════════════════════════════════════════════════
// Counter Conditions (shared with timers)
// ═══════════════════════════════════════════════════════════════════════════

/// Condition for counter-based timer/phase activation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
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

    /// Evaluate a comparison on f32 values (used for timer remaining seconds).
    pub fn evaluate_f32(&self, left: f32, right: f32) -> bool {
        match self {
            ComparisonOp::Eq => (left - right).abs() < f32::EPSILON,
            ComparisonOp::Lt => left < right,
            ComparisonOp::Gt => left > right,
            ComparisonOp::Lte => left <= right,
            ComparisonOp::Gte => left >= right,
            ComparisonOp::Ne => (left - right).abs() >= f32::EPSILON,
        }
    }
}
