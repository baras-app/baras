//! DTOs for Tauri IPC

use std::path::PathBuf;

// ─────────────────────────────────────────────────────────────────────────────
// DTOs for Tauri IPC
// ─────────────────────────────────────────────────────────────────────────────

/// Area visit info for display in file browser
#[derive(Debug, Clone, serde::Serialize)]
pub struct AreaVisitInfo {
    /// Display string: "AreaName Difficulty" (e.g., "Dxun NiM 8")
    pub display: String,
    /// Raw area name
    pub area_name: String,
    /// Difficulty string (may be empty)
    pub difficulty: String,
    /// Badge tier derived from the area and difficulty IDs
    /// ("arena"/"warzone"/"fp"/"nim"/"hm"/"sm"), language-independent; None
    /// for anything else
    pub difficulty_tier: Option<&'static str>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct LogFileInfo {
    pub path: PathBuf,
    pub display_name: String,
    pub character_name: Option<String>,
    pub date: String,
    /// Day of week (e.g., "Sunday")
    pub day_of_week: String,
    pub is_empty: bool,
    pub file_size: u64,
    /// Areas/operations visited in this file (None if not yet indexed)
    pub areas: Option<Vec<AreaVisitInfo>>,
    /// Saved Parsely link from a previous upload
    pub parsely: Option<String>,
    pub link: baras_types::history::LogLink,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionInfo {
    pub player_name: Option<String>,
    pub player_class: Option<String>,
    pub player_discipline: Option<String>,
    /// Discipline icon filename (e.g., "medicine.png") for display
    pub class_icon: Option<String>,
    /// Role icon filename (e.g., "icon_heal.png") for display
    pub role_icon: Option<String>,
    pub area_name: Option<String>,
    pub in_combat: bool,
    pub encounter_count: usize,
    /// Session start time extracted from log filename (formatted as "Jan 18, 3:45 PM")
    pub session_start: Option<String>,
    /// Short start time for inline display (e.g., "3:45 PM")
    pub session_start_short: Option<String>,
    /// Session end time for historical sessions (formatted as "Jan 18, 3:45 PM")
    pub session_end: Option<String>,
    /// Duration formatted as short form (e.g., "47m" or "1h 23m") — always computed
    pub duration_formatted: Option<String>,
    /// True if the log file's last event is older than 30 minutes (no active session)
    pub stale_session: bool,
    /// True if this log file contains events from multiple characters (corrupted)
    pub character_mismatch: bool,
    /// True if the log file started without an AreaEntered event
    pub missing_area: bool,
    /// `missing_area`, and engaging an NPC can still restore the previous
    /// log's area and player (see `AreaResume`)
    pub area_resume_pending: bool,
}
