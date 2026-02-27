//! Shared encounter types
//!
//! Data types for encounter history summaries, player metrics, and challenge results.
//! Used by the Data Explorer and other components.

use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// Upload State Tracking
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum UploadState {
    Idle,
    Uploading,
    Success(String), // Contains the Parsely link
    Error(String),   // Contains the error message
}

// ─────────────────────────────────────────────────────────────────────────────
// Data Types (mirrors backend)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerMetrics {
    pub entity_id: i64,
    pub name: String,
    #[serde(default)]
    pub discipline_name: Option<String>,
    #[serde(default)]
    pub class_name: Option<String>,
    #[serde(default)]
    pub class_icon: Option<String>,
    #[serde(default)]
    pub role_icon: Option<String>,
    pub dps: i64,
    pub edps: i64,
    pub bossdps: i64,
    pub total_damage: i64,
    pub total_damage_effective: i64,
    pub total_damage_boss: i64,
    pub damage_crit_pct: f32,
    pub hps: i64,
    pub ehps: i64,
    pub total_healing: i64,
    pub total_healing_effective: i64,
    pub heal_crit_pct: f32,
    pub effective_heal_pct: f32,
    pub tps: i64,
    pub total_threat: i64,
    pub dtps: i64,
    pub edtps: i64,
    pub total_damage_taken: i64,
    pub total_damage_taken_effective: i64,
    pub abs: i64,
    pub total_shielding: i64,
    pub apm: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EncounterSummary {
    pub encounter_id: u64,
    pub display_name: String,
    pub encounter_type: String,
    pub start_time: Option<String>,
    #[serde(default)]
    pub end_time: Option<String>,
    pub duration_seconds: i64,
    pub success: bool,
    pub area_name: String,
    pub difficulty: Option<String>,
    pub boss_name: Option<String>,
    pub player_metrics: Vec<PlayerMetrics>,
    #[serde(default)]
    pub is_phase_start: bool,
    #[serde(default)]
    pub npc_names: Vec<String>,
    #[serde(default)]
    pub challenges: Vec<ChallengeSummary>,
    // Line number tracking for per-encounter Parsely uploads
    #[serde(default)]
    pub area_entered_line: Option<u64>,
    #[serde(default)]
    pub event_start_line: Option<u64>,
    #[serde(default)]
    pub event_end_line: Option<u64>,
    // Parsely link (set after successful upload)
    #[serde(default)]
    pub parsely_link: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChallengeSummary {
    pub name: String,
    pub metric: String,
    pub total_value: i64,
    pub event_count: u32,
    pub duration_secs: f32,
    pub per_second: Option<f32>,
    pub by_player: Vec<ChallengePlayerSummary>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChallengePlayerSummary {
    pub entity_id: i64,
    pub name: String,
    pub value: i64,
    pub percent: f32,
    pub per_second: Option<f32>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper Functions
// ─────────────────────────────────────────────────────────────────────────────

/// Group encounters into sections by area (based on is_phase_start flag or area change)
pub fn group_by_area(
    encounters: &[EncounterSummary],
) -> Vec<(String, Option<String>, Vec<&EncounterSummary>)> {
    let mut sections: Vec<(String, Option<String>, Vec<&EncounterSummary>)> = Vec::new();

    for enc in encounters.iter() {
        // Start new section if: phase start, no sections yet, or area/difficulty changed
        let area_changed = sections
            .last()
            .map_or(false, |s| s.0 != enc.area_name || s.1 != enc.difficulty);

        if enc.is_phase_start || sections.is_empty() || area_changed {
            sections.push((enc.area_name.clone(), enc.difficulty.clone(), vec![enc]));
        } else if let Some(section) = sections.last_mut() {
            section.2.push(enc);
        }
    }

    sections
}
