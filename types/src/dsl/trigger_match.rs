//! Matching and inspection methods on [`Trigger`] that need no runtime types.

use std::collections::HashSet;

use super::*;

impl Trigger {
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
            | Self::DamageTaken { source, .. }
            | Self::DamageDealt { source, .. }
            | Self::HealingTaken { source, .. }
            | Self::ThreatModified { source, .. } => Some(source),
            _ => None,
        }
    }

    /// Get the target filter from this trigger (for event-based triggers).
    /// Returns `None` for triggers that don't have a target filter (treated as "any").
    pub fn target_filter(&self) -> Option<&EntityFilter> {
        match self {
            Self::AbilityCast { target, .. }
            | Self::EffectApplied { target, .. }
            | Self::EffectRemoved { target, .. }
            | Self::DamageTaken { target, .. }
            | Self::DamageDealt { target, .. }
            | Self::HealingTaken { target, .. }
            | Self::ThreatModified { target, .. }
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
            Self::AbilityCast {
                abilities, position, ..
            } => Self::AbilityCast {
                abilities,
                source,
                target,
                position,
            },
            Self::EffectApplied { effects, position, .. } => Self::EffectApplied {
                effects,
                source,
                target,
                position,
            },
            Self::EffectRemoved { effects, position, .. } => Self::EffectRemoved {
                effects,
                source,
                target,
                position,
            },
            Self::DamageTaken {
                abilities,
                mitigation,
                position,
                ..
            } => Self::DamageTaken {
                abilities,
                source,
                target,
                mitigation,
                position,
            },
            Self::DamageDealt {
                abilities,
                mitigation,
                position,
                ..
            } => Self::DamageDealt {
                abilities,
                source,
                target,
                mitigation,
                position,
            },
            Self::HealingTaken {
                abilities, position, ..
            } => Self::HealingTaken {
                abilities,
                source,
                target,
                position,
            },
            Self::ThreatModified {
                abilities,
                threat_value,
                ..
            } => Self::ThreatModified {
                abilities,
                source,
                target,
                threat_value,
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
                !abilities.is_empty() && abilities.iter().any(|s| s.matches(ability_id, ability_name))
            }
            Self::AnyOf { conditions } => conditions.iter().any(|c| c.matches_ability(ability_id, ability_name)),
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
    ///
    /// `defense_type_id` is 0 for a normal hit. When the trigger has a non-empty
    /// `mitigation` list, the event only matches if its defense type is in that list.
    pub fn matches_damage_taken(&self, ability_id: u64, ability_name: Option<&str>, defense_type_id: i64) -> bool {
        match self {
            Self::DamageTaken {
                abilities, mitigation, ..
            }
            | Self::DamageDealt {
                abilities, mitigation, ..
            } => {
                // Empty abilities = any ability; otherwise must match one selector
                if !abilities.is_empty() && !abilities.iter().any(|s| s.matches(ability_id, ability_name)) {
                    return false;
                }
                // Empty mitigation list = any hit result
                mitigation.is_empty() || mitigation.iter().any(|m| m.defense_type_id() == defense_type_id)
            }
            Self::AnyOf { conditions } => conditions
                .iter()
                .any(|c| c.matches_damage_taken(ability_id, ability_name, defense_type_id)),
            _ => false,
        }
    }

    /// Check if trigger matches healing taken from an ability.
    pub fn matches_healing_taken(&self, ability_id: u64, ability_name: Option<&str>) -> bool {
        match self {
            Self::HealingTaken { abilities, .. } => {
                !abilities.is_empty() && abilities.iter().any(|s| s.matches(ability_id, ability_name))
            }
            Self::AnyOf { conditions } => conditions
                .iter()
                .any(|c| c.matches_healing_taken(ability_id, ability_name)),
            _ => false,
        }
    }

    /// Check if trigger matches a threat modification event.
    /// Empty `abilities` matches any ability. `threat_value` filters by exact value when set.
    pub fn matches_threat_modified(&self, ability_id: u64, ability_name: Option<&str>, value: f32) -> bool {
        match self {
            Self::ThreatModified {
                abilities,
                threat_value,
                ..
            } => {
                let ability_ok = abilities.is_empty() || abilities.iter().any(|s| s.matches(ability_id, ability_name));
                let value_ok = threat_value.is_none_or(|v| (v - value).abs() < 0.5);
                ability_ok && value_ok
            }
            Self::AnyOf { conditions } => conditions
                .iter()
                .any(|c| c.matches_threat_modified(ability_id, ability_name, value)),
            _ => false,
        }
    }

    /// Check if trigger is or contains an `AnyPhaseChange` variant.
    /// Used by the timer system for cancel trigger matching on phase changes.
    pub fn is_any_phase_change(&self) -> bool {
        match self {
            Self::AnyPhaseChange => true,
            Self::AnyOf { conditions } => conditions.iter().any(|c| c.is_any_phase_change()),
            _ => false,
        }
    }

    /// Check if trigger matches a phase being entered.
    pub fn matches_phase_entered(&self, phase_id: &str) -> bool {
        match self {
            Self::PhaseEntered {
                phase_id: trigger_phase,
            } => trigger_phase == phase_id,
            Self::AnyOf { conditions } => conditions.iter().any(|c| c.matches_phase_entered(phase_id)),
            _ => false,
        }
    }

    /// Check if trigger matches a phase ending.
    pub fn matches_phase_ended(&self, phase_id: &str) -> bool {
        match self {
            Self::PhaseEnded {
                phase_id: trigger_phase,
            } => trigger_phase == phase_id,
            Self::AnyOf { conditions } => conditions.iter().any(|c| c.matches_phase_ended(phase_id)),
            _ => false,
        }
    }

    /// Check if trigger matches a counter reaching a specific value.
    ///
    /// Fires when `new_value` equals the target value and `old_value` was
    /// different — i.e., the counter just arrived at that value, regardless
    /// of whether it got there via increment or decrement.
    pub fn matches_counter_reaches(&self, counter_id: &str, old_value: u32, new_value: u32) -> bool {
        match self {
            Self::CounterReaches {
                counter_id: trigger_counter,
                value,
            } => trigger_counter == counter_id && new_value == *value && old_value != *value,
            Self::AnyOf { conditions } => conditions
                .iter()
                .any(|c| c.matches_counter_reaches(counter_id, old_value, new_value)),
            _ => false,
        }
    }

    /// Check if trigger matches a counter value changing (any change).
    pub fn matches_counter_changes(&self, counter_id: &str) -> bool {
        match self {
            Self::CounterChanges {
                counter_id: trigger_counter,
            } => trigger_counter == counter_id,
            Self::AnyOf { conditions } => conditions.iter().any(|c| c.matches_counter_changes(counter_id)),
            _ => false,
        }
    }

    /// Check if trigger matches another effect's charges changing.
    pub fn matches_charges_changed(
        &self,
        effect_id: u64,
        effect_name: Option<&str>,
        old_charges: u8,
        new_charges: u8,
    ) -> bool {
        match self {
            Self::ChargesChanged { effects, direction } => {
                if !effects.is_empty() && !effects.iter().any(|s| s.matches(effect_id, effect_name)) {
                    return false;
                }
                match direction {
                    Some(ChargeDirection::Increased) => new_charges > old_charges,
                    Some(ChargeDirection::Decreased) => new_charges < old_charges,
                    Some(ChargeDirection::Neutral) => new_charges == old_charges,
                    None => true,
                }
            }
            Self::AnyOf { conditions } => conditions
                .iter()
                .any(|c| c.matches_charges_changed(effect_id, effect_name, old_charges, new_charges)),
            _ => false,
        }
    }

    /// Check if trigger matches this effect's own charges changing.
    pub fn matches_self_charges_changed(&self, old_charges: u8, new_charges: u8) -> bool {
        match self {
            Self::SelfChargesChanged { direction } => match direction {
                Some(ChargeDirection::Increased) => new_charges > old_charges,
                Some(ChargeDirection::Decreased) => new_charges < old_charges,
                Some(ChargeDirection::Neutral) => new_charges == old_charges,
                None => true,
            },
            Self::AnyOf { conditions } => conditions
                .iter()
                .any(|c| c.matches_self_charges_changed(old_charges, new_charges)),
            _ => false,
        }
    }

    /// Returns the `secs` threshold if this is a `TimeElapsed` trigger.
    pub fn time_elapsed_secs(&self) -> Option<f32> {
        match self {
            Self::TimeElapsed { secs } => Some(*secs),
            _ => None,
        }
    }

    /// Check if the TimeElapsed condition is met: combat time >= threshold.
    /// Unlike `matches_time_elapsed`, this is a simple state check, not a
    /// threshold-crossing detector. Safe to call repeatedly — callers handle dedup.
    pub fn is_time_elapsed_met(&self, combat_secs: f32) -> bool {
        match self {
            Self::TimeElapsed { secs } => combat_secs >= *secs,
            Self::AnyOf { conditions } => conditions.iter().any(|c| c.is_time_elapsed_met(combat_secs)),
            _ => false,
        }
    }

    /// Check if this trigger's combat-time condition is met.
    /// Treats CombatStart as TimeElapsed { secs: 0 } — both are just
    /// "fire when combat has been running >= threshold seconds."
    /// Safe to call repeatedly; callers handle deduplication.
    pub fn is_combat_time_met(&self, combat_secs: f32) -> bool {
        match self {
            Self::CombatStart => combat_secs >= 0.0,
            Self::TimeElapsed { secs } => combat_secs >= *secs,
            Self::AnyOf { conditions } => conditions.iter().any(|c| c.is_combat_time_met(combat_secs)),
            _ => false,
        }
    }

    /// Returns the combat-time threshold in seconds for this trigger.
    /// CombatStart = 0.0, TimeElapsed = configured secs.
    /// Returns None for non-combat-time triggers.
    pub fn combat_time_threshold(&self) -> Option<f32> {
        match self {
            Self::CombatStart => Some(0.0),
            Self::TimeElapsed { secs } => Some(*secs),
            _ => None,
        }
    }

    /// Check if trigger matches time elapsed crossing a threshold.
    /// Used by the phase system where one-shot threshold detection is needed.
    pub fn matches_time_elapsed(&self, old_secs: f32, new_secs: f32) -> bool {
        match self {
            Self::TimeElapsed { secs } => old_secs < *secs && new_secs >= *secs,
            Self::AnyOf { conditions } => conditions.iter().any(|c| c.matches_time_elapsed(old_secs, new_secs)),
            _ => false,
        }
    }

    /// Check if trigger matches a timer expiring.
    pub fn matches_timer_expires(&self, timer_id: &str) -> bool {
        match self {
            Self::TimerExpires { timer_id: trigger_id } => trigger_id == timer_id,
            Self::AnyOf { conditions } => conditions.iter().any(|c| c.matches_timer_expires(timer_id)),
            _ => false,
        }
    }

    /// Check if trigger matches a timer starting.
    pub fn matches_timer_started(&self, timer_id: &str) -> bool {
        match self {
            Self::TimerStarted { timer_id: trigger_id } => trigger_id == timer_id,
            Self::AnyOf { conditions } => conditions.iter().any(|c| c.matches_timer_started(timer_id)),
            _ => false,
        }
    }

    /// Check if trigger matches a timer being canceled.
    pub fn matches_timer_canceled(&self, timer_id: &str) -> bool {
        match self {
            Self::TimerCanceled { timer_id: trigger_id } => trigger_id == timer_id,
            Self::AnyOf { conditions } => conditions.iter().any(|c| c.matches_timer_canceled(timer_id)),
            _ => false,
        }
    }

    /// Check if this trigger contains variants that are unsupported for shields.
    ///
    /// Shield triggers are evaluated against the post-emission `GameSignal` vec
    /// (including synthesized `TimerStarted`/`TimerExpired`/`TimerCanceled` from
    /// the timer feedback loop). `CombatStart` and `TimeElapsed` are time-based
    /// and never produce a signal, so they will silently never fire.
    pub fn contains_unsupported_for_shields(&self) -> Option<&'static str> {
        match self {
            Self::CombatStart => Some("combat_start"),
            Self::TimeElapsed { .. } => Some("time_elapsed"),
            Self::ChargesChanged { .. } => Some("charges_changed"),
            Self::SelfChargesChanged { .. } => Some("self_charges_changed"),
            Self::ResourceSpent { .. } => Some("resource_spent"),
            Self::KillingBlow => Some("killing_blow"),
            Self::HealingDealt { .. } => Some("healing_dealt"),
            Self::AnyOf { conditions } => conditions.iter().find_map(|c| c.contains_unsupported_for_shields()),
            _ => None,
        }
    }

    /// Check if this trigger contains variants that are unsupported for counters/phases.
    ///
    /// `TargetSet`, `TimeElapsed`, and charge triggers only work in the timer/modifier
    /// systems. Using them on counters or phases will silently never fire.
    pub fn contains_unsupported_for_counters_phases(&self) -> Option<&'static str> {
        match self {
            Self::TargetSet { .. } => Some("target_set"),
            Self::TimeElapsed { .. } => Some("time_elapsed"),
            Self::ChargesChanged { .. } => Some("charges_changed"),
            Self::SelfChargesChanged { .. } => Some("self_charges_changed"),
            Self::ResourceSpent { .. } => Some("resource_spent"),
            Self::KillingBlow => Some("killing_blow"),
            Self::HealingDealt { .. } => Some("healing_dealt"),
            Self::AnyOf { conditions } => conditions
                .iter()
                .find_map(|c| c.contains_unsupported_for_counters_phases()),
            _ => None,
        }
    }

    /// Check if this trigger contains variants that are unsupported for victory triggers.
    pub fn contains_unsupported_for_victory(&self) -> Option<&'static str> {
        match self {
            Self::TimeElapsed { .. } => Some("time_elapsed"),
            Self::TimerExpires { .. } => Some("timer_expires"),
            Self::TimerStarted { .. } => Some("timer_started"),
            Self::TimerCanceled { .. } => Some("timer_canceled"),
            Self::ChargesChanged { .. } => Some("charges_changed"),
            Self::SelfChargesChanged { .. } => Some("self_charges_changed"),
            Self::ResourceSpent { .. } => Some("resource_spent"),
            Self::KillingBlow => Some("killing_blow"),
            Self::HealingDealt { .. } => Some("healing_dealt"),
            Self::AnyOf { conditions } => conditions.iter().find_map(|c| c.contains_unsupported_for_victory()),
            _ => None,
        }
    }

    /// Collect all timer IDs referenced by this trigger (recursively for AnyOf).
    /// Returns TimerExpires, TimerStarted, and TimerCanceled timer_id values.
    pub fn collect_timer_refs(&self, out: &mut HashSet<String>) {
        match self {
            Self::TimerExpires { timer_id } | Self::TimerStarted { timer_id } | Self::TimerCanceled { timer_id } => {
                out.insert(timer_id.clone());
            }
            Self::AnyOf { conditions } => {
                for c in conditions {
                    c.collect_timer_refs(out);
                }
            }
            _ => {}
        }
    }
}
