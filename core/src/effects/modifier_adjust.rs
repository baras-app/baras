//! A proc'd `EffectModifier` and how it changes an effect's duration.

use std::time::Duration;

use baras_types::EffectModifier;
use chrono::NaiveDateTime;

use super::ActiveEffect;

/// One modifier proc, stamped with the log time of its triggering signal
pub(super) struct ModifierAdjustment {
    pub mod_idx: usize,
    /// Log time of the triggering signal
    pub at: NaiveDateTime,
    pub adjust_duration_secs: f32,
    pub refill_duration: bool,
    pub icd_secs: Option<f32>,
    pub icd_from_application: bool,
    pub max_duration_secs: Option<f32>,
    pub cancel: bool,
}

impl ModifierAdjustment {
    /// `scale` multiplies the duration adjustment (`ResourceSpent` per-amount);
    /// `icd_secs` is the effective (alacrity-scaled) internal cooldown
    pub fn new(
        modifier: &EffectModifier,
        mod_idx: usize,
        at: NaiveDateTime,
        scale: f32,
        icd_secs: Option<f32>,
    ) -> Self {
        Self {
            mod_idx,
            at,
            adjust_duration_secs: modifier.adjust_duration_secs * scale,
            refill_duration: modifier.refill_duration,
            icd_secs,
            icd_from_application: modifier.icd_from_application,
            max_duration_secs: modifier.max_duration_secs,
            cancel: modifier.cancel,
        }
    }
}

fn secs(s: f32) -> chrono::Duration {
    chrono::Duration::milliseconds((s * 1000.0) as i64)
}

impl ActiveEffect {
    /// Apply a proc whose ICD has already been checked and recorded: cancel,
    /// refill (to `refill`, the effective duration) or shift `expires_at`,
    /// capped at `max_duration_secs` from application and never before the proc.
    /// With `revive`, an extension the log places after the proc revives a
    /// timer-expired effect.
    pub(super) fn apply_modifier(&mut self, adj: &ModifierAdjustment, refill: Option<Duration>, revive: bool) {
        if adj.cancel {
            self.mark_removed();
            return;
        }
        let Some(expires) = self.expires_at else {
            return;
        };
        let new_expires = if adj.refill_duration {
            let Some(dur) = refill else {
                return;
            };
            adj.at + dur
        } else if adj.adjust_duration_secs != 0.0 {
            expires + secs(adj.adjust_duration_secs)
        } else {
            return;
        };
        let clamped = match adj.max_duration_secs {
            Some(max) => {
                let cap = self.applied_at + secs(max);
                self.max_expires_at.get_or_insert(cap);
                new_expires.min(cap)
            }
            None => new_expires,
        };
        let final_expires = clamped.max(adj.at);
        self.expires_at = Some(final_expires);
        // `duration` (the fill denominator) intentionally stays at the base
        // duration: fill_percent clamps to 1.0, so an extension shows a full
        // bar until remaining drops back below it. Inflating `duration` folded
        // elapsed time into the denominator and shrank the bar on every proc.

        if adj.refill_duration || adj.adjust_duration_secs > 0.0 {
            self.audio_played = false;
            self.countdown_announced = [false; 10];
            self.on_end_alert_fired = false;
        }
        if revive && final_expires > adj.at {
            self.revive();
        }
    }
}
