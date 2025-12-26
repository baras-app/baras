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

use super::EffectCategory;
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

    // ─── Entities ───────────────────────────────────────────────────────────
    /// Entity ID of who applied this effect
    pub source_entity_id: i64,

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

    /// Show on raid frames overlay
    pub show_on_raid_frames: bool,

    /// Show on effects countdown overlay
    pub show_on_effects_overlay: bool,
}

#[allow(clippy::too_many_arguments)]
impl ActiveEffect {
    /// Create a new active effect
    pub fn new(
        definition_id: String,
        game_effect_id: u64,
        name: String,
        source_entity_id: i64,
        target_entity_id: i64,
        target_name: IStr,
        is_from_local_player: bool,
        event_timestamp: NaiveDateTime,
        duration: Option<Duration>,
        color: [u8; 4],
        category: EffectCategory,
        show_on_raid_frames: bool,
        show_on_effects_overlay: bool,
    ) -> Self {
        let now = Instant::now();
        let expires_at = duration.map(|d| {
            event_timestamp + chrono::Duration::milliseconds(d.as_millis() as i64)
        });

        Self {
            definition_id,
            game_effect_id,
            name,
            source_entity_id,
            target_entity_id,
            target_name,
            is_from_local_player,
            applied_at: event_timestamp,
            applied_instant: now,
            expires_at,
            last_refreshed_at: event_timestamp,
            duration,
            removed_at: None,
            stacks: 1,
            color,
            category,
            show_on_raid_frames,
            show_on_effects_overlay,
        }
    }

    /// Refresh the effect (reapplication extends duration)
    pub fn refresh(&mut self, event_timestamp: NaiveDateTime, duration: Option<Duration>) {
        self.last_refreshed_at = event_timestamp;

        if let Some(d) = duration {
            self.expires_at = Some(
                event_timestamp + chrono::Duration::milliseconds(d.as_millis() as i64)
            );
            self.duration = Some(d);
            // Update system time instant so overlay expiry calculation is correct
            self.applied_instant = Instant::now();
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
    pub fn is_near_expiration(&self, current_game_time: NaiveDateTime, threshold_secs: f32) -> bool {
        self.remaining_secs(current_game_time)
            .map(|r| r <= threshold_secs && r > 0.0)
            .unwrap_or(false)
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
