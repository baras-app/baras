//! Training dummy parse summary: the numbers Parsely ranks plus GCD delays

use serde::{Deserialize, Serialize};

use super::RatingSource;

/// A press is drawn as long when the next one follows this far past the
/// parse's typical GCD
pub const LONG_GCD_MARGIN_SECS: f32 = 0.2;

/// One on-GCD press and how long until the next one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GcdPress {
    /// Combat time `ability` was pressed
    pub time_secs: f32,
    /// Time until `next` was pressed
    pub interval_secs: f32,
    pub ability: String,
    pub ability_id: i64,
    pub next: String,
    pub next_id: i64,
}

/// Time until the next press, across every press of one ability.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AbilityInterval {
    pub ability_id: i64,
    pub ability_name: String,
    pub uses: u32,
    pub median_secs: f32,
    pub longest_secs: f32,
}

/// A player's on-GCD presses and the time between them, as logged: nothing
/// is assumed about how long an ability should take.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GcdAnalysis {
    /// On-GCD abilities activated
    pub gcd_count: u32,
    /// Median time between two on-GCD activations
    pub typical_gcd_secs: f32,
    /// Time between each two consecutive on-GCD activations, ascending
    pub intervals_secs: Vec<f32>,
    /// Every press but the last, chronological
    pub presses: Vec<GcdPress>,
    /// Most used first
    pub abilities: Vec<AbilityInterval>,
}

impl GcdAnalysis {
    /// `interval_secs` runs well past the typical GCD
    pub fn is_long(&self, interval_secs: f32) -> bool {
        interval_secs >= self.typical_gcd_secs + LONG_GCD_MARGIN_SECS
    }
}

/// Percentiles of a full dummy kill among Parsely's rated parses.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DummyPercentiles {
    pub dps: u8,
    pub crit: u8,
    pub crit_damage: u8,
    pub apm: u8,
    pub ttk: u8,
    /// Parses the discipline's percentiles are drawn from (mirror included)
    pub count: u32,
    pub source: RatingSource,
}

/// One player's training dummy parse.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DummyParse {
    pub dps: i64,
    pub apm: f32,
    /// Share of damaging hits that crit, 0-100
    pub crit_pct: f32,
    /// Share of damage dealt by crits, 0-100
    pub crit_damage_pct: f32,
    pub duration_secs: i64,
    /// The dummy died, so the parse can be rated
    pub killed: bool,
    /// None when the parse isn't a kill or the rating table couldn't be had
    pub percentiles: Option<DummyPercentiles>,
    pub gcd: GcdAnalysis,
}
