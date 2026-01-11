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
//! After removal, a brief fade-out animation plays before the effect
//! is deleted from the tracker.

use std::time::{Duration, Instant};

use chrono::NaiveDateTime;

use super::{DisplayTarget, EffectCategory};
use crate::context::IStr;

/// How long to show a faded effect after removal before deleting
const FADE_DURATION: Duration = Duration::from_secs(2);

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

    /// When we processed the apply event (system time)
    /// Used to calculate offset between game time and system time
    pub applied_instant: Instant,

    /// When the effect will expire based on duration (game time)
    /// None = indefinite duration (no auto-expire)
    pub expires_at: Option<NaiveDateTime>,

    /// When the effect was last refreshed (game time)
    pub last_refreshed_at: NaiveDateTime,

    /// Total duration (cached for fill calculations)
    pub duration: Option<Duration>,

    // ─── Removal state (system time for UI) ─────────────────────────────────
    /// When the effect was removed (system time)
    /// Set by either EffectRemoved signal OR duration expiry.
    /// Used for fade-out animation. None = still active.
    pub removed_at: Option<Instant>,

    // ─── State ──────────────────────────────────────────────────────────────
    /// Current stack/charge count
    pub stacks: u8,

    // ─── Display (cached from definition) ───────────────────────────────────
    /// RGBA color for display
    pub color: [u8; 4],

    /// Effect category
    pub category: EffectCategory,

    /// Which overlay should display this effect
    pub display_target: DisplayTarget,

    /// Ability ID for icon lookup (may differ from game_effect_id)
    pub icon_ability_id: u64,

    /// Show on raid frames overlay
    pub show_on_raid_frames: bool,

    /// Only show when remaining time is at or below this (0 = always show)
    pub show_at_secs: f32,

    /// Whether to show the icon (true) or use colored square (false)
    pub show_icon: bool,

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
        category: EffectCategory,
        display_target: DisplayTarget,
        icon_ability_id: u64,
        show_on_raid_frames: bool,
        show_at_secs: f32,
        show_icon: bool,
        cooldown_ready_secs: f32,
        audio: &crate::dsl::AudioConfig,
    ) -> Self {
        // Calculate lag compensation: how far behind was the game event from system time?
        // This accounts for file I/O delay, processing time, etc.
        let now_system = chrono::Local::now().naive_local();
        let lag = now_system.signed_duration_since(event_timestamp);
        let lag_ms = lag.num_milliseconds().max(0) as u64;
        let lag_duration = Duration::from_millis(lag_ms);

        // Backdate applied_instant to when the event actually happened in game
        // This ensures remaining_secs_realtime() reflects actual game time
        let now = Instant::now();
        let applied_instant = now.checked_sub(lag_duration).unwrap_or(now);

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
            applied_instant,
            expires_at,
            last_refreshed_at: event_timestamp,
            duration,
            removed_at: None,
            stacks: 1,
            color,
            category,
            display_target,
            icon_ability_id,
            show_on_raid_frames,
            show_at_secs,
            show_icon,
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
        }
    }

    /// Refresh the effect (reapplication extends duration)
    pub fn refresh(&mut self, event_timestamp: NaiveDateTime, duration: Option<Duration>) {
        self.last_refreshed_at = event_timestamp;

        if let Some(d) = duration {
            self.expires_at =
                Some(event_timestamp + chrono::Duration::milliseconds(d.as_millis() as i64));
            self.duration = Some(d);

            // Calculate lag between game event and system processing
            let now_system = chrono::Local::now().naive_local();
            let lag_ms = now_system
                .signed_duration_since(event_timestamp)
                .num_milliseconds()
                .max(0) as u64;
            let lag_duration = Duration::from_millis(lag_ms);

            // Backdate applied_instant to account for processing lag
            let now = Instant::now();
            self.applied_instant = now.checked_sub(lag_duration).unwrap_or(now);
        }

        // Clear removed state if we were fading out (effect came back)
        self.removed_at = None;
    }

    /// Update stack count
    pub fn set_stacks(&mut self, stacks: u8) {
        self.stacks = stacks;
    }

    /// Mark the effect as removed (starts fade-out)
    /// Called when we receive an EffectRemoved signal OR duration expires
    pub fn mark_removed(&mut self) {
        if self.removed_at.is_none() {
            self.removed_at = Some(Instant::now());
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

    /// Check if the effect has finished fading and should be deleted
    pub fn should_remove(&self) -> bool {
        self.removed_at
            .map(|t| t.elapsed() > FADE_DURATION)
            .unwrap_or(false)
    }

    /// Get opacity for rendering (1.0 = full, fades to 0.0 after removal)
    pub fn opacity(&self) -> f32 {
        match self.removed_at {
            None => 1.0,
            Some(removed) => {
                let elapsed = removed.elapsed().as_secs_f32();
                let fade_secs = FADE_DURATION.as_secs_f32();
                (1.0 - elapsed / fade_secs).max(0.0)
            }
        }
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

    // ─── Audio Methods ──────────────────────────────────────────────────────────

    /// Get remaining time in seconds using realtime (for audio sync)
    /// Uses system time (Instant) for smooth countdown independent of game log timing
    pub fn remaining_secs_realtime(&self) -> f32 {
        let Some(dur) = self.duration else { return 0.0 };
        let elapsed = self.applied_instant.elapsed();
        let remaining = dur.saturating_sub(elapsed);
        remaining.as_secs_f32()
    }

    /// Check if effect should be visible based on show_at_secs threshold
    ///
    /// Returns true if:
    /// - show_at_secs is 0 (always show), OR
    /// - remaining time is at or below show_at_secs threshold
    pub fn is_visible(&self) -> bool {
        if self.show_at_secs <= 0.0 {
            return true; // 0 means always show
        }
        self.remaining_secs_realtime() <= self.show_at_secs
    }

    /// Check if countdown should be announced (matches timer logic exactly)
    ///
    /// Announces N when remaining is in [N, N+0.3) to sync with visual display:
    /// - remaining 3.8s → no announcement (too early)
    /// - remaining 3.2s → announces 3 (in window [3.0, 3.3))
    /// - remaining 2.2s → announces 2
    pub fn check_countdown(&mut self) -> Option<u8> {
        // Don't announce if effect was manually removed
        if self.removed_at.is_some() {
            return None;
        }

        if !self.audio_enabled || self.countdown_start == 0 {
            return None;
        }

        let remaining = self.remaining_secs_realtime();

        // Check each second from countdown_start down to 1
        for seconds in (1..=self.countdown_start.min(10)).rev() {
            let lower = seconds as f32;
            let upper = lower + 0.3;

            // Announce when remaining is in [N, N+0.3)
            if remaining >= lower && remaining < upper {
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
    pub fn check_audio_offset(&mut self) -> bool {
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

        let remaining = self.remaining_secs_realtime();

        // Fire when we cross into the offset window
        if remaining <= self.audio_offset as f32 && remaining > 0.0 {
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
    /// Uses a small window like countdown to ensure we catch expiration
    /// before the effect is removed from the tracker.
    pub fn check_expiration_audio(&mut self) -> bool {
        // Don't play if effect was manually removed (cleansed, clicked off, etc.)
        if self.removed_at.is_some() {
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

        let remaining = self.remaining_secs_realtime();

        // Fire in window [0, 0.3) - catches expiration before effect is removed
        // This matches the countdown window logic
        if (0.0..0.3).contains(&remaining) {
            self.audio_played = true;
            return true;
        }

        false
    }
}

/// Key for identifying unique effect instances
///
/// An effect is unique per (definition, target) pair.
/// If the same effect is reapplied to the same target, it refreshes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EffectKey {
    pub definition_id: String,
    pub target_entity_id: i64,
}

impl EffectKey {
    pub fn new(definition_id: &str, target_entity_id: i64) -> Self {
        Self {
            definition_id: definition_id.to_string(),
            target_entity_id,
        }
    }
}
