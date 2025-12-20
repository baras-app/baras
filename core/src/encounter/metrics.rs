use super::IStr;
use super::EntityType;

#[derive(Debug, Clone, Default)]
pub struct MetricAccumulator {
    // Damage dealing
    pub damage_dealt: i64,
    pub damage_dealt_effective: i64,
    pub damage_hit_count: u32,
    pub damage_crit_count: u32,

    // Damage receiving
    pub damage_received: i64,
    pub damage_received_effective: i64,
    pub damage_absorbed: i64,
    pub attacks_received: u32,

    // Defense stats (dodge/parry/resist/deflect)
    pub defense_count: u32,
    // Natural shield rolls (tank stat, not effect shields)
    pub shield_roll_count: u32,
    pub shield_roll_absorbed: i64,

    // Healing given
    pub healing_done: i64,
    pub healing_effective: i64,
    pub heal_count: u32,
    pub heal_crit_count: u32,

    // Healing received
    pub healing_received: i64,
    pub healing_received_effective: i64,

    // Effect shielding (Static Barrier, etc.)
    pub shielding_given: i64,

    // General
    pub actions: u32,
    pub threat_generated: f64,
    pub taunt_count: u32,
}


#[derive(Debug, Clone)]
pub struct EntityMetrics {
    pub entity_id: i64,
    pub name: IStr,
    pub entity_type: EntityType,

    // Damage dealing
    pub total_damage: i64,
    pub total_damage_effective: i64,
    pub dps: i32,
    pub edps: i32,
    pub damage_crit_pct: f32,

    // Healing dealing
    pub total_healing: i64,
    pub total_healing_effective: i64,
    pub hps: i32,
    pub ehps: i32,
    pub heal_crit_pct: f32,
    pub effective_heal_pct: f32,

    // Shielding (effect shields like Static Barrier)
    pub abs: i32,
    pub total_shielding: i64,

    // Damage taken
    pub total_damage_taken: i64,
    pub total_damage_taken_effective: i64,
    pub dtps: i32,
    pub edtps: i32,

    // Healing received
    pub htps: i32,
    pub ehtps: i32,

    // Tank stats
    pub defense_pct: f32,
    pub shield_pct: f32,
    pub total_shield_absorbed: i64,
    pub taunt_count: u32,

    // General
    pub apm: f32,
    pub tps: i32,
    pub total_threat: i64,
}
