use std::borrow;

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
pub struct Timestamp {
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub millis: u16,
}

#[derive(Debug, Clone, Default)]
pub struct Entity {
    pub name: String,
    pub class_id: i64,
    pub log_id: i64,
    pub entity_type: EntityType,
    pub health: (i32, i32),
}

#[derive(Debug, Clone, Default)]
pub struct CombatEvent {
    pub line_number: usize,
    pub timestamp: Timestamp,
    pub source_entity: Entity,
    pub target_entity: Entity,
    pub action_id: Option<String>,
    pub action_name: Option<String>,
    pub effect_type_id: Option<String>,
    pub effect_type_name: Option<String>,
    pub effect_id: Option<String>,
    pub effect_name: Option<String>,
    pub charges: Option<i64>,
    pub damage: Option<i64>,
    pub effective_damage: Option<i64>,
    pub damage_type_id: Option<String>,
    pub is_critical: Option<bool>,
    pub is_reflected: Option<bool>,
    pub threat: Option<f64>,
    pub reduction_class_id: Option<String>,
    pub damage_reduced: Option<String>,
    pub reduction_type_id: Option<String>,
    pub heal: Option<i64>,
    pub effective_heal: Option<i64>,
}
