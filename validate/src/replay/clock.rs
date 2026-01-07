//! Virtual clock for replay timing simulation
//!
//! Supports multiple replay modes:
//! - Realtime (1x speed): Sleep between events for accurate timing
//! - Accelerated: Fast replay with virtual time tracking
//! - Custom speed: Any multiplier (0.5x slow-mo, 10x fast-forward, etc.)

use chrono::NaiveDateTime;
use std::time::{Duration, Instant};

/// Virtual clock that maps game timestamps to system time
#[derive(Debug)]
pub struct VirtualClock {
    /// Combat start time (game time from log)
    combat_start: NaiveDateTime,

    /// Current simulated game time
    current_game_time: NaiveDateTime,

    /// System time when replay started
    replay_started_at: Instant,

    /// Speed multiplier (1.0 = realtime, 0.0 = instant, 10.0 = 10x speed)
    speed_multiplier: f32,
}

impl VirtualClock {
    /// Create a new virtual clock starting at the given game time
    pub fn new(combat_start: NaiveDateTime, speed_multiplier: f32) -> Self {
        Self {
            combat_start,
            current_game_time: combat_start,
            replay_started_at: Instant::now(),
            speed_multiplier,
        }
    }

    /// Create a clock for instant (accelerated) replay
    pub fn instant(combat_start: NaiveDateTime) -> Self {
        Self::new(combat_start, 0.0)
    }

    /// Create a clock for realtime (1x) replay
    pub fn realtime(combat_start: NaiveDateTime) -> Self {
        Self::new(combat_start, 1.0)
    }

    /// Advance the clock to a specific game timestamp, optionally sleeping
    pub fn advance_to(&mut self, game_time: NaiveDateTime) {
        if self.speed_multiplier > 0.0 && game_time > self.current_game_time {
            let delta = game_time - self.current_game_time;
            let delta_ms = delta.num_milliseconds().max(0) as f32;
            let sleep_ms = (delta_ms / self.speed_multiplier) as u64;

            if sleep_ms > 0 {
                std::thread::sleep(Duration::from_millis(sleep_ms));
            }
        }

        self.current_game_time = game_time;
    }

    /// Get elapsed combat time in seconds
    pub fn combat_elapsed_secs(&self) -> f32 {
        let delta = self.current_game_time - self.combat_start;
        delta.num_milliseconds() as f32 / 1000.0
    }

    /// Format combat elapsed time as MM:SS.ms
    pub fn format_combat_time(&self) -> String {
        let secs = self.combat_elapsed_secs();
        let mins = (secs / 60.0).floor() as u32;
        let secs_remainder = secs % 60.0;
        format!("{:02}:{:05.2}", mins, secs_remainder)
    }

    /// Map a game time to an Instant for lag compensation
    ///
    /// This is critical for timer display accuracy. In live mode, timers use
    /// Instant for smooth countdown display. During replay, we need to map
    /// game timestamps to what the Instant would have been.
    pub fn game_time_to_instant(&self, game_time: NaiveDateTime) -> Instant {
        if self.speed_multiplier == 0.0 {
            // Instant mode: all events happen "now"
            return Instant::now();
        }

        let delta = game_time - self.combat_start;
        let delta_ms = delta.num_milliseconds().max(0) as u64;
        let scaled_ms = (delta_ms as f32 / self.speed_multiplier) as u64;

        self.replay_started_at + Duration::from_millis(scaled_ms)
    }

    /// Get the current game time
    pub fn current_game_time(&self) -> NaiveDateTime {
        self.current_game_time
    }

    /// Get combat start time
    pub fn combat_start(&self) -> NaiveDateTime {
        self.combat_start
    }

    /// Check if we're in instant (accelerated) mode
    pub fn is_instant_mode(&self) -> bool {
        self.speed_multiplier == 0.0
    }

    /// Get speed multiplier
    pub fn speed(&self) -> f32 {
        self.speed_multiplier
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn make_time(hour: u32, min: u32, sec: u32, ms: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2025, 1, 1)
            .unwrap()
            .and_hms_milli_opt(hour, min, sec, ms)
            .unwrap()
    }

    #[test]
    fn test_combat_elapsed() {
        let start = make_time(12, 0, 0, 0);
        let mut clock = VirtualClock::instant(start);

        assert_eq!(clock.combat_elapsed_secs(), 0.0);

        clock.advance_to(make_time(12, 0, 30, 0));
        assert_eq!(clock.combat_elapsed_secs(), 30.0);

        clock.advance_to(make_time(12, 1, 15, 500));
        assert!((clock.combat_elapsed_secs() - 75.5).abs() < 0.001);
    }

    #[test]
    fn test_format_combat_time() {
        let start = make_time(12, 0, 0, 0);
        let mut clock = VirtualClock::instant(start);

        clock.advance_to(make_time(12, 0, 15, 230));
        assert_eq!(clock.format_combat_time(), "00:15.23");

        clock.advance_to(make_time(12, 2, 45, 500));
        assert_eq!(clock.format_combat_time(), "02:45.50");
    }

    #[test]
    fn test_instant_mode() {
        let clock = VirtualClock::instant(make_time(12, 0, 0, 0));
        assert!(clock.is_instant_mode());
        assert_eq!(clock.speed(), 0.0);
    }

    #[test]
    fn test_realtime_mode() {
        let clock = VirtualClock::realtime(make_time(12, 0, 0, 0));
        assert!(!clock.is_instant_mode());
        assert_eq!(clock.speed(), 1.0);
    }
}
