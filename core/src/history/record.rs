//! The persisted shape of one pull.
//!
//! `EncounterSummary` is the worker → app transfer object and carries every
//! live-UI field; `PullRecord` is the subset the History tab needs, so the
//! live struct can change without a store version bump. Built once per pull
//! via `From<&EncounterSummary>`.

use serde::{Deserialize, Serialize};

use baras_types::history::{PullDeath, PullEnd, PullProgress};

use crate::encounter::summary::{ChallengeSummary, EncounterSummary};
use crate::encounter::{PhaseType, PvpOutcome};
use crate::game_data::Discipline;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRecord {
    /// Stable key within the log: combat start as milliseconds since the Unix
    /// epoch, taking the log's local timestamps at face value. Survives
    /// re-parses and definition changes, unlike `encounter_id`.
    pub start_ms: i64,
    /// Session ordinal, only a hint for "open in Data Explorer"
    pub encounter_id: u64,
    /// ISO 8601 local time (empty when combat never started)
    pub start_time: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_time: Option<String>,
    pub duration_secs: i64,
    pub kind: PhaseType,
    pub area_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub difficulty: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boss_name: Option<String>,
    /// Fallback label when there is no boss (dummy parses)
    pub display_name: String,
    pub success: bool,
    #[serde(default)]
    pub end_reason: PullEnd,
    #[serde(default)]
    pub partial: bool,
    pub raid_size: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<PullProgress>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deaths: Vec<PullDeath>,
    pub roster: Vec<RosterLine>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub challenges: Vec<ChallengeLine>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pvp_outcome: Option<PvpOutcome>,
    #[serde(default)]
    pub is_warzone: bool,
}

/// One player's numbers on the pull.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RosterLine {
    pub entity_id: i64,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discipline: Option<Discipline>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discipline_name: Option<String>,
    pub dps: i64,
    pub edps: i64,
    pub bossdps: i64,
    pub hps: i64,
    pub ehps: i64,
    pub dtps: i64,
    pub apm: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeLine {
    pub name: String,
    pub metric: String,
    pub total_value: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub per_second: Option<f32>,
    /// `(entity_id, value)` sorted by value desc
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub by_player: Vec<(i64, i64)>,
}

/// Summary timestamps are `%Y-%m-%dT%H:%M:%S`; parsed as naive local time.
pub fn start_ms_of(iso: &str) -> Option<i64> {
    chrono::NaiveDateTime::parse_from_str(iso, "%Y-%m-%dT%H:%M:%S")
        .ok()
        .map(|t| t.and_utc().timestamp_millis())
}

impl From<&EncounterSummary> for PullRecord {
    fn from(s: &EncounterSummary) -> Self {
        let start_time = s.start_time.clone().unwrap_or_default();
        // A pull with no start time (should not happen for recorded pulls)
        // still needs a deterministic key; the end time is the next best.
        let start_ms = start_ms_of(&start_time)
            .or_else(|| s.end_time.as_deref().and_then(start_ms_of))
            .unwrap_or(s.encounter_id as i64);
        Self {
            start_ms,
            encounter_id: s.encounter_id,
            start_time,
            end_time: s.end_time.clone(),
            duration_secs: s.duration_seconds,
            kind: s.encounter_type,
            area_name: s.area_name.clone(),
            difficulty: s.difficulty.clone(),
            boss_name: s.boss_name.clone(),
            display_name: s.display_name.clone(),
            success: s.success,
            end_reason: s.end_reason,
            partial: s.partial,
            raid_size: s.raid_size,
            progress: s.progress.clone(),
            deaths: s.deaths.clone(),
            roster: s
                .player_metrics
                .iter()
                .map(|m| RosterLine {
                    entity_id: m.entity_id,
                    name: m.name.clone(),
                    discipline: m.discipline,
                    discipline_name: m.discipline_name.clone(),
                    dps: m.dps,
                    edps: m.edps,
                    bossdps: m.bossdps,
                    hps: m.hps,
                    ehps: m.ehps,
                    dtps: m.dtps,
                    apm: m.apm,
                })
                .collect(),
            challenges: s.challenges.iter().map(ChallengeLine::from).collect(),
            pvp_outcome: s.pvp_outcome,
            is_warzone: s.is_warzone,
        }
    }
}

impl From<&ChallengeSummary> for ChallengeLine {
    fn from(c: &ChallengeSummary) -> Self {
        Self {
            name: c.name.clone(),
            metric: c.metric.clone(),
            total_value: c.total_value,
            per_second: c.per_second,
            by_player: c.by_player.iter().map(|p| (p.entity_id, p.value)).collect(),
        }
    }
}
