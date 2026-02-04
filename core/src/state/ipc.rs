//! Shared IPC types for communication between parse-worker and app.
//!
//! These types define the contract for subprocess output, ensuring both sides
//! use identical struct definitions for serialization/deserialization.

use crate::context::{intern, resolve};
use crate::encounter::entity_info::PlayerInfo;
use crate::encounter::summary::EncounterSummary;
use crate::state::AreaInfo;
use serde::{Deserialize, Serialize};

/// Player info for IPC (uses plain String instead of IStr for serialization).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerPlayerInfo {
    pub name: String,
    pub class_name: String,
    pub discipline_name: String,
    pub entity_id: i64,
}

impl WorkerPlayerInfo {
    /// Create from internal PlayerInfo (for worker output).
    pub fn from_player(player: &PlayerInfo) -> Self {
        Self {
            name: resolve(player.name).to_string(),
            class_name: player.class_name.clone(),
            discipline_name: player.discipline_name.clone(),
            entity_id: player.id,
        }
    }

    /// Apply to internal PlayerInfo (for app import).
    /// Returns true if player data was present (non-empty name).
    pub fn apply_to(&self, player: &mut PlayerInfo) -> bool {
        player.name = intern(&self.name);
        player.id = self.entity_id;
        player.class_name = self.class_name.clone();
        player.discipline_name = self.discipline_name.clone();
        !self.name.is_empty()
    }
}

/// Player discipline entry for IPC (all players in session).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerPlayerDiscipline {
    pub entity_id: i64,
    pub name: String,
    pub class_id: i64,
    pub class_name: String,
    pub discipline_id: i64,
    pub discipline_name: String,
}

impl WorkerPlayerDiscipline {
    /// Create from internal PlayerInfo (for worker output).
    pub fn from_player(player: &PlayerInfo) -> Self {
        Self {
            entity_id: player.id,
            name: resolve(player.name).to_string(),
            class_id: player.class_id,
            class_name: player.class_name.clone(),
            discipline_id: player.discipline_id,
            discipline_name: player.discipline_name.clone(),
        }
    }

    /// Convert to internal PlayerInfo (for app import).
    pub fn to_player_info(&self) -> PlayerInfo {
        PlayerInfo {
            id: self.entity_id,
            name: intern(&self.name),
            class_id: self.class_id,
            class_name: self.class_name.clone(),
            discipline_id: self.discipline_id,
            discipline_name: self.discipline_name.clone(),
            is_dead: false,
            death_time: None,
            received_revive_immunity: false,
            current_target_id: 0,
            last_seen_at: None,
        }
    }
}

/// Area info for IPC (subset of AreaInfo fields needed for restore).
/// Note: AreaInfo itself now has Serialize/Deserialize, but we use this
/// wrapper to control exactly which fields are sent over IPC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerAreaInfo {
    pub area_name: String,
    pub area_id: i64,
    pub difficulty_id: i64,
    pub difficulty_name: String,
    pub entered_at_line: Option<u64>,
}

impl WorkerAreaInfo {
    /// Create from internal AreaInfo (for worker output).
    pub fn from_area(area: &AreaInfo) -> Self {
        Self {
            area_name: area.area_name.clone(),
            area_id: area.area_id,
            difficulty_id: area.difficulty_id,
            difficulty_name: area.difficulty_name.clone(),
            entered_at_line: area.entered_at_line,
        }
    }

    /// Apply to internal AreaInfo (for app import).
    pub fn apply_to(&self, area: &mut AreaInfo) {
        area.area_name = self.area_name.clone();
        area.area_id = self.area_id;
        area.difficulty_id = self.difficulty_id;
        area.difficulty_name = self.difficulty_name.clone();
        area.entered_at_line = self.entered_at_line;
    }
}

/// Output from the parse worker subprocess.
///
/// This is the main IPC contract between parse-worker and app.
/// Both sides use this exact struct for serialization/deserialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParseWorkerOutput {
    /// Final byte position in the file (for tailing).
    /// If there's an incomplete encounter, this is the byte position where it started.
    /// Otherwise, this is the end of the file.
    pub end_pos: u64,
    /// Final line number parsed (for correct line numbering during tailing).
    pub line_count: u64,
    /// Number of events parsed.
    pub event_count: usize,
    /// Number of encounters written.
    pub encounter_count: usize,
    /// Encounter summaries for the main process.
    pub encounters: Vec<EncounterSummary>,
    /// Player info at end of file.
    pub player: WorkerPlayerInfo,
    /// Area info at end of file.
    pub area: WorkerAreaInfo,
    /// Player disciplines for all players in session (for Data Explorer enrichment).
    pub player_disciplines: Vec<WorkerPlayerDiscipline>,
    /// Elapsed time in milliseconds.
    pub elapsed_ms: u128,
}
