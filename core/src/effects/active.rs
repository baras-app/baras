//! Active effect instances (runtime state)
//!
//! An `ActiveEffect` represents a currently active effect on a specific entity.
//! It is created when a game signal matches an `EffectDefinition` and tracks
//! the live state until the effect is removed.
//!
//! # Removal Triggers
//!
//! Effects are removed when EITHER:
//! 1. `EffectRemoved` signal received from game (authoritative)
//! 2. Duration timer expires (fallback for missed/missing remove events)
//!
//! After removal, the effect is immediately deleted from the tracker.

use std::time::{Duration, Instant};

use chrono::NaiveDateTime;

use super::DisplayTarget;
use crate::context::IStr;

// Instant is still used for `removed_at` (a boolean-like flag for GC)

/// An active effect instance on a specific entity
///
/// Created when an `EffectDefinition` matches a game signal.
/// The renderer receives these to display effect indicators.
#[derive(Debug, Clone)]
pub struct ActiveEffect {
    /// ID of the definition this effect came from
    pub definition_id: String,

    /// Game's numeric effect ID (from combat log)
    pub game_effect_id: u64,

    /// Display name (cached from definition)
    pub name: String,

    /// Display text for overlay (defaults to name if not set)
    pub display_text: String,

    // ─── Entities ───────────────────────────────────────────────────────────
    /// Entity ID of who applied this effect
    pub source_entity_id: i64,

    /// Name of the source entity (from event)
    pub source_name: IStr,

    /// Entity ID of who has this effect
    pub target_entity_id: i64,

    /// Name of the target entity (from event)
    pub target_name: IStr,

    /// Is this effect from the local player?
    pub is_from_local_player: bool,

    // ─── Timing (game time from combat log) ─────────────────────────────────
    /// When the effect was applied (game time)
    pub applied_at: NaiveDateTime,

    /// When the effect will expire based on duration (game time)
    /// None = indefinite duration (no auto-expire)
    pub expires_at: Option<NaiveDateTime>,

    /// When the effect was last refreshed (game time)
    pub last_refreshed_at: NaiveDateTime,

    /// Total duration (cached for fill calculations)
    pub duration: Option<Duration>,

    // ─── Removal state ─────────────────────────────────────────────────────
    /// When the effect was removed (system time).
    /// Set by either EffectRemoved signal OR duration expiry.
    /// Effect is immediately deleted from tracker on next tick.
    pub removed_at: Option<Instant>,

    // ─── State ──────────────────────────────────────────────────────────────
    /// Current stack/charge count
    pub stacks: u8,

    // ─── Display (cached from definition) ───────────────────────────────────
    /// RGBA color for display
    pub color: [u8; 4],

    /// Which overlays should display this effect (may be multiple)
    pub display_targets: Vec<DisplayTarget>,

    /// Ability ID for icon lookup (may differ from game_effect_id)
    pub icon_ability_id: u64,

    /// Only show when remaining time is at or below this (0 = always show)
    pub show_at_secs: f32,

    /// Whether to show the icon (true) or use colored square (false)
    pub show_icon: bool,

    /// Whether to display the source entity name on personal overlays
    pub display_source: bool,

    /// Seconds to show "Ready" state after cooldown ends (0 = disabled)
    pub cooldown_ready_secs: f32,

    // ─── Audio ────────────────────────────────────────────────────────────────
    /// Whether on-apply audio has been played
    pub audio_played: bool,

    /// Tracks which countdown seconds have been announced (1-10)
    pub countdown_announced: [bool; 10],

    /// Countdown start second (0 = disabled)
    pub countdown_start: u8,

    /// Voice pack for countdown
    pub countdown_voice: String,

    /// Audio file for on-apply sound
    pub audio_file: Option<String>,

    /// Audio offset (seconds before expiration to play sound)
    pub audio_offset: u8,

    /// Whether audio is enabled for this effect
    pub audio_enabled: bool,

    /// Whether the "on end" alert has been fired (to prevent duplicates)
    pub on_end_alert_fired: bool,

    /// Alert text to display (None = no alert configured)
    pub alert_text: Option<String>,

    /// Whether to fire alert on expiration (vs on apply)
    pub alert_on_expire: bool,

    /// Whether this effect was expired by the app's duration timer (heuristic)
    /// rather than an authoritative EffectRemoved signal.
    ///
    /// Timer-expired effects remain in `active_effects` so that `refresh_abilities`
    /// can still revive them (the game duration may exceed our estimate). They are
    /// hidden from display and alerts fire normally on timer expiry.
    /// After a grace period, the effect is hard-removed for GC.
    pub timer_expired: bool,
}

#[allow(clippy::too_many_arguments)]
impl ActiveEffect {
    /// Create a new active effect
    pub fn new(
        definition_id: String,
        game_effect_id: u64,
        name: String,
        display_text: String,
        source_entity_id: i64,
        source_name: IStr,
        target_entity_id: i64,
        target_name: IStr,
        is_from_local_player: bool,
        event_timestamp: NaiveDateTime,
        duration: Option<Duration>,
        color: [u8; 4],
        display_targets: Vec<DisplayTarget>,
        icon_ability_id: u64,
        show_at_secs: f32,
        show_icon: bool,
        display_source: bool,
        cooldown_ready_secs: f32,
        audio: &crate::dsl::AudioConfig,
        alert_text: Option<String>,
        alert_on_expire: bool,
    ) -> Self {
        let expires_at = duration
            .map(|d| event_timestamp + chrono::Duration::milliseconds(d.as_millis() as i64));

        Self {
            definition_id,
            game_effect_id,
            name,
            display_text,
            source_entity_id,
            source_name,
            target_entity_id,
            target_name,
            is_from_local_player,
            applied_at: event_timestamp,
            expires_at,
            last_refreshed_at: event_timestamp,
            duration,
            removed_at: None,
            stacks: 1,
            color,
            display_targets,
            icon_ability_id,
            show_at_secs,
            show_icon,
            display_source,
            cooldown_ready_secs,
            audio_played: false,
            countdown_announced: [false; 10],
            countdown_start: audio.countdown_start,
            countdown_voice: audio
                .countdown_voice
                .clone()
                .unwrap_or_else(|| "Amy".to_string()),
            audio_file: audio.file.clone(),
            audio_offset: audio.offset,
            audio_enabled: audio.enabled,
            on_end_alert_fired: false,
            alert_text,
            alert_on_expire,
            timer_expired: false,
        }
    }

    /// Refresh the effect (reapplication extends duration)
    pub fn refresh(&mut self, event_timestamp: NaiveDateTime, duration: Option<Duration>) {
        self.last_refreshed_at = event_timestamp;

        if let Some(d) = duration {
            self.expires_at =
                Some(event_timestamp + chrono::Duration::milliseconds(d.as_millis() as i64));
            self.duration = Some(d);
        }

        // Clear removed state if we were fading out (effect came back)
        self.removed_at = None;
        // Clear timer-expired state (refresh revives the effect)
        self.timer_expired = false;
        // Reset on-end alert so it can fire again
        self.on_end_alert_fired = false;
        // Reset audio so it fires again on the next expiration/offset
        self.audio_played = false;
        self.countdown_announced = [false; 10];
    }

    /// Refresh only the duration (without updating last_refreshed_at)
    ///
    /// Used by ModifyCharges with is_refreshed_on_modify. Unlike `refresh()`,
    /// this does NOT update `last_refreshed_at`, so the stale-removal window
    /// (which guards against DOT reapplication) isn't affected.
    pub fn refresh_duration(&mut self, event_timestamp: NaiveDateTime, duration: Duration) {
        self.expires_at =
            Some(event_timestamp + chrono::Duration::milliseconds(duration.as_millis() as i64));
        self.duration = Some(duration);

        self.removed_at = None;
        self.timer_expired = false;
        self.on_end_alert_fired = false;
        // Reset audio so it fires again on the next expiration/offset
        self.audio_played = false;
        self.countdown_announced = [false; 10];
    }

    /// Update stack count
    pub fn set_stacks(&mut self, stacks: u8) {
        self.stacks = stacks;
    }

    /// Mark the effect as removed (starts fade-out)
    /// Called when we receive an EffectRemoved signal OR duration expires
    /// Returns true if this was the first time marking removed (for counter tracking)
    pub fn mark_removed(&mut self) -> bool {
        if self.removed_at.is_none() {
            self.removed_at = Some(Instant::now());
            true
        } else {
            false
        }
    }

    /// Check if the effect has expired based on duration
    /// Returns true if we have a duration and current game time is past expiry
    pub fn has_duration_expired(&self, current_game_time: NaiveDateTime) -> bool {
        self.expires_at
            .map(|expires| current_game_time >= expires)
            .unwrap_or(false)
    }

    /// Check if the effect is still active (not removed and not expired)
    pub fn is_active(&self, current_game_time: NaiveDateTime) -> bool {
        self.removed_at.is_none() && !self.has_duration_expired(current_game_time)
    }

    /// Get fill percentage for countdown display (1.0 = full, 0.0 = expired)
    ///
    /// Pass the current game time (latest log timestamp) for accurate calculation.
    pub fn fill_percent(&self, current_game_time: NaiveDateTime) -> f32 {
        match (self.expires_at, self.duration) {
            (Some(expires), Some(duration)) => {
                let remaining = expires.signed_duration_since(current_game_time);
                let remaining_ms = remaining.num_milliseconds().max(0) as f32;
                let duration_ms = duration.as_millis() as f32;

                if duration_ms > 0.0 {
                    (remaining_ms / duration_ms).clamp(0.0, 1.0)
                } else {
                    1.0
                }
            }
            // No duration = indefinite, show full
            _ => 1.0,
        }
    }

    /// Get remaining time in seconds (None if indefinite)
    pub fn remaining_secs(&self, current_game_time: NaiveDateTime) -> Option<f32> {
        self.expires_at.map(|expires| {
            let remaining = expires.signed_duration_since(current_game_time);
            (remaining.num_milliseconds().max(0) as f32) / 1000.0
        })
    }

    /// Check if effect is near expiration (within threshold)
    pub fn is_near_expiration(
        &self,
        current_game_time: NaiveDateTime,
        threshold_secs: f32,
    ) -> bool {
        self.remaining_secs(current_game_time)
            .map(|r| r <= threshold_secs && r > 0.0)
            .unwrap_or(false)
    }

    // ─── Audio/Display Methods ────────────────────────────────────────────────
    //
    // All methods that need remaining time now accept it as a parameter.
    // The EffectTracker computes interpolated game time once per tick and
    // passes the resulting remaining seconds to each effect. This avoids
    // comparing SWTOR's clock with the system clock (which can drift).

    /// Get remaining BASE time (excludes ready state time)
    ///
    /// `remaining_total` is the total remaining seconds (from `remaining_secs(game_time)`)
    /// computed by the tracker using interpolated game time.
    pub fn remaining_base_secs(&self, remaining_total: f32) -> f32 {
        (remaining_total - self.cooldown_ready_secs).max(0.0)
    }

    /// Check if base duration has ended (entered ready state or fully expired)
    /// Returns true when remaining <= cooldown_ready_secs (or <= 0 if no ready state)
    ///
    /// `remaining_total` is the total remaining seconds from interpolated game time.
    pub fn has_base_duration_ended(&self, remaining_total: f32) -> bool {
        self.remaining_base_secs(remaining_total) <= 0.0
    }

    /// Check if effect should be visible based on show_at_secs threshold
    ///
    /// Returns true if:
    /// - show_at_secs is 0 (always show), OR
    /// - remaining BASE time is at or below show_at_secs threshold
    ///
    /// For cooldowns with ready state, we add cooldown_ready_secs to the threshold
    /// so that show_at_secs represents seconds of BASE duration remaining.
    /// E.g., show_at_secs=30 with ready_secs=20 shows when total remaining <= 50,
    /// which is when base remaining = 30.
    ///
    /// `remaining_total` is the total remaining seconds from interpolated game time.
    pub fn is_visible(&self, remaining_total: f32) -> bool {
        if self.show_at_secs <= 0.0 {
            return true; // 0 means always show
        }
        // Add ready_secs to threshold so comparison is against total remaining
        // but threshold represents base duration remaining
        let effective_threshold = self.show_at_secs + self.cooldown_ready_secs;
        remaining_total <= effective_threshold
    }

    /// Check if countdown should be announced (matches timer logic exactly)
    ///
    /// Announces N when remaining is in [N, N+0.3) to sync with visual display:
    /// - remaining 3.8s → no announcement (too early)
    /// - remaining 3.2s → announces 3 (in window [3.0, 3.3))
    /// - remaining 2.2s → announces 2
    ///
    /// `remaining_base` is the BASE remaining seconds (excludes ready state time),
    /// computed from interpolated game time.
    pub fn check_countdown(&mut self, remaining_base: f32) -> Option<u8> {
        // Don't announce if effect was manually removed
        if self.removed_at.is_some() {
            return None;
        }

        if !self.audio_enabled || self.countdown_start == 0 {
            return None;
        }

        // Check each second from countdown_start down to 1
        for seconds in (1..=self.countdown_start.min(10)).rev() {
            let lower = seconds as f32;
            let upper = lower + 0.3;

            // Announce when remaining is in [N, N+0.3)
            if remaining_base >= lower && remaining_base < upper {
                let index = (seconds - 1) as usize;
                if !self.countdown_announced[index] {
                    self.countdown_announced[index] = true;
                    return Some(seconds);
                }
            }
        }
        None
    }

    /// Check if audio should fire at the configured offset (matches timer logic)
    ///
    /// Returns true (and marks as fired) when:
    /// - audio_file is Some
    /// - audio_offset > 0 (offset of 0 means fire on expiration, handled separately)
    /// - remaining time crossed below the offset threshold
    /// - hasn't already fired
    ///
    /// `remaining_base` is the BASE remaining seconds (excludes ready state time),
    /// computed from interpolated game time.
    pub fn check_audio_offset(&mut self, remaining_base: f32) -> bool {
        // Don't play if effect was manually removed
        if self.removed_at.is_some() {
            return false;
        }

        // No audio file configured
        if self.audio_file.is_none() {
            return false;
        }

        // offset=0 means fire on expiration, not here
        if self.audio_offset == 0 {
            return false;
        }

        // Already fired
        if self.audio_played {
            return false;
        }

        // Fire when we cross into the offset window
        if remaining_base <= self.audio_offset as f32 && remaining_base > 0.0 {
            self.audio_played = true;
            return true;
        }

        false
    }

    /// Check if audio should fire on expiration (offset == 0)
    ///
    /// Returns true (and marks as fired) when:
    /// - audio_file is Some
    /// - audio_offset == 0 (fire on expiration)
    /// - remaining time is in [0, 0.3) window (fires just as effect expires)
    /// - hasn't already fired
    ///
    /// `remaining_base` is the BASE remaining seconds (excludes ready state time),
    /// computed from interpolated game time.
    pub fn check_expiration_audio(&mut self, remaining_base: f32) -> bool {
        // Audio must be enabled for this effect
        if !self.audio_enabled {
            return false;
        }

        // No audio file configured
        if self.audio_file.is_none() {
            return false;
        }

        // Only handle offset=0 (on expiration)
        if self.audio_offset != 0 {
            return false;
        }

        // Already fired
        if self.audio_played {
            return false;
        }

        // Fire if effect was removed early (charges depleted, cleansed, etc.)
        if self.removed_at.is_some() {
            self.audio_played = true;
            return true;
        }

        // Fire in window [0, 0.3) - catches base duration expiration
        if (0.0..0.3).contains(&remaining_base) {
            self.audio_played = true;
            return true;
        }

        false
    }

    // Note: On-end expiration alerts are fired exclusively by EffectTracker::tick(),
    // which checks on_end_alert_fired + has_base_duration_ended()/removed_at after
    // signals have been dispatched. This avoids false "effect ended" alerts when an
    // effect is refreshed just before its timer expires.
}

/// Key for identifying unique effect instances
///
/// An effect is unique per (definition, source, target) triple.
/// Source is included so that two healers of the same class tracking
/// the same game effect (e.g., Kolto Shell from local vs other player)
/// don't collide — each source entity gets its own slot.
/// If the same source reapplies the same effect to the same target, it refreshes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EffectKey {
    pub definition_id: String,
    pub source_entity_id: i64,
    pub target_entity_id: i64,
}

impl EffectKey {
    pub fn new(definition_id: &str, source_entity_id: i64, target_entity_id: i64) -> Self {
        Self {
            definition_id: definition_id.to_string(),
            source_entity_id,
            target_entity_id,
        }
    }
}
