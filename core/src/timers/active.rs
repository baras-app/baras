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

    // ─── Audio (countdown tracking) ───────────────────────────────────────
    /// Tracks which countdown seconds have been announced (5, 4, 3, 2, 1)
    /// Index 0 = 1 second, index 4 = 5 seconds
    countdown_announced: [bool; 5],
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
    ) -> Self {
        let now = Instant::now();
        let expires_at = event_timestamp
            + chrono::Duration::milliseconds(duration.as_millis() as i64);

        Self {
            definition_id,
            name,
            target_entity_id,
            started_at: event_timestamp,
            started_instant: now,
            expires_at,
            duration,
            repeat_count: 0,
            max_repeats,
            alert_fired: false,
            color,
            triggers_timer,
            show_on_raid_frames,
            countdown_announced: [false; 5],
        }
    }

    /// Refresh the timer (restart from now)
    pub fn refresh(&mut self, event_timestamp: NaiveDateTime) {
        self.started_at = event_timestamp;
        self.started_instant = Instant::now();
        self.expires_at = event_timestamp
            + chrono::Duration::milliseconds(self.duration.as_millis() as i64);
        self.alert_fired = false;
        self.countdown_announced = [false; 5];
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

    /// Get remaining time in seconds
    pub fn remaining_secs(&self, current_game_time: NaiveDateTime) -> f32 {
        let remaining = self.expires_at.signed_duration_since(current_game_time);
        (remaining.num_milliseconds().max(0) as f32) / 1000.0
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

    /// Check for countdown seconds to announce (5, 4, 3, 2, 1)
    ///
    /// Returns Some(seconds) if we've crossed into a new second boundary
    /// that hasn't been announced yet. Returns None otherwise.
    ///
    /// This uses floor to determine the current second window:
    /// - remaining 5.2s → announces 5
    /// - remaining 4.8s → announces 4
    pub fn check_countdown(&mut self, current_game_time: NaiveDateTime) -> Option<u8> {
        let remaining = self.remaining_secs(current_game_time);

        // Floor to get whole seconds remaining
        let remaining_floor = remaining.floor() as i32;

        // Only announce 5, 4, 3, 2, 1 (index 0-4)
        if remaining_floor >= 1 && remaining_floor <= 5 {
            let seconds = remaining_floor as u8;
            // Index: 1 → 0, 2 → 1, 3 → 2, 4 → 3, 5 → 4
            let index = (seconds - 1) as usize;

            if !self.countdown_announced[index] {
                self.countdown_announced[index] = true;
                return Some(seconds);
            }
        }

        None
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
