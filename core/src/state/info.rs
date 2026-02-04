use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AreaInfo {
    pub area_name: String,
    pub area_id: i64,
    pub difficulty_id: i64,
    pub difficulty_name: String,
    pub entered_at: Option<NaiveDateTime>,
    /// Line number of the AreaEntered event in the combat log.
    /// Used for per-encounter Parsely uploads.
    pub entered_at_line: Option<u64>,
    /// Monotonic counter incremented on every area transition (even re-entering the same area).
    /// Used to detect phase boundaries for encounter history grouping.
    pub generation: u64,
}
