//! Shared configuration types for BARAS
//!
//! This crate contains serializable configuration types that are shared between
//! the native backend (baras-core) and the WASM frontend (app-ui).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
        if self.is_healing() {
            "heal_amount"
        } else {
            "dmg_amount"
        }
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

    // Computed fields (require duration/total context)
    #[serde(default)]
    pub dps: f64,
    #[serde(default)]
    pub percent_of_total: f64,
}

/// Query result for damage/healing by source entity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityBreakdown {
    pub source_name: String,
    pub source_id: i64,
    pub entity_type: String, // "Player", "Npc", "Companion"
    pub total_value: f64,
    pub abilities_used: i64,
}

/// Raid overview row - aggregated stats per player across all metrics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RaidOverviewRow {
    pub name: String,
    pub entity_type: String,
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

    // Healing done
    pub healing_total: f64,
    pub hps: f64,
    /// Effective healing (not overheal)
    pub healing_effective: f64,
    pub ehps: f64,
    /// Percentage of total raid effective healing
    pub healing_pct: f64,
}

/// Query result for time-series data (DPS/HPS over time).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimeSeriesPoint {
    pub bucket_start_ms: i64,
    pub total_value: f64,
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

/// A player death event for the death tracker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerDeath {
    /// Player name
    pub name: String,
    /// Time of death in seconds from combat start
    pub death_time_secs: f32,
}

/// A single row in the combat log viewer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CombatLogRow {
    /// Row index for virtual scrolling
    pub row_idx: u64,
    /// Combat time in seconds from start
    pub time_secs: f32,
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
}

/// A phase segment - one occurrence of a phase (phases can repeat).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhaseSegment {
    pub phase_id: String,
    pub phase_name: String,
    pub instance: i64,
    pub start_secs: f32,
    pub end_secs: f32,
}

/// Encounter timeline with duration and phase segments.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EncounterTimeline {
    pub duration_secs: f32,
    pub phases: Vec<PhaseSegment>,
}

/// Time range filter for queries (in seconds from combat start).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct TimeRange {
    pub start: f32,
    pub end: f32,
}

impl TimeRange {
    pub fn new(start: f32, end: f32) -> Self {
        Self { start, end }
    }

    pub fn full(duration: f32) -> Self {
        Self {
            start: 0.0,
            end: duration,
        }
    }

    pub fn is_full(&self, duration: f32) -> bool {
        self.start <= 0.01 && (self.end - duration).abs() < 0.01
    }

    /// Generate SQL WHERE clause fragment for filtering by time range.
    pub fn sql_filter(&self) -> String {
        format!(
            "combat_time_secs >= {} AND combat_time_secs <= {}",
            self.start, self.end
        )
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Color Type
// ─────────────────────────────────────────────────────────────────────────────

/// RGBA color as [r, g, b, a] bytes
pub type Color = [u8; 4];

// ─────────────────────────────────────────────────────────────────────────────
// Selectors (unified ID-or-Name matching)
// ─────────────────────────────────────────────────────────────────────────────

/// Selector for effects - can match by ID or name.
/// Uses untagged serde for clean serialization: numbers as IDs, strings as names.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EffectSelector {
    Id(u64),
    Name(String),
}

impl EffectSelector {
    /// Parse from user input - tries ID first, falls back to name.
    pub fn from_input(input: &str) -> Self {
        match input.trim().parse::<u64>() {
            Ok(id) => Self::Id(id),
            Err(_) => Self::Name(input.trim().to_string()),
        }
    }

    /// Returns the display string for this selector.
    pub fn display(&self) -> String {
        match self {
            Self::Id(id) => id.to_string(),
            Self::Name(name) => name.clone(),
        }
    }

    /// Check if this selector matches the given ID or name.
    pub fn matches(&self, id: u64, name: Option<&str>) -> bool {
        match self {
            Self::Id(expected) => *expected == id,
            Self::Name(expected) => name
                .map(|n| n.eq_ignore_ascii_case(expected))
                .unwrap_or(false),
        }
    }
}

/// Selector for abilities - can match by ID or name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AbilitySelector {
    Id(u64),
    Name(String),
}

impl AbilitySelector {
    /// Parse from user input - tries ID first, falls back to name.
    pub fn from_input(input: &str) -> Self {
        match input.trim().parse::<u64>() {
            Ok(id) => Self::Id(id),
            Err(_) => Self::Name(input.trim().to_string()),
        }
    }

    /// Returns the display string for this selector.
    pub fn display(&self) -> String {
        match self {
            Self::Id(id) => id.to_string(),
            Self::Name(name) => name.clone(),
        }
    }

    /// Check if this selector matches the given ID or name.
    pub fn matches(&self, id: u64, name: Option<&str>) -> bool {
        match self {
            Self::Id(expected) => *expected == id,
            Self::Name(expected) => name
                .map(|n| n.eq_ignore_ascii_case(expected))
                .unwrap_or(false),
        }
    }
}

/// Selector for entities - can match by NPC ID, roster alias, or name.
/// Uses untagged serde: numbers as IDs, strings as roster alias or name.
/// Priority when matching: Roster Alias → NPC ID → Name (resolved at runtime).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EntitySelector {
    Id(i64),
    Name(String),
}

impl EntitySelector {
    /// Parse from user input - tries NPC ID first, falls back to name/alias.
    pub fn from_input(input: &str) -> Self {
        match input.trim().parse::<i64>() {
            Ok(id) => Self::Id(id),
            Err(_) => Self::Name(input.trim().to_string()),
        }
    }

    /// Returns the display string for this selector.
    pub fn display(&self) -> String {
        match self {
            Self::Id(id) => id.to_string(),
            Self::Name(name) => name.clone(),
        }
    }
}

/// Wrapper for entity selectors used in source/target filters.
/// Matches the backend's EntityMatcher serialization format.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EntityMatcher {
    #[serde(default)]
    pub selector: Vec<EntitySelector>,
}

impl EntityMatcher {
    pub fn new(selector: Vec<EntitySelector>) -> Self {
        Self { selector }
    }

    pub fn is_empty(&self) -> bool {
        self.selector.is_empty()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Trigger Types (shared across timers, phases, counters)
// ─────────────────────────────────────────────────────────────────────────────

/// Unified trigger type for timers, phases, and counters.
///
/// Different systems use different subsets:
/// - `[T]` = Timer only
/// - `[P]` = Phase only
/// - `[C]` = Counter only
/// - `[TPC]` = All systems
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Trigger {
    // ─── Combat State [TPC] ────────────────────────────────────────────────
    /// Combat starts. [TPC]
    CombatStart,

    /// Combat ends. [C only]
    CombatEnd,

    // ─── Abilities & Effects [TPC] ─────────────────────────────────────────
    /// Ability is cast. [TPC]
    AbilityCast {
        #[serde(default)]
        abilities: Vec<AbilitySelector>,
        #[serde(default = "EntityFilter::default_any")]
        source: EntityFilter,
        #[serde(default = "EntityFilter::default_any")]
        target: EntityFilter,
    },

    /// Effect/buff is applied. [TPC]
    EffectApplied {
        #[serde(default)]
        effects: Vec<EffectSelector>,
        #[serde(default)]
        source: EntityFilter,
        #[serde(default)]
        target: EntityFilter,
    },

    /// Effect/buff is removed. [TPC]
    EffectRemoved {
        #[serde(default)]
        effects: Vec<EffectSelector>,
        #[serde(default)]
        source: EntityFilter,
        #[serde(default)]
        target: EntityFilter,
    },

    /// Damage is taken from an ability. [TPC]
    DamageTaken {
        #[serde(default)]
        abilities: Vec<AbilitySelector>,
        #[serde(default)]
        source: EntityFilter,
        #[serde(default)]
        target: EntityFilter,
    },

    // ─── HP Thresholds [TPC] ───────────────────────────────────────────────
    /// Boss HP drops below threshold. [TPC]
    BossHpBelow {
        hp_percent: f32,
        #[serde(default)]
        selector: Vec<EntitySelector>,
    },

    /// Boss HP rises above threshold. [P only]
    BossHpAbove {
        hp_percent: f32,
        #[serde(default)]
        selector: Vec<EntitySelector>,
    },

    // ─── Entity Lifecycle [TPC] ────────────────────────────────────────────
    /// NPC appears (first seen in combat). [TPC]
    NpcAppears {
        #[serde(default)]
        selector: Vec<EntitySelector>,
    },

    /// Entity dies. [TPC]
    EntityDeath {
        #[serde(default)]
        selector: Vec<EntitySelector>,
    },

    /// NPC sets its target. [T only]
    TargetSet {
        #[serde(default)]
        selector: Vec<EntitySelector>,
        #[serde(default)]
        target: EntityFilter,
    },

    // ─── Phase Events [TPC] ────────────────────────────────────────────────
    /// Phase is entered. [TC]
    PhaseEntered { phase_id: String },

    /// Phase ends. [TPC]
    PhaseEnded { phase_id: String },

    /// Any phase change occurs. [C only]
    AnyPhaseChange,

    // ─── Counter Events [TP] ───────────────────────────────────────────────
    /// Counter reaches a specific value. [TP]
    CounterReaches { counter_id: String, value: u32 },

    // ─── Timer Events [T only] ─────────────────────────────────────────────
    /// Another timer expires (chaining). [T only]
    TimerExpires { timer_id: String },

    /// Another timer starts (for cancellation). [T only]
    TimerStarted { timer_id: String },

    // ─── Time-based [TP] ───────────────────────────────────────────────────
    /// Time elapsed since combat start. [TP]
    TimeElapsed { secs: f32 },

    // ─── System-specific ───────────────────────────────────────────────────
    /// Manual/debug trigger. [T only]
    Manual,

    /// Never triggers. [C only]
    Never,

    // ─── Composition [TPC] ─────────────────────────────────────────────────
    /// Any condition suffices (OR logic). [TPC]
    AnyOf { conditions: Vec<Trigger> },
}

impl Trigger {
    /// Returns a human-readable label for this trigger type.
    pub fn label(&self) -> &'static str {
        match self {
            Self::CombatStart => "Combat Start",
            Self::CombatEnd => "Combat End",
            Self::AbilityCast { .. } => "Ability Cast",
            Self::EffectApplied { .. } => "Effect Applied",
            Self::EffectRemoved { .. } => "Effect Removed",
            Self::DamageTaken { .. } => "Damage Taken",
            Self::BossHpBelow { .. } => "Boss HP Below",
            Self::BossHpAbove { .. } => "Boss HP Above",
            Self::NpcAppears { .. } => "NPC Appears",
            Self::EntityDeath { .. } => "Entity Death",
            Self::TargetSet { .. } => "Target Set",
            Self::PhaseEntered { .. } => "Phase Entered",
            Self::PhaseEnded { .. } => "Phase Ended",
            Self::AnyPhaseChange => "Any Phase Change",
            Self::CounterReaches { .. } => "Counter Reaches",
            Self::TimerExpires { .. } => "Timer Expires",
            Self::TimerStarted { .. } => "Timer Started",
            Self::TimeElapsed { .. } => "Time Elapsed",
            Self::Manual => "Manual",
            Self::Never => "Never",
            Self::AnyOf { .. } => "Any Of (OR)",
        }
    }

    /// Returns the snake_case type name for this trigger.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::CombatStart => "combat_start",
            Self::CombatEnd => "combat_end",
            Self::AbilityCast { .. } => "ability_cast",
            Self::EffectApplied { .. } => "effect_applied",
            Self::EffectRemoved { .. } => "effect_removed",
            Self::DamageTaken { .. } => "damage_taken",
            Self::BossHpBelow { .. } => "boss_hp_below",
            Self::BossHpAbove { .. } => "boss_hp_above",
            Self::NpcAppears { .. } => "npc_appears",
            Self::EntityDeath { .. } => "entity_death",
            Self::TargetSet { .. } => "target_set",
            Self::PhaseEntered { .. } => "phase_entered",
            Self::PhaseEnded { .. } => "phase_ended",
            Self::AnyPhaseChange => "any_phase_change",
            Self::CounterReaches { .. } => "counter_reaches",
            Self::TimerExpires { .. } => "timer_expires",
            Self::TimerStarted { .. } => "timer_started",
            Self::TimeElapsed { .. } => "time_elapsed",
            Self::Manual => "manual",
            Self::Never => "never",
            Self::AnyOf { .. } => "any_of",
        }
    }
}

impl Default for Trigger {
    fn default() -> Self {
        Self::CombatStart
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Default Color Constants
// ─────────────────────────────────────────────────────────────────────────────

/// Default colors for overlay types
pub mod overlay_colors {
    use super::Color;

    pub const WHITE: Color = [255, 255, 255, 255];
    pub const DPS: Color = [180, 50, 50, 255]; // Red
    pub const HPS: Color = [50, 180, 50, 255]; // Green
    pub const TPS: Color = [50, 100, 180, 255]; // Blue
    pub const DTPS: Color = [180, 80, 80, 255]; // Dark red
    pub const ABS: Color = [100, 150, 200, 255]; // Light blue
    pub const BOSS_BAR: Color = [200, 50, 50, 255]; // Boss health red
    pub const FRAME_BG: Color = [40, 40, 40, 200]; // Raid frame background

    /// Get the default bar color for an overlay type by its config key
    pub fn for_key(key: &str) -> Color {
        match key {
            "dps" | "edps" | "bossdps" => DPS,
            "hps" | "ehps" => HPS,
            "tps" => TPS,
            "dtps" | "edtps" => DTPS,
            "abs" => ABS,
            _ => DPS,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Serde Default Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn default_true() -> bool {
    true
}
fn default_opacity() -> u8 {
    180
}
fn default_scaling_factor() -> f32 {
    1.0
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Appearance Config
// ─────────────────────────────────────────────────────────────────────────────

/// Per-overlay appearance configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayAppearanceConfig {
    #[serde(default = "default_true")]
    pub show_header: bool,
    #[serde(default = "default_true")]
    pub show_footer: bool,
    #[serde(default)]
    pub show_class_icons: bool,
    #[serde(default = "default_font_color")]
    pub font_color: Color,
    #[serde(default = "default_bar_color")]
    pub bar_color: Color,
    #[serde(default = "default_max_entries")]
    pub max_entries: u8,
    #[serde(default)]
    pub show_total: bool,
    #[serde(default = "default_true")]
    pub show_per_second: bool,
    #[serde(default = "default_true")]
    pub show_percent: bool,
    #[serde(default = "default_true")]
    pub show_duration: bool,
}

fn default_font_color() -> Color {
    overlay_colors::WHITE
}
fn default_bar_color() -> Color {
    overlay_colors::DPS
}
fn default_max_entries() -> u8 {
    16
}

impl Default for OverlayAppearanceConfig {
    fn default() -> Self {
        Self {
            show_header: true,
            show_footer: true,
            show_class_icons: false,
            font_color: overlay_colors::WHITE,
            bar_color: overlay_colors::DPS,
            max_entries: 16,
            show_total: false,
            show_per_second: true,
            show_percent: true,
            show_duration: true,
        }
    }
}

impl OverlayAppearanceConfig {
    /// Get default appearance for an overlay type by its config key.
    pub fn default_for_type(key: &str) -> Self {
        Self {
            bar_color: overlay_colors::for_key(key),
            ..Self::default()
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Personal Stats
// ─────────────────────────────────────────────────────────────────────────────

/// Stats that can be displayed on the personal overlay
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PersonalStat {
    EncounterName,
    Difficulty,
    EncounterTime,
    EncounterCount,
    Apm,
    Dps,
    EDps,
    BossDps,
    TotalDamage,
    BossDamage,
    Hps,
    EHps,
    TotalHealing,
    Dtps,
    Tps,
    TotalThreat,
    DamageCritPct,
    HealCritPct,
    EffectiveHealPct,
    ClassDiscipline,
    /// Current boss phase (if any)
    Phase,
    /// Time in current phase
    PhaseTime,
}

impl PersonalStat {
    /// Get the display label for this stat
    pub fn label(&self) -> &'static str {
        match self {
            Self::EncounterName => "Encounter Name",
            Self::Difficulty => "Difficulty",
            Self::EncounterTime => "Duration",
            Self::EncounterCount => "Encounter",
            Self::Apm => "APM",
            Self::Dps => "DPS",
            Self::EDps => "eDPS",
            Self::BossDps => "Boss DPS",
            Self::BossDamage => "Boss Damage",
            Self::TotalDamage => "Total Damage",
            Self::Hps => "HPS",
            Self::EHps => "eHPS",
            Self::TotalHealing => "Total Healing",
            Self::Dtps => "eDTPS",
            Self::Tps => "TPS",
            Self::TotalThreat => "Total Threat",
            Self::DamageCritPct => "Dmg Crit %",
            Self::HealCritPct => "Heal Crit %",
            Self::EffectiveHealPct => "Eff Heal %",
            Self::ClassDiscipline => "Spec",
            Self::Phase => "Phase",
            Self::PhaseTime => "Phase Time",
        }
    }

    /// Get all stats in display order
    pub fn all() -> &'static [PersonalStat] {
        &[
            Self::EncounterName,
            Self::Difficulty,
            Self::EncounterTime,
            Self::EncounterCount,
            Self::ClassDiscipline,
            Self::Apm,
            Self::Dps,
            Self::EDps,
            Self::BossDamage,
            Self::BossDps,
            Self::TotalDamage,
            Self::Hps,
            Self::EHps,
            Self::TotalHealing,
            Self::Dtps,
            Self::Tps,
            Self::TotalThreat,
            Self::DamageCritPct,
            Self::HealCritPct,
            Self::EffectiveHealPct,
            Self::Phase,
            Self::PhaseTime,
        ]
    }
}

/// Configuration for the personal stats overlay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonalOverlayConfig {
    #[serde(default = "default_personal_stats")]
    pub visible_stats: Vec<PersonalStat>,
    #[serde(default = "default_font_color")]
    pub font_color: Color,
    #[serde(default = "default_font_color")]
    pub label_color: Color,
}

fn default_personal_stats() -> Vec<PersonalStat> {
    vec![
        PersonalStat::EncounterName,
        PersonalStat::Difficulty,
        PersonalStat::EncounterTime,
        PersonalStat::Dps,
        PersonalStat::Hps,
        PersonalStat::Dtps,
        PersonalStat::Apm,
    ]
}

impl Default for PersonalOverlayConfig {
    fn default() -> Self {
        Self {
            visible_stats: default_personal_stats(),
            font_color: overlay_colors::WHITE,
            label_color: overlay_colors::WHITE,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Position
// ─────────────────────────────────────────────────────────────────────────────

/// Position configuration for an overlay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayPositionConfig {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub monitor_id: Option<String>,
}

impl Default for OverlayPositionConfig {
    fn default() -> Self {
        Self {
            x: 50,
            y: 50,
            width: 280,
            height: 200,
            monitor_id: None,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Raid Overlay Settings
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration for the raid frame overlay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaidOverlaySettings {
    #[serde(default = "default_grid_columns")]
    pub grid_columns: u8,
    #[serde(default = "default_grid_rows")]
    pub grid_rows: u8,
    #[serde(default = "default_max_effects")]
    pub max_effects_per_frame: u8,
    #[serde(default = "default_effect_size")]
    pub effect_size: f32,
    #[serde(default = "default_effect_offset")]
    pub effect_vertical_offset: f32,
    #[serde(default = "default_frame_bg")]
    pub frame_bg_color: Color,
    #[serde(default = "default_true")]
    pub show_role_icons: bool,
    #[serde(default = "default_effect_fill_opacity")]
    pub effect_fill_opacity: u8,
}

fn default_grid_columns() -> u8 {
    2
}
fn default_grid_rows() -> u8 {
    4
}
fn default_max_effects() -> u8 {
    4
}
fn default_effect_size() -> f32 {
    14.0
}
fn default_effect_offset() -> f32 {
    3.0
}
fn default_frame_bg() -> Color {
    overlay_colors::FRAME_BG
}
fn default_effect_fill_opacity() -> u8 {
    255
}

impl Default for RaidOverlaySettings {
    fn default() -> Self {
        Self {
            grid_columns: 2,
            grid_rows: 4,
            max_effects_per_frame: 4,
            effect_size: 14.0,
            effect_vertical_offset: 3.0,
            frame_bg_color: overlay_colors::FRAME_BG,
            show_role_icons: true,
            effect_fill_opacity: 255,
        }
    }
}

impl RaidOverlaySettings {
    /// Validate that grid dimensions result in 4, 8, or 16 total slots
    pub fn is_valid_grid(&self) -> bool {
        let total = self.grid_columns as u16 * self.grid_rows as u16;
        matches!(total, 4 | 8 | 16)
    }

    /// Get total number of slots
    pub fn total_slots(&self) -> u8 {
        self.grid_columns * self.grid_rows
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Boss Health Settings
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration for the boss health bar overlay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BossHealthConfig {
    #[serde(default = "default_boss_bar_color")]
    pub bar_color: Color,
    #[serde(default = "default_font_color")]
    pub font_color: Color,
    #[serde(default = "default_true")]
    pub show_percent: bool,
    #[serde(default = "default_true")]
    pub show_target: bool,
}

fn default_boss_bar_color() -> Color {
    overlay_colors::BOSS_BAR
}

impl Default for BossHealthConfig {
    fn default() -> Self {
        Self {
            bar_color: overlay_colors::BOSS_BAR,
            font_color: overlay_colors::WHITE,
            show_percent: true,
            show_target: true,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Timer Overlay Configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration for the timer bar overlay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimerOverlayConfig {
    /// Default bar color for timers (individual timers may override)
    #[serde(default = "default_timer_bar_color")]
    pub default_bar_color: Color,
    /// Font color for timer text
    #[serde(default = "default_font_color")]
    pub font_color: Color,
    /// Maximum number of timers to display
    #[serde(default = "default_max_timers")]
    pub max_display: u8,
    /// Sort by remaining time (vs. activation order)
    #[serde(default = "default_true")]
    pub sort_by_remaining: bool,
}

fn default_timer_bar_color() -> Color {
    [100, 180, 220, 255]
}
fn default_max_timers() -> u8 {
    10
}

impl Default for TimerOverlayConfig {
    fn default() -> Self {
        Self {
            default_bar_color: default_timer_bar_color(),
            font_color: overlay_colors::WHITE,
            max_display: 10,
            sort_by_remaining: true,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Alerts Overlay Configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration for the alerts text overlay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertsOverlayConfig {
    /// Font size for alert text (default 12)
    #[serde(default = "default_alerts_font_size")]
    pub font_size: u8,
    /// Maximum number of alerts to display at once
    #[serde(default = "default_alerts_max_display")]
    pub max_display: u8,
    /// Seconds to show each alert at full opacity
    #[serde(default = "default_alerts_duration")]
    pub default_duration: f32,
    /// Seconds for fade-out effect after duration expires
    #[serde(default = "default_alerts_fade_duration")]
    pub fade_duration: f32,
}

fn default_alerts_font_size() -> u8 {
    12
}
fn default_alerts_max_display() -> u8 {
    5
}
fn default_alerts_duration() -> f32 {
    5.0
}
fn default_alerts_fade_duration() -> f32 {
    1.0
}

impl Default for AlertsOverlayConfig {
    fn default() -> Self {
        Self {
            font_size: default_alerts_font_size(),
            max_display: default_alerts_max_display(),
            default_duration: default_alerts_duration(),
            fade_duration: default_alerts_fade_duration(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Challenge Overlay Configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Layout direction for challenge cards
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeLayout {
    /// Stack challenges vertically (default)
    #[default]
    Vertical,
    /// Arrange challenges horizontally
    Horizontal,
}

/// Column display mode for individual challenges
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeColumns {
    /// Show total value and percent (default)
    #[default]
    TotalPercent,
    /// Show total value and per-second rate
    TotalPerSecond,
    /// Show per-second rate and percent
    PerSecondPercent,
    /// Show only total value
    TotalOnly,
    /// Show only per-second rate
    PerSecondOnly,
    /// Show only percent
    PercentOnly,
}

/// Configuration for the challenge overlay (global settings)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeOverlayConfig {
    /// Font color for challenge text
    #[serde(default = "default_font_color")]
    pub font_color: Color,
    /// Default bar color for challenges (individual challenges may override)
    #[serde(default = "default_challenge_bar_color")]
    pub default_bar_color: Color,
    /// Show footer with totals
    #[serde(default = "default_true")]
    pub show_footer: bool,
    /// Show duration in header
    #[serde(default = "default_true")]
    pub show_duration: bool,
    /// Maximum challenges to display
    #[serde(default = "default_max_challenges")]
    pub max_display: u8,
    /// Layout direction for challenge cards
    #[serde(default)]
    pub layout: ChallengeLayout,
}

fn default_challenge_bar_color() -> Color {
    overlay_colors::DPS
}
fn default_max_challenges() -> u8 {
    4
}

impl Default for ChallengeOverlayConfig {
    fn default() -> Self {
        Self {
            font_color: overlay_colors::WHITE,
            default_bar_color: overlay_colors::DPS,
            show_footer: true,
            show_duration: true,
            max_display: 4,
            layout: ChallengeLayout::Vertical,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Effects A/B Overlay Config (consolidated personal effects)
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration for Effects A overlay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectsAConfig {
    /// Icon size in pixels
    #[serde(default = "default_icon_size")]
    pub icon_size: u8,
    /// Maximum effects to display
    #[serde(default = "default_max_buffs")]
    pub max_display: u8,
    /// Use vertical layout (true) or horizontal (false)
    #[serde(default)]
    pub layout_vertical: bool,
    /// Show effect names below/beside icons
    #[serde(default)]
    pub show_effect_names: bool,
    /// Show countdown text on icons
    #[serde(default = "default_true")]
    pub show_countdown: bool,
    /// When true, stacks are shown large and centered; timer is secondary
    #[serde(default)]
    pub stack_priority: bool,
}

fn default_icon_size() -> u8 {
    32
}
fn default_max_buffs() -> u8 {
    8
}

impl Default for EffectsAConfig {
    fn default() -> Self {
        Self {
            icon_size: 32,
            max_display: 8,
            layout_vertical: false,
            show_effect_names: false,
            show_countdown: true,
            stack_priority: false,
        }
    }
}

/// Configuration for Effects B overlay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectsBConfig {
    /// Icon size in pixels
    #[serde(default = "default_icon_size")]
    pub icon_size: u8,
    /// Maximum effects to display
    #[serde(default = "default_max_buffs")]
    pub max_display: u8,
    /// Use vertical layout (true) or horizontal (false)
    #[serde(default)]
    pub layout_vertical: bool,
    /// Show effect names below/beside icons
    #[serde(default)]
    pub show_effect_names: bool,
    /// Show countdown text on icons
    #[serde(default = "default_true")]
    pub show_countdown: bool,
    /// Highlight cleansable effects
    #[serde(default = "default_true")]
    pub highlight_cleansable: bool,
    /// When true, stacks are shown large and centered; timer is secondary
    #[serde(default)]
    pub stack_priority: bool,
}

impl Default for EffectsBConfig {
    fn default() -> Self {
        Self {
            icon_size: 32,
            max_display: 8,
            layout_vertical: false,
            show_effect_names: false,
            show_countdown: true,
            highlight_cleansable: true,
            stack_priority: false,
        }
    }
}

// Legacy aliases for backwards compatibility
pub type PersonalBuffsConfig = EffectsAConfig;
pub type PersonalDebuffsConfig = EffectsBConfig;

// ─────────────────────────────────────────────────────────────────────────────
// Cooldown Tracker Overlay Config
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration for the cooldown tracker overlay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CooldownTrackerConfig {
    /// Icon size in pixels
    #[serde(default = "default_icon_size")]
    pub icon_size: u8,
    /// Maximum cooldowns to display
    #[serde(default = "default_max_cooldowns")]
    pub max_display: u8,
    /// Show ability names
    #[serde(default = "default_true")]
    pub show_ability_names: bool,
    /// Sort by remaining time
    #[serde(default = "default_true")]
    pub sort_by_remaining: bool,
    /// Show source name
    #[serde(default)]
    pub show_source_name: bool,
    /// Show target of ability (for targeted CDs like taunts)
    #[serde(default)]
    pub show_target_name: bool,
}

fn default_max_cooldowns() -> u8 {
    10
}

impl Default for CooldownTrackerConfig {
    fn default() -> Self {
        Self {
            icon_size: 32,
            max_display: 10,
            show_ability_names: true,
            sort_by_remaining: true,
            show_source_name: false,
            show_target_name: false,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// DOT Tracker Overlay Config
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration for the multi-target DOT tracker overlay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DotTrackerConfig {
    /// Maximum targets to track simultaneously
    #[serde(default = "default_max_targets")]
    pub max_targets: u8,
    /// Icon size in pixels
    #[serde(default = "default_small_icon")]
    pub icon_size: u8,
    /// How many seconds to keep a target after last DOT expires
    #[serde(default = "default_prune_delay")]
    pub prune_delay_secs: f32,
    /// Font color for target names
    #[serde(default = "default_font_color")]
    pub font_color: Color,
    /// Show DOT names alongside icons
    #[serde(default)]
    pub show_effect_names: bool,
    /// Show source name (who applied)
    #[serde(default)]
    pub show_source_name: bool,
}

fn default_max_targets() -> u8 {
    6
}
fn default_small_icon() -> u8 {
    20
}
fn default_prune_delay() -> f32 {
    2.0
}

impl Default for DotTrackerConfig {
    fn default() -> Self {
        Self {
            max_targets: 6,
            icon_size: 20,
            prune_delay_secs: 2.0,
            font_color: overlay_colors::WHITE,
            show_effect_names: false,
            show_source_name: false,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Hotkey Settings
// ─────────────────────────────────────────────────────────────────────────────

/// Global hotkey configuration
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct HotkeySettings {
    #[serde(default)]
    pub toggle_visibility: Option<String>,
    #[serde(default)]
    pub toggle_move_mode: Option<String>,
    #[serde(default)]
    pub toggle_rearrange_mode: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Profiles
// ─────────────────────────────────────────────────────────────────────────────

/// Maximum number of profiles a user can create
pub const MAX_PROFILES: usize = 12;

/// A named snapshot of all overlay settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayProfile {
    pub name: String,
    pub settings: OverlaySettings,
}

impl OverlayProfile {
    pub fn new(name: String, settings: OverlaySettings) -> Self {
        Self { name, settings }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Settings (combined)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlaySettings {
    #[serde(default)]
    pub positions: HashMap<String, OverlayPositionConfig>,
    #[serde(default)]
    pub appearances: HashMap<String, OverlayAppearanceConfig>,
    #[serde(default, alias = "visibility")]
    pub enabled: HashMap<String, bool>,
    #[serde(default = "default_true")]
    pub overlays_visible: bool,
    #[serde(default)]
    pub personal_overlay: PersonalOverlayConfig,
    #[serde(default = "default_opacity")]
    pub metric_opacity: u8,
    #[serde(default = "default_true")]
    pub metric_show_empty_bars: bool,
    #[serde(default)]
    pub metric_stack_from_bottom: bool,
    #[serde(default = "default_scaling_factor")]
    pub metric_scaling_factor: f32,
    #[serde(default = "default_opacity")]
    pub personal_opacity: u8,
    #[serde(default)]
    pub class_icons_enabled: bool,
    #[serde(default)]
    pub default_appearances: HashMap<String, OverlayAppearanceConfig>,
    #[serde(default)]
    pub raid_overlay: RaidOverlaySettings,
    #[serde(default = "default_opacity")]
    pub raid_opacity: u8,
    #[serde(default)]
    pub boss_health: BossHealthConfig,
    #[serde(default = "default_opacity")]
    pub boss_health_opacity: u8,
    #[serde(default)]
    pub timer_overlay: TimerOverlayConfig,
    #[serde(default = "default_opacity")]
    pub timer_opacity: u8,
    #[serde(default)]
    pub effects_overlay: TimerOverlayConfig,
    #[serde(default = "default_opacity")]
    pub effects_opacity: u8,
    #[serde(default)]
    pub challenge_overlay: ChallengeOverlayConfig,
    #[serde(default = "default_opacity")]
    pub challenge_opacity: u8,
    #[serde(default)]
    pub alerts_overlay: AlertsOverlayConfig,
    #[serde(default = "default_opacity")]
    pub alerts_opacity: u8,
    #[serde(default, alias = "personal_buffs")]
    pub effects_a: EffectsAConfig,
    #[serde(default = "default_opacity", alias = "personal_buffs_opacity")]
    pub effects_a_opacity: u8,
    #[serde(default, alias = "personal_debuffs")]
    pub effects_b: EffectsBConfig,
    #[serde(default = "default_opacity", alias = "personal_debuffs_opacity")]
    pub effects_b_opacity: u8,
    #[serde(default)]
    pub cooldown_tracker: CooldownTrackerConfig,
    #[serde(default = "default_opacity")]
    pub cooldown_tracker_opacity: u8,
    #[serde(default)]
    pub dot_tracker: DotTrackerConfig,
    #[serde(default = "default_opacity")]
    pub dot_tracker_opacity: u8,
    /// Auto-hide overlays when local player is in a conversation
    #[serde(default)]
    pub hide_during_conversations: bool,
}

impl Default for OverlaySettings {
    fn default() -> Self {
        Self {
            positions: HashMap::new(),
            appearances: HashMap::new(),
            enabled: HashMap::new(),
            overlays_visible: true,
            personal_overlay: PersonalOverlayConfig::default(),
            metric_opacity: 180,
            metric_show_empty_bars: true,
            metric_stack_from_bottom: false,
            metric_scaling_factor: 1.0,
            personal_opacity: 180,
            class_icons_enabled: false,
            default_appearances: HashMap::new(),
            raid_overlay: RaidOverlaySettings::default(),
            raid_opacity: 180,
            boss_health: BossHealthConfig::default(),
            boss_health_opacity: 180,
            timer_overlay: TimerOverlayConfig::default(),
            timer_opacity: 180,
            effects_overlay: TimerOverlayConfig::default(),
            effects_opacity: 180,
            challenge_overlay: ChallengeOverlayConfig::default(),
            challenge_opacity: 180,
            alerts_overlay: AlertsOverlayConfig::default(),
            alerts_opacity: 180,
            effects_a: EffectsAConfig::default(),
            effects_a_opacity: 180,
            effects_b: EffectsBConfig::default(),
            effects_b_opacity: 180,
            cooldown_tracker: CooldownTrackerConfig::default(),
            cooldown_tracker_opacity: 180,
            dot_tracker: DotTrackerConfig::default(),
            dot_tracker_opacity: 180,
            hide_during_conversations: false,
        }
    }
}

impl OverlaySettings {
    pub fn get_position(&self, overlay_type: &str) -> OverlayPositionConfig {
        self.positions
            .get(overlay_type)
            .cloned()
            .unwrap_or_default()
    }

    pub fn set_position(&mut self, overlay_type: &str, config: OverlayPositionConfig) {
        self.positions.insert(overlay_type.to_string(), config);
    }

    pub fn get_appearance(&self, overlay_type: &str) -> OverlayAppearanceConfig {
        self.appearances
            .get(overlay_type)
            .cloned()
            .unwrap_or_default()
    }

    pub fn set_appearance(&mut self, overlay_type: &str, config: OverlayAppearanceConfig) {
        self.appearances.insert(overlay_type.to_string(), config);
    }

    pub fn is_enabled(&self, overlay_type: &str) -> bool {
        self.enabled.get(overlay_type).copied().unwrap_or(false)
    }

    pub fn set_enabled(&mut self, overlay_type: &str, enabled: bool) {
        self.enabled.insert(overlay_type.to_string(), enabled);
    }

    pub fn enabled_types(&self) -> Vec<String> {
        self.enabled
            .iter()
            .filter_map(|(k, &v)| if v { Some(k.clone()) } else { None })
            .collect()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// App Config
// ─────────────────────────────────────────────────────────────────────────────

/// Audio settings for timer alerts and countdowns
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioSettings {
    /// Master enable for all audio
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Volume level (0-100)
    #[serde(default = "default_audio_volume")]
    pub volume: u8,

    /// Enable countdown sounds (e.g., "Shield 3... 2... 1...")
    #[serde(default = "default_true")]
    pub countdown_enabled: bool,

    /// Enable alert speech when timers fire
    #[serde(default = "default_true")]
    pub alerts_enabled: bool,
}

fn default_audio_volume() -> u8 {
    80
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            volume: 80,
            countdown_enabled: true,
            alerts_enabled: true,
        }
    }
}

/// Parsely.io upload settings
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ParselySettings {
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub guild: String,
}

///
/// Note: Persistence methods (load/save) are provided by baras-core via the
/// `AppConfigExt` trait, as they require platform-specific dependencies.
/// The frontend derives Default (getting empty values) which is fine for deserialization.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub log_directory: String,
    #[serde(default)]
    pub auto_delete_empty_files: bool,
    #[serde(default)]
    pub auto_delete_old_files: bool,
    #[serde(default = "default_retention_days")]
    pub log_retention_days: u32,
    #[serde(default = "default_true")]
    pub minimize_to_tray: bool,
    #[serde(default)]
    pub overlay_settings: OverlaySettings,
    #[serde(default)]
    pub hotkeys: HotkeySettings,
    #[serde(default)]
    pub profiles: Vec<OverlayProfile>,
    #[serde(default)]
    pub active_profile_name: Option<String>,
    #[serde(default)]
    pub parsely: ParselySettings,
    #[serde(default)]
    pub audio: AudioSettings,
    #[serde(default)]
    pub show_only_bosses: bool,

    /// Hide log files smaller than 1MB in the file browser (enabled by default).
    #[serde(default = "default_true")]
    pub hide_small_log_files: bool,

    /// Player alacrity percentage (e.g., 15.4 for 15.4% alacrity).
    /// Used to calculate actual effect durations.
    #[serde(default)]
    pub alacrity_percent: f32,

    /// Average network latency in milliseconds (e.g., 50 for 50ms).
    /// Used to adjust effect duration calculations.
    #[serde(default)]
    pub latency_ms: u16,
}

fn default_retention_days() -> u32 {
    21
}

impl AppConfig {
    /// Create a new AppConfig with the specified log directory.
    /// Other fields use their default values.
    pub fn with_log_directory(log_directory: String) -> Self {
        Self {
            log_directory,
            auto_delete_empty_files: false,
            auto_delete_old_files: false,
            log_retention_days: 21,
            minimize_to_tray: false,
            overlay_settings: OverlaySettings::default(),
            hotkeys: HotkeySettings::default(),
            profiles: Vec::new(),
            active_profile_name: None,
            parsely: ParselySettings::default(),
            audio: AudioSettings::default(),
            show_only_bosses: false,
            hide_small_log_files: true,
            alacrity_percent: 0.0,
            latency_ms: 0,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Entity Filter
// ─────────────────────────────────────────────────────────────────────────────

/// Filter for matching entities (used for both source and target filtering).
///
/// Shared between core (for timer/effect matching) and frontend (for UI editing).
/// The actual matching logic lives in core since it requires runtime types.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityFilter {
    /// The local player only
    LocalPlayer,
    /// Local player's companion
    OtherPlayers,
    /// Any player (including local)
    AnyPlayer,
    /// Any companion (any player's)
    AnyCompanion,
    /// Any player or companion
    AnyPlayerOrCompanion,
    /// Group members (players in the local player's group)
    GroupMembers,
    /// Group members except local player
    GroupMembersExceptLocal,
    /// Boss NPCs specifically
    Boss,
    /// Non-boss NPCs (trash mobs / adds)
    NpcExceptBoss,
    /// Any NPC (boss or trash)
    AnyNpc,
    /// Specific entities by selector (IDs, names, or roster aliases)
    Selector(Vec<EntitySelector>),
    /// Any entity whatsoever
    #[default]
    Any,
}

impl EntityFilter {
    /// Get a user-friendly label for this filter
    pub fn label(&self) -> &'static str {
        match self {
            Self::LocalPlayer => "Local Player",
            Self::OtherPlayers => "Other Players",
            Self::AnyPlayer => "Any Player",
            Self::AnyCompanion => "Any Companion",
            Self::AnyPlayerOrCompanion => "Any Player or Companion",
            Self::GroupMembers => "Group Members",
            Self::GroupMembersExceptLocal => "Other Group Members",
            Self::Boss => "Boss",
            Self::NpcExceptBoss => "Adds (Non-Boss)",
            Self::AnyNpc => "Any NPC",
            Self::Selector(_) => "Specific Selector",
            Self::Any => "Any",
        }
    }

    /// Default for trigger source/target (any entity)
    pub fn default_any() -> Self {
        Self::Any
    }

    /// Returns true if this filter matches anything (no restriction)
    pub fn is_any(&self) -> bool {
        matches!(self, Self::Any)
    }

    /// Returns true if this is the LocalPlayer filter
    pub fn is_local_player(&self) -> bool {
        matches!(self, Self::LocalPlayer)
    }

    /// Returns true if this is the Boss filter
    pub fn is_boss(&self) -> bool {
        matches!(self, Self::Boss)
    }

    /// Check if this filter matches a specific NPC by class ID
    pub fn matches_npc_id(&self, npc_id: i64) -> bool {
        match self {
            Self::Selector(selectors) => selectors
                .iter()
                .any(|s| matches!(s, EntitySelector::Id(id) if *id == npc_id)),
            Self::AnyNpc | Self::Boss | Self::NpcExceptBoss | Self::Any => true,
            _ => false,
        }
    }

    /// Check if this filter matches by name (case insensitive)
    pub fn matches_name(&self, name: &str) -> bool {
        match self {
            Self::Selector(selectors) => selectors
                .iter()
                .any(|s| matches!(s, EntitySelector::Name(n) if n.eq_ignore_ascii_case(name))),
            Self::Any => true,
            _ => false,
        }
    }

    /// Common options for source/target dropdowns (challenges)
    pub fn common_options() -> &'static [EntityFilter] {
        &[
            Self::Boss,
            Self::NpcExceptBoss,
            Self::AnyNpc,
            Self::AnyPlayer,
            Self::LocalPlayer,
            Self::Any,
        ]
    }

    /// Get the snake_case type name for serialization
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::LocalPlayer => "local_player",
            Self::OtherPlayers => "other_players",
            Self::AnyPlayer => "any_player",
            Self::AnyCompanion => "any_companion",
            Self::AnyPlayerOrCompanion => "any_player_or_companion",
            Self::GroupMembers => "group_members",
            Self::GroupMembersExceptLocal => "group_members_except_local",
            Self::Boss => "boss",
            Self::NpcExceptBoss => "npc_except_boss",
            Self::AnyNpc => "any_npc",
            Self::Selector(_) => "selector",
            Self::Any => "any",
        }
    }

    /// All filters for source field (timers/effects/triggers)
    pub fn source_options() -> &'static [EntityFilter] {
        &[
            Self::Any,
            Self::LocalPlayer,
            Self::OtherPlayers,
            Self::AnyPlayer,
            Self::AnyCompanion,
            Self::AnyPlayerOrCompanion,
            Self::GroupMembers,
            Self::GroupMembersExceptLocal,
            Self::Boss,
            Self::NpcExceptBoss,
            Self::AnyNpc,
        ]
    }

    /// All filters for target field (timers/effects/triggers)
    pub fn target_options() -> &'static [EntityFilter] {
        &[
            Self::Any,
            Self::LocalPlayer,
            Self::OtherPlayers,
            Self::AnyPlayer,
            Self::AnyCompanion,
            Self::AnyPlayerOrCompanion,
            Self::GroupMembers,
            Self::GroupMembersExceptLocal,
            Self::Boss,
            Self::NpcExceptBoss,
            Self::AnyNpc,
        ]
    }
}
