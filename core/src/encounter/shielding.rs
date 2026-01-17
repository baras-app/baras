//! Simple FIFO shield attribution.
//!
//! The combat log's `dmg_absorbed` shows the TOTAL absorbed by all active shields.
//! We use simple FIFO attribution: credit all absorbed damage to whoever applied
//! the first (oldest) active shield.
//!
//! If no shields are active, we check for recently closed shields (500ms grace window)
//! to handle timing edge cases.

use super::CombatEncounter;
use crate::combat_log::CombatEvent;
use crate::game_data::get_shield_info;
use chrono::NaiveDateTime;

/// Grace period for attributing absorption to a recently closed shield
const RECENTLY_CLOSED_GRACE_MS: i64 = 500;

/// Shield context stored per damage event for attribution at query time.
/// Sorted by FIFO order (position 1 = first applied).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ShieldContext {
    /// Shield effect ID
    pub effect_id: i64,
    /// Source entity ID (who applied the shield)
    pub source_id: i64,
    /// FIFO position (1 = first applied, 2 = second, etc.)
    pub position: u8,
    /// Estimated max absorb capacity (0 if unknown/unlimited)
    pub estimated_max: i64,
}

/// Lightweight shield reference for attribution without borrowing issues
struct ActiveShield {
    source_id: i64,
}

impl CombatEncounter {
    /// Process a damage event that has absorption.
    /// Credits all absorbed damage to the first (oldest) active shield using FIFO.
    pub fn attribute_shield_absorption(&mut self, event: &CombatEvent) {
        let absorbed = event.details.dmg_absorbed as i64;
        if absorbed == 0 {
            return;
        }

        let target_id = event.target_entity.log_id;

        // Find the first active shield (FIFO - oldest applied)
        if let Some(first_shield) = self.get_first_active_shield(target_id, event.timestamp) {
            self.credit_shielding(first_shield.source_id, absorbed);
        } else {
            // No active shields - try recently closed (grace window)
            self.credit_recently_closed_shield(target_id, absorbed, event.timestamp);
        }
    }

    /// Get the first (oldest) active shield for a target
    fn get_first_active_shield(&self, target_id: i64, timestamp: NaiveDateTime) -> Option<ActiveShield> {
        let effects = self.effects.get(&target_id)?;

        effects
            .iter()
            .filter(|e| {
                e.is_shield
                    && e.applied_at < timestamp
                    && e.removed_at.map_or(true, |r| r >= timestamp)
            })
            .min_by_key(|e| e.applied_at)
            .map(|e| ActiveShield {
                source_id: e.source_id,
            })
    }

    /// Credit absorption to a recently closed shield within the grace window
    fn credit_recently_closed_shield(
        &mut self,
        target_id: i64,
        absorbed: i64,
        timestamp: NaiveDateTime,
    ) {
        let Some(effects) = self.effects.get(&target_id) else {
            return;
        };

        // Find a shield that was recently closed (within grace period)
        let recent_shield = effects
            .iter()
            .filter(|e| e.is_shield)
            .filter(|e| {
                e.removed_at
                    .map(|removed| {
                        let delta = timestamp.signed_duration_since(removed).num_milliseconds();
                        (0..=RECENTLY_CLOSED_GRACE_MS).contains(&delta)
                    })
                    .unwrap_or(false)
            })
            .max_by_key(|e| e.removed_at);

        if let Some(shield) = recent_shield {
            self.credit_shielding(shield.source_id, absorbed);
        }
    }

    /// Add absorption credit to an entity's shielding_given metric
    fn credit_shielding(&mut self, source_id: i64, amount: i64) {
        if amount <= 0 {
            return;
        }
        self.accumulated_data
            .entry(source_id)
            .or_default()
            .shielding_given += amount;
    }

    /// Get shield context for a target at a given timestamp.
    /// Returns active shields + recently closed shields (grace window) as ShieldContext vec.
    /// Used for parquet storage to enable simple attribution at query time.
    ///
    /// Grace window shields are ALWAYS included (not just when 0 active shields),
    /// because they may have contributed to absorption due to log timing.
    pub fn get_shield_context(&self, target_id: i64, timestamp: NaiveDateTime) -> Vec<ShieldContext> {
        let Some(effects) = self.effects.get(&target_id) else {
            return Vec::new();
        };

        // Collect active shields
        let mut shields: Vec<_> = effects
            .iter()
            .filter(|e| {
                e.is_shield
                    && e.applied_at < timestamp
                    && e.removed_at.map_or(true, |r| r >= timestamp)
            })
            .collect();

        // ALWAYS include recently closed shields (grace window)
        // These may have contributed to absorption due to log timing issues
        for effect in effects.iter() {
            if !effect.is_shield {
                continue;
            }
            let Some(removed) = effect.removed_at else {
                continue;
            };
            let delta = timestamp.signed_duration_since(removed).num_milliseconds();
            if (0..RECENTLY_CLOSED_GRACE_MS).contains(&delta) {
                // Don't add duplicates (shield might already be in active list at boundary)
                if !shields.iter().any(|s| {
                    s.effect_id == effect.effect_id
                        && s.source_id == effect.source_id
                        && s.applied_at == effect.applied_at
                }) {
                    shields.push(effect);
                }
            }
        }

        // Sort by application time (FIFO)
        shields.sort_by_key(|s| s.applied_at);

        // Convert to ShieldContext with position and estimated_max
        shields
            .into_iter()
            .enumerate()
            .map(|(idx, e)| {
                let estimated_max = get_shield_info(e.effect_id)
                    .and_then(|info| info.estimated_absorb())
                    .unwrap_or(0);
                ShieldContext {
                    effect_id: e.effect_id,
                    source_id: e.source_id,
                    position: (idx + 1) as u8,
                    estimated_max,
                }
            })
            .collect()
    }
}
