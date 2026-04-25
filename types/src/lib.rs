//! Shared configuration types for BARAS
//!
//! This crate contains serializable configuration types that are shared between
//! the native backend (baras-core) and the WASM frontend (app-ui).

pub mod formatting;

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
}

/// Query result for time-series data (DPS/HPS over time).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimeSeriesPoint {
    pub bucket_start_ms: i64,
    pub total_value: f64,
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

/// A player death event for the death tracker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerDeath {
    /// Player name
    pub name: String,
    /// Time of death in seconds from combat start
    pub death_time_secs: f32,
}

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
}

/// Filter options for combat log event types.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CombatLogFilters {
    /// Show damage events
    pub damage: bool,
    /// Show healing events
    pub healing: bool,
    /// Show action events (AbilityActivate/Deactivate/Interrupt)
    pub actions: bool,
    /// Show effect events (buff/debuff gained/lost)
    pub effects: bool,
    /// Show other events (TargetSet, Death, EnterCombat, etc.)
    pub other: bool,
}

/// Entity names grouped by type for combat log filter dropdowns.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GroupedEntityNames {
    /// Players and companions
    pub friendly: Vec<String>,
    /// NPCs/enemies
    pub npcs: Vec<String>,
}

/// A match result from the combat log find feature.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CombatLogFindMatch {
    /// Position in the filtered result set (for scrolling)
    pub pos: u64,
    /// Row index / line_number (for highlighting)
    pub row_idx: u64,
}

// ─────────────────────────────────────────────────────────────────────────────
// Rotation Analysis Types
// ─────────────────────────────────────────────────────────────────────────────

/// A single ability activation event for rotation analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RotationEvent {
    pub time_secs: f32,
    pub ability_id: i64,
    pub ability_name: String,
}

/// A GCD slot: one on-GCD ability plus any off-GCD abilities weaved with it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GcdSlot {
    pub gcd_ability: RotationEvent,
    pub off_gcd: Vec<RotationEvent>,
    /// Seconds since the previous GCD activation (None for the first slot).
    pub gcd_gap: Option<f32>,
}

/// One rotation cycle (anchor-to-anchor).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RotationCycle {
    pub slots: Vec<GcdSlot>,
    pub duration_secs: f32,
    pub total_damage: f64,
    pub effective_heal: f64,
    pub crit_count: i64,
    pub hit_count: i64,
}

/// Full rotation analysis result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RotationAnalysis {
    pub cycles: Vec<RotationCycle>,
    /// Distinct abilities for the anchor dropdown: (ability_id, ability_name).
    pub abilities: Vec<(i64, String)>,
}

/// Per-ability usage statistics for a single player.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AbilityUsageRow {
    pub ability_name: String,
    pub ability_id: i64,
    pub cast_count: i64,
    /// Seconds from combat start of first cast.
    pub first_cast_secs: f32,
    /// Seconds from combat start of last cast.
    pub last_cast_secs: f32,
    /// Average time between consecutive casts (0.0 if < 2 casts).
    pub avg_time_between: f32,
    /// Median time between consecutive casts (0.0 if < 2 casts).
    pub median_time_between: f32,
    /// Minimum time between consecutive casts (0.0 if < 2 casts).
    pub min_time_between: f32,
    /// Maximum time between consecutive casts (0.0 if < 2 casts).
    pub max_time_between: f32,
    /// Raw cast timestamps (combat_time_secs) for timeline visualization.
    pub timestamps: Vec<f32>,
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

/// When an ability can trigger a refresh
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefreshTrigger {
    /// Refresh on ability activation (default)
    #[default]
    Activation,
    /// Refresh on heal completion (for abilities with cast time)
    Heal,
}

/// An ability that can refresh an effect, with optional conditions.
/// Supports both simple syntax (just ability ID/name) and conditional syntax.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RefreshAbility {
    /// Simple ability selector (backward compatible): just ID or name
    Simple(AbilitySelector),
    /// Ability with conditions
    Conditional {
        /// The ability that triggers refresh
        ability: AbilitySelector,
        /// Minimum stacks required for refresh (None = any stack count)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min_stacks: Option<u8>,
        /// When the refresh triggers
        #[serde(default)]
        trigger: RefreshTrigger,
    },
}

impl RefreshAbility {
    /// Get the ability selector
    pub fn ability(&self) -> &AbilitySelector {
        match self {
            Self::Simple(selector) => selector,
            Self::Conditional { ability, .. } => ability,
        }
    }

    /// Get minimum stacks requirement (None = any)
    pub fn min_stacks(&self) -> Option<u8> {
        match self {
            Self::Simple(_) => None,
            Self::Conditional { min_stacks, .. } => *min_stacks,
        }
    }

    /// Get the trigger type
    pub fn trigger(&self) -> RefreshTrigger {
        match self {
            Self::Simple(_) => RefreshTrigger::Activation,
            Self::Conditional { trigger, .. } => *trigger,
        }
    }

    /// Check if this ability matches the given ID or name
    pub fn matches(&self, id: u64, name: Option<&str>) -> bool {
        self.ability().matches(id, name)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Alert Types (shared across effects and timers)
// ─────────────────────────────────────────────────────────────────────────────

/// When to trigger an alert notification.
///
/// Used by both the effect and timer systems to control when alert text
/// is displayed. For effects: OnApply = effect starts, OnExpire = effect ends.
/// For timers: OnApply = timer starts, OnExpire = timer expires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertTrigger {
    /// No alert
    #[default]
    None,
    /// Alert on start (effect applied / timer started)
    OnApply,
    /// Alert on end (effect expired / timer expired)
    OnExpire,
}

impl AlertTrigger {
    /// Returns all variants for UI dropdowns.
    pub fn all() -> &'static [AlertTrigger] {
        &[Self::None, Self::OnApply, Self::OnExpire]
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
// Effect Stack Tracking (for counter-based effect stack monitoring)
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration for automatic effect stack tracking on a counter.
///
/// When a counter has this config, it bypasses normal increment/decrement
/// triggers and instead automatically updates based on game effect events
/// (ApplyEffect, ModifyCharges, RemoveEffect).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectStackConfig {
    /// Which effects to track (by ID or name)
    pub effects: Vec<EffectSelector>,

    /// Who has the effect (required — determines which entities' stacks to track)
    #[serde(default = "EntityFilter::default_any")]
    pub target: EntityFilter,

    /// How to aggregate when multiple entities match the target filter
    #[serde(default)]
    pub aggregation: StackAggregation,
}

/// How to aggregate stack counts across multiple entities
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StackAggregation {
    /// Maximum stacks on any matching entity (most common — "someone has N stacks")
    #[default]
    Max,
    /// Sum of stacks across all matching entities
    Sum,
    /// Minimum stacks on any matching entity (0 if no entity has the effect)
    Min,
}

// ─────────────────────────────────────────────────────────────────────────────
// Mitigation / Defense Type
// ─────────────────────────────────────────────────────────────────────────────

/// Defense result that reduced or negated damage.
///
/// Maps directly to the game's `defense_type_id` log field.
/// Used as an optional filter on `Trigger::DamageTaken` to fire only when a
/// specific mitigation result occurs (e.g., only on IMMUNE, only on RESIST).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MitigationType {
    Miss,
    Parry,
    Dodge,
    Immune,
    Resist,
    Deflect,
    Shield,
    Absorbed,
    Cover,
    Reflected,
}

impl MitigationType {
    /// Returns the game's numeric `defense_type_id` for this mitigation result.
    pub fn defense_type_id(self) -> i64 {
        match self {
            Self::Miss => 836045448945502,
            Self::Parry => 836045448945503,
            Self::Dodge => 836045448945505,
            Self::Immune => 836045448945506,
            Self::Resist => 836045448945507,
            Self::Deflect => 836045448945508,
            Self::Shield => 836045448945509,
            Self::Absorbed => 836045448945511,
            Self::Cover => 836045448945510,
            Self::Reflected => 836045448953649,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Miss => "Miss",
            Self::Parry => "Parry",
            Self::Dodge => "Dodge",
            Self::Immune => "Immune",
            Self::Resist => "Resist",
            Self::Deflect => "Deflect",
            Self::Shield => "Shield",
            Self::Absorbed => "Absorbed",
            Self::Cover => "Cover",
            Self::Reflected => "Reflected",
        }
    }

    pub const ALL: &'static [Self] = &[
        Self::Miss,
        Self::Parry,
        Self::Dodge,
        Self::Immune,
        Self::Resist,
        Self::Deflect,
        Self::Shield,
        Self::Absorbed,
        Self::Cover,
        Self::Reflected,
    ];
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
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Trigger {
    // ─── Combat State [TPC] ────────────────────────────────────────────────
    /// Combat starts. [TPC]
    #[default]
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
        /// Optional mitigation filter — if non-empty, only fires when the hit
        /// result matches one of the listed types (e.g. IMMUNE, RESIST).
        /// Empty (default) matches any hit result.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        mitigation: Vec<MitigationType>,
    },

    /// Healing is received from an ability. [TPC]
    HealingTaken {
        #[serde(default)]
        abilities: Vec<AbilitySelector>,
        #[serde(default)]
        source: EntityFilter,
        #[serde(default)]
        target: EntityFilter,
    },

    /// Threat is modified by an ability (MODIFYTHREAT or TAUNT). [TPC]
    ThreatModified {
        /// Ability selectors. Empty matches any ability.
        #[serde(default)]
        abilities: Vec<AbilitySelector>,
        /// Who generated the threat (default: any)
        #[serde(default = "EntityFilter::default_any")]
        source: EntityFilter,
        /// Who received the threat change (default: any)
        #[serde(default = "EntityFilter::default_any")]
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

    /// Counter value changes (any change, not just threshold crossing). [TPC]
    CounterChanges { counter_id: String },

    // ─── Timer Events [T only] ─────────────────────────────────────────────
    /// Another timer expires (chaining). [T only]
    TimerExpires { timer_id: String },

    /// Another timer starts (for cancellation). [T only]
    TimerStarted { timer_id: String },

    /// A timer has been canceled
    TimerCanceled { timer_id: String },

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
            Self::HealingTaken { .. } => "Healing Taken",
            Self::ThreatModified { .. } => "Threat Modified",
            Self::BossHpBelow { .. } => "Boss HP Below",
            Self::BossHpAbove { .. } => "Boss HP Above",
            Self::NpcAppears { .. } => "NPC Appears",
            Self::EntityDeath { .. } => "Entity Death",
            Self::TargetSet { .. } => "Target Set",
            Self::PhaseEntered { .. } => "Phase Entered",
            Self::PhaseEnded { .. } => "Phase Ended",
            Self::AnyPhaseChange => "Any Phase Change",
            Self::CounterReaches { .. } => "Counter Reaches",
            Self::CounterChanges { .. } => "Counter Changes",
            Self::TimerExpires { .. } => "Timer Expires",
            Self::TimerStarted { .. } => "Timer Started",
            Self::TimerCanceled { .. } => "Timer Canceled",
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
            Self::HealingTaken { .. } => "healing_taken",
            Self::ThreatModified { .. } => "threat_modified",
            Self::BossHpBelow { .. } => "boss_hp_below",
            Self::BossHpAbove { .. } => "boss_hp_above",
            Self::NpcAppears { .. } => "npc_appears",
            Self::EntityDeath { .. } => "entity_death",
            Self::TargetSet { .. } => "target_set",
            Self::PhaseEntered { .. } => "phase_entered",
            Self::PhaseEnded { .. } => "phase_ended",
            Self::AnyPhaseChange => "any_phase_change",
            Self::CounterReaches { .. } => "counter_reaches",
            Self::CounterChanges { .. } => "counter_changes",
            Self::TimerExpires { .. } => "timer_expires",
            Self::TimerStarted { .. } => "timer_started",
            Self::TimerCanceled { .. } => "timer_canceled",
            Self::TimeElapsed { .. } => "time_elapsed",
            Self::Manual => "manual",
            Self::Never => "never",
            Self::AnyOf { .. } => "any_of",
        }
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
            "abs" => HPS,
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

/// Which icon to show next to player names in metric overlays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ClassIconMode {
    /// No icon
    None,
    /// Class silhouette icon (role-tinted)
    #[default]
    Class,
    /// Discipline-specific icon (full color)
    Discipline,
}

// ─────────────────────────────────────────────────────────────────────────────
// Class Color Config
// ─────────────────────────────────────────────────────────────────────────────

/// Per-archetype bar colors for metric overlays.
///
/// Each field covers a mirror-class pair (Imperial / Republic).
/// Used when `use_class_color` is enabled on a metric overlay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassColorConfig {
    /// Sorcerer / Sage
    #[serde(default = "default_class_color_sorcerer")]
    pub sorcerer_sage: Color,
    /// Assassin / Shadow
    #[serde(default = "default_class_color_assassin")]
    pub assassin_shadow: Color,
    /// Juggernaut / Guardian
    #[serde(default = "default_class_color_juggernaut")]
    pub juggernaut_guardian: Color,
    /// Marauder / Sentinel
    #[serde(default = "default_class_color_marauder")]
    pub marauder_sentinel: Color,
    /// Mercenary / Commando
    #[serde(default = "default_class_color_mercenary")]
    pub mercenary_commando: Color,
    /// Powertech / Vanguard
    #[serde(default = "default_class_color_powertech")]
    pub powertech_vanguard: Color,
    /// Operative / Scoundrel
    #[serde(default = "default_class_color_operative")]
    pub operative_scoundrel: Color,
    /// Sniper / Gunslinger
    #[serde(default = "default_class_color_sniper")]
    pub sniper_gunslinger: Color,
}

fn default_class_color_sorcerer() -> Color { [128, 64, 192, 255] }   // violet
fn default_class_color_assassin() -> Color { [90, 48, 128, 255] }    // dark purple
fn default_class_color_juggernaut() -> Color { [192, 48, 48, 255] }  // crimson
fn default_class_color_marauder() -> Color { [208, 80, 32, 255] }    // red-orange
fn default_class_color_mercenary() -> Color { [64, 128, 64, 255] }   // green
fn default_class_color_powertech() -> Color { [192, 104, 32, 255] }  // orange
fn default_class_color_operative() -> Color { [96, 120, 48, 255] }   // olive
fn default_class_color_sniper() -> Color { [192, 160, 32, 255] }     // gold

impl Default for ClassColorConfig {
    fn default() -> Self {
        Self {
            sorcerer_sage: default_class_color_sorcerer(),
            assassin_shadow: default_class_color_assassin(),
            juggernaut_guardian: default_class_color_juggernaut(),
            marauder_sentinel: default_class_color_marauder(),
            mercenary_commando: default_class_color_mercenary(),
            powertech_vanguard: default_class_color_powertech(),
            operative_scoundrel: default_class_color_operative(),
            sniper_gunslinger: default_class_color_sniper(),
        }
    }
}

impl ClassColorConfig {
    /// Look up the bar color for a class by its display name.
    ///
    /// Accepts both Imperial and Republic class names (e.g., "Sorcerer" or "Sage").
    /// Returns `None` if the name is unrecognized — callers should fall back to
    /// the configured bar color.
    pub fn for_class_name(&self, name: &str) -> Option<Color> {
        match name {
            "Sorcerer" | "Sage" => Some(self.sorcerer_sage),
            "Assassin" | "Shadow" => Some(self.assassin_shadow),
            "Juggernaut" | "Guardian" => Some(self.juggernaut_guardian),
            "Marauder" | "Sentinel" => Some(self.marauder_sentinel),
            "Mercenary" | "Commando" => Some(self.mercenary_commando),
            "Powertech" | "Vanguard" => Some(self.powertech_vanguard),
            "Operative" | "Scoundrel" => Some(self.operative_scoundrel),
            "Sniper" | "Gunslinger" => Some(self.sniper_gunslinger),
            _ => None,
        }
    }
}

/// Per-overlay appearance configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayAppearanceConfig {
    #[serde(default = "default_true")]
    pub show_header: bool,
    #[serde(default = "default_true")]
    pub show_footer: bool,
    #[serde(default = "default_true")]
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
    /// Color each player's bar using their class color (from global ClassColorConfig).
    /// Falls back to `bar_color` when class is unknown.
    #[serde(default)]
    pub use_class_color: bool,
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
            show_class_icons: true,
            font_color: overlay_colors::WHITE,
            bar_color: overlay_colors::DPS,
            max_entries: 16,
            show_total: false,
            show_per_second: true,
            show_percent: true,
            show_duration: true,
            use_class_color: false,
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

/// Category for personal stats (used for auto-coloring and grouping)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PersonalStatCategory {
    /// Contextual info (encounter name, difficulty, time, spec, phase)
    Info,
    /// Damage metrics
    Damage,
    /// Healing metrics
    Healing,
    /// Damage taken / mitigation metrics
    Mitigation,
    /// Threat metrics
    Threat,
    /// Defensive tank stats (defense %, shield %)
    Defensive,
    /// Utility metrics (APM)
    Utility,
}

/// Stats that can be displayed on the personal overlay.
///
/// Compound groups (e.g., `DamageGroup`) render multiple values in a single row.
/// Info stats and APM remain as single-value rows.
///
/// Legacy individual metric variants (Dps, Hps, etc.) are preserved for config
/// compatibility but render as no-ops (skipped during display). Users should
/// manually switch to compound groups in their settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PersonalStat {
    // ── Info (single-value rows) ──
    EncounterName,
    Difficulty,
    EncounterTime,
    EncounterCount,
    ClassDiscipline,

    // ── Compound metric groups ──
    /// DPS  |  Total  |  Crit: X%
    DamageGroup,
    /// Boss DPS  |  Boss Total
    BossDamageGroup,
    /// HPS  |  eHPS  |  Eff: X%
    HealingGroup,
    /// Total  |  Total Eff  |  Crit: X%
    HealingAdvanced,
    /// TPS  |  Total
    ThreatGroup,
    /// eDTPS  |  Total Taken
    MitigationGroup,
    /// Def: X%  |  Shield: X%
    DefensiveGroup,
    /// Phase  |  Time
    PhaseGroup,

    // ── Standalone metric ──
    Apm,

    // ── Layout ──
    /// Visual separator line (not a real stat)
    Separator,

    // ── Legacy variants (no-ops, kept for config compatibility) ──
    // These deserialize from old configs but are skipped during rendering.
    // Not shown in the "add stats" UI. Users should switch to compound groups.
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
    Phase,
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
            Self::ClassDiscipline => "Spec",
            Self::DamageGroup => "Damage",
            Self::BossDamageGroup => "Boss Dmg",
            Self::HealingGroup => "Healing",
            Self::HealingAdvanced => "Heal+",
            Self::ThreatGroup => "Threat",
            Self::MitigationGroup => "DTPS",
            Self::DefensiveGroup => "Defense",
            Self::PhaseGroup => "Phase",
            Self::Apm => "APM",
            Self::Separator => "── Separator ──",
            // Legacy no-ops
            Self::Dps => "DPS (legacy)",
            Self::EDps => "eDPS (legacy)",
            Self::BossDps => "Boss DPS (legacy)",
            Self::TotalDamage => "Total Damage (legacy)",
            Self::BossDamage => "Boss Damage (legacy)",
            Self::Hps => "HPS (legacy)",
            Self::EHps => "eHPS (legacy)",
            Self::TotalHealing => "Total Healing (legacy)",
            Self::Dtps => "eDTPS (legacy)",
            Self::Tps => "TPS (legacy)",
            Self::TotalThreat => "Total Threat (legacy)",
            Self::DamageCritPct => "Dmg Crit % (legacy)",
            Self::HealCritPct => "Heal Crit % (legacy)",
            Self::EffectiveHealPct => "Eff Heal % (legacy)",
            Self::Phase => "Phase (legacy)",
            Self::PhaseTime => "Phase Time (legacy)",
        }
    }

    /// Whether this stat is a legacy no-op (kept for config compatibility only)
    pub fn is_legacy(&self) -> bool {
        matches!(
            self,
            Self::Dps
                | Self::EDps
                | Self::BossDps
                | Self::TotalDamage
                | Self::BossDamage
                | Self::Hps
                | Self::EHps
                | Self::TotalHealing
                | Self::Dtps
                | Self::Tps
                | Self::TotalThreat
                | Self::DamageCritPct
                | Self::HealCritPct
                | Self::EffectiveHealPct
                | Self::Phase
                | Self::PhaseTime
        )
    }

    /// Get the category for this stat (used for auto-coloring)
    pub fn category(&self) -> PersonalStatCategory {
        match self {
            Self::EncounterName
            | Self::Difficulty
            | Self::EncounterTime
            | Self::EncounterCount
            | Self::ClassDiscipline => PersonalStatCategory::Info,

            Self::DamageGroup | Self::BossDamageGroup => PersonalStatCategory::Damage,

            Self::HealingGroup | Self::HealingAdvanced => PersonalStatCategory::Healing,

            Self::MitigationGroup => PersonalStatCategory::Mitigation,

            Self::ThreatGroup => PersonalStatCategory::Threat,

            Self::DefensiveGroup => PersonalStatCategory::Defensive,

            Self::PhaseGroup => PersonalStatCategory::Info,

            Self::Apm => PersonalStatCategory::Utility,

            Self::Separator => PersonalStatCategory::Info,

            // Legacy no-ops — map to their original categories
            Self::Dps | Self::EDps | Self::BossDps | Self::TotalDamage | Self::BossDamage => {
                PersonalStatCategory::Damage
            }
            Self::Hps
            | Self::EHps
            | Self::TotalHealing
            | Self::HealCritPct
            | Self::EffectiveHealPct => PersonalStatCategory::Healing,
            Self::DamageCritPct => PersonalStatCategory::Damage,
            Self::Dtps => PersonalStatCategory::Mitigation,
            Self::Tps | Self::TotalThreat => PersonalStatCategory::Threat,
            Self::Phase | Self::PhaseTime => PersonalStatCategory::Info,
        }
    }

    /// Whether this stat is informational context (not a combat metric)
    pub fn is_info(&self) -> bool {
        matches!(
            self,
            Self::EncounterName
                | Self::Difficulty
                | Self::EncounterTime
                | Self::EncounterCount
                | Self::ClassDiscipline
        )
    }

    /// Whether this stat renders as a compound multi-value row
    pub fn is_compound(&self) -> bool {
        matches!(
            self,
            Self::DamageGroup
                | Self::BossDamageGroup
                | Self::HealingGroup
                | Self::HealingAdvanced
                | Self::ThreatGroup
                | Self::MitigationGroup
                | Self::DefensiveGroup
                | Self::PhaseGroup
        )
    }

    /// Get all stats in display order
    pub fn all() -> &'static [PersonalStat] {
        &[
            Self::EncounterName,
            Self::Difficulty,
            Self::EncounterTime,
            Self::EncounterCount,
            Self::ClassDiscipline,
            Self::DamageGroup,
            Self::BossDamageGroup,
            Self::HealingGroup,
            Self::HealingAdvanced,
            Self::ThreatGroup,
            Self::MitigationGroup,
            Self::DefensiveGroup,
            Self::PhaseGroup,
            Self::Apm,
            Self::Separator,
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
    /// Font scale multiplier (1.0 - 2.0, default 1.0)
    #[serde(default = "default_scaling_factor")]
    pub font_scale: f32,
    /// When true, background shrinks to fit content instead of filling the window
    #[serde(default)]
    pub dynamic_background: bool,
    /// When true, value text is automatically colored by stat category
    /// (red for damage, green for healing, orange for mitigation, blue for threat)
    #[serde(default = "default_true")]
    pub auto_color_values: bool,
    /// Line spacing multiplier (0.7 - 1.5, default 1.0)
    #[serde(default = "default_scaling_factor")]
    pub line_spacing: f32,
    /// When true, stats with zero/empty values are hidden
    #[serde(default)]
    pub hide_empty_values: bool,
}

fn default_personal_stats() -> Vec<PersonalStat> {
    vec![
        PersonalStat::EncounterName,
        PersonalStat::Difficulty,
        PersonalStat::EncounterTime,
        PersonalStat::Separator,
        PersonalStat::DamageGroup,
        PersonalStat::HealingGroup,
        PersonalStat::MitigationGroup,
        PersonalStat::Separator,
        PersonalStat::Apm,
    ]
}

impl Default for PersonalOverlayConfig {
    fn default() -> Self {
        Self {
            visible_stats: default_personal_stats(),
            font_color: overlay_colors::WHITE,
            label_color: overlay_colors::WHITE,
            font_scale: 1.0,
            dynamic_background: false,
            auto_color_values: true,
            line_spacing: 1.0,
            hide_empty_values: false,
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
    #[serde(default)]
    pub show_class_icons: bool,
    #[serde(default = "default_effect_fill_opacity")]
    pub effect_fill_opacity: u8,
    #[serde(default)]
    pub show_effect_icons: bool,
    #[serde(default = "default_frame_spacing")]
    pub frame_spacing: f32,
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
fn default_frame_spacing() -> f32 {
    4.0
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
            show_class_icons: false,
            effect_fill_opacity: 255,
            show_effect_icons: false,
            frame_spacing: 4.0,
        }
    }
}

impl RaidOverlaySettings {
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
    /// Font scale multiplier (1.0 - 2.0, default 1.0)
    #[serde(default = "default_scaling_factor")]
    pub font_scale: f32,
    /// When true, background shrinks to fit content instead of filling the window
    #[serde(default)]
    pub dynamic_background: bool,
    /// When true (default), boss health clears after combat ends
    #[serde(default = "default_true")]
    pub clear_after_combat: bool,
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
            font_scale: 1.0,
            dynamic_background: false,
            clear_after_combat: true,
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
    /// Font scale multiplier (1.0 - 2.0, default 1.0)
    #[serde(default = "default_scaling_factor")]
    pub font_scale: f32,
    /// When true, background shrinks to fit content instead of filling the window
    #[serde(default)]
    pub dynamic_background: bool,
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
            font_scale: 1.0,
            dynamic_background: false,
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
    /// Show ability icon to the left of alert text (default true)
    #[serde(default = "default_true")]
    pub show_icons: bool,
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
            show_icons: true,
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
    /// Show total value and percent
    TotalPercent,
    /// Show total value and per-second rate
    TotalPerSecond,
    /// Show per-second rate and percent (default)
    #[default]
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
    /// When true, show grey background bar behind each player's fill bar
    #[serde(default)]
    pub show_background_bar: bool,
    /// Font scale multiplier (1.0 - 2.0, default 1.0)
    #[serde(default = "default_scaling_factor")]
    pub font_scale: f32,
    /// When true, background shrinks to fit content instead of filling the window
    #[serde(default)]
    pub dynamic_background: bool,
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
            show_background_bar: false,
            font_scale: 1.0,
            dynamic_background: false,
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
    /// Render as stacked progress bars — overrides layout_vertical
    #[serde(default)]
    pub layout_bar: bool,
    /// Show effect names below/beside icons
    #[serde(default)]
    pub show_effect_names: bool,
    /// Show countdown text on icons
    #[serde(default = "default_true")]
    pub show_countdown: bool,
    /// When true, stacks are shown large and centered; timer is secondary
    #[serde(default)]
    pub stack_priority: bool,
    /// Show header title above overlay
    #[serde(default)]
    pub show_header: bool,
    /// Font scale multiplier (1.0 - 2.0, default 1.0)
    #[serde(default = "default_scaling_factor")]
    pub font_scale: f32,
    /// When true, background shrinks to fit content instead of filling the window
    #[serde(default)]
    pub dynamic_background: bool,
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
            layout_bar: false,
            show_effect_names: false,
            show_countdown: true,
            stack_priority: false,
            show_header: false,
            font_scale: 1.0,
            dynamic_background: false,
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
    /// Render as stacked progress bars — overrides layout_vertical
    #[serde(default)]
    pub layout_bar: bool,
    /// Show effect names below/beside icons
    #[serde(default)]
    pub show_effect_names: bool,
    /// Show countdown text on icons
    #[serde(default = "default_true")]
    pub show_countdown: bool,
    /// When true, stacks are shown large and centered; timer is secondary
    #[serde(default)]
    pub stack_priority: bool,
    /// Show header title above overlay
    #[serde(default)]
    pub show_header: bool,
    /// Font scale multiplier (1.0 - 2.0, default 1.0)
    #[serde(default = "default_scaling_factor")]
    pub font_scale: f32,
    /// When true, background shrinks to fit content instead of filling the window
    #[serde(default)]
    pub dynamic_background: bool,
}

impl Default for EffectsBConfig {
    fn default() -> Self {
        Self {
            icon_size: 32,
            max_display: 8,
            layout_vertical: false,
            layout_bar: false,
            show_effect_names: false,
            show_countdown: true,
            stack_priority: false,
            show_header: false,
            font_scale: 1.0,
            dynamic_background: false,
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
    /// Show header title above overlay
    #[serde(default)]
    pub show_header: bool,
    /// Font scale multiplier (1.0 - 2.0, default 1.0)
    #[serde(default = "default_scaling_factor")]
    pub font_scale: f32,
    /// When true, background shrinks to fit content instead of filling the window
    #[serde(default)]
    pub dynamic_background: bool,
    /// Render cooldowns as stacked progress bars instead of icons
    #[serde(default)]
    pub layout_bar: bool,
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
            show_header: false,
            font_scale: 1.0,
            dynamic_background: false,
            layout_bar: false,
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
    /// Show header title above overlay
    #[serde(default)]
    pub show_header: bool,
    /// Show countdown timers on icons
    #[serde(default = "default_true")]
    pub show_countdown: bool,
    /// Font scale multiplier (1.0 - 2.0, default 1.0)
    #[serde(default = "default_scaling_factor")]
    pub font_scale: f32,
    /// When true, background shrinks to fit content instead of filling the window
    #[serde(default)]
    pub dynamic_background: bool,
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
            show_header: false,
            show_countdown: true,
            font_scale: 1.0,
            dynamic_background: false,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Notes Overlay Configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration for the encounter notes overlay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotesOverlayConfig {
    /// Font size for notes text (default 14)
    #[serde(default = "default_notes_font_size")]
    pub font_size: u8,
    /// Font color for notes text
    #[serde(default = "default_font_color")]
    pub font_color: Color,
    #[serde(default)]
    pub dynamic_background: bool,
}

fn default_notes_font_size() -> u8 {
    14
}

impl Default for NotesOverlayConfig {
    fn default() -> Self {
        Self {
            font_size: default_notes_font_size(),
            font_color: overlay_colors::WHITE,
            dynamic_background: false,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Combat Time Overlay Configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration for the standalone combat time overlay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombatTimeOverlayConfig {
    /// Whether to show the "Combat Time" title and separator
    #[serde(default = "default_true")]
    pub show_title: bool,
    /// Font scale multiplier (0.5 - 3.0, default 1.0)
    #[serde(default = "default_scaling_factor")]
    pub font_scale: f32,
    /// Font color (RGBA)
    #[serde(default = "default_font_color")]
    pub font_color: Color,
    /// When true, background shrinks to fit content
    #[serde(default)]
    pub dynamic_background: bool,
    /// When true, overlay clears when combat ends; otherwise keeps last time
    #[serde(default = "default_true")]
    pub clear_after_combat: bool,
}

impl Default for CombatTimeOverlayConfig {
    fn default() -> Self {
        Self {
            show_title: true,
            font_scale: 1.0,
            font_color: overlay_colors::WHITE,
            dynamic_background: false,
            clear_after_combat: true,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Operation Timer Overlay Configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration for the operation timer overlay (persistent timer across encounters)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationTimerOverlayConfig {
    /// Whether to show the title (operation name or "Op Timer") and separator
    #[serde(default = "default_true")]
    pub show_title: bool,
    /// Font scale multiplier (0.5 - 3.0, default 1.0)
    #[serde(default = "default_scaling_factor")]
    pub font_scale: f32,
    /// Font color (RGBA)
    #[serde(default = "default_font_color")]
    pub font_color: Color,
    /// When true, background shrinks to fit content
    #[serde(default)]
    pub dynamic_background: bool,
}

impl Default for OperationTimerOverlayConfig {
    fn default() -> Self {
        Self {
            show_title: true,
            font_scale: 1.0,
            font_color: overlay_colors::WHITE,
            dynamic_background: false,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Ability Queue Overlay Configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration for the ability queue overlay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbilityQueueOverlayConfig {
    /// Maximum entries to display across all tiers
    #[serde(default = "default_max_display")]
    pub max_display: u8,
    /// Font scale multiplier (default 1.0)
    #[serde(default = "default_scaling_factor")]
    pub font_scale: f32,
    /// Font color (RGBA)
    #[serde(default = "default_font_color")]
    pub font_color: Color,
    /// GCD bar accent color (RGBA)
    #[serde(default = "default_gcd_color")]
    pub gcd_color: Color,
    /// When true, background shrinks to fit content
    #[serde(default = "default_true")]
    pub dynamic_background: bool,
}

fn default_max_display() -> u8 { 12 }
fn default_gcd_color() -> Color { [120, 200, 255, 255] }

impl Default for AbilityQueueOverlayConfig {
    fn default() -> Self {
        Self {
            max_display: 12,
            font_scale: 1.0,
            font_color: overlay_colors::WHITE,
            gcd_color: [120, 200, 255, 255],
            dynamic_background: true,
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
    #[serde(default)]
    pub toggle_operation_timer: Option<String>,
    #[serde(default)]
    pub toggle_live_mode: Option<String>,
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
    /// Font scale multiplier for metric overlays (1.0 - 2.0, default 1.0)
    #[serde(default = "default_scaling_factor")]
    pub metric_font_scale: f32,
    /// When true, metric overlay backgrounds shrink to fit content
    #[serde(default)]
    pub metric_dynamic_background: bool,
    /// When true, show grey background bar behind each player's fill bar
    #[serde(default)]
    pub metric_show_background_bar: bool,
    #[serde(default = "default_opacity")]
    pub personal_opacity: u8,
    #[serde(default)]
    pub class_icon_mode: ClassIconMode,
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
    #[serde(default, alias = "timer_overlay")]
    pub timers_a_overlay: TimerOverlayConfig,
    #[serde(default = "default_opacity", alias = "timer_opacity")]
    pub timers_a_opacity: u8,
    #[serde(default)]
    pub timers_b_overlay: TimerOverlayConfig,
    #[serde(default = "default_opacity")]
    pub timers_b_opacity: u8,
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
    #[serde(default)]
    pub notes_overlay: NotesOverlayConfig,
    #[serde(default = "default_opacity")]
    pub notes_opacity: u8,
    #[serde(default)]
    pub combat_time: CombatTimeOverlayConfig,
    #[serde(default = "default_opacity")]
    pub combat_time_opacity: u8,
    #[serde(default)]
    pub operation_timer: OperationTimerOverlayConfig,
    #[serde(default = "default_opacity")]
    pub operation_timer_opacity: u8,
    #[serde(default)]
    pub ability_queue: AbilityQueueOverlayConfig,
    #[serde(default = "default_opacity")]
    pub ability_queue_opacity: u8,
    /// Auto-hide overlays when local player is in a conversation
    #[serde(default)]
    pub hide_during_conversations: bool,
    /// Auto-hide overlays when not in a live session (historical, logged out, etc.)
    #[serde(default)]
    pub hide_when_not_live: bool,
    /// Per-archetype bar colors used when `use_class_color` is enabled on a metric overlay.
    #[serde(default)]
    pub class_colors: ClassColorConfig,
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
            metric_font_scale: 1.0,
            metric_dynamic_background: false,
            metric_show_background_bar: false,
            personal_opacity: 180,
            class_icon_mode: ClassIconMode::Class,
            default_appearances: HashMap::new(),
            class_colors: ClassColorConfig::default(),
            raid_overlay: RaidOverlaySettings::default(),
            raid_opacity: 180,
            boss_health: BossHealthConfig::default(),
            boss_health_opacity: 180,
            timers_a_overlay: TimerOverlayConfig::default(),
            timers_a_opacity: 180,
            timers_b_overlay: TimerOverlayConfig::default(),
            timers_b_opacity: 180,
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
            notes_overlay: NotesOverlayConfig::default(),
            notes_opacity: 180,
            combat_time: CombatTimeOverlayConfig::default(),
            combat_time_opacity: 180,
            operation_timer: OperationTimerOverlayConfig::default(),
            operation_timer_opacity: 180,
            ability_queue: AbilityQueueOverlayConfig::default(),
            ability_queue_opacity: 180,
            hide_during_conversations: false,
            hide_when_not_live: false,
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
    /// Legacy single-guild field. Older configs stored a single guild here;
    /// kept for backwards-compatible deserialization. Migrated into `guilds`
    /// on load and not written back out when empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub guild: String,
    /// All configured guilds the user can upload to.
    #[serde(default)]
    pub guilds: Vec<String>,
    /// Last selected guild (used as the default in the upload modal).
    #[serde(default)]
    pub selected_guild: Option<String>,
}

impl ParselySettings {
    /// Migrate the legacy `guild: String` field into `guilds` + `selected_guild`.
    /// Idempotent — safe to call repeatedly.
    pub fn migrate_legacy(&mut self) {
        if !self.guild.is_empty() {
            if !self.guilds.iter().any(|g| g == &self.guild) {
                self.guilds.push(self.guild.clone());
            }
            if self.selected_guild.is_none() {
                self.selected_guild = Some(self.guild.clone());
            }
            self.guild.clear();
        }
        // Ensure selected_guild references a known guild (or is None).
        if let Some(sel) = &self.selected_guild
            && !self.guilds.iter().any(|g| g == sel)
        {
            self.selected_guild = self.guilds.first().cloned();
        }
    }

    /// Resolve the active guild for upload. Returns the selected guild if it's
    /// in the configured list, otherwise the first configured guild, otherwise None.
    pub fn active_guild(&self) -> Option<&str> {
        if let Some(sel) = self.selected_guild.as_deref()
            && self.guilds.iter().any(|g| g == sel)
        {
            return Some(sel);
        }
        self.guilds.first().map(|s| s.as_str())
    }
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
    pub auto_delete_small_files: bool,
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

    /// Show ability/entity IDs in the combat log.
    #[serde(default)]
    pub show_log_ids: bool,

    /// Hide log files smaller than 1MB in the file browser (enabled by default).
    #[serde(default = "default_true")]
    pub hide_small_log_files: bool,

    /// Player alacrity percentage (e.g., 15.4 for 15.4% alacrity).
    /// Used to calculate actual effect durations.
    #[serde(default = "default_alacrity")]
    pub alacrity_percent: f32,

    /// Average network latency in milliseconds (e.g., 50 for 50ms).
    /// Used to adjust effect duration calculations.
    #[serde(default = "default_latency")]
    pub latency_ms: u16,

    /// Last version for which the changelog was shown.
    /// Used to show "What's New" popup only once per version.
    #[serde(default)]
    pub last_viewed_changelog_version: Option<String>,

    /// Use European number formatting (swap `.` and `,` in numbers).
    /// e.g., `1.50K` becomes `1,50K` and `1,500` becomes `1.500`.
    #[serde(default)]
    pub european_number_format: bool,

    /// Automatically enter live mode when combat starts in the Data Explorer.
    /// When false, live mode must be activated manually via the Live button.
    #[serde(default)]
    pub data_explorer_auto_live: bool,

    /// Default profile name per role for auto-switching on discipline change.
    /// Keys are role names: "Tank", "Healer", "Dps".
    #[serde(default)]
    pub default_profile_per_role: std::collections::HashMap<String, String>,
}

fn default_retention_days() -> u32 {
    21
}

fn default_alacrity() -> f32 {
    7.5
}

fn default_latency() -> u16 {
    80
}

impl AppConfig {
    /// Create a new AppConfig with the specified log directory.
    /// Other fields use their default values.
    pub fn with_log_directory(log_directory: String) -> Self {
        Self {
            log_directory,
            auto_delete_empty_files: false,
            auto_delete_small_files: false,
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
            show_log_ids: false,
            hide_small_log_files: true,
            alacrity_percent: 0.0,
            latency_ms: 0,
            last_viewed_changelog_version: None,
            european_number_format: false,
            data_explorer_auto_live: false,
            default_profile_per_role: std::collections::HashMap::new(),
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
    /// Other players (not local player)
    #[serde(alias = "group_members_except_local")]
    OtherPlayers,
    /// Any player (including local)
    #[serde(alias = "group_members")]
    AnyPlayer,
    /// Any companion (any player's)
    AnyCompanion,
    /// Any player or companion
    AnyPlayerOrCompanion,
    /// Any entity except local player (players, companions, NPCs)
    AnyExceptLocal,
    /// The local player's current target
    CurrentTarget,
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
            Self::AnyExceptLocal => "Any Except Local",
            Self::CurrentTarget => "Current Target",
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
            Self::AnyExceptLocal => "any_except_local",
            Self::CurrentTarget => "current_target",
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
            Self::AnyExceptLocal,
            Self::CurrentTarget,
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
            Self::AnyExceptLocal,
            Self::CurrentTarget,
            Self::Boss,
            Self::NpcExceptBoss,
            Self::AnyNpc,
        ]
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// UI Session State - persists across tab switches
// ─────────────────────────────────────────────────────────────────────────────

/// Global UI session state that persists across tab switches (in-memory only).
/// Lives in the App component and flows down via props/signals.
#[derive(Debug, Clone, PartialEq)]
pub struct UiSessionState {
    /// Currently active main tab
    pub active_tab: MainTab,

    /// Data Explorer state
    pub data_explorer: DataExplorerState,

    /// Combat Log state (within Data Explorer)
    pub combat_log: CombatLogSessionState,

    /// Encounter Builder state
    pub encounter_builder: EncounterBuilderState,

    /// Effects Editor state
    pub effects_editor: EffectsEditorState,

    /// Use European number formatting (swap `.` and `,`)
    pub european_number_format: bool,
}

impl Default for UiSessionState {
    fn default() -> Self {
        Self {
            active_tab: MainTab::default(),
            data_explorer: DataExplorerState::default(),
            combat_log: CombatLogSessionState::default(),
            encounter_builder: EncounterBuilderState::default(),
            effects_editor: EffectsEditorState::default(),
            european_number_format: false,
        }
    }
}

impl UiSessionState {
    /// Reset session-specific state when opening a new file.
    /// Preserves user preferences like show_only_bosses, show_ids, view modes, etc.
    pub fn reset_session(&mut self) {
        self.data_explorer.reset_session();
        self.combat_log.reset_session();
        // Note: encounter_builder and effects_editor are not reset since they
        // contain user configuration, not session-specific data
    }
}

/// Main application tabs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MainTab {
    #[default]
    DataExplorer,
    Overlays,
    EncounterBuilder,
    Effects,
}

impl MainTab {
    /// Convert to string ID used in old code
    pub fn as_str(&self) -> &'static str {
        match self {
            MainTab::DataExplorer => "explorer",
            MainTab::Overlays => "overlays",
            MainTab::EncounterBuilder => "timers",
            MainTab::Effects => "effects",
        }
    }

    /// Parse from string ID used in old code
    pub fn from_str(s: &str) -> Self {
        match s {
            // "session" maps to DataExplorer for backwards compatibility
            "session" | "explorer" => MainTab::DataExplorer,
            "overlays" => MainTab::Overlays,
            "timers" => MainTab::EncounterBuilder,
            "effects" => MainTab::Effects,
            _ => MainTab::DataExplorer,
        }
    }
}

/// Data Explorer view mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewMode {
    #[default]
    Overview,
    Charts,
    CombatLog,
    Detailed(DataTab),
    Rotation,
    Usage,
}

impl ViewMode {
    /// Get the DataTab if in Detailed mode, otherwise None
    pub fn tab(&self) -> Option<DataTab> {
        match self {
            ViewMode::Detailed(tab) => Some(*tab),
            _ => None,
        }
    }
}

/// Sort column for ability breakdown table
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortColumn {
    Target,
    Ability,
    #[default]
    Total,
    Percent,
    Rate,
    Hits,
    Avg,
    CritPct,
    MissPct,
    AvgHit,
    AvgCrit,
    Activations,
    Effective,
    EffectivePct,
    ShieldTotal,
    Sps,
    AttackType,
    DamageType,
    ShldPct,
    Absorbed,
}

/// Sort column for ability usage table
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageSortColumn {
    Ability,
    #[default]
    CastCount,
    FirstCast,
    LastCast,
    AvgTime,
    MedianTime,
    MinTime,
    MaxTime,
}

/// Sort direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    #[default]
    Desc,
    Asc,
}

impl SortDirection {
    pub fn sql(&self) -> &'static str {
        match self {
            SortDirection::Asc => "ASC",
            SortDirection::Desc => "DESC",
        }
    }

    pub fn toggle(&self) -> Self {
        match self {
            SortDirection::Asc => SortDirection::Desc,
            SortDirection::Desc => SortDirection::Asc,
        }
    }
}

/// Sort column for combat log viewer (server-side sort for paginated virtual scrolling)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatLogSortColumn {
    #[default]
    Time,
    Source,
    Target,
    Type,
    Ability,
    Effect,
    Value,
    Absorbed,
    Overheal,
    Threat,
}

impl CombatLogSortColumn {
    pub fn sql_column(&self) -> &'static str {
        match self {
            CombatLogSortColumn::Time => "combat_time_secs",
            CombatLogSortColumn::Source => "source_name",
            CombatLogSortColumn::Target => "target_name",
            CombatLogSortColumn::Type => "effect_type_id",
            CombatLogSortColumn::Ability => "ability_name",
            CombatLogSortColumn::Effect => "effect_name",
            CombatLogSortColumn::Value => {
                "COALESCE(dmg_effective, 0) + COALESCE(heal_effective, 0)"
            }
            CombatLogSortColumn::Absorbed => "COALESCE(dmg_absorbed, 0)",
            CombatLogSortColumn::Overheal => {
                "GREATEST(COALESCE(heal_amount, 0) - COALESCE(heal_effective, 0), 0)"
            }
            CombatLogSortColumn::Threat => "COALESCE(threat, 0.0)",
        }
    }
}

/// Data Explorer session state
#[derive(Debug, Clone, PartialEq)]
pub struct DataExplorerState {
    /// Selected encounter index
    pub selected_encounter: Option<u32>,
    /// Current view mode (Overview, Charts, CombatLog, Detailed)
    pub view_mode: ViewMode,
    /// Selected source entity for detailed breakdown
    pub selected_source: Option<String>,
    /// Breakdown mode toggles
    pub breakdown_mode: BreakdownMode,
    /// Players-only filter
    pub show_players_only: bool,
    /// Bosses-only filter in sidebar
    pub show_only_bosses: bool,
    /// Sort column for ability table
    pub sort_column: SortColumn,
    /// Sort direction
    pub sort_direction: SortDirection,
    /// Collapsed sections in sidebar (set of section names: "Operations", "Flashpoints", etc.)
    pub collapsed_sections: std::collections::HashSet<String>,
    /// Timeline selection (time range filter)
    pub time_range: TimeRange,
    /// Selected anchor ability for rotation view (persists across encounters)
    pub selected_rotation_anchor: Option<i64>,
    /// Sort column for usage table
    pub usage_sort_column: UsageSortColumn,
    /// Sort direction for usage table
    pub usage_sort_direction: SortDirection,
    /// Whether live mode auto-activates on combat start (loaded from config)
    pub auto_live: bool,
}

impl Default for DataExplorerState {
    fn default() -> Self {
        Self {
            selected_encounter: None,
            view_mode: ViewMode::default(),
            selected_source: None,
            breakdown_mode: BreakdownMode::ability_only(),
            show_players_only: true,
            show_only_bosses: false,
            sort_column: SortColumn::default(),
            sort_direction: SortDirection::default(),
            collapsed_sections: std::collections::HashSet::new(),
            time_range: TimeRange::default(),
            selected_rotation_anchor: None,
            usage_sort_column: UsageSortColumn::default(),
            usage_sort_direction: SortDirection::default(),
            auto_live: false,
        }
    }
}

impl DataExplorerState {
    /// Reset session-specific state (encounter selection, time range, etc.)
    /// while preserving user preferences (show_only_bosses, view_mode, sort settings).
    pub fn reset_session(&mut self) {
        self.selected_encounter = None;
        self.selected_source = None;
        self.time_range = TimeRange::default();
        self.selected_rotation_anchor = None;
        // Preserve: show_only_bosses, view_mode, breakdown_mode, show_players_only,
        // sort_column, sort_direction, collapsed_sections
    }
}

/// Combat Log session state (filters, scroll position, etc.)
#[derive(Debug, Clone, PartialEq)]
pub struct CombatLogSessionState {
    /// Current encounter index being viewed
    pub encounter_idx: Option<u32>,
    /// Source filter
    pub source_filter: Option<String>,
    /// Target filter
    pub target_filter: Option<String>,
    /// Search text
    pub search_text: String,
    /// Event type filters
    pub filter_damage: bool,
    pub filter_healing: bool,
    pub filter_actions: bool,
    pub filter_effects: bool,
    pub filter_other: bool,
    /// Show IDs toggle (IMPORTANT: now persisted!)
    pub show_ids: bool,
    /// Scroll position
    pub scroll_offset: f64,
    /// Sort column
    pub sort_column: CombatLogSortColumn,
    /// Sort direction
    pub sort_direction: SortDirection,
}

impl Default for CombatLogSessionState {
    fn default() -> Self {
        Self {
            encounter_idx: None,
            source_filter: None,
            target_filter: None,
            search_text: String::new(),
            filter_damage: true,
            filter_healing: true,
            filter_actions: true,
            filter_effects: true,
            filter_other: true,
            show_ids: true,
            scroll_offset: 0.0,
            sort_column: CombatLogSortColumn::default(),
            sort_direction: SortDirection::default(),
        }
    }
}

impl CombatLogSessionState {
    /// Reset session-specific state (encounter, filters, scroll position)
    /// while preserving user preferences (show_ids, filter toggles).
    pub fn reset_session(&mut self) {
        self.encounter_idx = None;
        self.source_filter = None;
        self.target_filter = None;
        self.search_text = String::new();
        self.scroll_offset = 0.0;
        // Preserve: show_ids, filter_damage, filter_healing, filter_actions,
        // filter_effects, filter_other
    }
}

/// Encounter Builder session state
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EncounterBuilderState {
    /// Selected area file path
    pub selected_area_path: Option<String>,
    /// Selected area name (for display)
    pub selected_area_name: Option<String>,
    /// Expanded boss name
    pub expanded_boss: Option<String>,
    /// Active tab within boss editor (timers, phases, counters, etc.)
    pub active_boss_tab: Option<String>,
    /// Filter text in area sidebar
    pub area_filter: String,
    /// Expanded timer ID within Timers tab
    pub expanded_timer: Option<String>,
    /// Expanded phase ID within Phases tab
    pub expanded_phase: Option<String>,
    /// Expanded counter ID within Counters tab
    pub expanded_counter: Option<String>,
    /// Expanded challenge ID within Challenges tab
    pub expanded_challenge: Option<String>,
    /// Expanded entity ID within Entities tab
    pub expanded_entity: Option<String>,
    /// Whether to hide disabled timers in the Timers tab
    pub hide_disabled_timers: bool,
    /// Whether to hide disabled phases in the Phases tab
    pub hide_disabled_phases: bool,
    /// Whether to hide disabled counters in the Counters tab
    pub hide_disabled_counters: bool,
}

/// Effects Editor session state
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EffectsEditorState {
    /// Expanded effect ID
    pub expanded_effect: Option<String>,
    /// Search query
    pub search_query: String,
    /// Whether to hide disabled effects
    pub hide_disabled_effects: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_refresh_ability_simple_parsing() {
        // Test simple ability ID
        let toml = r#"value = 814832605462528"#;

        #[derive(Deserialize, Debug)]
        struct Test {
            value: RefreshAbility,
        }

        let parsed: Test = toml::from_str(toml).unwrap();
        println!("Parsed simple: {:?}", parsed.value);

        assert!(matches!(
            parsed.value,
            RefreshAbility::Simple(AbilitySelector::Id(814832605462528))
        ));
        assert_eq!(parsed.value.min_stacks(), None);
        assert_eq!(parsed.value.trigger(), RefreshTrigger::Activation);
    }

    #[test]
    fn test_refresh_ability_conditional_parsing() {
        // Test conditional with min_stacks and trigger
        let toml = r#"
            [value]
            ability = 1014376786034688
            min_stacks = 2
            trigger = "heal"
        "#;

        #[derive(Deserialize, Debug)]
        struct Test {
            value: RefreshAbility,
        }

        let parsed: Test = toml::from_str(toml).unwrap();
        println!("Parsed conditional: {:?}", parsed.value);

        assert!(matches!(parsed.value, RefreshAbility::Conditional { .. }));
        assert_eq!(parsed.value.min_stacks(), Some(2));
        assert_eq!(parsed.value.trigger(), RefreshTrigger::Heal);
    }

    #[test]
    fn test_refresh_ability_array_parsing() {
        // Test array with mixed simple and conditional - exactly like the TOML config
        let toml = r#"
            refresh_abilities = [
                814832605462528,
                { ability = 1014376786034688, min_stacks = 2, trigger = "heal" },
                { ability = 815240627355648, min_stacks = 2, trigger = "heal" },
            ]
        "#;

        #[derive(Deserialize, Debug)]
        struct Test {
            refresh_abilities: Vec<RefreshAbility>,
        }

        let parsed: Test = toml::from_str(toml).unwrap();
        println!("Parsed array: {:?}", parsed.refresh_abilities);

        // First entry: Simple
        assert!(matches!(
            parsed.refresh_abilities[0],
            RefreshAbility::Simple(_)
        ));
        assert_eq!(parsed.refresh_abilities[0].min_stacks(), None);
        assert_eq!(
            parsed.refresh_abilities[0].trigger(),
            RefreshTrigger::Activation
        );

        // Second entry: Conditional with min_stacks = 2, trigger = heal
        assert!(matches!(
            parsed.refresh_abilities[1],
            RefreshAbility::Conditional { .. }
        ));
        assert_eq!(parsed.refresh_abilities[1].min_stacks(), Some(2));
        assert_eq!(parsed.refresh_abilities[1].trigger(), RefreshTrigger::Heal);

        // Third entry: Conditional with min_stacks = 2, trigger = heal
        assert!(matches!(
            parsed.refresh_abilities[2],
            RefreshAbility::Conditional { .. }
        ));
        assert_eq!(parsed.refresh_abilities[2].min_stacks(), Some(2));
        assert_eq!(parsed.refresh_abilities[2].trigger(), RefreshTrigger::Heal);
    }
}
