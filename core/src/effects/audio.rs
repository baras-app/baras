//! Per-poll effect audio: countdown numbers and offset warnings. Expiration
//! sounds are emitted by `tick()` as alerts, since the effect may be gone by
//! the next poll.

use chrono::NaiveDateTime;

use super::EffectTracker;

/// A sound the audio service should play for an effect
#[derive(Debug, Clone, PartialEq)]
pub enum EffectAudio {
    /// Voice-pack number for the remaining seconds
    Countdown { name: String, seconds: u8, voice: String },
    /// Alert sound (custom file, or TTS of `name` when `file` is None)
    Sound { name: String, file: Option<String> },
}

impl EffectTracker {
    /// Audio due now (interpolated game time). Called on every service poll.
    pub fn take_audio(&mut self) -> Vec<EffectAudio> {
        self.interpolated_game_time()
            .map(|now| self.take_audio_at(now))
            .unwrap_or_default()
    }

    /// Whether a live effect still has audio to come (countdown, offset
    /// warning or expiration sound): the service polls fast while true so
    /// countdown windows aren't skipped.
    pub fn has_pending_audio(&self) -> bool {
        self.active_effects()
            .any(|e| e.audio_enabled && e.removed_at.is_none() && (!e.audio_played || e.countdown_start > 0))
    }

    /// Audio due at `now`; marks what it returns as played.
    pub fn take_audio_at(&mut self, now: NaiveDateTime) -> Vec<EffectAudio> {
        let mut out = Vec::new();
        for effect in self.active_effects_mut() {
            if !effect.audio_enabled {
                continue;
            }
            let remaining_total = effect.remaining_secs(now).unwrap_or(0.0);
            let remaining_base = effect.remaining_base_secs(remaining_total);

            if let Some(seconds) = effect.check_countdown(remaining_base) {
                out.push(EffectAudio::Countdown {
                    name: effect.display_text.clone(),
                    seconds,
                    voice: effect.countdown_voice.clone(),
                });
            }
            if effect.check_audio_offset(remaining_base) {
                out.push(EffectAudio::Sound {
                    name: effect.display_text.clone(),
                    file: effect.audio_file.clone(),
                });
            }
        }
        out
    }
}
