//! Unified trigger system for timers, phases, and counters.
//!
//! This module provides a single `Trigger` enum that replaces the previously
//! separate `TimerTrigger`, `PhaseTrigger`, and `CounterTrigger` types.
//! Each system only responds to the trigger variants it supports.

mod matchers;
mod position;

pub use matchers::{AbilitySelector, EffectSelector, EntitySelector, EntitySelectorExt};
pub use position::{PositionAxis, PositionConstraint, PositionEntity, PositionOp, matches_position_constraints};

// Re-export EntityFilter for use in triggers
pub use baras_types::{ChargeDirection, EntityFilter, MitigationType};

use crate::dsl::EntityDefinition;

// ═══════════════════════════════════════════════════════════════════════════
// Trigger Kind (discriminant-only enum for indexing)
// ═══════════════════════════════════════════════════════════════════════════

/// Lightweight discriminant of a [`Trigger`] variant, used as an index key.
///
/// Each variant maps 1:1 to a `Trigger` variant (except `AnyOf`, which is
/// expanded into its constituent kinds). `CombatTime` covers both
/// `CombatStart` and `TimeElapsed` since they are evaluated together.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TriggerKind {
    CombatTime, // CombatStart + TimeElapsed
    AbilityCast,
    EffectApplied,
    EffectRemoved,
    DamageTaken,
    DamageDealt,
    HealingTaken,
    ThreatModified,
    BossHpBelow,
    BossHpAbove,
    NpcAppears,
    EntityDeath,
    TargetSet,
    PhaseEntered,
    PhaseEnded,
    AnyPhaseChange,
    CounterReaches,
    CounterChanges,
    ChargesChanged,
    SelfChargesChanged,
    ResourceSpent,
    KillingBlow,
    TimerExpires,
    TimerStarted,
    TimerCanceled,
}

// ═══════════════════════════════════════════════════════════════════════════
// Unified Trigger Enum
// ═══════════════════════════════════════════════════════════════════════════

pub use baras_types::Trigger;

/// Trigger matching that needs core types (`TriggerKind`, the entity roster).
/// Everything else is an inherent method on `baras_types::Trigger`.
pub trait TriggerExt {
    /// Collect all [`TriggerKind`]s this trigger can match.
    ///
    /// For simple variants this is a single kind. For `AnyOf`, each nested
    /// condition's kinds are collected recursively.
    fn collect_kinds(&self, out: &mut Vec<TriggerKind>);

    /// Check if trigger matches boss HP crossing below a threshold.
    /// The entity whose HP changed must match the selector.
    fn matches_boss_hp_below(
        &self,
        entities: &[EntityDefinition],
        npc_id: i64,
        entity_name: &str,
        old_hp: f32,
        new_hp: f32,
    ) -> bool;

    /// Check if trigger matches boss HP crossing above a threshold.
    /// Used for heal-check mechanics.
    fn matches_boss_hp_above(
        &self,
        entities: &[EntityDefinition],
        npc_id: i64,
        entity_name: &str,
        old_hp: f32,
        new_hp: f32,
    ) -> bool;

    /// Check if trigger matches NPC first appearing.
    fn matches_npc_appears(&self, entities: &[EntityDefinition], npc_id: i64, entity_name: &str) -> bool;

    /// Check if trigger matches entity death.
    fn matches_entity_death(&self, entities: &[EntityDefinition], npc_id: i64, entity_name: &str) -> bool;

    /// Check if trigger matches target set (NPC targeting something).
    fn matches_target_set(&self, entities: &[EntityDefinition], source_npc_id: i64, source_name: Option<&str>) -> bool;
}

impl TriggerExt for Trigger {
    fn collect_kinds(&self, out: &mut Vec<TriggerKind>) {
        match self {
            Self::CombatStart | Self::TimeElapsed { .. } => out.push(TriggerKind::CombatTime),
            // Not indexed; `HealingDealt` only exists for effect modifiers
            Self::CombatEnd | Self::Manual | Self::Never | Self::HealingDealt { .. } => {}
            Self::AbilityCast { .. } => out.push(TriggerKind::AbilityCast),
            Self::EffectApplied { .. } => out.push(TriggerKind::EffectApplied),
            Self::EffectRemoved { .. } => out.push(TriggerKind::EffectRemoved),
            Self::DamageTaken { .. } => out.push(TriggerKind::DamageTaken),
            Self::DamageDealt { .. } => out.push(TriggerKind::DamageDealt),
            Self::HealingTaken { .. } => out.push(TriggerKind::HealingTaken),
            Self::ThreatModified { .. } => out.push(TriggerKind::ThreatModified),
            Self::BossHpBelow { .. } => out.push(TriggerKind::BossHpBelow),
            Self::BossHpAbove { .. } => out.push(TriggerKind::BossHpAbove),
            Self::NpcAppears { .. } => out.push(TriggerKind::NpcAppears),
            Self::EntityDeath { .. } => out.push(TriggerKind::EntityDeath),
            Self::TargetSet { .. } => out.push(TriggerKind::TargetSet),
            Self::PhaseEntered { .. } => out.push(TriggerKind::PhaseEntered),
            Self::PhaseEnded { .. } => out.push(TriggerKind::PhaseEnded),
            Self::AnyPhaseChange => out.push(TriggerKind::AnyPhaseChange),
            Self::CounterReaches { .. } => out.push(TriggerKind::CounterReaches),
            Self::CounterChanges { .. } => out.push(TriggerKind::CounterChanges),
            Self::ChargesChanged { .. } => out.push(TriggerKind::ChargesChanged),
            Self::SelfChargesChanged { .. } => out.push(TriggerKind::SelfChargesChanged),
            Self::ResourceSpent { .. } => out.push(TriggerKind::ResourceSpent),
            Self::KillingBlow => out.push(TriggerKind::KillingBlow),
            Self::TimerExpires { .. } => out.push(TriggerKind::TimerExpires),
            Self::TimerStarted { .. } => out.push(TriggerKind::TimerStarted),
            Self::TimerCanceled { .. } => out.push(TriggerKind::TimerCanceled),
            Self::AnyOf { conditions } => {
                for condition in conditions {
                    condition.collect_kinds(out);
                }
            }
        }
    }

    fn matches_boss_hp_below(
        &self,
        entities: &[EntityDefinition],
        npc_id: i64,
        entity_name: &str,
        old_hp: f32,
        new_hp: f32,
    ) -> bool {
        match self {
            Self::BossHpBelow { hp_percent, selector } => {
                // Check HP threshold crossing
                let crossed = old_hp > *hp_percent && new_hp <= *hp_percent + 0.01;
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

    fn matches_boss_hp_above(
        &self,
        entities: &[EntityDefinition],
        npc_id: i64,
        entity_name: &str,
        old_hp: f32,
        new_hp: f32,
    ) -> bool {
        match self {
            Self::BossHpAbove { hp_percent, selector } => {
                // Check HP threshold crossing
                let crossed = old_hp < *hp_percent && new_hp >= *hp_percent - 0.01;
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

    fn matches_npc_appears(&self, entities: &[EntityDefinition], npc_id: i64, entity_name: &str) -> bool {
        match self {
            Self::NpcAppears { selector, .. } => {
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

    fn matches_entity_death(&self, entities: &[EntityDefinition], npc_id: i64, entity_name: &str) -> bool {
        match self {
            Self::EntityDeath { selector, .. } => {
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

    fn matches_target_set(&self, entities: &[EntityDefinition], source_npc_id: i64, source_name: Option<&str>) -> bool {
        match self {
            Self::TargetSet { selector, .. } => {
                // Require explicit filter
                if selector.is_empty() {
                    return false;
                }
                selector.matches_with_roster(entities, source_npc_id, source_name)
            }
            Self::AnyOf { conditions } => conditions
                .iter()
                .any(|c| c.matches_target_set(entities, source_npc_id, source_name)),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contains_combat_start_nested() {
        let trigger = Trigger::AnyOf {
            conditions: vec![
                Trigger::AbilityCast {
                    abilities: vec![AbilitySelector::Id(123)],
                    source: EntityFilter::Any,
                    target: EntityFilter::Any,
                    position: vec![],
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
            target: EntityFilter::Any,
            position: vec![],
        };
        let toml = toml::to_string(&trigger).unwrap();
        let parsed: Trigger = toml::from_str(&toml).unwrap();
        assert_eq!(trigger, parsed);
    }

    #[test]
    fn serde_mixed_selectors() {
        let trigger = Trigger::EffectApplied {
            effects: vec![EffectSelector::Id(100), EffectSelector::Name("Burn".to_string())],
            source: EntityFilter::Any,
            target: EntityFilter::Any,
            position: vec![],
        };
        let toml = toml::to_string(&trigger).unwrap();
        let parsed: Trigger = toml::from_str(&toml).unwrap();
        assert_eq!(trigger, parsed);
    }
}
