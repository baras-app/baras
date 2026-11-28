//! Per-tick expiry, end-of-effect alerts/sounds and countdown alerts.

use super::*;

impl EffectTracker {
    /// Tick the tracker - removes expired effects and updates state
    ///
    /// Uses interpolated game time for accurate remaining time calculations
    /// without comparing SWTOR's clock to the system clock.
    pub fn tick(&mut self) {
        if let Some(now) = self.interpolated_game_time() {
            self.tick_at(now);
        }
    }

    /// Tick at an explicit game time. Live code goes through `tick()`; replay
    /// tools pass log time so results don't depend on wall-clock speed.
    pub fn tick_at(&mut self, interp_time: NaiveDateTime) {
        let Some(current_time) = self.current_game_time else {
            return;
        };

        for (key, effect) in self.active_effects.iter_mut() {
            // Handle duration-expired effects.
            // Timer expiry is a heuristic — the in-game effect may outlast our estimate.
            // Instead of immediately removing, mark as timer_expired so the effect stays
            // in active_effects and can be revived by refresh_abilities.
            // After the grace period, hard-remove for GC.
            if !effect.is_removed() && effect.has_duration_expired(interp_time) {
                effect.mark_timer_expired();

                // After grace period, hard-remove (GC on next retain pass)
                if let Some(expires_at) = effect.expires_at {
                    let since_expiry_ms = interp_time.signed_duration_since(expires_at).num_milliseconds();
                    if since_expiry_ms > TIMER_EXPIRY_GRACE_MS {
                        effect.mark_removed();
                    }
                }
            }

            // End-of-effect alert and expiration sound (any reason except a
            // silent discard). The single owner of expiration audio: the
            // effect may be dropped below before any audio poll could see it.
            // An effect with no duration only ends when it is removed.
            let base_ended = effect
                .remaining_secs(interp_time)
                .is_some_and(|remaining| effect.has_base_duration_ended(remaining));
            if effect.on_end_alert_fired || !(base_ended || effect.is_removed()) {
                continue;
            }
            effect.on_end_alert_fired = true;
            let play_audio =
                effect.audio_enabled && !effect.audio_played && effect.audio_offset == 0 && effect.audio_file.is_some();
            effect.audio_played |= play_audio;
            let def = self.definitions.def(key.def);
            let alert = match &def.alert_text {
                Some(text) if def.alert_on == AlertTrigger::OnExpire => {
                    let text = effect.log_names().format(text, effect.stacks);
                    alerts::text_alert(def, text, current_time)
                }
                // No on-expire alert to carry the sound: send it audio-only
                _ if play_audio => FiredAlert {
                    alert_text_enabled: false,
                    ..alerts::text_alert(def, effect.display_text.clone(), current_time)
                },
                _ => continue,
            };
            self.fired_alerts.push(FiredAlert {
                audio_enabled: play_audio,
                audio_file: effect.audio_file.clone(),
                ..alert
            });
        }

        // Emit live countdown alerts for active effects whose remaining time
        // has entered their configured trailing window. Each tick pushes a
        // fresh FiredAlert carrying remaining_secs — the alerts overlay
        // dedupes by id and auto-suppresses when remaining hits zero.
        //
        // For cooldown effects (`cooldown_ready_secs > 0`) the alert tracks
        // the BASE cooldown, not the total time-to-expiry. We want the
        // countdown to fire in the last N seconds before the cooldown becomes
        // ready, then disappear — not during the ready-state tail.
        for (key, effect) in &self.active_effects {
            if !effect.is_displayed() {
                continue;
            }
            let def = self.definitions.def(key.def);
            if def.alert_on != AlertTrigger::Countdown {
                continue;
            }
            let Some(window) = def.alert_countdown_secs else {
                continue;
            };
            if window <= 0.0 {
                continue;
            }
            let Some(remaining_total) = effect.remaining_secs(interp_time) else {
                continue;
            };
            // Base = total - ready_state. For non-cooldown effects this
            // collapses to `remaining_total`.
            let remaining_base = effect.remaining_base_secs(remaining_total);
            if remaining_base <= 0.0 || remaining_base > window {
                continue;
            }

            let raw_name = def.alert_text.as_deref().unwrap_or(&def.name);
            let formatted = effect.log_names().format(raw_name, effect.stacks);
            let text = format!("{} ({:.1})", formatted, remaining_base);
            self.fired_alerts.push(FiredAlert {
                remaining_secs: Some(remaining_base),
                ..alerts::text_alert(def, text, current_time)
            });
        }

        // Remove effects that have been marked removed (immediate, no fade delay)
        self.active_effects.retain(|_, effect| !effect.is_removed());
    }
}
