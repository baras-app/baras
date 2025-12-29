//! Frontend type definitions
//!
//! Contains types used by the Dioxus frontend, including re-exports from
//! baras-types and frontend-specific types that mirror backend structures.

use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// Re-exports from baras-types (shared with backend)
// ─────────────────────────────────────────────────────────────────────────────

pub use baras_types::{
    AppConfig, AudioSettings, BossHealthConfig, Color, OverlayAppearanceConfig,
    OverlaySettings, PersonalOverlayConfig, PersonalStat, RaidOverlaySettings,
    TimerOverlayConfig, MAX_PROFILES,
};

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

/// Timer trigger types (mirrors backend TimerTrigger)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TimerTrigger {
    CombatStart,
    AbilityCast {
        #[serde(default)]
        ability_ids: Vec<u64>,
    },
    EffectApplied {
        #[serde(default)]
        effect_ids: Vec<u64>,
    },
    EffectRemoved {
        #[serde(default)]
        effect_ids: Vec<u64>,
    },
    TimerExpires {
        timer_id: String,
    },
    TimerStarted {
        timer_id: String,
    },
    PhaseEntered {
        phase_id: String,
    },
    PhaseEnded {
        phase_id: String,
    },
    #[serde(alias = "boss_hp_below")]
    BossHpThreshold {
        hp_percent: f32,
        #[serde(default)]
        npc_id: Option<i64>,
        #[serde(default)]
        boss_name: Option<String>,
    },
    CounterReaches {
        counter_id: String,
        value: u32,
    },
    EntityFirstSeen {
        #[serde(default)]
        entity: Option<String>,
        #[serde(default)]
        npc_id: Option<i64>,
        #[serde(default)]
        entity_name: Option<String>,
    },
    EntityDeath {
        #[serde(default)]
        entity: Option<String>,
        #[serde(default)]
        npc_id: Option<i64>,
        #[serde(default)]
        entity_name: Option<String>,
    },
    TargetSet {
        #[serde(default)]
        entity: Option<String>,
        #[serde(default)]
        npc_id: Option<i64>,
        #[serde(default)]
        entity_name: Option<String>,
    },
    TimeElapsed {
        secs: f32,
    },
    Manual,
    AnyOf {
        conditions: Vec<TimerTrigger>,
    },
}

impl TimerTrigger {
    /// Human-readable label for the trigger type
    pub fn label(&self) -> &'static str {
        match self {
            Self::CombatStart => "Combat Start",
            Self::AbilityCast { .. } => "Ability Cast",
            Self::EffectApplied { .. } => "Effect Applied",
            Self::EffectRemoved { .. } => "Effect Removed",
            Self::TimerExpires { .. } => "Timer Expires",
            Self::TimerStarted { .. } => "Timer Started",
            Self::PhaseEntered { .. } => "Phase Entered",
            Self::PhaseEnded { .. } => "Phase Ended",
            Self::BossHpThreshold { .. } => "Boss HP Threshold",
            Self::CounterReaches { .. } => "Counter Reaches",
            Self::EntityFirstSeen { .. } => "Entity First Seen",
            Self::EntityDeath { .. } => "Entity Death",
            Self::TargetSet { .. } => "Target Set",
            Self::TimeElapsed { .. } => "Time Elapsed",
            Self::Manual => "Manual",
            Self::AnyOf { .. } => "Any Of (OR)",
        }
    }

    /// Machine-readable type name for the trigger (matches serde tag)
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::CombatStart => "combat_start",
            Self::AbilityCast { .. } => "ability_cast",
            Self::EffectApplied { .. } => "effect_applied",
            Self::EffectRemoved { .. } => "effect_removed",
            Self::TimerExpires { .. } => "timer_expires",
            Self::TimerStarted { .. } => "timer_started",
            Self::PhaseEntered { .. } => "phase_entered",
            Self::PhaseEnded { .. } => "phase_ended",
            Self::BossHpThreshold { .. } => "boss_hp_threshold",
            Self::CounterReaches { .. } => "counter_reaches",
            Self::EntityFirstSeen { .. } => "entity_first_seen",
            Self::EntityDeath { .. } => "entity_death",
            Self::TargetSet { .. } => "target_set",
            Self::TimeElapsed { .. } => "time_elapsed",
            Self::Manual => "manual",
            Self::AnyOf { .. } => "any_of",
        }
    }
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

/// Entity filter for source/target matching
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityFilter {
    #[default]
    LocalPlayer,
    LocalCompanion,
    LocalPlayerOrCompanion,
    OtherPlayers,
    OtherCompanions,
    AnyPlayer,
    AnyCompanion,
    AnyPlayerOrCompanion,
    GroupMembers,
    GroupMembersExceptLocal,
    Boss,
    NpcExceptBoss,
    AnyNpc,
    Specific(String),
    Any,
}

impl EntityFilter {
    pub fn label(&self) -> &'static str {
        match self {
            Self::LocalPlayer => "Local Player",
            Self::LocalCompanion => "Local Companion",
            Self::LocalPlayerOrCompanion => "Local Player or Companion",
            Self::OtherPlayers => "Other Players",
            Self::OtherCompanions => "Other Companions",
            Self::AnyPlayer => "Any Player",
            Self::AnyCompanion => "Any Companion",
            Self::AnyPlayerOrCompanion => "Any Player or Companion",
            Self::GroupMembers => "Group Members",
            Self::GroupMembersExceptLocal => "Group (Except Local)",
            Self::Boss => "Boss",
            Self::NpcExceptBoss => "NPC (Non-Boss)",
            Self::AnyNpc => "Any NPC",
            Self::Specific(_) => "Specific",
            Self::Any => "Any",
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Self::LocalPlayer => "local_player",
            Self::LocalCompanion => "local_companion",
            Self::LocalPlayerOrCompanion => "local_player_or_companion",
            Self::OtherPlayers => "other_players",
            Self::OtherCompanions => "other_companions",
            Self::AnyPlayer => "any_player",
            Self::AnyCompanion => "any_companion",
            Self::AnyPlayerOrCompanion => "any_player_or_companion",
            Self::GroupMembers => "group_members",
            Self::GroupMembersExceptLocal => "group_members_except_local",
            Self::Boss => "boss",
            Self::NpcExceptBoss => "npc_except_boss",
            Self::AnyNpc => "any_npc",
            Self::Specific(_) => "specific",
            Self::Any => "any",
        }
    }

    /// Common filters for source field
    pub fn source_options() -> &'static [EntityFilter] {
        &[
            Self::LocalPlayer,
            Self::OtherPlayers,
            Self::AnyPlayer,
            Self::Boss,
            Self::AnyNpc,
            Self::Any,
        ]
    }

    /// Common filters for target field
    pub fn target_options() -> &'static [EntityFilter] {
        &[
            Self::LocalPlayer,
            Self::GroupMembers,
            Self::GroupMembersExceptLocal,
            Self::AnyPlayer,
            Self::Boss,
            Self::AnyNpc,
            Self::Any,
        ]
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

    // Matching
    pub effect_ids: Vec<u64>,
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

/// Phase trigger types (mirrors backend PhaseTrigger)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PhaseTrigger {
    CombatStart,
    BossHpBelow {
        hp_percent: f32,
        #[serde(default)]
        entity: Option<String>,
        #[serde(default)]
        npc_id: Option<i64>,
        #[serde(default)]
        boss_name: Option<String>,
    },
    BossHpAbove {
        hp_percent: f32,
        #[serde(default)]
        entity: Option<String>,
        #[serde(default)]
        npc_id: Option<i64>,
        #[serde(default)]
        boss_name: Option<String>,
    },
    AbilityCast {
        #[serde(default)]
        ability_ids: Vec<u64>,
    },
    EffectApplied {
        #[serde(default)]
        effect_ids: Vec<u64>,
    },
    EffectRemoved {
        #[serde(default)]
        effect_ids: Vec<u64>,
    },
    CounterReaches {
        counter_id: String,
        value: u32,
    },
    TimeElapsed {
        secs: f32,
    },
    EntityFirstSeen {
        #[serde(default)]
        entity: Option<String>,
        #[serde(default)]
        npc_id: Option<i64>,
        #[serde(default)]
        entity_name: Option<String>,
    },
    EntityDeath {
        #[serde(default)]
        entity: Option<String>,
        #[serde(default)]
        npc_id: Option<i64>,
        #[serde(default)]
        entity_name: Option<String>,
    },
    PhaseEnded {
        #[serde(default)]
        phase_id: Option<String>,
        #[serde(default)]
        phase_ids: Vec<String>,
    },
    AnyOf {
        conditions: Vec<PhaseTrigger>,
    },
}

impl PhaseTrigger {
    pub fn label(&self) -> &'static str {
        match self {
            Self::CombatStart => "Combat Start",
            Self::BossHpBelow { .. } => "Boss HP Below",
            Self::BossHpAbove { .. } => "Boss HP Above",
            Self::AbilityCast { .. } => "Ability Cast",
            Self::EffectApplied { .. } => "Effect Applied",
            Self::EffectRemoved { .. } => "Effect Removed",
            Self::CounterReaches { .. } => "Counter Reaches",
            Self::TimeElapsed { .. } => "Time Elapsed",
            Self::EntityFirstSeen { .. } => "Entity First Seen",
            Self::EntityDeath { .. } => "Entity Death",
            Self::PhaseEnded { .. } => "Phase Ended",
            Self::AnyOf { .. } => "Any Of (OR)",
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Self::CombatStart => "combat_start",
            Self::BossHpBelow { .. } => "boss_hp_below",
            Self::BossHpAbove { .. } => "boss_hp_above",
            Self::AbilityCast { .. } => "ability_cast",
            Self::EffectApplied { .. } => "effect_applied",
            Self::EffectRemoved { .. } => "effect_removed",
            Self::CounterReaches { .. } => "counter_reaches",
            Self::TimeElapsed { .. } => "time_elapsed",
            Self::EntityFirstSeen { .. } => "entity_first_seen",
            Self::EntityDeath { .. } => "entity_death",
            Self::PhaseEnded { .. } => "phase_ended",
            Self::AnyOf { .. } => "any_of",
        }
    }
}

/// Counter trigger types (mirrors backend CounterTrigger)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CounterTrigger {
    CombatStart,
    CombatEnd,
    AbilityCast {
        #[serde(default)]
        ability_ids: Vec<u64>,
        #[serde(default)]
        source: Option<String>,
    },
    EffectApplied {
        #[serde(default)]
        effect_ids: Vec<u64>,
        #[serde(default)]
        target: Option<String>,
    },
    EffectRemoved {
        #[serde(default)]
        effect_ids: Vec<u64>,
        #[serde(default)]
        target: Option<String>,
    },
    TimerExpires {
        timer_id: String,
    },
    TimerStarts {
        timer_id: String,
    },
    PhaseEntered {
        phase_id: String,
    },
    PhaseEnded {
        phase_id: String,
    },
    AnyPhaseChange,
    EntityFirstSeen {
        #[serde(default)]
        entity: Option<String>,
        #[serde(default)]
        npc_id: Option<i64>,
        #[serde(default)]
        entity_name: Option<String>,
    },
    EntityDeath {
        #[serde(default)]
        entity: Option<String>,
        #[serde(default)]
        npc_id: Option<i64>,
        #[serde(default)]
        entity_name: Option<String>,
    },
    CounterReaches {
        counter_id: String,
        value: u32,
    },
    BossHpBelow {
        hp_percent: f32,
        #[serde(default)]
        entity: Option<String>,
        #[serde(default)]
        boss_name: Option<String>,
    },
    Never,
}

impl Default for CounterTrigger {
    fn default() -> Self {
        CounterTrigger::CombatEnd
    }
}

impl CounterTrigger {
    pub fn label(&self) -> &'static str {
        match self {
            Self::CombatStart => "Combat Start",
            Self::CombatEnd => "Combat End",
            Self::AbilityCast { .. } => "Ability Cast",
            Self::EffectApplied { .. } => "Effect Applied",
            Self::EffectRemoved { .. } => "Effect Removed",
            Self::TimerExpires { .. } => "Timer Expires",
            Self::TimerStarts { .. } => "Timer Starts",
            Self::PhaseEntered { .. } => "Phase Entered",
            Self::PhaseEnded { .. } => "Phase Ended",
            Self::AnyPhaseChange => "Any Phase Change",
            Self::EntityFirstSeen { .. } => "Entity First Seen",
            Self::EntityDeath { .. } => "Entity Death",
            Self::CounterReaches { .. } => "Counter Reaches",
            Self::BossHpBelow { .. } => "Boss HP Below",
            Self::Never => "Never",
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Self::CombatStart => "combat_start",
            Self::CombatEnd => "combat_end",
            Self::AbilityCast { .. } => "ability_cast",
            Self::EffectApplied { .. } => "effect_applied",
            Self::EffectRemoved { .. } => "effect_removed",
            Self::TimerExpires { .. } => "timer_expires",
            Self::TimerStarts { .. } => "timer_starts",
            Self::PhaseEntered { .. } => "phase_entered",
            Self::PhaseEnded { .. } => "phase_ended",
            Self::AnyPhaseChange => "any_phase_change",
            Self::EntityFirstSeen { .. } => "entity_first_seen",
            Self::EntityDeath { .. } => "entity_death",
            Self::CounterReaches { .. } => "counter_reaches",
            Self::BossHpBelow { .. } => "boss_hp_below",
            Self::Never => "never",
        }
    }
}

/// Phase list item for the encounter editor
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhaseListItem {
    pub id: String,
    pub name: String,
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

/// Entity matcher for challenge conditions
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityMatcher {
    AnyBoss,
    AnyAdd,
    AnyNpc,
    AnyPlayer,
    LocalPlayer,
    Any,
    NpcIds(Vec<i64>),
    NpcNames(Vec<String>),
    PlayerNames(Vec<String>),
}

impl EntityMatcher {
    pub fn label(&self) -> &'static str {
        match self {
            Self::AnyBoss => "Any Boss",
            Self::AnyAdd => "Any Add",
            Self::AnyNpc => "Any NPC",
            Self::AnyPlayer => "Any Player",
            Self::LocalPlayer => "Local Player",
            Self::Any => "Any",
            Self::NpcIds(_) => "Specific NPC IDs",
            Self::NpcNames(_) => "Specific NPC Names",
            Self::PlayerNames(_) => "Specific Players",
        }
    }

    pub fn common_options() -> &'static [EntityMatcher] {
        &[
            Self::AnyBoss,
            Self::AnyAdd,
            Self::AnyNpc,
            Self::AnyPlayer,
            Self::LocalPlayer,
            Self::Any,
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
        matcher: EntityMatcher,
    },
    Target {
        #[serde(rename = "match")]
        matcher: EntityMatcher,
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
