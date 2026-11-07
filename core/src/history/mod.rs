//! Cross-session pull history.
//!
//! Boss pulls and dummy parses are persisted as one JSON file per combat log
//! under `~/.config/baras/history/` (`store`), so the History tab can compare
//! pulls across nights without re-parsing logs. `index` holds compact rows
//! over every file; `meta` is the separate user-owned store (favorites, tags,
//! notes, Parsely links) that a re-parse never rewrites.

mod catalog;
mod index;
mod meta;
mod record;
mod runs;
mod store;

use std::path::PathBuf;

use crate::encounter::PhaseType;
use crate::encounter::summary::EncounterSummary;

pub use catalog::AreaCatalog;
pub use index::{LogRef, PullIndex, RefreshReport};
pub use runs::{difficulty_parts, run_detail};
pub use meta::{LibraryMeta, meta_path, now_iso, pull_key};
pub use record::{ChallengeLine, PullRecord, RosterLine, start_ms_of};
pub use store::{
    HISTORY_VERSION, HistoryHeader, LogHistory, history_file_for, history_file_name, mtime_secs,
};

/// Tree group for training dummy parses (they have no boss or operation)
pub const DUMMY_GROUP: &str = "Training Dummy";

/// `~/.config/baras/history/` (created on demand)
pub fn history_dir() -> std::io::Result<PathBuf> {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("baras")
        .join("history");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Boss pulls (any content type except PvP) and training dummy parses.
pub fn is_recordable(summary: &EncounterSummary) -> bool {
    match summary.encounter_type {
        PhaseType::DummyParse => true,
        PhaseType::PvP => false,
        _ => summary.boss_name.is_some(),
    }
}
