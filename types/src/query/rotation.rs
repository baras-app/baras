//! Rotation analysis, ability usage and encounter timeline

use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// Rotation Analysis Types
// ─────────────────────────────────────────────────────────────────────────────

/// A single ability activation event for rotation analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RotationEvent {
    pub time_secs: f32,
    pub ability_id: i64,
    pub ability_name: String,
}

/// A GCD slot: one on-GCD ability plus any off-GCD abilities weaved with it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GcdSlot {
    pub gcd_ability: RotationEvent,
    pub off_gcd: Vec<RotationEvent>,
    /// Seconds since the previous GCD activation (None for the first slot).
    pub gcd_gap: Option<f32>,
}

/// One rotation cycle (anchor-to-anchor).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RotationCycle {
    pub slots: Vec<GcdSlot>,
    pub duration_secs: f32,
    pub total_damage: f64,
    pub effective_heal: f64,
    pub crit_count: i64,
    pub hit_count: i64,
}

/// Full rotation analysis result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RotationAnalysis {
    pub cycles: Vec<RotationCycle>,
    /// Distinct abilities for the anchor dropdown: (ability_id, ability_name).
    pub abilities: Vec<(i64, String)>,
}

/// Per-ability usage statistics for a single player.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AbilityUsageRow {
    pub ability_name: String,
    pub ability_id: i64,
    pub cast_count: i64,
    /// Seconds from combat start of first cast.
    pub first_cast_secs: f32,
    /// Seconds from combat start of last cast.
    pub last_cast_secs: f32,
    /// Average time between consecutive casts (0.0 if < 2 casts).
    pub avg_time_between: f32,
    /// Median time between consecutive casts (0.0 if < 2 casts).
    pub median_time_between: f32,
    /// Minimum time between consecutive casts (0.0 if < 2 casts).
    pub min_time_between: f32,
    /// Maximum time between consecutive casts (0.0 if < 2 casts).
    pub max_time_between: f32,
    /// Raw cast timestamps (combat_time_secs) for timeline visualization.
    pub timestamps: Vec<f32>,
    /// A defensive cooldown
    #[serde(default)]
    pub is_defensive: bool,
}

/// A phase segment - one occurrence of a phase (phases can repeat).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhaseSegment {
    pub phase_id: String,
    pub phase_name: String,
    pub instance: i64,
    pub start_secs: f32,
    pub end_secs: f32,
}

/// Encounter timeline with duration and phase segments.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EncounterTimeline {
    pub duration_secs: f32,
    pub phases: Vec<PhaseSegment>,
}

/// Time range filter for queries (in seconds from combat start).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct TimeRange {
    pub start: f32,
    pub end: f32,
}

impl TimeRange {
    pub fn new(start: f32, end: f32) -> Self {
        Self { start, end }
    }

    pub fn full(duration: f32) -> Self {
        Self {
            start: 0.0,
            end: duration,
        }
    }

    pub fn is_full(&self, duration: f32) -> bool {
        self.start <= 0.01 && (self.end - duration).abs() < 0.01
    }

    /// Generate SQL WHERE clause fragment for filtering by time range.
    pub fn sql_filter(&self) -> String {
        format!(
            "combat_time_secs >= {} AND combat_time_secs <= {}",
            self.start, self.end
        )
    }
}
