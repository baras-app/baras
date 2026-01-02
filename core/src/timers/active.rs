//! Active timer instances (runtime state)
//!
//! An `ActiveTimer` represents a currently running countdown timer.
//! Timers are triggered by game events (ability casts, effect applies, etc.)
//! and count down to zero, optionally chaining to other timers.
//!
//! # Lifecycle
//!
//! 1. Trigger event matches `TimerDefinition` → `ActiveTimer` created
//! 2. Timer counts down, optionally showing alert near end
//! 3. Timer expires → triggers chained timer (if any) → removed

use std::time::{Duration, Instant};

use chrono::NaiveDateTime;

use crate::dsl::AudioConfig;

/// An active timer instance
///
/// Created when a `TimerDefinition`'s trigger condition is met.
/// The renderer receives these to display countdown bars.
#[derive(Debug, Clone)]
pub struct ActiveTimer {
    /// ID of the definition this timer came from
    pub definition_id: String,

    /// Display name (cached from definition)
    pub name: String,

    // ─── Entities (optional, for targeted timers) ───────────────────────────
    /// Entity ID of the target (if this timer is per-target)
    pub target_entity_id: Option<i64>,

    // ─── Timing (game time from combat log) ─────────────────────────────────
    /// When the timer was started (game time)
    pub started_at: NaiveDateTime,

    /// When we processed the start event (system time)
    pub started_instant: Instant,

    /// When the timer will expire (game time)
    pub expires_at: NaiveDateTime,

    /// Total duration
    pub duration: Duration,

    // ─── State ──────────────────────────────────────────────────────────────
    /// How many times this timer has repeated (0 = first run)
    pub repeat_count: u8,

    /// Maximum repeats allowed (from definition)
    pub max_repeats: u8,

    /// Whether the alert has been fired for this timer instance
    pub alert_fired: bool,

    // ─── Display (cached from definition) ───────────────────────────────────
    /// RGBA color for display
    pub color: [u8; 4],

    /// Timer ID to trigger when this expires (if any)
    pub triggers_timer: Option<String>,

    /// Show on raid frames instead of timer bar?
    pub show_on_raid_frames: bool,

    /// Only show when remaining time is at or below this (0 = always show)
    pub show_at_secs: f32,

    // ─── Audio (countdown tracking) ───────────────────────────────────────
    /// Tracks which countdown seconds have been announced (1-10)
    /// Index 0 = 1 second, index 9 = 10 seconds
    countdown_announced: [bool; 10],

    /// When to start countdown audio (0 = disabled, 1-10)
    pub countdown_start: u8,

    /// Voice pack for countdown (Amy, Jim, Yolo, Nerevar)
    pub countdown_voice: String,

    /// Master toggle for all audio on this timer
    pub audio_enabled: bool,

    /// Audio file to play when timer expires (or at offset)
    pub audio_file: Option<String>,

    /// Seconds before expiration to play audio (0 = on expiration)
    pub audio_offset: u8,

    /// Whether the offset audio has been fired
    audio_offset_fired: bool,
}

impl ActiveTimer {
    /// Create a new active timer
    pub fn new(
        definition_id: String,
        name: String,
        target_entity_id: Option<i64>,
        event_timestamp: NaiveDateTime,
        duration: Duration,
        max_repeats: u8,
        color: [u8; 4],
        triggers_timer: Option<String>,
        show_on_raid_frames: bool,
        show_at_secs: f32,
        audio: &AudioConfig,
    ) -> Self {
        // Calculate lag compensation: how far behind was the game event from system time?
        // This accounts for file I/O delay, processing time, etc.
        let now_system = chrono::Local::now().naive_local();
        let lag = now_system.signed_duration_since(event_timestamp);
        let lag_ms = lag.num_milliseconds().max(0) as u64;
        let lag_duration = Duration::from_millis(lag_ms);

        // Backdate started_instant to when the event actually happened in game
        // This ensures remaining_secs_realtime() reflects actual game time
        let now = Instant::now();
        let started_instant = now.checked_sub(lag_duration).unwrap_or(now);

        let expires_at =
            event_timestamp + chrono::Duration::milliseconds(duration.as_millis() as i64);

        Self {
            definition_id,
            name,
            target_entity_id,
            started_at: event_timestamp,
            started_instant,
            expires_at,
            duration,
            repeat_count: 0,
            max_repeats,
            alert_fired: false,
            color,
            triggers_timer,
            show_on_raid_frames,
            show_at_secs,
            countdown_announced: [false; 10],
            countdown_start: audio.countdown_start,
            countdown_voice: audio
                .countdown_voice
                .clone()
                .unwrap_or_else(|| "Amy".to_string()),
            audio_enabled: audio.enabled,
            audio_file: audio.file.clone(),
            audio_offset: audio.offset,
            audio_offset_fired: false,
        }
    }

    /// Refresh the timer (restart from now)
    pub fn refresh(&mut self, event_timestamp: NaiveDateTime) {
        // Apply same lag compensation as new() for consistent timing
        let now_system = chrono::Local::now().naive_local();
        let lag = now_system.signed_duration_since(event_timestamp);
        let lag_ms = lag.num_milliseconds().max(0) as u64;
        let lag_duration = Duration::from_millis(lag_ms);
        let now = Instant::now();

        self.started_at = event_timestamp;
        self.started_instant = now.checked_sub(lag_duration).unwrap_or(now);
        self.expires_at =
            event_timestamp + chrono::Duration::milliseconds(self.duration.as_millis() as i64);
        self.alert_fired = false;
        self.audio_offset_fired = false;
        self.countdown_announced = [false; 10];
    }

    /// Repeat the timer (increment count, restart)
    /// Returns false if max repeats reached
    pub fn repeat(&mut self, event_timestamp: NaiveDateTime) -> bool {
        if self.repeat_count >= self.max_repeats {
            return false;
        }

        self.repeat_count += 1;
        self.refresh(event_timestamp);
        true
    }

    /// Check if the timer has expired
    pub fn has_expired(&self, current_game_time: NaiveDateTime) -> bool {
        current_game_time >= self.expires_at
    }

    /// Get fill percentage for countdown display (1.0 = full, 0.0 = expired)
    pub fn fill_percent(&self, current_game_time: NaiveDateTime) -> f32 {
        let remaining = self.expires_at.signed_duration_since(current_game_time);
        let remaining_ms = remaining.num_milliseconds().max(0) as f32;
        let duration_ms = self.duration.as_millis() as f32;

        if duration_ms > 0.0 {
            (remaining_ms / duration_ms).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    /// Get remaining time in seconds (game time - for expiration logic)
    pub fn remaining_secs(&self, current_game_time: NaiveDateTime) -> f32 {
        let remaining = self.expires_at.signed_duration_since(current_game_time);
        (remaining.num_milliseconds().max(0) as f32) / 1000.0
    }

    /// Get remaining time in seconds (realtime - for display and audio)
    ///
    /// Uses system time (`Instant`) instead of game time to avoid
    /// drift between combat log timestamps and current time.
    pub fn remaining_secs_realtime(&self) -> f32 {
        let elapsed = self.started_instant.elapsed();
        let remaining = self.duration.saturating_sub(elapsed);
        remaining.as_secs_f32()
    }

    /// Check if timer should be visible based on show_at_secs threshold
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

    /// Check if timer is within alert threshold and alert hasn't fired yet
    pub fn should_alert(&self, current_game_time: NaiveDateTime, threshold_secs: f32) -> bool {
        if self.alert_fired {
            return false;
        }

        let remaining = self.remaining_secs(current_game_time);
        remaining <= threshold_secs && remaining > 0.0
    }

    /// Mark alert as fired
    pub fn fire_alert(&mut self) {
        self.alert_fired = true;
    }

    /// Check if this timer can repeat
    pub fn can_repeat(&self) -> bool {
        self.max_repeats > 0 && self.repeat_count < self.max_repeats
    }

    /// Check for countdown seconds to announce (respects countdown_start setting)
    ///
    /// Returns Some(seconds) if we've crossed into the announcement window
    /// for that second and it hasn't been announced yet.
    ///
    /// Uses realtime (system Instant) for accurate audio sync.
    /// Announces N when remaining is in [N, N+0.3) to sync with visual display:
    /// - remaining 3.8s → no announcement (too early)
    /// - remaining 3.2s → announces 3 (in window [3.0, 3.3))
    /// - remaining 2.2s → announces 2
    pub fn check_countdown(&mut self) -> Option<u8> {
        // 0 means countdown disabled for this timer
        if self.countdown_start == 0 {
            return None;
        }

        let remaining = self.remaining_secs_realtime();

        // Check each second from countdown_start down to 1
        for seconds in (1..=self.countdown_start).rev() {
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

    /// Check if the audio should fire at the configured offset
    ///
    /// Returns true (and marks as fired) when:
    /// - audio_file is Some
    /// - audio_offset > 0 (offset of 0 means fire on expiration, handled separately)
    /// - remaining time just crossed below the offset threshold
    /// - hasn't already fired
    ///
    /// Uses realtime for accurate audio sync.
    pub fn check_audio_offset(&mut self) -> bool {
        // No audio file configured
        if self.audio_file.is_none() {
            return false;
        }

        // offset=0 means fire on expiration, not here
        if self.audio_offset == 0 {
            return false;
        }

        // Already fired
        if self.audio_offset_fired {
            return false;
        }

        let remaining = self.remaining_secs_realtime();

        // Fire when we cross into the offset window
        if remaining <= self.audio_offset as f32 && remaining > 0.0 {
            self.audio_offset_fired = true;
            return true;
        }

        false
    }
}

/// Key for identifying unique timer instances
///
/// A timer is unique per (definition, target) pair.
/// Target is optional for global timers.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TimerKey {
    pub definition_id: String,
    pub target_entity_id: Option<i64>,
}

impl TimerKey {
    pub fn new(definition_id: &str, target_entity_id: Option<i64>) -> Self {
        Self {
            definition_id: definition_id.to_string(),
            target_entity_id,
        }
    }

    pub fn global(definition_id: &str) -> Self {
        Self::new(definition_id, None)
    }
}
