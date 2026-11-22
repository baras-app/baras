//! Training dummy kills served to the Dummy Parses view. Abandoned parses
//! are not recorded here; everything the view aggregates is derived from
//! these rows in the frontend.

use serde::{Deserialize, Serialize};

/// One parse that killed the dummy, the logging character's numbers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DummyRow {
    pub filename: String,
    /// Log path, present only when the file still exists on disk
    pub path: Option<String>,
    /// Combat start (ms); the row key within the log
    pub start_ms: i64,
    /// Session ordinal, for "open in Data Explorer"
    pub encounter_id: u64,
    pub character: String,
    /// ISO 8601 start time
    pub timestamp: String,
    /// Server display name; empty when unknown
    pub server: String,
    /// Patch live on the parse date
    pub game_version: String,
    /// Max HP of the dummy; 0 when it was not recorded
    pub dummy_hp: i64,
    /// Time to kill
    pub duration_seconds: i64,
    pub discipline: Option<String>,
    /// "Tank" | "Healer" | "Dps"
    pub role: Option<String>,
    pub role_icon: Option<String>,
    pub discipline_icon: Option<String>,
    pub dps: i64,
    pub apm: f32,
    /// Crit rate of damaging hits, 0-100
    pub crit_pct: f32,
    /// Share of damage dealt by crits, 0-100
    pub crit_damage_pct: f32,
    /// Parsely DPS percentile for the discipline; set at query time, None
    /// when the table can't be had or the discipline isn't rated
    #[serde(default)]
    pub dps_pct: Option<u8>,
}
