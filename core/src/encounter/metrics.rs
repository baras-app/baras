use crate::context::IStr;
use crate::combat_log::EntityType;
use crate::context::resolve;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Default)]
pub struct MetricAccumulator {
    // Damage dealing
    pub damage_dealt: i64,
    pub damge_dealt_boss: i64,
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
    pub total_damage_boss: i64,
    pub total_damage_effective: i64,
    pub dps: i32,
    pub edps: i32,
    pub bossdps: i32,
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

impl EntityMetrics {
    /// Convert to PlayerMetrics for use across crate boundaries
    pub fn to_player_metrics(&self) -> PlayerMetrics {
        PlayerMetrics {
            entity_id: self.entity_id,
            name: resolve(self.name).to_string(),

            // Damage dealing
            dps: self.dps as i64,
            edps: self.edps as i64,
            bossdps: self.bossdps as i64,
            total_damage: self.total_damage,
            total_damage_effective: self.total_damage_effective,
            total_damage_boss: self.total_damage_boss,
            damage_crit_pct: self.damage_crit_pct,

            // Healing
            hps: self.hps as i64,
            ehps: self.ehps as i64,
            total_healing: self.total_healing,
            total_healing_effective: self.total_healing_effective,
            heal_crit_pct: self.heal_crit_pct,
            effective_heal_pct: self.effective_heal_pct,

            // Threat
            tps: self.tps as i64,
            total_threat: self.total_threat,

            // Damage taken
            dtps: self.dtps as i64,
            edtps: self.edtps as i64,
            total_damage_taken: self.total_damage_taken,
            total_damage_taken_effective: self.total_damage_taken_effective,

            // Shielding
            abs: self.abs as i64,
            total_shielding: self.total_shielding,

            // Activity
            apm: self.apm,
        }
    }
}

/// Unified player metrics struct for use across crate boundaries.
/// This is the canonical representation used by service and overlay layers.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlayerMetrics {
    pub entity_id: i64,
    pub name: String,

    // Damage dealing
    pub dps: i64,
    pub edps: i64,
    pub bossdps: i64,
    pub total_damage: i64,
    pub total_damage_effective: i64,
    pub total_damage_boss: i64,
    pub damage_crit_pct: f32,

    // Healing
    pub hps: i64,
    pub ehps: i64,
    pub total_healing: i64,
    pub total_healing_effective: i64,
    pub heal_crit_pct: f32,
    pub effective_heal_pct: f32,

    // Threat
    pub tps: i64,
    pub total_threat: i64,

    // Damage taken
    pub dtps: i64,
    pub edtps: i64,
    pub total_damage_taken: i64,
    pub total_damage_taken_effective: i64,

    // Shielding (absorbs)
    pub abs: i64,
    pub total_shielding: i64,

    // Activity
    pub apm: f32,
}
