//! Signal matching for `EffectModifier` triggers.
//!
//! Counts how many signals in a batch satisfy a modifier trigger for a given
//! effect holder. `AnyOf` recurses into its children and sums their hits so a
//! single modifier (and therefore a single ICD slot) can react to several
//! event kinds.

use baras_types::{ChargeDirection, Trigger};
use chrono::NaiveDateTime;

use crate::signal_processor::GameSignal;

/// `(effect_id, target_id) → (old_charges, new_charges)` snapshots for this batch.
pub(super) type ChargeSnapshot = ((i64, i64), u8, u8);

/// A signal that satisfied a modifier trigger.
pub(super) struct Hit {
    /// Log timestamp of the matching signal — procs and ICDs are measured in
    /// log time, never the wall-clock-interpolated tracker anchor.
    pub at: NaiveDateTime,
    /// Duration-adjust scale (1.0 unless `ResourceSpent` with `per_amount > 0`).
    pub scale: f32,
}

/// Push one `Hit` per matching signal onto `out`. `AnyOf` recurses and
/// appends its children's hits.
pub(super) fn collect_hits(
    trigger: &Trigger,
    requires_crit: bool,
    eid: i64,
    signals: &[GameSignal],
    charge_snapshots: &[ChargeSnapshot],
    out: &mut Vec<Hit>,
) {
    match trigger {
        Trigger::AnyOf { conditions } => {
            for c in conditions {
                collect_hits(c, requires_crit, eid, signals, charge_snapshots, out);
            }
        }
        // Handled on the ModifyCharges path, not here.
        Trigger::SelfChargesChanged { .. } => {}
        Trigger::ResourceSpent { per_amount } => {
            for s in signals {
                if let GameSignal::ResourceSpent {
                    source_id,
                    amount,
                    timestamp,
                    ..
                } = s
                    && *source_id == eid
                {
                    out.push(Hit {
                        at: *timestamp,
                        scale: if *per_amount > 0.0 { amount / per_amount } else { 1.0 },
                    });
                }
            }
        }
        Trigger::KillingBlow => {
            out.extend(
                signals
                    .iter()
                    .filter(|s| matches!(s, GameSignal::EntityDeath { killer_id, .. } if *killer_id == eid))
                    .map(|s| Hit {
                        at: s.timestamp(),
                        scale: 1.0,
                    }),
            );
        }
        Trigger::AbilityCast { abilities, .. } => {
            if requires_crit {
                return;
            }
            out.extend(
                signals
                    .iter()
                    .filter(|s| {
                        if let GameSignal::AbilityActivated {
                            ability_id,
                            ability_name,
                            source_id,
                            ..
                        } = s
                        {
                            if *source_id != eid {
                                return false;
                            }
                            let name = crate::context::resolve(*ability_name);
                            abilities.is_empty() || abilities.iter().any(|a| a.matches(*ability_id as u64, Some(name)))
                        } else {
                            false
                        }
                    })
                    .map(|s| Hit {
                        at: s.timestamp(),
                        scale: 1.0,
                    }),
            );
        }
        Trigger::DamageTaken {
            abilities, mitigation, ..
        }
        | Trigger::DamageDealt {
            abilities, mitigation, ..
        } => {
            let dealt = matches!(trigger, Trigger::DamageDealt { .. });
            out.extend(
                signals
                    .iter()
                    .filter(|s| {
                        if let GameSignal::DamageTaken {
                            ability_id,
                            ability_name,
                            defense_type_id,
                            is_crit,
                            source_id,
                            target_id,
                            ..
                        } = s
                        {
                            let holder = if dealt { *source_id } else { *target_id };
                            if holder != eid {
                                return false;
                            }
                            let name = crate::context::resolve(*ability_name);
                            let ability_ok = abilities.is_empty()
                                || abilities.iter().any(|a| a.matches(*ability_id as u64, Some(name)));
                            let mitigation_ok = mitigation.is_empty()
                                || mitigation.iter().any(|m| m.defense_type_id() == *defense_type_id);
                            ability_ok && mitigation_ok && (!requires_crit || *is_crit)
                        } else {
                            false
                        }
                    })
                    .map(|s| Hit {
                        at: s.timestamp(),
                        scale: 1.0,
                    }),
            );
        }
        Trigger::HealingTaken { abilities, .. } | Trigger::HealingDealt { abilities, .. } => {
            let dealt = matches!(trigger, Trigger::HealingDealt { .. });
            out.extend(
                signals
                    .iter()
                    .filter(|s| {
                        if let GameSignal::HealingDone {
                            ability_id,
                            ability_name,
                            source_id,
                            target_id,
                            is_crit,
                            ..
                        } = s
                        {
                            let holder = if dealt { *source_id } else { *target_id };
                            if holder != eid {
                                return false;
                            }
                            let name = crate::context::resolve(*ability_name);
                            let ability_ok = abilities.is_empty()
                                || abilities.iter().any(|a| a.matches(*ability_id as u64, Some(name)));
                            ability_ok && (!requires_crit || *is_crit)
                        } else {
                            false
                        }
                    })
                    .map(|s| Hit {
                        at: s.timestamp(),
                        scale: 1.0,
                    }),
            );
        }
        Trigger::EffectApplied { effects, .. } => {
            out.extend(
                signals
                    .iter()
                    .filter(|s| {
                        if let GameSignal::EffectApplied {
                            effect_id,
                            effect_name,
                            target_id,
                            ..
                        } = s
                        {
                            *target_id == eid
                                && !effects.is_empty()
                                && effects
                                    .iter()
                                    .any(|e| e.matches(*effect_id as u64, Some(crate::context::resolve(*effect_name))))
                        } else {
                            false
                        }
                    })
                    .map(|s| Hit {
                        at: s.timestamp(),
                        scale: 1.0,
                    }),
            );
        }
        Trigger::EffectRemoved { effects, .. } => {
            out.extend(
                signals
                    .iter()
                    .filter(|s| {
                        if let GameSignal::EffectRemoved {
                            effect_id,
                            effect_name,
                            target_id,
                            ..
                        } = s
                        {
                            *target_id == eid
                                && !effects.is_empty()
                                && effects
                                    .iter()
                                    .any(|e| e.matches(*effect_id as u64, Some(crate::context::resolve(*effect_name))))
                        } else {
                            false
                        }
                    })
                    .map(|s| Hit {
                        at: s.timestamp(),
                        scale: 1.0,
                    }),
            );
        }
        Trigger::ChargesChanged { effects, direction } => {
            out.extend(
                signals
                    .iter()
                    .filter(|s| {
                        if let GameSignal::EffectChargesChanged {
                            effect_id,
                            effect_name,
                            target_id,
                            ..
                        } = s
                        {
                            if *target_id != eid {
                                return false;
                            }
                            let name = crate::context::resolve(*effect_name);
                            let effect_ok =
                                effects.is_empty() || effects.iter().any(|e| e.matches(*effect_id as u64, Some(name)));
                            let dir_ok = match direction {
                                Some(dir) => {
                                    let snap_key = (*effect_id, *target_id);
                                    charge_snapshots
                                        .iter()
                                        .any(|(k, old, new)| *k == snap_key && direction_matches(*dir, *old, *new))
                                }
                                None => true,
                            };
                            effect_ok && dir_ok
                        } else {
                            false
                        }
                    })
                    .map(|s| Hit {
                        at: s.timestamp(),
                        scale: 1.0,
                    }),
            );
        }
        _ => {}
    }
}

/// Does this trigger fire when the holder's own stacks move `old → new`?
/// Recurses through `AnyOf`; non-self triggers never match here.
pub(super) fn matches_self_charges(trigger: &Trigger, old: u8, new: u8) -> bool {
    match trigger {
        Trigger::SelfChargesChanged { direction } => direction.is_none_or(|d| direction_matches(d, old, new)),
        Trigger::AnyOf { conditions } => conditions.iter().any(|c| matches_self_charges(c, old, new)),
        _ => false,
    }
}

fn direction_matches(dir: ChargeDirection, old: u8, new: u8) -> bool {
    match dir {
        ChargeDirection::Increased => new > old,
        ChargeDirection::Decreased => new < old,
        ChargeDirection::Neutral => new == old,
    }
}
