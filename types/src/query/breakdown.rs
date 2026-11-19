//! Data Explorer breakdown, overview and time series results

use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// Query Result Types (shared between backend and frontend)
// ─────────────────────────────────────────────────────────────────────────────

/// Data explorer tab type - determines what data to query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DataTab {
    /// Damage dealt by sources
    #[default]
    Damage,
    /// Healing done by sources
    Healing,
    /// Damage received (group by source who dealt damage)
    DamageTaken,
    /// Healing received (group by source who healed)
    HealingTaken,
    /// Time series charts with effect analysis
    Charts,
}

impl DataTab {
    /// Returns true if this tab shows outgoing data (dealt by source)
    pub fn is_outgoing(&self) -> bool {
        matches!(self, DataTab::Damage | DataTab::Healing)
    }

    /// Returns true if this tab shows healing data
    pub fn is_healing(&self) -> bool {
        matches!(self, DataTab::Healing | DataTab::HealingTaken)
    }

    /// Returns the value column to query (dmg_amount or heal_amount)
    pub fn value_column(&self) -> &'static str {
        if self.is_healing() { "heal_amount" } else { "dmg_amount" }
    }

    /// Returns the display label for the rate column (DPS, HPS, DTPS, HTPS)
    pub fn rate_label(&self) -> &'static str {
        match self {
            DataTab::Damage => "DPS",
            DataTab::Healing => "HPS",
            DataTab::DamageTaken => "DTPS",
            DataTab::HealingTaken => "HTPS",
            DataTab::Charts => "Rate", // Charts tab doesn't use this
        }
    }
}

/// Breakdown mode flags for ability queries.
/// Multiple can be enabled to create hierarchical groupings.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct BreakdownMode {
    /// Group by ability (default, always on at minimum)
    pub by_ability: bool,
    /// Group by target/source type (class_id) - context depends on DataTab
    pub by_target_type: bool,
    /// Group by target/source instance (log_id) - context depends on DataTab
    pub by_target_instance: bool,
}

impl BreakdownMode {
    pub fn ability_only() -> Self {
        Self {
            by_ability: true,
            by_target_type: false,
            by_target_instance: false,
        }
    }
}

/// Query result for damage/healing breakdown.
/// Can be grouped by ability, target type, or target instance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AbilityBreakdown {
    // Ability info
    pub ability_name: String,
    pub ability_id: i64,

    // Target info (populated when grouping by target)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_class_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_log_id: Option<i64>,
    /// First hit time in seconds (for distinguishing target instances)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_first_hit_secs: Option<f32>,

    // Metrics
    pub total_value: f64,
    pub hit_count: i64,
    pub crit_count: i64,
    pub crit_rate: f64,
    pub max_hit: f64,
    pub avg_hit: f64,

    // Extended metrics
    #[serde(default)]
    pub miss_count: i64,
    #[serde(default)]
    pub activation_count: i64,
    #[serde(default)]
    pub crit_total: f64,
    #[serde(default)]
    pub effective_total: f64,
    #[serde(default)]
    pub is_shield: bool,

    // DamageTaken-specific fields
    #[serde(default)]
    pub attack_type: String,
    #[serde(default)]
    pub damage_type: String,
    #[serde(default)]
    pub shield_count: i64,
    #[serde(default)]
    pub absorbed_total: f64,

    // Computed fields (require duration/total context)
    #[serde(default)]
    pub dps: f64,
    #[serde(default)]
    pub percent_of_total: f64,
}

/// Summary statistics for the Damage Taken tab.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DamageTakenSummary {
    pub internal_elemental_total: f64,
    pub internal_elemental_pct: f64,
    pub kinetic_energy_total: f64,
    pub kinetic_energy_pct: f64,
    pub force_tech_total: f64,
    pub force_tech_pct: f64,
    pub melee_ranged_total: f64,
    pub melee_ranged_pct: f64,
    pub avoided_pct: f64,
    pub shielded_pct: f64,
    pub absorbed_self_total: f64,
    pub absorbed_self_pct: f64,
    pub absorbed_given_total: f64,
    pub absorbed_given_pct: f64,
}

/// Query result for damage/healing by source entity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityBreakdown {
    pub source_name: String,
    pub source_id: i64,
    pub entity_type: String, // "Player", "Npc", "Companion"
    pub total_value: f64,
    pub abilities_used: i64,
    /// First combat second this entity acted (distinguishes same-named NPC spawns)
    #[serde(default)]
    pub first_hit_secs: Option<f32>,
}

/// Which side of a PvP match a player is on, relative to the local player.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PvpFaction {
    Friendly,
    Enemy,
}

impl PvpFaction {
    pub fn opposite(self) -> Self {
        match self {
            Self::Friendly => Self::Enemy,
            Self::Enemy => Self::Friendly,
        }
    }
}

/// How class ability names are displayed in the Data Explorer.
/// `Republic`/`Imperial` swap mirror-class abilities to that faction's name;
/// abilities without a mirror (NPC, generic) are always shown as logged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbilityNaming {
    #[default]
    AsLogged,
    Republic,
    Imperial,
}

/// Mirror-class counterpart of a class ability (from the ability data dump).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MirrorAbility {
    /// The ability this entry describes.
    pub id: i64,
    pub mirror_id: i64,
    pub mirror_name: String,
    /// Faction of the ability this entry is keyed by.
    pub is_imperial: bool,
}

/// Raid overview row - aggregated stats per player across all metrics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RaidOverviewRow {
    pub name: String,
    pub entity_type: String,
    /// Friend/enemy classification (PvP encounters only)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub faction: Option<PvpFaction>,
    pub class_name: Option<String>,
    pub discipline_name: Option<String>,
    /// Icon filename (e.g., "assassin.png") - derived from discipline
    pub class_icon: Option<String>,
    /// Role icon filename (e.g., "icon_tank.png") - derived from discipline role
    pub role_icon: Option<String>,

    // Damage dealt
    pub damage_total: f64,
    pub dps: f64,

    // Threat
    pub threat_total: f64,
    pub tps: f64,

    // Damage taken
    pub damage_taken_total: f64,
    pub dtps: f64,
    /// Absorbed damage per second (shields that protected this player)
    pub aps: f64,

    // Shielding given (shields this player cast)
    pub shielding_given_total: f64,
    pub sps: f64,

    // Healing done
    pub healing_total: f64,
    pub hps: f64,
    /// Effective healing (not overheal)
    pub healing_effective: f64,
    pub ehps: f64,
    /// Percentage of total raid effective healing
    pub healing_pct: f64,

    // Activity
    /// Actions per minute
    pub apm: f64,
    /// Interrupt ability casts
    #[serde(default)]
    pub interrupts: f64,
}

/// Query result for time-series data (DPS/HPS over time).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimeSeriesPoint {
    pub bucket_start_ms: i64,
    pub total_value: f64,
}

/// What the Overview's group chart plots per player
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupMetric {
    #[default]
    Damage,
    Healing,
    DamageTaken,
    Threat,
}

/// Per-player value per bucket, all series on the same time spine
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct GroupTimeSeries {
    /// Start of the first bucket, in combat seconds
    pub start_secs: f64,
    pub bucket_secs: f64,
    /// (player name, value per bucket)
    pub series: Vec<(String, Vec<f64>)>,
}

/// Query result for HP% over time — includes absolute HP for tooltips.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HpPoint {
    pub bucket_start_ms: i64,
    pub hp_pct: f64,
    pub current_hp: i64,
    pub max_hp: i64,
}

/// Time window when an effect was active (for chart highlighting).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectWindow {
    pub start_secs: f32,
    pub end_secs: f32,
}

/// Effect uptime data for the charts panel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectChartData {
    pub effect_id: i64,
    pub effect_name: String,
    /// Ability ID that triggered this effect (for icon lookup)
    pub ability_id: Option<i64>,
    /// True if triggered by ability activation (active), false if passive/proc
    pub is_active: bool,
    /// Number of times effect was applied
    pub count: i64,
    /// Total duration in seconds
    pub total_duration_secs: f32,
    /// Uptime percentage (0-100)
    pub uptime_pct: f32,
}

/// One cell of the Effect Uptime tab: one source's effect on one target
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectUptimeCell {
    pub target_name: String,
    /// "Player" / "Companion" / "Npc" / "Self"
    pub target_type: String,
    pub effect_id: i64,
    pub effect_name: String,
    /// Ability that applied it (icon lookup); 0 when unknown
    pub ability_id: i64,
    /// Applications inside the range
    pub count: i64,
    pub uptime_pct: f32,
    /// When it was up: merged (start, end) combat seconds, clipped to the range
    pub windows: Vec<(f32, f32)>,
}
