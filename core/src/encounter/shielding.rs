use super::CombatEncounter;
use super::EffectInstance;
use crate::combat_log::CombatEvent;
use chrono::NaiveDateTime;

/// Grace period when another shield is still active (tighter window)
const ABSORPTION_INSIDE_DELAY_MS: i64 = 500;
/// Grace period when no other shields active (more lenient for log lag)
const ABSORPTION_OUTSIDE_DELAY_MS: i64 = 3000;

/// A damage event with absorption waiting to be attributed to a shield
#[derive(Debug, Clone)]
pub struct PendingAbsorption {
    pub timestamp: NaiveDateTime,
    pub absorbed: i64,
    pub source_id: i64, // who to credit when resolved
}

/// Shield info extracted for attribution (avoids borrow conflicts)
struct ShieldInfo {
    source_id: i64,
    effect_id: i64,
    is_removed: bool,
}

impl CombatEncounter {
    /// Process a damage event that has absorption.
    /// - If exactly one shield is active: attribute immediately
    /// - If multiple shields active: queue for later resolution
    pub fn attribute_shield_absorption(&mut self, event: &CombatEvent) {
        let absorbed = event.details.dmg_absorbed as i64;
        if absorbed == 0 {
            return;
        }

        let target_id = event.target_entity.log_id;

        // Collect shield info to avoid borrow conflicts
        let (active_shields, recently_closed) = {
            let Some(effects) = self.effects.get(&target_id) else {
                return;
            };

            let active: Vec<ShieldInfo> = effects
                .iter()
                .filter(|e| {
                    e.is_shield
                        && !e.has_absorbed
                        && e.applied_at < event.timestamp
                        && is_shield_active_at(e, event.timestamp, false)
                })
                .map(|e| ShieldInfo {
                    source_id: e.source_id,
                    effect_id: e.effect_id,
                    is_removed: e.removed_at.is_some(),
                })
                .collect();

            let closed = if active.is_empty() {
                find_recently_closed_shield(effects, event.timestamp).map(|e| e.source_id)
            } else {
                None
            };

            (active, closed)
        };

        match active_shields.len() {
            0 => {
                // No active shields - try to find a recently closed one
                if let Some(source_id) = recently_closed {
                    let acc = self.accumulated_data.entry(source_id).or_default();
                    acc.shielding_given += absorbed;
                }
            }
            1 => {
                // Single shield: attribute immediately
                let shield = &active_shields[0];

                // Mark as consumed if the shield was removed (depleted) and damage got through
                // This prevents double-counting via find_recently_closed_shield later
                if shield.is_removed
                    && event.details.dmg_effective > 0
                    && let Some(effects) = self.effects.get_mut(&target_id)
                    && let Some(effect) = effects
                        .iter_mut()
                        .find(|e| e.is_shield && e.effect_id == shield.effect_id)
                {
                    effect.has_absorbed = true;
                }

                let acc = self.accumulated_data.entry(shield.source_id).or_default();
                acc.shielding_given += absorbed;
            }
            _ => {
                // Multiple shields: queue for resolution when one ends
                self.pending_absorptions
                    .entry(target_id)
                    .or_default()
                    .push(PendingAbsorption {
                        timestamp: event.timestamp,
                        absorbed,
                        source_id: 0, // will be set on resolution
                    });
            }
        }
    }

    /// Called when a shield effect is removed. Resolves any pending absorptions
    /// by attributing them to this shield (since it ended first).
    pub fn resolve_pending_absorptions(&mut self, target_id: i64, removed_shield: &EffectInstance) {
        let Some(pending) = self.pending_absorptions.get_mut(&target_id) else {
            return;
        };

        if pending.is_empty() {
            return;
        }

        let removal_time = removed_shield
            .removed_at
            .unwrap_or(removed_shield.applied_at);

        // Check if other shields are still active (affects grace period)
        let other_shields_active = self
            .effects
            .get(&target_id)
            .map(|effects| {
                effects.iter().any(|e| {
                    e.is_shield
                        && !e.has_absorbed
                        && e.effect_id != removed_shield.effect_id
                        && e.removed_at.is_none()
                })
            })
            .unwrap_or(false);

        let grace_ms = if other_shields_active {
            ABSORPTION_INSIDE_DELAY_MS
        } else {
            ABSORPTION_OUTSIDE_DELAY_MS
        };

        // Resolve pending absorptions that occurred before or shortly after removal
        let mut total_absorbed = 0i64;
        pending.retain(|p| {
            let delta_ms = removal_time
                .signed_duration_since(p.timestamp)
                .num_milliseconds();

            // Keep if: event happened AFTER removal + grace period (can't be this shield)
            // Resolve if: event happened BEFORE removal OR within grace period after
            if delta_ms >= -grace_ms {
                // This absorption belongs to the removed shield
                total_absorbed += p.absorbed;
                false // remove from pending
            } else {
                true // keep in pending
            }
        });

        if total_absorbed > 0 {
            let acc = self
                .accumulated_data
                .entry(removed_shield.source_id)
                .or_default();
            acc.shielding_given += total_absorbed;
        }

        // Clean up empty entries
        if pending.is_empty() {
            self.pending_absorptions.remove(&target_id);
        }
    }

    /// Flush any remaining pending absorptions at end of combat.
    /// Uses the outside delay window and attributes to most recent shield.
    pub fn flush_pending_absorptions(&mut self) {
        let pending_targets: Vec<i64> = self.pending_absorptions.keys().copied().collect();

        for target_id in pending_targets {
            let Some(pending) = self.pending_absorptions.remove(&target_id) else {
                continue;
            };

            // Find the most recently removed shield for this target
            let last_shield = self.effects.get(&target_id).and_then(|effects| {
                effects
                    .iter()
                    .filter(|e| e.is_shield && e.removed_at.is_some())
                    .max_by_key(|e| e.removed_at)
            });

            if let Some(shield) = last_shield {
                let total: i64 = pending.iter().map(|p| p.absorbed).sum();
                if total > 0 {
                    let acc = self.accumulated_data.entry(shield.source_id).or_default();
                    acc.shielding_given += total;
                }
            }
        }
    }
}

/// Checks if a shield is active at the given timestamp.
fn is_shield_active_at(
    effect: &EffectInstance,
    timestamp: NaiveDateTime,
    use_outside_window: bool,
) -> bool {
    match effect.removed_at {
        None => true,
        Some(removed) => {
            if removed >= timestamp {
                return true;
            }
            // Check grace period
            let grace_ms = if use_outside_window {
                ABSORPTION_OUTSIDE_DELAY_MS
            } else {
                ABSORPTION_INSIDE_DELAY_MS
            };
            let delta = removed
                .signed_duration_since(timestamp)
                .num_milliseconds()
                .abs();
            delta <= grace_ms
        }
    }
}

/// Find a shield that was recently closed (within outside delay window)
fn find_recently_closed_shield(
    effects: &[EffectInstance],
    timestamp: NaiveDateTime,
) -> Option<&EffectInstance> {
    effects
        .iter()
        .filter(|e| e.is_shield && !e.has_absorbed && e.removed_at.is_some())
        .filter(|e| {
            let removed = e.removed_at.unwrap();
            let delta = timestamp.signed_duration_since(removed).num_milliseconds();
            (0..=ABSORPTION_INSIDE_DELAY_MS).contains(&delta)
        })
        .max_by_key(|e| e.removed_at)
}
