//! Storage module for writing combat events to Parquet files.
//!
//! Each encounter is written to a separate parquet file with denormalized metadata.
//! Files are named `{encounter_idx:04}.parquet` (e.g., 0001.parquet, 0002.parquet).

pub mod error;
mod writer;

pub use error::StorageError;

pub use writer::{EncounterWriter, EventMetadata, EventRow};

use std::path::PathBuf;

/// Get the data storage directory for parquet files.
/// Creates `~/.config/baras/data/` (or equivalent on Windows/Mac) if it doesn't exist.
pub fn data_dir() -> std::io::Result<PathBuf> {
    let base = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("baras")
        .join("data");

    std::fs::create_dir_all(&base)?;
    Ok(base)
}

/// Get the encounters storage directory for a session.
/// Creates `~/.config/baras/data/{session_id}/` if it doesn't exist.
pub fn encounters_dir(session_id: &str) -> std::io::Result<PathBuf> {
    let base = data_dir()?.join(session_id);
    std::fs::create_dir_all(&base)?;
    Ok(base)
}

/// Clear all data in the data directory.
/// Called on app startup and when switching log files.
pub fn clear_data_dir() -> std::io::Result<()> {
    let dir = data_dir()?;
    if dir.exists() {
        // Remove all contents but keep the directory itself
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                std::fs::remove_dir_all(&path)?;
            } else {
                std::fs::remove_file(&path)?;
            }
        }
    }
    Ok(())
}

/// Generate parquet filename for an encounter.
pub fn encounter_filename(encounter_idx: u32) -> String {
    format!("{:04}.parquet", encounter_idx)
}
