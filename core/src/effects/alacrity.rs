//! Tracks alacrity-increasing buffs on the local player.
//!
//! When one of the known buffs (see `data/alacrity_abilities.csv`) is applied
//! to the local player, its bonus is added on top of the configured baseline
//! alacrity when the effect tracker computes durations for new entries.
//! Only new entries are affected — existing entries are not recomputed.

use chrono::NaiveDateTime;
use hashbrown::HashMap;

use crate::game_data::{ALACRITY_BUFFS, AlacrityBuff};

#[derive(Debug)]
struct ActiveAlacrityBuff {
    buff: &'static AlacrityBuff,
    stacks: u8,
    /// Apply time, or last ModifyCharges for stacking buffs.
    /// Used for the duration-based timeout when a remove event is missed.
    last_update: NaiveDateTime,
}

#[derive(Debug, Default)]
pub struct AlacrityBuffTracker {
    active: HashMap<i64, ActiveAlacrityBuff>,
}

impl AlacrityBuffTracker {
    pub fn on_applied(&mut self, effect_id: i64, charges: Option<u8>, timestamp: NaiveDateTime) {
        if let Some(buff) = ALACRITY_BUFFS.get(&effect_id) {
            self.active.insert(
                effect_id,
                ActiveAlacrityBuff {
                    buff,
                    stacks: charges.unwrap_or(1),
                    last_update: timestamp,
                },
            );
        }
    }

    pub fn on_charges_changed(&mut self, effect_id: i64, charges: u8, timestamp: NaiveDateTime) {
        if let Some(entry) = self.active.get_mut(&effect_id) {
            entry.stacks = charges;
            entry.last_update = timestamp;
        } else if let Some(buff) = ALACRITY_BUFFS.get(&effect_id) {
            // ModifyCharges without a prior apply (e.g. started tailing mid-buff)
            self.active.insert(
                effect_id,
                ActiveAlacrityBuff {
                    buff,
                    stacks: charges,
                    last_update: timestamp,
                },
            );
        }
    }

    pub fn on_removed(&mut self, effect_id: i64) {
        self.active.remove(&effect_id);
    }

    pub fn clear(&mut self) {
        self.active.clear();
    }

    /// Drop buffs that outlived their base duration (missed remove event).
    pub fn prune_expired(&mut self, now: NaiveDateTime) {
        self.active.retain(|_, e| !Self::is_expired(e, now));
    }

    /// Total bonus alacrity in percent (e.g. 20.0 for +20%) from active buffs at `now`.
    /// Expired entries are skipped but not removed — `prune_expired` handles cleanup.
    pub fn bonus_percent(&self, now: NaiveDateTime) -> f32 {
        self.active
            .values()
            .filter(|e| !Self::is_expired(e, now))
            .map(|e| {
                let mult = if e.buff.is_stack { e.stacks as f32 } else { 1.0 };
                e.buff.amount * mult * 100.0
            })
            .sum()
    }

    fn is_expired(entry: &ActiveAlacrityBuff, now: NaiveDateTime) -> bool {
        let elapsed_secs = now.signed_duration_since(entry.last_update).num_milliseconds() as f32 / 1000.0;
        elapsed_secs > entry.buff.duration_secs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeDelta;

    const POLARITY_SHIFT: i64 = 961325349994496; // 0.2, non-stack, 20s
    const FOCAL_POINT: i64 = 3428431874228224; // 0.01/stack, 15s

    fn ts() -> NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 1, 1)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap()
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 0.001, "expected ~{expected}, got {actual}");
    }

    #[test]
    fn non_stack_buff_applies_and_removes() {
        let mut t = AlacrityBuffTracker::default();
        t.on_applied(POLARITY_SHIFT, None, ts());
        assert_eq!(t.bonus_percent(ts()), 20.0);
        t.on_removed(POLARITY_SHIFT);
        assert_eq!(t.bonus_percent(ts()), 0.0);
    }

    #[test]
    fn stack_buff_multiplies_by_charges() {
        let mut t = AlacrityBuffTracker::default();
        t.on_applied(FOCAL_POINT, None, ts());
        assert_close(t.bonus_percent(ts()), 1.0);
        t.on_charges_changed(FOCAL_POINT, 5, ts());
        assert_close(t.bonus_percent(ts()), 5.0);
    }

    #[test]
    fn buffs_sum_and_unknown_effects_ignored() {
        let mut t = AlacrityBuffTracker::default();
        t.on_applied(POLARITY_SHIFT, None, ts());
        t.on_charges_changed(FOCAL_POINT, 3, ts()); // charges without prior apply
        t.on_applied(12345, None, ts()); // not an alacrity buff
        assert_close(t.bonus_percent(ts()), 23.0);
    }

    #[test]
    fn expires_after_duration_from_last_update() {
        let mut t = AlacrityBuffTracker::default();
        t.on_applied(POLARITY_SHIFT, None, ts());
        t.on_applied(FOCAL_POINT, None, ts());
        // Charges refresh pushes the stack buff's timeout forward
        t.on_charges_changed(FOCAL_POINT, 2, ts() + TimeDelta::seconds(10));

        // 21s in: Polarity Shift (20s) timed out; Focal Point alive (10s + 15s window)
        let now = ts() + TimeDelta::seconds(21);
        assert_close(t.bonus_percent(now), 2.0);
        t.prune_expired(now);
        assert_eq!(t.active.len(), 1);

        // 26s in: Focal Point timed out too
        assert_eq!(t.bonus_percent(ts() + TimeDelta::seconds(26)), 0.0);
    }
}
