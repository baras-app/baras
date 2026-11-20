use crate::combat_log::EntityType;
use crate::context::IStr;
use crate::context::resolve;
use crate::game_data::Discipline;
use baras_types::PvpFaction;
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Sliding window length for "recent" incoming damage rates
pub const INCOMING_DAMAGE_WINDOW_SECS: i64 = 20;

/// One attacker row for the Incoming Damage overlay.
/// Live: rate is windowed DTPS; finalized summaries: rate is encounter-wide DTPS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncomingDamageRow {
    pub entity_id: i64,
    pub name: String,
    /// Effective DTPS from this source
    pub rate: i64,
    /// Effective damage taken from this source over the whole encounter
    pub total: i64,
}

/// Tracks damage taken by the local player, attributed per source entity.
/// Rate is computed over a short sliding window (who is on you *right now*),
/// totals accumulate for the whole encounter.
#[derive(Debug, Clone, Default)]
pub struct IncomingDamageTracker {
    /// Encounter-total effective damage per source entity
    totals: hashbrown::HashMap<i64, i64>,
    /// Recent hits: (timestamp, source entity id, effective damage)
    window: VecDeque<(NaiveDateTime, i64, i32)>,
}

impl IncomingDamageTracker {
    pub fn record(&mut self, timestamp: NaiveDateTime, source_id: i64, effective: i32) {
        *self.totals.entry(source_id).or_default() += effective as i64;
        self.window.push_back((timestamp, source_id, effective));
        self.trim(timestamp);
    }

    fn trim(&mut self, now: NaiveDateTime) {
        let cutoff = now - chrono::Duration::seconds(INCOMING_DAMAGE_WINDOW_SECS);
        while self.window.front().is_some_and(|(ts, _, _)| *ts < cutoff) {
            self.window.pop_front();
        }
    }

    /// Per-source (rate, total) sorted by rate then total, descending.
    /// `now` anchors the sliding window (game time, not wall clock).
    pub fn snapshot(&self, now: NaiveDateTime) -> Vec<(i64, i64, i64)> {
        let cutoff = now - chrono::Duration::seconds(INCOMING_DAMAGE_WINDOW_SECS);
        let mut window_sums: hashbrown::HashMap<i64, i64> = hashbrown::HashMap::new();
        for (ts, source_id, effective) in &self.window {
            if *ts >= cutoff {
                *window_sums.entry(*source_id).or_default() += *effective as i64;
            }
        }
        let mut rows: Vec<(i64, i64, i64)> = self
            .totals
            .iter()
            .map(|(&id, &total)| {
                let rate = window_sums.get(&id).copied().unwrap_or(0) / INCOMING_DAMAGE_WINDOW_SECS;
                (id, rate, total)
            })
            .collect();
        rows.sort_by(|a, b| b.1.cmp(&a.1).then(b.2.cmp(&a.2)));
        rows
    }

    /// Encounter-total effective damage per source entity
    pub fn totals(&self) -> &hashbrown::HashMap<i64, i64> {
        &self.totals
    }
}

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
    /// All incoming attacks (hits + avoidances) — denominator for defense %
    pub attacks_received: u32,
    /// Attacks that landed damage (dmg_amount > 0) — denominator for shield %
    pub hits_received: u32,

    // Defense stats (dodge/parry/resist/deflect/miss)
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
    pub interrupt_casts: u32,
}

#[derive(Debug, Clone)]
pub struct EntityMetrics {
    pub entity_id: i64,
    pub name: IStr,
    pub entity_type: EntityType,
    pub discipline: Option<Discipline>,
    pub discipline_name: Option<String>,
    pub class_name: Option<String>,

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
    pub total_healing_received: i64,
    pub total_healing_received_effective: i64,

    // Tank stats
    pub defense_pct: f32,
    pub shield_pct: f32,
    pub total_shield_absorbed: i64,
    pub taunt_count: u32,

    // General
    pub apm: f32,
    pub tps: i32,
    pub total_threat: i64,
    pub interrupt_casts: u32,
}

impl EntityMetrics {
    /// Convert to PlayerMetrics for use across crate boundaries
    pub fn to_player_metrics(&self) -> PlayerMetrics {
        PlayerMetrics {
            entity_id: self.entity_id,
            name: resolve(self.name).to_string(),
            discipline: self.discipline,
            discipline_name: self.discipline_name.clone(),
            class_name: self.class_name.clone(),
            class_icon: self.discipline.map(|d| d.icon_name().to_string()),
            role_icon: self.discipline.map(|d| d.role().icon_name().to_string()),

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

            // Healing received
            htps: self.htps as i64,
            ehtps: self.ehtps as i64,
            total_healing_received: self.total_healing_received,
            total_healing_received_effective: self.total_healing_received_effective,

            // Tank stats
            defense_pct: self.defense_pct,
            shield_pct: self.shield_pct,
            total_shield_absorbed: self.total_shield_absorbed,

            // Activity
            apm: self.apm,
            interrupt_casts: self.interrupt_casts,

            // Annotated by the service in PvP encounters
            pvp_faction: None,
        }
    }
}

/// Unified player metrics struct for use across crate boundaries.
/// This is the canonical representation used by service and overlay layers.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlayerMetrics {
    pub entity_id: i64,
    pub name: String,
    pub discipline: Option<Discipline>,
    pub discipline_name: Option<String>,
    pub class_name: Option<String>,
    pub class_icon: Option<String>,
    pub role_icon: Option<String>,

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

    // Healing received
    pub htps: i64,
    pub ehtps: i64,
    pub total_healing_received: i64,
    pub total_healing_received_effective: i64,

    // Tank stats
    pub defense_pct: f32,
    pub shield_pct: f32,
    pub total_shield_absorbed: i64,

    // Activity
    pub apm: f32,
    #[serde(default)]
    pub interrupt_casts: u32,

    /// Faction relative to the local player (Some only in PvP encounters)
    #[serde(default)]
    pub pvp_faction: Option<PvpFaction>,
}
