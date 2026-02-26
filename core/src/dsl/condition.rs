//! State-based conditions for gating triggers, timers, phases, and victory triggers.
//!
//! Conditions are distinct from triggers: triggers are event-driven (fire when something
//! happens), while conditions are state guards (evaluate to true/false based on current
//! encounter state). Conditions gate whether a trigger is allowed to fire.
//!
//! Conditions support recursive composition via `AllOf`, `AnyOf`, and `Not`.

use serde::{Deserialize, Serialize};

use super::counter::{ComparisonOp, CounterCondition};

/// Default operator for TimerTimeRemaining (gte — "at least N seconds remaining").
fn default_gte() -> ComparisonOp {
    ComparisonOp::Gte
}

/// A state-based condition that evaluates to true/false.
///
/// Unlike triggers (which fire on events), conditions check current encounter state.
/// Used to gate timers, phases, and victory triggers.
///
/// Supports recursive composition:
/// ```toml
/// # Timer only fires during "burn" phase when stack_count >= 3
/// conditions = [
///   { type = "phase_active", phase_ids = ["burn"] },
///   { type = "counter_compare", counter_id = "stack_count", operator = "gte", value = 3 },
/// ]
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Condition {
    /// True when the encounter is in one of the specified phases.
    /// Replaces the old `phases: Vec<String>` field on timers.
    PhaseActive {
        /// Phase IDs to check (any match = true)
        phase_ids: Vec<String>,
    },

    /// True when a counter satisfies the comparison.
    /// Replaces the old `counter_condition` field on timers/phases.
    CounterCompare {
        /// Counter to check
        counter_id: String,
        /// Comparison operator (defaults to eq)
        #[serde(default)]
        operator: ComparisonOp,
        /// Value to compare against
        value: u32,
    },

    /// True when a timer's remaining time satisfies the comparison.
    /// Inactive timers are treated as having 0.0 seconds remaining.
    /// Use `operator = "gte", value = 0.01` to check if a timer is active,
    /// or `operator = "lte", value = 5.0` to check if it's about to expire.
    TimerTimeRemaining {
        /// Timer definition ID to check
        timer_id: String,
        /// Comparison operator (typically gte or lte)
        #[serde(default = "default_gte")]
        operator: ComparisonOp,
        /// Seconds to compare against
        value: f32,
    },

    // ─── Composition ────────────────────────────────────────────────────────
    /// All sub-conditions must be true (AND logic).
    AllOf { conditions: Vec<Condition> },

    /// Any sub-condition must be true (OR logic).
    AnyOf { conditions: Vec<Condition> },

    /// Negation: true when the inner condition is false.
    Not { condition: Box<Condition> },
}

impl Condition {
    /// Returns a human-readable label for this condition type.
    pub fn label(&self) -> &'static str {
        match self {
            Self::PhaseActive { .. } => "Phase Active",
            Self::CounterCompare { .. } => "Counter Compare",
            Self::TimerTimeRemaining { .. } => "Timer Time Remaining",
            Self::AllOf { .. } => "All Of (AND)",
            Self::AnyOf { .. } => "Any Of (OR)",
            Self::Not { .. } => "Not",
        }
    }

    /// Returns the snake_case type name for this condition.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::PhaseActive { .. } => "phase_active",
            Self::CounterCompare { .. } => "counter_compare",
            Self::TimerTimeRemaining { .. } => "timer_time_remaining",
            Self::AllOf { .. } => "all_of",
            Self::AnyOf { .. } => "any_of",
            Self::Not { .. } => "not",
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Backward Compatibility: Merge Legacy Fields into Conditions
// ═══════════════════════════════════════════════════════════════════════════

/// Build a merged list of conditions from the new `conditions` field
/// plus legacy `phases` and `counter_condition` fields.
///
/// This provides backward compatibility: old TOML configs with `phases = [...]`
/// or `counter_condition = { ... }` are transparently converted to `Condition`
/// entries and merged with any explicit `conditions`.
pub fn merge_legacy_conditions(
    conditions: &[Condition],
    phases: &[String],
    counter_condition: Option<&CounterCondition>,
) -> Vec<Condition> {
    let mut merged = conditions.to_vec();

    if !phases.is_empty() {
        merged.push(Condition::PhaseActive {
            phase_ids: phases.to_vec(),
        });
    }

    if let Some(cc) = counter_condition {
        merged.push(Condition::CounterCompare {
            counter_id: cc.counter_id.clone(),
            operator: cc.operator,
            value: cc.value,
        });
    }

    merged
}

/// Check if the merged conditions list is empty (for skip_serializing_if).
pub fn has_no_effective_conditions(
    conditions: &[Condition],
    phases: &[String],
    counter_condition: &Option<CounterCondition>,
) -> bool {
    conditions.is_empty() && phases.is_empty() && counter_condition.is_none()
}
