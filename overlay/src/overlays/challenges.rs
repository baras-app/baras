//! Challenge tracking overlay
//!
//! Displays challenge metrics during boss encounters (damage, healing, counts, etc.)

use std::collections::HashMap;

/// Data for the challenges overlay
#[derive(Debug, Clone, Default)]
pub struct ChallengeData {
    /// Challenge entries to display
    pub entries: Vec<ChallengeEntry>,
    /// Boss encounter name (for header)
    pub boss_name: Option<String>,
    /// Total encounter duration in seconds
    pub duration_secs: f32,
    /// Phase durations (phase_id → seconds)
    pub phase_durations: HashMap<String, f32>,
}

/// Single challenge entry for display
#[derive(Debug, Clone)]
pub struct ChallengeEntry {
    /// Challenge display name
    pub name: String,
    /// Current total value
    pub value: i64,
    /// Number of events contributing
    pub event_count: u32,
    /// Value per second (if time-based)
    pub per_second: Option<f32>,
    /// Per-player breakdown (sorted by value descending)
    pub by_player: Vec<PlayerContribution>,
}

/// A player's contribution to a challenge
#[derive(Debug, Clone)]
pub struct PlayerContribution {
    /// Player entity ID (for linking to encounter data)
    pub entity_id: i64,
    /// Player name (resolved from encounter)
    pub name: String,
    /// Player's value contribution
    pub value: i64,
    /// Percentage of total (0.0-100.0)
    pub percent: f32,
    /// Value per second (if applicable)
    pub per_second: Option<f32>,
}
