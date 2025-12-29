//! Phase definitions for boss encounters
//!
//! Phases represent distinct stages of a boss fight with different mechanics.

use serde::{Deserialize, Serialize};

use super::CounterCondition;

/// A phase within a boss encounter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseDefinition {
    /// Phase identifier (e.g., "p1", "walker_1", "kephess_2", "burn")
    pub id: String,

    /// Display name
    pub name: String,

    /// What triggers this phase to start
    #[serde(alias = "trigger")]
    pub start_trigger: PhaseTrigger,

    /// What triggers this phase to end (optional - otherwise ends when another phase starts)
    #[serde(default)]
    pub end_trigger: Option<PhaseTrigger>,

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

/// Triggers for phase transitions
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum PhaseTrigger {
    /// Combat start (initial phase)
    CombatStart,

    /// Boss HP drops below threshold
    /// Priority: entity > npc_id > boss_name > any boss
    BossHpBelow {
        hp_percent: f32,
        /// Entity reference from roster (preferred)
        #[serde(default)]
        entity: Option<String>,
        /// NPC class/template ID (legacy/fallback)
        #[serde(default)]
        npc_id: Option<i64>,
        /// Boss name (fallback - may vary by locale)
        #[serde(default)]
        boss_name: Option<String>,
    },

    /// Boss HP rises above threshold
    /// Priority: entity > npc_id > boss_name > any boss
    BossHpAbove {
        hp_percent: f32,
        /// Entity reference from roster (preferred)
        #[serde(default)]
        entity: Option<String>,
        /// NPC class/template ID (legacy/fallback)
        #[serde(default)]
        npc_id: Option<i64>,
        /// Boss name (fallback - may vary by locale)
        #[serde(default)]
        boss_name: Option<String>,
    },

    /// Specific ability is cast
    AbilityCast {
        #[serde(default)]
        ability_ids: Vec<u64>,
    },

    /// Effect applied to boss or players
    EffectApplied {
        #[serde(default)]
        effect_ids: Vec<u64>,
    },

    /// Effect removed
    EffectRemoved {
        #[serde(default)]
        effect_ids: Vec<u64>,
    },

    /// Counter reaches value
    CounterReaches { counter_id: String, value: u32 },

    /// Time elapsed since combat start
    TimeElapsed { secs: f32 },

    /// Entity is first seen (add spawn)
    EntityFirstSeen {
        /// Entity reference from roster (preferred)
        #[serde(default)]
        entity: Option<String>,
        /// NPC ID to watch for (legacy/fallback)
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
        /// NPC ID to watch for (legacy/fallback)
        #[serde(default)]
        npc_id: Option<i64>,
        /// Entity name fallback (runtime matching)
        #[serde(default)]
        entity_name: Option<String>,
    },

    /// Another phase's end_trigger fired
    PhaseEnded {
        /// Single phase ID (convenience)
        #[serde(default)]
        phase_id: Option<String>,
        /// Multiple phase IDs (any match triggers)
        #[serde(default)]
        phase_ids: Vec<String>,
    },

    // ─── Logical Composition ─────────────────────────────────────────────────

    /// Any condition suffices (OR logic)
    AnyOf {
        conditions: Vec<PhaseTrigger>,
    },
}

impl PhaseTrigger {
    /// Check if this trigger contains CombatStart (directly or nested in AnyOf)
    pub fn contains_combat_start(&self) -> bool {
        match self {
            PhaseTrigger::CombatStart => true,
            PhaseTrigger::AnyOf { conditions } => {
                conditions.iter().any(|c| c.contains_combat_start())
            }
            _ => false,
        }
    }
}
