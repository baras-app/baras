use chrono::NaiveDateTime;

#[derive(Debug, Clone, Default)]
pub struct AreaInfo {
    pub area_name: String,
    pub area_id: i64,
    pub difficulty_id: i64,
    pub difficulty_name: String,
    pub entered_at: Option<NaiveDateTime>,
}
