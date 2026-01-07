//! Unified trigger system for timers, phases, and counters.
//!
//! This module provides a single `Trigger` enum that replaces the previously
//! separate `TimerTrigger`, `PhaseTrigger`, and `CounterTrigger` types.
//! Each system only responds to the trigger variants it supports.

mod matchers;

pub use matchers::{AbilitySelector, EffectSelector, EntitySelector, EntitySelectorExt};

// Re-export EntityFilter for use in triggers
pub use baras_types::EntityFilter;

use crate::dsl::EntityDefinition;
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
        /// Who cast the ability (default: any)
        #[serde(default = "EntityFilter::default_any")]
        source: EntityFilter,
    },

    /// Effect/buff is applied. [TPC]
    EffectApplied {
        /// Effect selectors (ID or name).
        #[serde(default)]
        effects: Vec<EffectSelector>,
        /// Who applied the effect (default: any)
        #[serde(default = "EntityFilter::default_any")]
        source: EntityFilter,
        /// Who received the effect (default: any)
        #[serde(default = "EntityFilter::default_any")]
        target: EntityFilter,
    },

    /// Effect/buff is removed. [TPC]
    EffectRemoved {
        /// Effect selectors (ID or name).
        #[serde(default)]
        effects: Vec<EffectSelector>,
        /// Who applied the effect (default: any)
        #[serde(default = "EntityFilter::default_any")]
        source: EntityFilter,
        /// Who lost the effect (default: any)
        #[serde(default = "EntityFilter::default_any")]
        target: EntityFilter,
    },

    /// Damage is taken from an ability. [TPC]
    /// Useful for tank buster detection and raid-wide damage events.
    DamageTaken {
        /// Ability selectors (ID or name).
        #[serde(default)]
        abilities: Vec<AbilitySelector>,
        /// Who dealt the damage (default: any)
        #[serde(default = "EntityFilter::default_any")]
        source: EntityFilter,
        /// Who took the damage (default: any)
        #[serde(default = "EntityFilter::default_any")]
        target: EntityFilter,
    },

    // ─── HP Thresholds [TPC / P only] ──────────────────────────────────────
    /// Boss HP drops below threshold. [TPC]
    BossHpBelow {
        hp_percent: f32,
        /// Specific boss to monitor (empty = any boss)
        #[serde(default)]
        selector: Vec<EntitySelector>,
    },

    /// Boss HP rises above threshold. [P only]
    /// Used for heal-check mechanics.
    BossHpAbove {
        hp_percent: f32,
        /// Specific boss to monitor (empty = any boss)
        #[serde(default)]
        selector: Vec<EntitySelector>,
    },

    // ─── Entity Lifecycle [TPC] ────────────────────────────────────────────
    /// NPC appears (first seen in combat log). [TPC]
    NpcAppears {
        /// NPCs to match (by ID or name)
        #[serde(default)]
        selector: Vec<EntitySelector>,
    },

    /// Entity dies. [TPC]
    EntityDeath {
        /// Entities to match (empty = any death)
        #[serde(default)]
        selector: Vec<EntitySelector>,
    },

    /// NPC sets its target (e.g., sphere targeting player). [T only]
    TargetSet {
        /// Which NPC is doing the targeting (by ID or name)
        #[serde(default)]
        selector: Vec<EntitySelector>,
        /// Who is being targeted (default: any)
        #[serde(default = "EntityFilter::default_any")]
        target: EntityFilter,
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
            Self::PhaseEntered { .. } => {
                TriggerScope(TriggerScope::TIMER.0 | TriggerScope::COUNTER.0)
            }

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

    /// Get the source filter from this trigger (for event-based triggers).
    /// Returns `None` for triggers that don't have a source filter (treated as "any").
    pub fn source_filter(&self) -> Option<&EntityFilter> {
        match self {
            Self::AbilityCast { source, .. }
            | Self::EffectApplied { source, .. }
            | Self::EffectRemoved { source, .. }
            | Self::DamageTaken { source, .. } => Some(source),
            _ => None,
        }
    }

    /// Get the target filter from this trigger (for event-based triggers).
    /// Returns `None` for triggers that don't have a target filter (treated as "any").
    pub fn target_filter(&self) -> Option<&EntityFilter> {
        match self {
            Self::EffectApplied { target, .. }
            | Self::EffectRemoved { target, .. }
            | Self::DamageTaken { target, .. }
            | Self::TargetSet { target, .. } => Some(target),
            _ => None,
        }
    }

    /// Extract both source and target filters from this trigger.
    /// Returns default "Any" filters for triggers that don't have them.
    pub fn source_target_filters(&self) -> (EntityFilter, EntityFilter) {
        let source = self.source_filter().cloned().unwrap_or_default();
        let target = self.target_filter().cloned().unwrap_or_default();
        (source, target)
    }

    /// Create a new trigger with updated source and target filters.
    /// Only affects trigger variants that support these filters.
    pub fn with_source_target(self, source: EntityFilter, target: EntityFilter) -> Self {
        match self {
            Self::AbilityCast { abilities, .. } => Self::AbilityCast { abilities, source },
            Self::EffectApplied { effects, .. } => Self::EffectApplied {
                effects,
                source,
                target,
            },
            Self::EffectRemoved { effects, .. } => Self::EffectRemoved {
                effects,
                source,
                target,
            },
            Self::DamageTaken { abilities, .. } => Self::DamageTaken {
                abilities,
                source,
                target,
            },
            Self::TargetSet { selector, .. } => Self::TargetSet { selector, target },
            other => other, // Leave unchanged for triggers without source/target
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Unified Trigger Matching (used by timers, phases, and counters)
    // ═══════════════════════════════════════════════════════════════════════════

    /// Check if trigger matches an ability cast.
    pub fn matches_ability(&self, ability_id: u64, ability_name: Option<&str>) -> bool {
        match self {
            Self::AbilityCast { abilities, .. } => {
                // Require explicit selectors - empty list matches nothing
                !abilities.is_empty()
                    && abilities
                        .iter()
                        .any(|s| s.matches(ability_id, ability_name))
            }
            Self::AnyOf { conditions } => conditions
                .iter()
                .any(|c| c.matches_ability(ability_id, ability_name)),
            _ => false,
        }
    }

    /// Check if trigger matches an effect being applied.
    pub fn matches_effect_applied(&self, effect_id: u64, effect_name: Option<&str>) -> bool {
        match self {
            Self::EffectApplied { effects, .. } => {
                // Require explicit selectors - empty list matches nothing
                !effects.is_empty() && effects.iter().any(|s| s.matches(effect_id, effect_name))
            }
            Self::AnyOf { conditions } => conditions
                .iter()
                .any(|c| c.matches_effect_applied(effect_id, effect_name)),
            _ => false,
        }
    }

    /// Check if trigger matches an effect being removed.
    pub fn matches_effect_removed(&self, effect_id: u64, effect_name: Option<&str>) -> bool {
        match self {
            Self::EffectRemoved { effects, .. } => {
                // Require explicit selectors - empty list matches nothing
                !effects.is_empty() && effects.iter().any(|s| s.matches(effect_id, effect_name))
            }
            Self::AnyOf { conditions } => conditions
                .iter()
                .any(|c| c.matches_effect_removed(effect_id, effect_name)),
            _ => false,
        }
    }

    /// Check if trigger matches damage taken from an ability.
    pub fn matches_damage_taken(&self, ability_id: u64, ability_name: Option<&str>) -> bool {
        match self {
            Self::DamageTaken { abilities, .. } => {
                // Require explicit selectors - empty list matches nothing
                !abilities.is_empty()
                    && abilities
                        .iter()
                        .any(|s| s.matches(ability_id, ability_name))
            }
            Self::AnyOf { conditions } => conditions
                .iter()
                .any(|c| c.matches_damage_taken(ability_id, ability_name)),
            _ => false,
        }
    }

    /// Check if trigger matches boss HP crossing below a threshold.
    /// The entity whose HP changed must match the selector.
    pub fn matches_boss_hp_below(
        &self,
        entities: &[EntityDefinition],
        npc_id: i64,
        entity_name: &str,
        old_hp: f32,
        new_hp: f32,
    ) -> bool {
        match self {
            Self::BossHpBelow {
                hp_percent,
                selector,
            } => {
                // Check HP threshold crossing
                let crossed = old_hp > *hp_percent && new_hp <= *hp_percent;
                if !crossed {
                    return false;
                }

                // No selector = any boss crossing threshold
                if selector.is_empty() {
                    return true;
                }

                // Match via roster alias → NPC ID → name
                selector.matches_with_roster(entities, npc_id, Some(entity_name))
            }
            Self::AnyOf { conditions } => conditions
                .iter()
                .any(|c| c.matches_boss_hp_below(entities, npc_id, entity_name, old_hp, new_hp)),
            _ => false,
        }
    }

    /// Check if trigger matches boss HP crossing above a threshold.
    /// Used for heal-check mechanics.
    pub fn matches_boss_hp_above(
        &self,
        entities: &[EntityDefinition],
        npc_id: i64,
        entity_name: &str,
        old_hp: f32,
        new_hp: f32,
    ) -> bool {
        match self {
            Self::BossHpAbove {
                hp_percent,
                selector,
            } => {
                // Check HP threshold crossing
                let crossed = old_hp < *hp_percent && new_hp >= *hp_percent;
                if !crossed {
                    return false;
                }

                // No selector = any boss crossing threshold
                if selector.is_empty() {
                    return true;
                }

                // Match via roster alias → NPC ID → name
                selector.matches_with_roster(entities, npc_id, Some(entity_name))
            }
            Self::AnyOf { conditions } => conditions
                .iter()
                .any(|c| c.matches_boss_hp_above(entities, npc_id, entity_name, old_hp, new_hp)),
            _ => false,
        }
    }

    /// Check if trigger matches NPC first appearing.
    pub fn matches_npc_appears(
        &self,
        entities: &[EntityDefinition],
        npc_id: i64,
        entity_name: &str,
    ) -> bool {
        match self {
            Self::NpcAppears { selector } => {
                // Require explicit filter for NPC appears
                if selector.is_empty() {
                    return false;
                }
                // Match via roster alias → NPC ID → name
                selector.matches_with_roster(entities, npc_id, Some(entity_name))
            }
            Self::AnyOf { conditions } => conditions
                .iter()
                .any(|c| c.matches_npc_appears(entities, npc_id, entity_name)),
            _ => false,
        }
    }

    /// Check if trigger matches entity death.
    pub fn matches_entity_death(
        &self,
        entities: &[EntityDefinition],
        npc_id: i64,
        entity_name: &str,
    ) -> bool {
        match self {
            Self::EntityDeath { selector } => {
                // Empty selector = any death
                if selector.is_empty() {
                    return true;
                }
                // Match via roster alias → NPC ID → name
                selector.matches_with_roster(entities, npc_id, Some(entity_name))
            }
            Self::AnyOf { conditions } => conditions
                .iter()
                .any(|c| c.matches_entity_death(entities, npc_id, entity_name)),
            _ => false,
        }
    }

    /// Check if trigger matches a phase being entered.
    pub fn matches_phase_entered(&self, phase_id: &str) -> bool {
        match self {
            Self::PhaseEntered {
                phase_id: trigger_phase,
            } => trigger_phase == phase_id,
            Self::AnyOf { conditions } => {
                conditions.iter().any(|c| c.matches_phase_entered(phase_id))
            }
            _ => false,
        }
    }

    /// Check if trigger matches a phase ending.
    pub fn matches_phase_ended(&self, phase_id: &str) -> bool {
        match self {
            Self::PhaseEnded {
                phase_id: trigger_phase,
            } => trigger_phase == phase_id,
            Self::AnyOf { conditions } => {
                conditions.iter().any(|c| c.matches_phase_ended(phase_id))
            }
            _ => false,
        }
    }

    /// Check if trigger matches a counter reaching a value.
    pub fn matches_counter_reaches(
        &self,
        counter_id: &str,
        old_value: u32,
        new_value: u32,
    ) -> bool {
        match self {
            Self::CounterReaches {
                counter_id: trigger_counter,
                value,
            } => trigger_counter == counter_id && old_value < *value && new_value >= *value,
            Self::AnyOf { conditions } => conditions
                .iter()
                .any(|c| c.matches_counter_reaches(counter_id, old_value, new_value)),
            _ => false,
        }
    }

    /// Check if trigger matches time elapsed crossing a threshold.
    pub fn matches_time_elapsed(&self, old_secs: f32, new_secs: f32) -> bool {
        match self {
            Self::TimeElapsed { secs } => old_secs < *secs && new_secs >= *secs,
            Self::AnyOf { conditions } => conditions
                .iter()
                .any(|c| c.matches_time_elapsed(old_secs, new_secs)),
            _ => false,
        }
    }

    /// Check if trigger matches a timer expiring.
    pub fn matches_timer_expires(&self, timer_id: &str) -> bool {
        match self {
            Self::TimerExpires {
                timer_id: trigger_id,
            } => trigger_id == timer_id,
            Self::AnyOf { conditions } => {
                conditions.iter().any(|c| c.matches_timer_expires(timer_id))
            }
            _ => false,
        }
    }

    /// Check if trigger matches target set (NPC targeting something).
    pub fn matches_target_set(&self, source_npc_id: i64, source_name: &str) -> bool {
        match self {
            Self::TargetSet { selector, .. } => {
                // Require explicit filter
                if selector.is_empty() {
                    return false;
                }
                selector.matches_npc_id(source_npc_id) || selector.matches_name_only(source_name)
            }
            Self::AnyOf { conditions } => conditions
                .iter()
                .any(|c| c.matches_target_set(source_npc_id, source_name)),
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
        let trigger = Trigger::TimerExpires {
            timer_id: "test".into(),
        };
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
            selector: vec![],
        };
        assert!(!trigger.valid_for_timer());
        assert!(trigger.valid_for_phase());
        assert!(!trigger.valid_for_counter());
    }

    #[test]
    fn contains_combat_start_nested() {
        let trigger = Trigger::AnyOf {
            conditions: vec![
                Trigger::AbilityCast {
                    abilities: vec![AbilitySelector::Id(123)],
                    source: EntityFilter::Any,
                },
                Trigger::CombatStart,
            ],
        };
        assert!(trigger.contains_combat_start());
    }

    #[test]
    fn serde_round_trip() {
        let trigger = Trigger::AbilityCast {
            abilities: vec![AbilitySelector::Id(123), AbilitySelector::Id(456)],
            source: EntityFilter::Selector(vec![EntitySelector::Id(789)]),
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
            source: EntityFilter::Any,
            target: EntityFilter::Any,
        };
        let toml = toml::to_string(&trigger).unwrap();
        let parsed: Trigger = toml::from_str(&toml).unwrap();
        assert_eq!(trigger, parsed);
    }
}
