//! Storage module for writing combat events to Parquet files.
//!
//! Each encounter is written to a separate parquet file with denormalized metadata.
//! Files are named `{encounter_idx:04}.parquet` (e.g., 0001.parquet, 0002.parquet).

mod writer;

pub use writer::{EncounterWriter, EventMetadata, EventRow};

use std::path::PathBuf;

/// Get the encounters storage directory for a session.
/// Creates `~/.local/share/baras/encounters/{session_id}/` if it doesn't exist.
pub fn encounters_dir(session_id: &str) -> std::io::Result<PathBuf> {
    let base = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("baras")
        .join("encounters")
        .join(session_id);

    std::fs::create_dir_all(&base)?;
    Ok(base)
}

/// Generate parquet filename for an encounter.
pub fn encounter_filename(encounter_idx: u32) -> String {
    format!("{:04}.parquet", encounter_idx)
}
