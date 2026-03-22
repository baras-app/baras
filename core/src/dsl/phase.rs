//! Phase definitions for boss encounters
//!
//! Phases represent distinct stages of a boss fight with different mechanics.

use serde::{Deserialize, Serialize};

use super::condition::Condition;
use super::triggers::Trigger;
use super::CounterCondition;

// Re-export Trigger as PhaseTrigger for backward compatibility during migration
pub use super::triggers::Trigger as PhaseTrigger;

/// A phase within a boss encounter
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhaseDefinition {
    /// Phase identifier (auto-generated from name if empty)
    pub id: String,

    /// Display name (used for ID generation, must be unique within encounter)
    pub name: String,

    /// Whether this phase is active (disabled phases are skipped at runtime)
    #[serde(default = "crate::serde_defaults::default_true")]
    pub enabled: bool,

    /// Optional in-game display text (defaults to name if not set)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_text: Option<String>,

    /// What triggers this phase to start
    #[serde(alias = "trigger")]
    pub start_trigger: Trigger,

    /// What triggers this phase to end (optional - otherwise ends when another phase starts)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_trigger: Option<Trigger>,

    /// Phase that must immediately precede this one (guard condition)
    /// e.g., walker_2 has preceded_by = "kephess_1" so it only fires after kephess_1
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preceded_by: Option<String>,

    /// State conditions that must be satisfied for this phase to activate.
    /// Implicitly AND'd — all conditions must be true.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<Condition>,

    /// DEPRECATED: Use `conditions` with `counter_compare` instead.
    /// Only activate when counter meets condition (guard).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counter_condition: Option<CounterCondition>,

    /// Counters to reset when entering this phase
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resets_counters: Vec<String>,

    /// Difficulty tiers this phase applies to (e.g., ["veteran", "master"]).
    /// Empty = all difficulties. When set, the phase start trigger is only
    /// evaluated on matching difficulties; on other difficulties it is silently
    /// skipped (timers that condition on this phase will also not fire).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub difficulties: Vec<String>,
}
