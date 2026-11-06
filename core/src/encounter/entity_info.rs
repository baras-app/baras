use crate::combat_log::EntityType;
use crate::context::{empty_istr, IStr};
use chrono::NaiveDateTime;

#[derive(Debug, Clone)]
pub struct PlayerInfo {
    pub name: IStr,
    pub id: i64,
    pub class_id: i64,
    pub class_name: String,
    pub discipline_id: i64,
    pub discipline_name: String,
    /// Discipline was inferred from a signature ability rather than a
    /// DisciplineChanged event. Inferred assignments are evicted on area
    /// change (players can respec between PvP matches).
    pub discipline_inferred: bool,
    pub is_dead: bool,
    pub death_time: Option<NaiveDateTime>,
    pub received_revive_immunity: bool,
    pub current_target_id: i64,
    /// Last entity this player damaged — target fallback when TargetSet
    /// events don't cover them (enemy players in PvP)
    pub last_offensive_target_id: i64,
    /// First time this player appeared in an event (stable slot ordering)
    pub first_seen_at: Option<NaiveDateTime>,
    /// Last time this player was seen in an event (for filtering stale players)
    pub last_seen_at: Option<NaiveDateTime>,
    /// Most recent health reading from the log.
    pub current_hp: i32,
    /// Most recent max health from the log.
    pub max_hp: i32,
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
            discipline_inferred: false,
            is_dead: false,
            death_time: None,
            received_revive_immunity: false,
            current_target_id: 0,
            last_offensive_target_id: 0,
            first_seen_at: None,
            last_seen_at: None,
            current_hp: 0,
            max_hp: 0,
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
    pub is_boss: bool,
    pub first_seen_at: Option<NaiveDateTime>,
    pub death_time: Option<NaiveDateTime>,
    pub current_hp: i32,
    pub max_hp: i32,
    /// Lowest HP % seen this encounter and the combat second it happened
    /// (pull progress for the History tab)
    pub lowest_hp_pct: f32,
    pub lowest_hp_at_secs: f32,
    pub current_target_id: i64,
    /// Once this NPC has qualified for the boss HP overlay (took damage or
    /// gained a shield), keep showing it even if it heals back to full HP.
    pub is_displayed: bool,
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
            is_boss: false,
            first_seen_at: None,
            death_time: None,
            current_hp: 0,
            max_hp: 0,
            lowest_hp_pct: 100.0,
            lowest_hp_at_secs: 0.0,
            current_target_id: 0,
            is_displayed: false,
        }
    }
}

impl NpcInfo {
    #[inline]
    pub fn hp_percent(&self) -> f32 {
        if self.max_hp > 0 {
            (self.current_hp as f32 / self.max_hp as f32) * 100.0
        } else {
            100.0
        }
    }
}
