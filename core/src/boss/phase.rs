//! Phase definitions for boss encounters
//!
//! Phases represent distinct stages of a boss fight with different mechanics.

use serde::{Deserialize, Serialize};

use super::CounterCondition;
use crate::triggers::Trigger;

// Re-export Trigger as PhaseTrigger for backward compatibility during migration
pub use crate::triggers::Trigger as PhaseTrigger;

/// A phase within a boss encounter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseDefinition {
    /// Phase identifier (auto-generated from name if empty)
    pub id: String,

    /// Display name (used for ID generation, must be unique within encounter)
    pub name: String,

    /// Optional in-game display text (defaults to name if not set)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_text: Option<String>,

    /// What triggers this phase to start
    #[serde(alias = "trigger")]
    pub start_trigger: Trigger,

    /// What triggers this phase to end (optional - otherwise ends when another phase starts)
    #[serde(default)]
    pub end_trigger: Option<Trigger>,

    /// Phase that must immediately precede this one (guard condition)
    /// e.g., walker_2 has preceded_by = "kephess_1" so it only fires after kephess_1
    #[serde(default)]
    pub preceded_by: Option<String>,

    /// Only activate when counter meets condition (guard)
    /// e.g., trandos phase only fires when siege_droid_deaths >= 3
    #[serde(default)]
    pub counter_condition: Option<CounterCondition>,

    /// Counters to reset when entering this phase
    #[serde(default)]
    pub resets_counters: Vec<String>,
}
