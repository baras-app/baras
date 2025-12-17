use chrono::NaiveDateTime;

#[derive(Debug, Clone, Default)]
pub struct Action {
    pub name: String,
    pub action_id: i64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum EntityType {
    Player,
    Npc,
    Companion,
    #[default]
    Empty,
    SelfReference,
}

#[derive(Debug, Clone, Default)]
pub struct Entity {
    pub name: String,
    pub class_id: i64,
    pub log_id: i64,
    pub entity_type: EntityType,
    pub health: (i32, i32),
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

#[derive(Debug, Clone, Default)]
pub struct Effect {
    pub type_name: String,
    pub type_id: i64,
    pub effect_name: String,
    pub effect_id: i64,
    pub difficulty_name: String,
    pub difficulty_id: i64,
    pub discipline_name: String,
    pub discipline_id: i64,
}

#[derive(Debug, Clone, Default)]
pub struct Details {
    pub dmg_amount: i32,
    pub is_crit: bool,
    pub is_reflect: bool,
    pub dmg_effective: i32,
    pub dmg_type: String,
    pub dmg_type_id: i64,
    pub avoid_type: String,
    pub dmg_absorbed: i32,
    pub threat: f32,
    pub heal_amount: i32,
    pub heal_effective: i32,
    pub charges: i32,
    pub ability_id: i64,
    pub spend: f32,
}
