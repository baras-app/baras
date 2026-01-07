use crate::context::{IStr, empty_istr};
use chrono::NaiveDateTime;

#[derive(Debug, Clone)]
pub struct Action {
    pub name: IStr,
    pub action_id: i64,
}

impl Default for Action {
    fn default() -> Self {
        Self {
            name: empty_istr(),
            action_id: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Copy)]
pub enum EntityType {
    Player,
    Npc,
    Companion,
    #[default]
    Empty,
    SelfReference,
}

#[derive(Debug, Clone)]
pub struct Entity {
    pub name: IStr,
    pub class_id: i64,
    pub log_id: i64,
    pub entity_type: EntityType,
    pub health: (i32, i32),
}

impl Default for Entity {
    fn default() -> Self {
        Self {
            name: empty_istr(),
            class_id: 0,
            log_id: 0,
            entity_type: EntityType::default(),
            health: (0, 0),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CombatEvent {
    pub line_number: u64,
    pub timestamp: NaiveDateTime,
    pub source_entity: Entity,
    pub target_entity: Entity,
    pub action: Action,
    pub effect: Effect,
    pub details: Details,
}

#[derive(Debug, Clone)]
pub struct Effect {
    pub type_name: IStr,
    pub type_id: i64,
    pub effect_name: IStr,
    pub effect_id: i64,
    pub difficulty_name: IStr,
    pub difficulty_id: i64,
    pub discipline_name: IStr,
    pub discipline_id: i64,
}

impl Default for Effect {
    fn default() -> Self {
        Self {
            type_name: empty_istr(),
            type_id: 0,
            effect_name: empty_istr(),
            effect_id: 0,
            difficulty_name: empty_istr(),
            difficulty_id: 0,
            discipline_name: empty_istr(),
            discipline_id: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Details {
    pub dmg_amount: i32,
    pub is_crit: bool,
    pub is_reflect: bool,
    pub dmg_effective: i32,
    pub dmg_type: IStr,
    pub dmg_type_id: i64,
    pub defense_type_id: i64,
    pub dmg_absorbed: i32,
    pub threat: f32,
    pub heal_amount: i32,
    pub heal_effective: i32,
    pub charges: i32,
    pub ability_id: i64,
    pub spend: f32,
}

impl Default for Details {
    fn default() -> Self {
        Self {
            dmg_amount: 0,
            is_crit: false,
            is_reflect: false,
            dmg_effective: 0,
            dmg_type: empty_istr(),
            dmg_type_id: 0,
            defense_type_id: 0,
            dmg_absorbed: 0,
            threat: 0.0,
            heal_amount: 0,
            heal_effective: 0,
            charges: 0,
            ability_id: 0,
            spend: 0.0,
        }
    }
}
