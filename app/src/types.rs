//! Frontend type definitions
//!
//! Contains types used by the Dioxus frontend, including re-exports from
//! baras-types and frontend-specific types that mirror backend structures.

use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// Re-exports from baras-types (shared with backend)
// ─────────────────────────────────────────────────────────────────────────────

pub use baras_types::{
    // Config types
    AppConfig, AudioSettings, BossHealthConfig, Color, EntityFilter, OverlayAppearanceConfig,
    OverlaySettings, PersonalOverlayConfig, PersonalStat, RaidOverlaySettings,
    TimerOverlayConfig, MAX_PROFILES,
    // Selectors (unified ID-or-Name matching)
    AbilitySelector, EffectSelector, EntityMatcher, EntitySelector,
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
    pub effects_running: bool,
    pub effects_enabled: bool,
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
    Effects,
}

// ─────────────────────────────────────────────────────────────────────────────
// Timer Editor Types
// ─────────────────────────────────────────────────────────────────────────────

/// Flattened timer item for the timer editor list view
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimerListItem {
    // Identity
    pub timer_id: String,
    pub boss_id: String,
    pub boss_name: String,
    pub area_name: String,
    pub category: String,
    pub file_path: String,

    // Timer data
    pub name: String,
    /// Optional in-game display text (defaults to name if not set)
    #[serde(default)]
    pub display_text: Option<String>,
    pub enabled: bool,
    pub duration_secs: f32,
    pub color: [u8; 4],
    pub phases: Vec<String>,
    pub difficulties: Vec<String>,

    // Trigger info
    pub trigger: TimerTrigger,

    // Entity filters
    #[serde(default)]
    pub source: EntityFilter,
    #[serde(default)]
    pub target: EntityFilter,

    // Counter guard condition
    #[serde(default)]
    pub counter_condition: Option<CounterCondition>,

    // Cancel trigger
    #[serde(default)]
    pub cancel_trigger: Option<TimerTrigger>,

    // Behavior
    pub can_be_refreshed: bool,
    pub repeats: u8,
    pub chains_to: Option<String>,

    // Alert options
    pub alert_at_secs: Option<f32>,
    #[serde(default)]
    pub is_alert: bool,
    #[serde(default)]
    pub alert_text: Option<String>,

    // Display
    pub show_on_raid_frames: bool,

    // Audio
    #[serde(default)]
    pub audio_file: Option<String>,
}

/// Minimal boss info for the "New Timer" dropdown
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BossListItem {
    pub id: String,
    pub name: String,
    pub area_name: String,
    pub category: String,
    pub file_path: String,
}

/// Area summary for lazy-loading timer editor
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

/// When the effect tracking should start
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectTriggerMode {
    /// Track starts when effect is applied (default)
    #[default]
    EffectApplied,
    /// Track starts when effect is removed
    EffectRemoved,
}

impl EffectTriggerMode {
    pub fn label(&self) -> &'static str {
        match self {
            Self::EffectApplied => "Effect Applied",
            Self::EffectRemoved => "Effect Removed",
        }
    }

    pub fn all() -> &'static [EffectTriggerMode] {
        &[Self::EffectApplied, Self::EffectRemoved]
    }
}

/// Effect item for the effect editor list view (matches backend EffectListItem)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectListItem {
    // Identity
    pub id: String,
    pub name: String,
    pub file_path: String,

    // Core
    pub enabled: bool,
    pub category: EffectCategory,
    pub trigger: EffectTriggerMode,

    // Matching
    pub effects: Vec<EffectSelector>,
    pub refresh_abilities: Vec<u64>,

    // Filtering
    pub source: EntityFilter,
    pub target: EntityFilter,

    // Duration
    pub duration_secs: Option<f32>,
    pub can_be_refreshed: bool,
    pub max_stacks: u8,

    // Display
    pub color: Option<[u8; 4]>,
    pub show_on_raid_frames: bool,
    pub show_on_effects_overlay: bool,

    // Behavior (advanced)
    pub persist_past_death: bool,
    pub track_outside_combat: bool,

    // Timer integration (advanced)
    pub on_apply_trigger_timer: Option<String>,
    pub on_expire_trigger_timer: Option<String>,

    // Context (advanced)
    pub encounters: Vec<String>,

    // Alerts (advanced)
    pub alert_near_expiration: bool,
    pub alert_threshold_secs: f32,
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

/// Phase list item for the encounter editor
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhaseListItem {
    pub id: String,
    pub name: String,
    /// Optional in-game display text (defaults to name if not set)
    #[serde(default)]
    pub display_text: Option<String>,
    pub boss_id: String,
    pub boss_name: String,
    pub file_path: String,
    pub start_trigger: PhaseTrigger,
    #[serde(default)]
    pub end_trigger: Option<PhaseTrigger>,
    #[serde(default)]
    pub preceded_by: Option<String>,
    #[serde(default)]
    pub counter_condition: Option<CounterCondition>,
    #[serde(default)]
    pub resets_counters: Vec<String>,
}

/// Counter list item for the encounter editor
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CounterListItem {
    pub id: String,
    /// Display name (used for ID generation)
    #[serde(default)]
    pub name: Option<String>,
    /// Optional in-game display text (defaults to name if not set)
    #[serde(default)]
    pub display_text: Option<String>,
    pub boss_id: String,
    pub boss_name: String,
    pub file_path: String,
    pub increment_on: CounterTrigger,
    #[serde(default)]
    pub reset_on: CounterTrigger,
    #[serde(default)]
    pub initial_value: u32,
    #[serde(default)]
    pub decrement: bool,
    #[serde(default)]
    pub set_value: Option<u32>,
}

/// Challenge metric types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeMetric {
    Damage,
    Healing,
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

/// Challenge list item for the encounter editor
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChallengeListItem {
    pub id: String,
    pub name: String,
    /// Optional in-game display text (defaults to name if not set)
    #[serde(default)]
    pub display_text: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub boss_id: String,
    pub boss_name: String,
    pub file_path: String,
    pub metric: ChallengeMetric,
    #[serde(default)]
    pub conditions: Vec<ChallengeCondition>,
}

/// Entity list item for the encounter editor
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityListItem {
    pub name: String,
    pub boss_id: String,
    pub boss_name: String,
    pub file_path: String,
    #[serde(default)]
    pub ids: Vec<i64>,
    #[serde(default)]
    pub is_boss: bool,
    #[serde(default)]
    pub triggers_encounter: bool,
    #[serde(default)]
    pub is_kill_target: bool,
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
