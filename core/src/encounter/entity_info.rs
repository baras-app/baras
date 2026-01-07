use crate::combat_log::EntityType;
use crate::context::{IStr, empty_istr};
use chrono::NaiveDateTime;

#[derive(Debug, Clone)]
pub struct PlayerInfo {
    pub name: IStr,
    pub id: i64,
    pub class_id: i64,
    pub class_name: String,
    pub discipline_id: i64,
    pub discipline_name: String,
    pub is_dead: bool,
    pub death_time: Option<NaiveDateTime>,
}

impl Default for PlayerInfo {
    fn default() -> Self {
        PlayerInfo {
            name: empty_istr(),
            id: 0,
            class_id: 0,
            class_name: String::new(),
            discipline_id: 0,
            discipline_name: String::new(),
            is_dead: false,
            death_time: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NpcInfo {
    pub name: IStr,
    pub entity_type: EntityType,
    pub display_name: Option<String>,
    pub log_id: i64,
    pub class_id: i64,
    pub is_dead: bool,
    pub first_seen_at: Option<NaiveDateTime>,
    pub death_time: Option<NaiveDateTime>,
}

impl Default for NpcInfo {
    fn default() -> Self {
        NpcInfo {
            name: empty_istr(),
            entity_type: EntityType::Npc,
            display_name: None,
            log_id: 0,
            class_id: 0,
            is_dead: false,
            first_seen_at: None,
            death_time: None,
        }
    }
}
