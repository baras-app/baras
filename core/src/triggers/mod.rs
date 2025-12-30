//! Unified trigger system for timers, phases, and counters.
//!
//! This module provides a single `Trigger` enum that replaces the previously
//! separate `TimerTrigger`, `PhaseTrigger`, and `CounterTrigger` types.
//! Each system only responds to the trigger variants it supports.

mod matchers;

pub use matchers::{
    AbilityMatcher, AbilitySelector, EffectMatcher, EffectSelector, EntityMatcher, EntitySelector,
    EntitySelectorExt,
};

use serde::{Deserialize, Serialize};

// ═══════════════════════════════════════════════════════════════════════════
// Trigger Scope
// ═══════════════════════════════════════════════════════════════════════════

/// Which systems respond to a trigger type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TriggerScope(u8);

impl TriggerScope {
    pub const TIMER: Self = Self(0b001);
    pub const PHASE: Self = Self(0b010);
    pub const COUNTER: Self = Self(0b100);
    pub const ALL: Self = Self(0b111);

    /// Timer + Phase (no counter)
    pub const TIMER_PHASE: Self = Self(0b011);

    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Unified Trigger Enum
// ═══════════════════════════════════════════════════════════════════════════

/// Unified trigger for timers, phases, and counters.
///
/// Each variant documents which systems respond to it:
/// - `[T]` = Timer
/// - `[P]` = Phase
/// - `[C]` = Counter
/// - `[TPC]` = All systems
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum Trigger {
    // ─── Combat State [TPC] ────────────────────────────────────────────────

    /// Combat starts. [TPC]
    CombatStart,

    /// Combat ends. [C only]
    /// Default reset behavior for counters.
    CombatEnd,

    // ─── Abilities & Effects [TPC] ─────────────────────────────────────────

    /// Ability is cast. [TPC]
    AbilityCast {
        /// Ability selectors (ID or name).
        #[serde(default)]
        abilities: Vec<AbilitySelector>,
        #[serde(default)]
        source: EntityMatcher,
    },

    /// Effect/buff is applied. [TPC]
    EffectApplied {
        /// Effect selectors (ID or name).
        #[serde(default)]
        effects: Vec<EffectSelector>,
        #[serde(default)]
        source: EntityMatcher,
        #[serde(default)]
        target: EntityMatcher,
    },

    /// Effect/buff is removed. [TPC]
    EffectRemoved {
        /// Effect selectors (ID or name).
        #[serde(default)]
        effects: Vec<EffectSelector>,
        #[serde(default)]
        source: EntityMatcher,
        #[serde(default)]
        target: EntityMatcher,
    },

    /// Damage is taken from an ability. [TPC]
    /// Useful for tank buster detection and raid-wide damage events.
    DamageTaken {
        /// Ability selectors (ID or name).
        #[serde(default)]
        abilities: Vec<AbilitySelector>,
        #[serde(default)]
        source: EntityMatcher,
        #[serde(default)]
        target: EntityMatcher,
    },

    // ─── HP Thresholds [TPC / P only] ──────────────────────────────────────

    /// Boss HP drops below threshold. [TPC]
    BossHpBelow {
        hp_percent: f32,
        #[serde(flatten)]
        entity: EntityMatcher,
    },

    /// Boss HP rises above threshold. [P only]
    /// Used for heal-check mechanics.
    BossHpAbove {
        hp_percent: f32,
        #[serde(flatten)]
        entity: EntityMatcher,
    },

    // ─── Entity Lifecycle [TPC] ────────────────────────────────────────────

    /// NPC appears (first seen in combat log). [TPC]
    NpcAppears {
        #[serde(flatten)]
        entity: EntityMatcher,
    },

    /// Entity dies. [TPC]
    EntityDeath {
        #[serde(flatten)]
        entity: EntityMatcher,
    },

    /// NPC sets its target (e.g., sphere targeting player). [T only]
    TargetSet {
        #[serde(flatten)]
        entity: EntityMatcher,
    },

    // ─── Phase Events [TPC / C only] ───────────────────────────────────────

    /// Phase is entered. [TC]
    PhaseEntered { phase_id: String },

    /// Phase ends. [TPC]
    PhaseEnded { phase_id: String },

    /// Any phase change occurs. [C only]
    AnyPhaseChange,

    // ─── Counter Events [TP] ───────────────────────────────────────────────

    /// Counter reaches a specific value. [TP]
    CounterReaches { counter_id: String, value: u32 },

    // ─── Timer Events [T only] ─────────────────────────────────────────────

    /// Another timer expires (chaining). [T only]
    TimerExpires { timer_id: String },

    /// Another timer starts (for cancellation). [T only]
    TimerStarted { timer_id: String },

    // ─── Time-based [TP] ───────────────────────────────────────────────────

    /// Time elapsed since combat start. [TP]
    TimeElapsed { secs: f32 },

    // ─── System-specific ───────────────────────────────────────────────────

    /// Manual/debug trigger. [T only]
    Manual,

    /// Never triggers. [C only]
    /// Use for counters that should never auto-reset.
    Never,

    // ─── Composition [TPC] ─────────────────────────────────────────────────

    /// Any condition suffices (OR logic). [TPC]
    AnyOf { conditions: Vec<Trigger> },
}

impl Trigger {
    /// Returns which systems respond to this trigger type.
    pub const fn scope(&self) -> TriggerScope {
        match self {
            // Universal (all systems)
            Self::CombatStart
            | Self::AbilityCast { .. }
            | Self::EffectApplied { .. }
            | Self::EffectRemoved { .. }
            | Self::DamageTaken { .. }
            | Self::BossHpBelow { .. }
            | Self::NpcAppears { .. }
            | Self::EntityDeath { .. }
            | Self::PhaseEnded { .. }
            | Self::AnyOf { .. } => TriggerScope::ALL,

            // Timer + Phase
            Self::TimeElapsed { .. } | Self::CounterReaches { .. } => TriggerScope::TIMER_PHASE,

            // Timer + Counter
            Self::PhaseEntered { .. } => TriggerScope(TriggerScope::TIMER.0 | TriggerScope::COUNTER.0),

            // Timer only
            Self::TimerExpires { .. }
            | Self::TimerStarted { .. }
            | Self::TargetSet { .. }
            | Self::Manual => TriggerScope::TIMER,

            // Phase only
            Self::BossHpAbove { .. } => TriggerScope::PHASE,

            // Counter only
            Self::CombatEnd | Self::AnyPhaseChange | Self::Never => TriggerScope::COUNTER,
        }
    }

    /// Check if this trigger is valid for use as a timer trigger.
    pub const fn valid_for_timer(&self) -> bool {
        self.scope().contains(TriggerScope::TIMER)
    }

    /// Check if this trigger is valid for use as a phase trigger.
    pub const fn valid_for_phase(&self) -> bool {
        self.scope().contains(TriggerScope::PHASE)
    }

    /// Check if this trigger is valid for use as a counter trigger.
    pub const fn valid_for_counter(&self) -> bool {
        self.scope().contains(TriggerScope::COUNTER)
    }

    /// Check if this trigger contains CombatStart (directly or nested in AnyOf).
    pub fn contains_combat_start(&self) -> bool {
        match self {
            Self::CombatStart => true,
            Self::AnyOf { conditions } => conditions.iter().any(|c| c.contains_combat_start()),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trigger_scope_combat_start() {
        let trigger = Trigger::CombatStart;
        assert!(trigger.valid_for_timer());
        assert!(trigger.valid_for_phase());
        assert!(trigger.valid_for_counter());
    }

    #[test]
    fn trigger_scope_timer_only() {
        let trigger = Trigger::TimerExpires { timer_id: "test".into() };
        assert!(trigger.valid_for_timer());
        assert!(!trigger.valid_for_phase());
        assert!(!trigger.valid_for_counter());
    }

    #[test]
    fn trigger_scope_counter_only() {
        let trigger = Trigger::CombatEnd;
        assert!(!trigger.valid_for_timer());
        assert!(!trigger.valid_for_phase());
        assert!(trigger.valid_for_counter());
    }

    #[test]
    fn trigger_scope_phase_only() {
        let trigger = Trigger::BossHpAbove {
            hp_percent: 50.0,
            entity: EntityMatcher::default(),
        };
        assert!(!trigger.valid_for_timer());
        assert!(trigger.valid_for_phase());
        assert!(!trigger.valid_for_counter());
    }

    #[test]
    fn contains_combat_start_nested() {
        let trigger = Trigger::AnyOf {
            conditions: vec![
                Trigger::AbilityCast { abilities: vec![AbilitySelector::Id(123)], source: EntityMatcher::default() },
                Trigger::CombatStart,
            ],
        };
        assert!(trigger.contains_combat_start());
    }

    #[test]
    fn serde_round_trip() {
        let trigger = Trigger::AbilityCast {
            abilities: vec![AbilitySelector::Id(123), AbilitySelector::Id(456)],
            source: EntityMatcher::by_npc_id(789),
        };
        let toml = toml::to_string(&trigger).unwrap();
        let parsed: Trigger = toml::from_str(&toml).unwrap();
        assert_eq!(trigger, parsed);
    }

    #[test]
    fn serde_mixed_selectors() {
        let trigger = Trigger::EffectApplied {
            effects: vec![
                EffectSelector::Id(100),
                EffectSelector::Name("Burn".to_string()),
            ],
            source: EntityMatcher::default(),
            target: EntityMatcher::default(),
        };
        let toml = toml::to_string(&trigger).unwrap();
        let parsed: Trigger = toml::from_str(&toml).unwrap();
        assert_eq!(trigger, parsed);
    }
}
