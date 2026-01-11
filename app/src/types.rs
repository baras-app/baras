//! Frontend type definitions
//!
//! Contains types used by the Dioxus frontend, including re-exports from
//! baras-types and frontend-specific types that mirror backend structures.

use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// Re-exports from baras-types (shared with backend)
// ─────────────────────────────────────────────────────────────────────────────

pub use baras_types::{
    // Selectors (unified ID-or-Name matching)
    AbilitySelector,
    // Config types
    AlertsOverlayConfig,
    AppConfig,
    BossHealthConfig,
    ChallengeColumns,
    ChallengeLayout,
    Color,
    CooldownTrackerConfig,
    DotTrackerConfig,
    EffectSelector,
    EntityFilter,
    EntitySelector,
    MAX_PROFILES,
    OverlayAppearanceConfig,
    OverlaySettings,
    PersonalBuffsConfig,
    PersonalDebuffsConfig,
    PersonalOverlayConfig,
    PersonalStat,
    RaidOverlaySettings,
    TimerOverlayConfig,
    // Trigger type (shared across timers, phases, counters)
    Trigger,
};

// Type aliases for context-specific trigger usage
pub type TimerTrigger = Trigger;
pub type PhaseTrigger = Trigger;
pub type CounterTrigger = Trigger;

// ─────────────────────────────────────────────────────────────────────────────
// Frontend-Only Types (mirror backend structures)
// ─────────────────────────────────────────────────────────────────────────────

/// Session information from the backend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub player_name: Option<String>,
    pub player_class: Option<String>,
    pub player_discipline: Option<String>,
    pub area_name: Option<String>,
    pub in_combat: bool,
    pub encounter_count: usize,
    pub session_start: Option<String>,
}

/// Overlay status response from backend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayStatus {
    pub running: Vec<String>,
    pub enabled: Vec<String>,
    pub personal_running: bool,
    pub personal_enabled: bool,
    pub raid_running: bool,
    pub raid_enabled: bool,
    pub boss_health_running: bool,
    pub boss_health_enabled: bool,
    pub timers_running: bool,
    pub timers_enabled: bool,
    pub challenges_running: bool,
    pub challenges_enabled: bool,
    pub alerts_running: bool,
    pub alerts_enabled: bool,
    pub personal_buffs_running: bool,
    pub personal_buffs_enabled: bool,
    pub personal_debuffs_running: bool,
    pub personal_debuffs_enabled: bool,
    pub cooldowns_running: bool,
    pub cooldowns_enabled: bool,
    pub dot_tracker_running: bool,
    pub dot_tracker_enabled: bool,
    pub overlays_visible: bool,
    pub move_mode: bool,
    pub rearrange_mode: bool,
}

/// Log file metadata for file browser
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LogFileInfo {
    pub path: String,
    pub display_name: String,
    pub character_name: Option<String>,
    pub date: String,
    pub is_empty: bool,
    pub file_size: u64,
}

/// Update availability info from backend
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateInfo {
    pub version: String,
    pub notes: Option<String>,
    pub date: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Metric Types
// ─────────────────────────────────────────────────────────────────────────────

/// Available metric overlay types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MetricType {
    Dps,
    EDps,
    BossDps,
    Hps,
    EHps,
    Abs,
    Dtps,
    Tps,
}

impl MetricType {
    /// Human-readable label for display
    pub fn label(&self) -> &'static str {
        match self {
            MetricType::Dps => "Damage",
            MetricType::EDps => "Effective Damage",
            MetricType::BossDps => "Boss Damage",
            MetricType::Hps => "Healing",
            MetricType::EHps => "Effective Healing",
            MetricType::Tps => "Threat",
            MetricType::Dtps => "Damage Taken",
            MetricType::Abs => "Shielding Given",
        }
    }

    /// Config key used for persistence
    pub fn config_key(&self) -> &'static str {
        match self {
            MetricType::Dps => "dps",
            MetricType::EDps => "edps",
            MetricType::BossDps => "bossdps",
            MetricType::Hps => "hps",
            MetricType::EHps => "ehps",
            MetricType::Tps => "tps",
            MetricType::Dtps => "dtps",
            MetricType::Abs => "abs",
        }
    }

    /// All metric overlay types (for iteration)
    pub fn all() -> &'static [MetricType] {
        &[
            MetricType::Dps,
            MetricType::EDps,
            MetricType::BossDps,
            MetricType::Hps,
            MetricType::EHps,
            MetricType::Abs,
            MetricType::Dtps,
            MetricType::Tps,
        ]
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Type Enum
// ─────────────────────────────────────────────────────────────────────────────

/// Unified overlay kind - matches backend OverlayType
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(tag = "type", content = "value")]
pub enum OverlayType {
    Metric(MetricType),
    Personal,
    Raid,
    BossHealth,
    Timers,
    Challenges,
    Alerts,
    PersonalBuffs,
    PersonalDebuffs,
    Cooldowns,
    DotTracker,
}

// ─────────────────────────────────────────────────────────────────────────────
// Audio Configuration (shared across timers, effects, alerts)
// ─────────────────────────────────────────────────────────────────────────────

/// Audio configuration shared by timers, effects, and alerts
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AudioConfig {
    /// Master toggle for audio on this item
    #[serde(default)]
    pub enabled: bool,

    /// Audio file to play (relative to sounds directory)
    #[serde(default)]
    pub file: Option<String>,

    /// Seconds before expiration to play audio (0 = on expiration)
    #[serde(default)]
    pub offset: u8,

    /// Start countdown audio at N seconds remaining (0 = disabled)
    #[serde(default)]
    pub countdown_start: u8,

    /// Voice pack for countdown (None = default)
    #[serde(default)]
    pub countdown_voice: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// DSL Types (mirror backend for direct use)
// ─────────────────────────────────────────────────────────────────────────────

/// Boss definition with file path context (mirrors baras_core::boss::BossWithPath)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BossWithPath {
    pub boss: BossEncounterDefinition,
    pub file_path: String,
    pub category: String,
}

/// Full boss encounter definition (mirrors baras_core::dsl::BossEncounterDefinition)
/// NOTE: Uses snake_case to match core type serialization (no camelCase transform)
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BossEncounterDefinition {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub area_name: String,
    #[serde(default)]
    pub area_id: i64,
    #[serde(default)]
    pub difficulties: Vec<String>,
    #[serde(default)]
    pub entities: Vec<EntityDefinition>,
    #[serde(default)]
    pub phases: Vec<PhaseDefinition>,
    #[serde(default)]
    pub counters: Vec<CounterDefinition>,
    #[serde(default, rename = "timer")]
    pub timers: Vec<BossTimerDefinition>,
    #[serde(default)]
    pub challenges: Vec<ChallengeDefinition>,
}

fn default_enabled() -> bool {
    true
}

/// Timer definition (mirrors baras_core::dsl::BossTimerDefinition)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BossTimerDefinition {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub display_text: Option<String>,
    pub trigger: Trigger,
    #[serde(default)]
    pub duration_secs: f32,
    #[serde(default)]
    pub is_alert: bool,
    #[serde(default)]
    pub alert_text: Option<String>,
    #[serde(default = "default_timer_color")]
    pub color: [u8; 4],
    #[serde(default)]
    pub phases: Vec<String>,
    #[serde(default)]
    pub counter_condition: Option<CounterCondition>,
    #[serde(default)]
    pub difficulties: Vec<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub can_be_refreshed: bool,
    #[serde(default)]
    pub repeats: u8,
    #[serde(default)]
    pub chains_to: Option<String>,
    #[serde(default)]
    pub cancel_trigger: Option<Trigger>,
    #[serde(default)]
    pub alert_at_secs: Option<f32>,
    #[serde(default)]
    pub show_on_raid_frames: bool,
    #[serde(default)]
    pub show_at_secs: f32,
    #[serde(default)]
    pub audio: AudioConfig,
}

fn default_timer_color() -> [u8; 4] {
    [255, 128, 0, 255]
}

/// Phase definition (mirrors baras_core::dsl::PhaseDefinition)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhaseDefinition {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub display_text: Option<String>,
    #[serde(alias = "trigger")]
    pub start_trigger: Trigger,
    #[serde(default)]
    pub end_trigger: Option<Trigger>,
    #[serde(default)]
    pub preceded_by: Option<String>,
    #[serde(default)]
    pub counter_condition: Option<CounterCondition>,
    #[serde(default)]
    pub resets_counters: Vec<String>,
}

/// Counter definition (mirrors baras_core::dsl::CounterDefinition)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CounterDefinition {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub display_text: Option<String>,
    pub increment_on: Trigger,
    #[serde(default)]
    pub decrement_on: Option<Trigger>,
    #[serde(default = "default_reset_trigger")]
    pub reset_on: Trigger,
    #[serde(default)]
    pub initial_value: u32,
    #[serde(default)]
    pub decrement: bool,
    #[serde(default)]
    pub set_value: Option<u32>,
}

fn default_reset_trigger() -> Trigger {
    Trigger::CombatEnd
}

/// Challenge definition (mirrors baras_core::dsl::ChallengeDefinition)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChallengeDefinition {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub display_text: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub metric: ChallengeMetric,
    #[serde(default)]
    pub conditions: Vec<ChallengeCondition>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub color: Option<[u8; 4]>,
    #[serde(default)]
    pub columns: ChallengeColumns,
}

/// Entity definition (mirrors baras_core::dsl::EntityDefinition)
/// NOTE: triggers_encounter and show_on_hp_overlay are Option<bool> to match backend
/// - None means "use is_boss value as default"
/// - Some(true/false) means explicitly set
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityDefinition {
    pub name: String,
    #[serde(default)]
    pub ids: Vec<i64>,
    #[serde(default)]
    pub is_boss: bool,
    /// Defaults to is_boss if None
    #[serde(default)]
    pub triggers_encounter: Option<bool>,
    #[serde(default)]
    pub is_kill_target: bool,
    /// Defaults to is_boss if None
    #[serde(default)]
    pub show_on_hp_overlay: Option<bool>,
}

/// Unified encounter item enum for CRUD operations (mirrors backend EncounterItem)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "item_type", rename_all = "snake_case")]
pub enum EncounterItem {
    Timer(BossTimerDefinition),
    Phase(PhaseDefinition),
    Counter(CounterDefinition),
    Challenge(ChallengeDefinition),
    Entity(EntityDefinition),
}

// ─────────────────────────────────────────────────────────────────────────────
// Encounter Editor Types
// ─────────────────────────────────────────────────────────────────────────────

/// Area summary for lazy-loading encounter editor
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AreaListItem {
    pub name: String,
    pub area_id: i64,
    pub file_path: String,
    pub category: String,
    pub boss_count: usize,
    pub timer_count: usize,
}

// ─────────────────────────────────────────────────────────────────────────────
// Effect Editor Types
// ─────────────────────────────────────────────────────────────────────────────

/// Which overlay should display this effect
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisplayTarget {
    #[default]
    None,
    RaidFrames,
    PersonalBuffs,
    PersonalDebuffs,
    Cooldowns,
    DotTracker,
    EffectsOverlay,
}

impl DisplayTarget {
    pub fn label(&self) -> &'static str {
        match self {
            Self::None => "None",
            Self::RaidFrames => "Raid Frames",
            Self::PersonalBuffs => "Personal Buffs",
            Self::PersonalDebuffs => "Personal Debuffs",
            Self::Cooldowns => "Cooldowns",
            Self::DotTracker => "DOT Tracker",
            Self::EffectsOverlay => "Effects Overlay",
        }
    }

    pub fn all() -> &'static [DisplayTarget] {
        &[
            Self::None,
            Self::RaidFrames,
            Self::PersonalBuffs,
            Self::PersonalDebuffs,
            Self::Cooldowns,
            Self::DotTracker,
            Self::EffectsOverlay,
        ]
    }
}

/// Effect category for display grouping
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectCategory {
    #[default]
    Hot,
    Shield,
    Buff,
    Debuff,
    Cleansable,
    Proc,
    Mechanic,
}

impl EffectCategory {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Hot => "HoT",
            Self::Shield => "Shield",
            Self::Buff => "Buff",
            Self::Debuff => "Debuff",
            Self::Cleansable => "Cleansable",
            Self::Proc => "Proc",
            Self::Mechanic => "Mechanic",
        }
    }

    pub fn all() -> &'static [EffectCategory] {
        &[
            Self::Hot,
            Self::Shield,
            Self::Buff,
            Self::Debuff,
            Self::Cleansable,
            Self::Proc,
            Self::Mechanic,
        ]
    }
}

/// Effect item for the effect editor list view (matches backend EffectListItem)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectListItem {
    // Identity
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub display_text: Option<String>,
    pub file_path: String,

    // Core
    pub enabled: bool,
    pub category: EffectCategory,
    pub trigger: Trigger,

    // If true, ignore game EffectRemoved - use duration_secs only
    // Note: Cooldowns always ignore effect removed events
    #[serde(default)]
    pub ignore_effect_removed: bool,

    // Matching - abilities that refresh the effect duration
    pub refresh_abilities: Vec<AbilitySelector>,

    // Duration
    pub duration_secs: Option<f32>,
    #[serde(default)]
    pub is_refreshed_on_modify: bool,

    // Display
    pub color: Option<[u8; 4]>,
    #[serde(default)]
    pub show_at_secs: f32,

    // Display routing
    #[serde(default)]
    pub display_target: DisplayTarget,
    #[serde(default)]
    pub icon_ability_id: Option<u64>,
    #[serde(default = "crate::utils::default_true")]
    pub show_icon: bool,

    // Duration modifiers
    #[serde(default)]
    pub is_affected_by_alacrity: bool,
    #[serde(default)]
    pub cooldown_ready_secs: f32,

    // Behavior
    #[serde(default)]
    pub persist_past_death: bool,
    #[serde(default)]
    pub track_outside_combat: bool,

    // Timer integration
    pub on_apply_trigger_timer: Option<String>,
    pub on_expire_trigger_timer: Option<String>,

    // Audio
    #[serde(default)]
    pub audio: AudioConfig,
}

// ─────────────────────────────────────────────────────────────────────────────
// Encounter Editor Types (Phases, Counters, Challenges, Entities)
// ─────────────────────────────────────────────────────────────────────────────

/// Comparison operators for counter conditions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonOp {
    #[default]
    Eq,
    Lt,
    Gt,
    Lte,
    Gte,
    Ne,
}

impl ComparisonOp {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Eq => "=",
            Self::Lt => "<",
            Self::Gt => ">",
            Self::Lte => "<=",
            Self::Gte => ">=",
            Self::Ne => "!=",
        }
    }

    pub fn all() -> &'static [ComparisonOp] {
        &[Self::Eq, Self::Lt, Self::Gt, Self::Lte, Self::Gte, Self::Ne]
    }
}

/// Counter condition for timer/phase guards
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CounterCondition {
    pub counter_id: String,
    #[serde(default)]
    pub operator: ComparisonOp,
    pub value: u32,
}

/// Challenge metric types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeMetric {
    Damage,
    Healing,
    EffectiveHealing,
    DamageTaken,
    HealingTaken,
    AbilityCount,
    EffectCount,
    Deaths,
    Threat,
}

impl ChallengeMetric {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Damage => "Damage",
            Self::Healing => "Healing",
            Self::EffectiveHealing => "Effective Healing",
            Self::DamageTaken => "Damage Taken",
            Self::HealingTaken => "Healing Taken",
            Self::AbilityCount => "Ability Count",
            Self::EffectCount => "Effect Count",
            Self::Deaths => "Deaths",
            Self::Threat => "Threat",
        }
    }

    pub fn all() -> &'static [ChallengeMetric] {
        &[
            Self::Damage,
            Self::Healing,
            Self::EffectiveHealing,
            Self::DamageTaken,
            Self::HealingTaken,
            Self::AbilityCount,
            Self::EffectCount,
            Self::Deaths,
            Self::Threat,
        ]
    }
}

/// Challenge condition types
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChallengeCondition {
    Phase {
        phase_ids: Vec<String>,
    },
    Source {
        #[serde(rename = "match")]
        matcher: EntityFilter,
    },
    Target {
        #[serde(rename = "match")]
        matcher: EntityFilter,
    },
    Ability {
        ability_ids: Vec<u64>,
    },
    Effect {
        effect_ids: Vec<u64>,
    },
    Counter {
        counter_id: String,
        operator: ComparisonOp,
        value: u32,
    },
    BossHpRange {
        #[serde(default)]
        min_hp: Option<f32>,
        #[serde(default)]
        max_hp: Option<f32>,
        #[serde(default)]
        npc_id: Option<i64>,
    },
}

impl ChallengeCondition {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Phase { .. } => "Phase",
            Self::Source { .. } => "Source",
            Self::Target { .. } => "Target",
            Self::Ability { .. } => "Ability",
            Self::Effect { .. } => "Effect",
            Self::Counter { .. } => "Counter",
            Self::BossHpRange { .. } => "Boss HP Range",
        }
    }
}

/// Boss item for full editing
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BossEditItem {
    pub id: String,
    pub name: String,
    pub area_name: String,
    pub area_id: i64,
    pub file_path: String,
    #[serde(default)]
    pub difficulties: Vec<String>,
}

/// Request to create a new area
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewAreaRequest {
    pub name: String,
    pub area_id: i64,
    #[serde(default = "default_area_type")]
    pub area_type: String,
}

fn default_area_type() -> String {
    "operation".to_string()
}
