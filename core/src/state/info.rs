use chrono::NaiveDateTime;

#[derive(Debug, Clone, Default)]
pub struct AreaInfo {
    pub area_name: String,
    pub area_id: i64,
    pub difficulty_id: i64,
    pub difficulty_name: String,
    pub entered_at: Option<NaiveDateTime>,
    /// Monotonic counter incremented on every area transition (even re-entering the same area).
    /// Used to detect phase boundaries for encounter history grouping.
    pub generation: u64,
}
