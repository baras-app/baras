//! Simplified shield attribution using Parsely-style logic.
//!
//! SWTOR's combat log only shows what the LAST shield absorbed, not the total.
//! When multiple shields are active and damage gets through, earlier shields
//! absorbed their full capacity but this isn't shown in the log.
//!
//! ## Algorithm
//!
//! - **No active shields**: Check recently closed shields (500ms grace window)
//! - **Single shield**: Credit it fully with dmg_absorbed
//! - **Multiple shields + full absorb**: Credit FIRST applied (FIFO)
//! - **Multiple shields + damage through**:
//!   - Credit earlier shields with ESTIMATED absorb (from lookup table)
//!   - Credit LAST shield with ACTUAL dmg_absorbed (what the log shows)

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
    effect_id: i64,
    applied_at: NaiveDateTime,
}

impl CombatEncounter {
    /// Process a damage event that has absorption.
    /// Attributes the absorbed damage to the appropriate shield source(s).
    pub fn attribute_shield_absorption(&mut self, event: &CombatEvent) {
        let absorbed = event.details.dmg_absorbed as i64;
        if absorbed == 0 {
            return;
        }

        let target_id = event.target_entity.log_id;
        let damage_got_through = event.details.dmg_effective > 0;

        // Collect active shields to avoid borrow conflicts
        let active_shields = self.get_active_shields(target_id, event.timestamp);

        match (active_shields.len(), damage_got_through) {
            (0, _) => {
                // No active shields - try recently closed
                self.credit_recently_closed_shield(target_id, absorbed, event.timestamp);
            }
            (1, _) => {
                // Single shield: credit it fully
                self.credit_shielding(active_shields[0].source_id, absorbed);
            }
            (_, false) => {
                // Multiple shields, full absorb: credit first applied (FIFO)
                self.credit_shielding(active_shields[0].source_id, absorbed);
            }
            (_, true) => {
                // Multiple shields, damage got through - attribute based on shield types
                self.attribute_multi_shield_break(&active_shields, absorbed);
            }
        }
    }

    /// Get active shields for a target, sorted by application time (FIFO)
    fn get_active_shields(&self, target_id: i64, timestamp: NaiveDateTime) -> Vec<ActiveShield> {
        let Some(effects) = self.effects.get(&target_id) else {
            return Vec::new();
        };

        let mut shields: Vec<ActiveShield> = effects
            .iter()
            .filter(|e| {
                e.is_shield
                    && e.applied_at < timestamp
                    && e.removed_at.map_or(true, |r| r >= timestamp)
            })
            .map(|e| ActiveShield {
                source_id: e.source_id,
                effect_id: e.effect_id,
                applied_at: e.applied_at,
            })
            .collect();

        // Sort by application time (oldest first = FIFO)
        shields.sort_by_key(|s| s.applied_at);
        shields
    }

    /// Handle multi-shield break scenario.
    ///
    /// When damage gets through with multiple shields active:
    /// - The log's dmg_absorbed only shows what the LAST shield absorbed
    /// - Earlier shields absorbed their full capacity (not shown in log)
    ///
    /// Credit earlier shields with ESTIMATED absorb, last shield with ACTUAL.
    fn attribute_multi_shield_break(&mut self, shields: &[ActiveShield], dmg_absorbed: i64) {
        let len = shields.len();

        // Credit all shields EXCEPT the last with their estimated absorb
        for shield in shields.iter().take(len.saturating_sub(1)) {
            if let Some(info) = get_shield_info(shield.effect_id)
                && let Some(estimated) = info.estimated_absorb()
            {
                self.credit_shielding(shield.source_id, estimated);
            }
        }

        // Credit the last shield with the actual dmg_absorbed from the log
        if let Some(last) = shields.last() {
            self.credit_shielding(last.source_id, dmg_absorbed);
        }
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
            .filter(|e| e.is_shield && e.removed_at.is_some())
            .filter(|e| {
                let removed = e.removed_at.unwrap();
                let delta = timestamp.signed_duration_since(removed).num_milliseconds();
                (0..=RECENTLY_CLOSED_GRACE_MS).contains(&delta)
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
