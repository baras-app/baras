//! Combat log rows, filters and NPC lookups

use serde::{Deserialize, Serialize};

/// Final health state of an NPC in an encounter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NpcHealthRow {
    pub name: String,
    /// Combat time (seconds) when this NPC first appeared
    pub first_seen_secs: f32,
    /// Combat time (seconds) when this NPC died, if it died
    pub death_time_secs: Option<f32>,
    pub max_hp: i64,
    pub final_hp: i64,
    pub final_hp_pct: f32,
}

/// A single row in the combat log viewer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CombatLogRow {
    /// Row index for virtual scrolling
    pub row_idx: u64,
    /// Combat time in seconds from start
    pub time_secs: f32,
    /// Raw timestamp in milliseconds (from log file)
    pub timestamp_ms: i64,
    /// Source entity name
    pub source_name: String,
    /// Source entity type (Player, Companion, NPC)
    pub source_type: String,
    /// Target entity name
    pub target_name: String,
    /// Target entity type
    pub target_type: String,
    /// Effect type (ApplyEffect, Event, Damage, Heal, etc.)
    pub effect_type: String,
    /// Ability name
    pub ability_name: String,
    /// Ability ID (for icon lookup)
    pub ability_id: i64,
    /// Effect/result name (for buffs/debuffs)
    pub effect_name: String,
    /// Damage or heal value (effective)
    pub value: i32,
    /// Absorbed amount
    pub absorbed: i32,
    /// Overheal amount (heal_amount - heal_effective)
    pub overheal: i32,
    /// Threat generated
    pub threat: f32,
    /// Whether this was a critical hit
    pub is_crit: bool,
    /// Damage type name
    pub damage_type: String,
    /// Avoid type (miss, dodge, parry, etc.)
    pub defense_type_id: i64,
    /// Effect ID for readable event type mapping
    pub effect_id: i64,
    /// Effect type ID for event type filtering
    pub effect_type_id: i64,
    /// Source entity class_id for Show IDs feature (consistent across encounters)
    pub source_class_id: i64,
    /// Target entity class_id for Show IDs feature (consistent across encounters)
    pub target_class_id: i64,
    /// Per-spawn instance ids (what `id:` filters match on)
    #[serde(default)]
    pub source_id: i64,
    #[serde(default)]
    pub target_id: i64,
    /// Entity HP as reported on this line (0 when the entity has no HP data)
    #[serde(default)]
    pub source_hp: i32,
    #[serde(default)]
    pub source_max_hp: i32,
    #[serde(default)]
    pub target_hp: i32,
    #[serde(default)]
    pub target_max_hp: i32,
    /// Source world position (None for empty entities or pre-coordinate parquet files)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_x: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_y: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_z: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_facing: Option<f32>,
    /// Target world position (None for empty entities or pre-coordinate parquet files)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_x: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_y: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_z: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_facing: Option<f32>,
}

/// Combat log event category, for the combat log's event type filter
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogEventCategory {
    Damage,
    Healing,
    /// AbilityActivate/Deactivate/Interrupt/Cancel
    Actions,
    /// Buffs/debuffs gained or lost
    Effects,
    /// TargetSet/TargetCleared
    Targeting,
    /// ModifyThreat/Taunt
    Threat,
    /// ModifyCharges
    Charges,
    /// Spend/Restore
    Resources,
    /// EnterCombat/ExitCombat/Death/Revived
    Combat,
    /// Anything not in another category
    Other,
}

impl LogEventCategory {
    pub const ALL: [Self; 10] = [
        Self::Damage,
        Self::Healing,
        Self::Actions,
        Self::Effects,
        Self::Targeting,
        Self::Threat,
        Self::Charges,
        Self::Resources,
        Self::Combat,
        Self::Other,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Damage => "Damage",
            Self::Healing => "Healing",
            Self::Actions => "Actions",
            Self::Effects => "Effects",
            Self::Targeting => "Targeting",
            Self::Threat => "Threat",
            Self::Charges => "Charges",
            Self::Resources => "Resources",
            Self::Combat => "Combat State",
            Self::Other => "Other",
        }
    }
}

/// Combat log event type filter: the categories to show
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CombatLogFilters {
    pub categories: Vec<LogEventCategory>,
}

/// One NPC spawn, for names that occur with several instance ids in an encounter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NpcSpawn {
    pub name: String,
    /// Log instance id (`source_id` / `target_id`)
    pub id: i64,
    /// First combat second this spawn appeared
    pub first_hit_secs: f32,
}

/// Entity names grouped by type for combat log filter dropdowns.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GroupedEntityNames {
    /// Players and companions
    pub friendly: Vec<String>,
    /// NPCs/enemies
    pub npcs: Vec<String>,
    /// Per-instance rows for NPC names that appear more than once (ordered by first hit)
    #[serde(default)]
    pub npc_spawns: Vec<NpcSpawn>,
}

/// A match result from the combat log find feature.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CombatLogFindMatch {
    /// Position in the filtered result set (for scrolling)
    pub pos: u64,
    /// Row index / line_number (for highlighting)
    pub row_idx: u64,
}
