//! Death tracker and death recap results

use crate::PvpFaction;
use serde::{Deserialize, Serialize};

/// A player death event for the death tracker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerDeath {
    /// Player name
    pub name: String,
    /// Time of death in seconds from combat start
    pub death_time_secs: f32,
    /// First revive after this death, in seconds from combat start (None = stayed dead)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revive_time_secs: Option<f32>,
    /// Victim's side (PvP encounters only)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub faction: Option<PvpFaction>,
    /// The victim is the local player (PvP only)
    #[serde(default)]
    pub is_local: bool,
    /// The local player dealt the killing blow (PvP only)
    #[serde(default)]
    pub local_kill: bool,
}

/// The final hit that killed a player.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeathRecapKillingBlow {
    pub ability_name: String,
    pub source_name: String,
    /// Effective damage of the killing blow
    pub amount: i32,
    /// Damage beyond the player's remaining HP
    pub overkill: i32,
    pub is_crit: bool,
    pub damage_type: String,
}

/// Damage taken from one ability, aggregated over the recap window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeathRecapDamageRow {
    pub ability_name: String,
    /// For the row icon
    #[serde(default)]
    pub ability_id: i64,
    pub hits: u32,
    pub crits: u32,
    /// Total effective damage
    pub total: i64,
    /// Largest single effective hit
    pub max_hit: i32,
    /// Total damage absorbed by shields
    pub absorbed: i64,
    /// "kinetic" | "energy" | "internal" | "elemental"; empty when the log
    /// gave none (e.g. only avoided hits)
    #[serde(default)]
    pub damage_type: String,
}

/// Damage taken from one source entity (per NPC spawn instance), with a
/// per-ability breakdown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeathRecapDamageSource {
    pub source_name: String,
    pub source_id: i64,
    pub hits: u32,
    pub crits: u32,
    pub total: i64,
    pub max_hit: i32,
    pub absorbed: i64,
    /// Sorted by total damage descending
    pub abilities: Vec<DeathRecapDamageRow>,
}

/// Healing received from one ability, aggregated over the recap window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeathRecapHealRow {
    pub ability_name: String,
    /// For the row icon
    #[serde(default)]
    pub ability_id: i64,
    pub casts: u32,
    pub effective: i64,
}

/// Healing received from one healer, with a per-ability breakdown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeathRecapHealSource {
    pub source_name: String,
    pub source_id: i64,
    pub casts: u32,
    pub effective: i64,
    /// Sorted by effective healing descending
    pub abilities: Vec<DeathRecapHealRow>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeathRecapEventKind {
    Damage,
    Heal,
    Death,
}

/// One entry in the death recap event ledger.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeathRecapEvent {
    pub time_secs: f32,
    pub kind: DeathRecapEventKind,
    pub source_name: String,
    pub ability_name: String,
    /// For the ledger icon; 0 for the death row
    #[serde(default)]
    pub ability_id: i64,
    /// Effective damage or healing
    pub value: i32,
    pub absorbed: i32,
    pub is_crit: bool,
    /// Hit was fully avoided (dodge/parry/miss/...)
    pub avoided: bool,
    /// Damage events only; empty for heals, the death, and untyped hits
    #[serde(default)]
    pub damage_type: String,
    /// Player HP% after this event (last known sample)
    pub hp_pct: f32,
}

/// HP sample for the death recap sparkline.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DeathRecapHpPoint {
    pub time_secs: f32,
    pub hp_pct: f32,
}

/// An effect that was on the player during the recap window, per source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeathRecapEffect {
    pub effect_id: i64,
    pub effect_name: String,
    /// For the icon; 0 when no apply was logged (up since before combat)
    pub ability_id: i64,
    pub source_name: String,
    /// Applied by something other than a player or companion
    pub from_npc: bool,
    /// Last logged stack count; 1 for effects that do not stack
    pub stacks: i32,
    /// (start, end) in combat seconds, chronological. A start before the
    /// recap window is kept as logged; an end never passes the death.
    pub windows: Vec<(f32, f32)>,
    /// (time, stacks) at each change, from the last one before the recap
    /// window on. Empty for effects that never stacked.
    #[serde(default)]
    pub stack_changes: Vec<(f32, i32)>,
    pub active_at_death: bool,
    /// A defensive cooldown's effect
    #[serde(default)]
    pub is_defensive: bool,
}

/// A defensive cooldown the player pressed before the death.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeathRecapDefensive {
    pub ability_id: i64,
    pub ability_name: String,
    /// Combat seconds of the last press before the death
    pub last_used_secs: f32,
    /// Presses in the fight up to the death
    pub uses: u32,
}

/// Full recap of a single player death.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeathRecap {
    pub player_name: String,
    pub death_time_secs: f32,
    /// Start of the analysis window (combat seconds), 15s before death.
    /// Events up to 3s after death are also included since killing damage is
    /// often logged after the death event.
    pub window_start_secs: f32,
    pub phase_name: Option<String>,
    pub killing_blow: Option<DeathRecapKillingBlow>,
    /// Total effective damage taken in the window
    pub damage_total: i64,
    pub absorbed_total: i64,
    /// Total effective healing received in the window
    pub healing_total: i64,
    pub hp_timeline: Vec<DeathRecapHpPoint>,
    /// Per-source damage taken, sorted by total descending
    pub damage_rows: Vec<DeathRecapDamageSource>,
    /// Per-source healing received, sorted by effective descending
    pub healing_rows: Vec<DeathRecapHealSource>,
    /// Chronological ledger of meaningful events in the final seconds
    pub events: Vec<DeathRecapEvent>,
    /// Effects on the player in the window: enemy-applied first, then those
    /// still up at the death, then by start
    #[serde(default)]
    pub effects: Vec<DeathRecapEffect>,
    /// Defensive cooldowns the player pressed in the fight up to the death,
    /// latest press first
    #[serde(default)]
    pub defensives: Vec<DeathRecapDefensive>,
}
