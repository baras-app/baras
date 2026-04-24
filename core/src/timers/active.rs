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

use std::time::Duration;

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

    /// Whether to fire an alert when the timer expires (from alert_on == OnExpire)
    pub alert_on_expire: bool,

    /// Custom alert text (from definition, None = use timer name)
    pub alert_text: Option<String>,

    // ─── Display (cached from definition) ───────────────────────────────────
    /// RGBA color for display
    pub color: [u8; 4],

    /// Optional ability ID for icon display on the timer bar
    pub icon_ability_id: Option<u64>,

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

    /// Which overlay should display this timer
    pub display_target: crate::timers::TimerDisplayTarget,

    /// Whether this timer is hidden due to role filtering
    /// (still ticks/chains/expires, but produces no visual or audio output)
    pub role_hidden: bool,

    // ─── State ──────────────────────────────────────────────────────────────
    /// Whether this timer is in "queued/ready" state (held at zero, not removed).
    /// Set to true when a timer with `queue_on_expire` reaches zero.
    pub is_queued: bool,

    /// Cached from definition: if true, hold at zero instead of removing on expire.
    pub queue_on_expire: bool,

    /// Cached from definition: sort priority for queued entries (higher = higher priority).
    pub queue_priority: u8,

    /// Cached from definition: names of other timers in the same encounter
    /// that block this ability queue entry from being considered ready.
    pub queue_blocking_timers: Vec<String>,

    /// Cached from definition: render this timer's ability-queue row as a
    /// trickling-down bar instead of the default filling-up progress bar.
    pub queue_countdown_bar: bool,

    /// Cached from definition: never mark this timer as a "next cast"
    /// candidate. Used for display-only rows that aren't castable abilities.
    pub queue_hide_from_next: bool,
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
        icon_ability_id: Option<u64>,
        triggers_timer: Option<String>,
        show_on_raid_frames: bool,
        show_at_secs: f32,
        audio: &AudioConfig,
        display_target: crate::timers::TimerDisplayTarget,
        alert_on_expire: bool,
        alert_text: Option<String>,
        role_hidden: bool,
        queue_on_expire: bool,
        queue_priority: u8,
        queue_blocking_timers: Vec<String>,
        queue_countdown_bar: bool,
        queue_hide_from_next: bool,
    ) -> Self {
        let expires_at =
            event_timestamp + chrono::Duration::milliseconds(duration.as_millis() as i64);

        Self {
            definition_id,
            name,
            target_entity_id,
            started_at: event_timestamp,
            expires_at,
            duration,
            repeat_count: 0,
            max_repeats,
            alert_fired: false,
            alert_on_expire,
            alert_text,
            color,
            icon_ability_id,
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
            display_target,
            role_hidden,
            is_queued: false,
            queue_on_expire,
            queue_priority,
            queue_blocking_timers,
            queue_countdown_bar,
            queue_hide_from_next,
        }
    }

    /// Refresh the timer (restart from event timestamp)
    pub fn refresh(&mut self, event_timestamp: NaiveDateTime) {
        self.started_at = event_timestamp;
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

    /// Check if timer should be visible based on show_at_secs threshold
    ///
    /// Returns true if:
    /// - show_at_secs is 0 (always show), OR
    /// - remaining time is at or below show_at_secs threshold
    ///
    /// `remaining` is the pre-computed remaining seconds from the manager's
    /// interpolated game time (computed once per tick for all timers).
    pub fn is_visible(&self, remaining: f32) -> bool {
        if self.show_at_secs <= 0.0 {
            return true; // 0 means always show
        }
        remaining <= self.show_at_secs
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
    /// `remaining` is the pre-computed remaining seconds from the manager's
    /// interpolated game time (computed once per tick for all timers).
    ///
    /// Announces N when remaining is in [N, N+0.3) to sync with visual display:
    /// - remaining 3.8s → no announcement (too early)
    /// - remaining 3.2s → announces 3 (in window [3.0, 3.3))
    /// - remaining 2.2s → announces 2
    pub fn check_countdown(&mut self, remaining: f32) -> Option<u8> {
        // 0 means countdown disabled for this timer
        if self.countdown_start == 0 {
            return None;
        }

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
    /// `remaining` is the pre-computed remaining seconds from the manager's
    /// interpolated game time (computed once per tick for all timers).
    pub fn check_audio_offset(&mut self, remaining: f32) -> bool {
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

        // Fire when we cross into the offset window
        if remaining <= self.audio_offset as f32 && remaining > 0.0 {
            self.audio_offset_fired = true;
            return true;
        }

        false
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Active GCD
// ═══════════════════════════════════════════════════════════════════════════

/// A synthetic GCD countdown created when an ability-queue timer fires.
/// Contains only timing fields — the overlay uses its own configurable accent color.
#[derive(Debug, Clone)]
pub struct ActiveGcd {
    /// When the GCD started (game time)
    pub started_at: NaiveDateTime,
    /// When the GCD expires (game time)
    pub expires_at: NaiveDateTime,
}

impl ActiveGcd {
    pub fn new(started_at: NaiveDateTime, gcd_secs: f32) -> Self {
        let expires_at = started_at
            + chrono::Duration::milliseconds((gcd_secs * 1000.0) as i64);
        Self {
            started_at,
            expires_at,
        }
    }

    /// Check if the GCD has expired
    pub fn has_expired(&self, current_time: NaiveDateTime) -> bool {
        current_time >= self.expires_at
    }

    /// Get remaining seconds
    pub fn remaining_secs(&self, current_time: NaiveDateTime) -> f32 {
        let remaining = self.expires_at.signed_duration_since(current_time);
        (remaining.num_milliseconds().max(0) as f32) / 1000.0
    }

    /// Get fill percentage (1.0 = full, 0.0 = expired)
    pub fn fill_percent(&self, current_time: NaiveDateTime) -> f32 {
        let total = self.expires_at.signed_duration_since(self.started_at);
        let remaining = self.expires_at.signed_duration_since(current_time);
        let total_ms = total.num_milliseconds() as f32;
        let remaining_ms = remaining.num_milliseconds().max(0) as f32;
        if total_ms > 0.0 {
            (remaining_ms / total_ms).clamp(0.0, 1.0)
        } else {
            0.0
        }
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
