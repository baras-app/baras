//! PvP matches served to the PvP Statistics view.
//!
//! A warzone is one match. An arena match is best of three rounds, each
//! round its own combat; the index groups the rounds and decides the match
//! (two round wins). Everything the view aggregates (record, maps,
//! disciplines, people) is derived from these rows in the frontend.

use serde::{Deserialize, Serialize};

/// Someone else on the field, relative to the logging character
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PvpPlayer {
    pub name: String,
    pub discipline: Option<String>,
    pub friendly: bool,
}

/// One arena round (a warzone has exactly one)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PvpRound {
    pub encounter_id: u64,
    pub start_ms: i64,
    /// Decided by which team was wiped; `None` for a timeout and for warzones
    pub won: Option<bool>,
    pub duration_seconds: i64,
}

/// One warzone match or best-of-3 arena match, from the logging character's
/// point of view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PvpMatchRow {
    pub filename: String,
    /// Log path, present only when the file still exists on disk
    pub path: Option<String>,
    /// First round's combat start (ms); the row key within the log
    pub start_ms: i64,
    /// First round's session ordinal, for "open in Data Explorer"
    pub encounter_id: u64,
    pub character: String,
    /// ISO 8601 start time
    pub timestamp: String,
    /// Server display name; empty when unknown
    pub server: String,
    /// Patch live on the match date
    pub game_version: String,
    pub area_id: i64,
    /// Map name as the log names it ("The Voidstar")
    pub map: String,
    /// 4-player arena (false: 8-player warzone)
    pub arena: bool,
    /// Arenas: two round wins decide it; `None` when the rounds did not
    /// (timeouts, a quit) and for every warzone (no objective score in the log)
    pub won: Option<bool>,
    pub rounds: Vec<PvpRound>,
    /// Combat time across the rounds
    pub duration_seconds: i64,
    // Logging character
    pub discipline: Option<String>,
    /// "Tank" | "Healer" | "Dps"
    pub role: Option<String>,
    pub role_icon: Option<String>,
    pub discipline_icon: Option<String>,
    /// Everyone else seen in the match, teammates and opponents
    pub players: Vec<PvpPlayer>,
}

impl PvpMatchRow {
    /// `(my round wins, enemy round wins)`
    pub fn score(&self) -> (u8, u8) {
        self.rounds.iter().fold((0, 0), |(w, l), r| match r.won {
            Some(true) => (w + 1, l),
            Some(false) => (w, l + 1),
            None => (w, l),
        })
    }
}
