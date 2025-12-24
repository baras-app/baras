//! Boss definition types
//!
//! Definitions are loaded from TOML config files and describe boss encounters
//! with their phases, counters, and phase-aware timers.

use serde::{Deserialize, Serialize};

// ═══════════════════════════════════════════════════════════════════════════
// Root Config Structure
// ═══════════════════════════════════════════════════════════════════════════

/// Area header for consolidated encounter files
/// Contains area metadata for indexing and lazy loading
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AreaConfig {
    /// Display name of the area (e.g., "Dxun", "The Ravagers")
    pub name: String,

    /// SWTOR area ID for this operation/flashpoint
    /// Used to match AreaEntered signals for lazy loading
    #[serde(default)]
    pub area_id: i64,

    /// Category for UI grouping: "operations", "flashpoints", "lair_bosses"
    #[serde(default)]
    pub category: String,
}

/// Root structure for boss config files (TOML)
/// A file can contain one or more boss definitions.
///
/// New format includes `[area]` header:
/// ```toml
/// [area]
/// name = "Dxun"
/// area_id = 833571547775792
///
/// [[boss]]
/// id = "red"
/// ...
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BossConfig {
    /// Area metadata (new consolidated format)
    #[serde(default)]
    pub area: Option<AreaConfig>,

    /// Boss definitions in this file
    #[serde(default, rename = "boss")]
    pub bosses: Vec<BossDefinition>,
}

// ═══════════════════════════════════════════════════════════════════════════
// Boss Definition
// ═══════════════════════════════════════════════════════════════════════════

/// Definition of a boss encounter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BossDefinition {
    /// Unique identifier (e.g., "apex_vanguard")
    pub id: String,

    /// Display name
    pub name: String,

    /// Area name as it appears in the game log (for matching)
    /// E.g., "Dxun", "Blood Hunt"
    /// In consolidated format, this is populated from the [area] header
    #[serde(default)]
    pub area_name: String,

    /// Difficulties this boss config applies to (empty = all)
    #[serde(default)]
    pub difficulties: Vec<String>,

    /// NPC names that identify this boss (for detection, fallback)
    #[serde(default)]
    pub npc_names: Vec<String>,

    /// NPC class IDs for precise detection (preferred over names)
    #[serde(default)]
    pub npc_ids: Vec<i64>,

    /// Phase definitions
    #[serde(default, rename = "phase")]
    pub phases: Vec<PhaseDefinition>,

    /// Counter definitions
    #[serde(default, rename = "counter")]
    pub counters: Vec<CounterDefinition>,

    /// Boss-specific timers
    #[serde(default, rename = "timer")]
    pub timers: Vec<BossTimerDefinition>,
}

// ═══════════════════════════════════════════════════════════════════════════
// Phase Definitions
// ═══════════════════════════════════════════════════════════════════════════

/// A phase within a boss encounter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseDefinition {
    /// Phase identifier (e.g., "p1", "intermission", "burn")
    pub id: String,

    /// Display name
    pub name: String,

    /// What triggers this phase to start
    pub trigger: PhaseTrigger,

    /// Counters to reset when entering this phase
    #[serde(default)]
    pub resets_counters: Vec<String>,
}

/// Triggers for phase transitions
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum PhaseTrigger {
    /// Combat start (initial phase)
    CombatStart,

    /// Boss HP drops below threshold
    /// If `npc_id` is specified, only that NPC's HP triggers the phase (most reliable).
    /// If `boss_name` is specified, uses name matching as fallback.
    /// If neither is specified, any tracked boss reaching the threshold triggers.
    BossHpBelow {
        hp_percent: f32,
        /// NPC class/template ID (preferred - consistent across locales)
        #[serde(default)]
        npc_id: Option<i64>,
        /// Boss name (fallback - may vary by locale)
        #[serde(default)]
        boss_name: Option<String>,
    },

    /// Boss HP rises above threshold
    /// If `npc_id` is specified, only that NPC's HP triggers the phase (most reliable).
    /// If `boss_name` is specified, uses name matching as fallback.
    BossHpAbove {
        hp_percent: f32,
        /// NPC class/template ID (preferred - consistent across locales)
        #[serde(default)]
        npc_id: Option<i64>,
        /// Boss name (fallback - may vary by locale)
        #[serde(default)]
        boss_name: Option<String>,
    },

    /// Specific ability is cast
    AbilityCast {
        #[serde(default)]
        ability_ids: Vec<u64>,
    },

    /// Effect applied to boss or players
    EffectApplied {
        #[serde(default)]
        effect_ids: Vec<u64>,
    },

    /// Effect removed
    EffectRemoved {
        #[serde(default)]
        effect_ids: Vec<u64>,
    },

    /// Timer expires
    TimerExpires { timer_id: String },

    /// Counter reaches value
    CounterReaches { counter_id: String, value: u32 },

    /// Time elapsed since combat start
    TimeElapsed { secs: f32 },

    // ─── Logical Composition ─────────────────────────────────────────────────

    /// All conditions must be met (AND logic)
    AllOf {
        conditions: Vec<PhaseTrigger>,
    },

    /// Any condition suffices (OR logic)
    AnyOf {
        conditions: Vec<PhaseTrigger>,
    },
}

// ═══════════════════════════════════════════════════════════════════════════
// Counter Definitions
// ═══════════════════════════════════════════════════════════════════════════

/// A counter that tracks occurrences during a boss fight
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CounterDefinition {
    /// Counter identifier (e.g., "bull_count")
    pub id: String,

    /// What increments this counter
    pub increment_on: CounterTrigger,

    /// When to reset (default: combat end)
    #[serde(default)]
    pub reset_on: CounterReset,

    /// Starting value
    #[serde(default)]
    pub initial_value: u32,
}

/// Events that increment a counter
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum CounterTrigger {
    AbilityCast {
        #[serde(default)]
        ability_ids: Vec<u64>,
    },
    EffectApplied {
        #[serde(default)]
        effect_ids: Vec<u64>,
    },
    TimerExpires {
        timer_id: String,
    },
    PhaseEntered {
        phase_id: String,
    },
}

/// When a counter resets
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CounterReset {
    #[default]
    CombatEnd,
    PhaseChange,
    Never,
}

// ═══════════════════════════════════════════════════════════════════════════
// Boss Timer Definitions
// ═══════════════════════════════════════════════════════════════════════════

/// Timer definition with phase/counter conditions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BossTimerDefinition {
    /// Unique identifier
    pub id: String,

    /// Display name
    pub name: String,

    /// What triggers this timer
    pub trigger: BossTimerTrigger,

    /// Duration in seconds
    pub duration_secs: f32,

    /// Display color [R, G, B, A]
    #[serde(default = "default_timer_color")]
    pub color: [u8; 4],

    /// Only active during these phases (empty = all phases)
    #[serde(default)]
    pub phases: Vec<String>,

    /// Only active when counter meets condition
    #[serde(default)]
    pub counter_condition: Option<CounterCondition>,

    /// Difficulties this timer applies to
    #[serde(default)]
    pub difficulties: Vec<String>,

    /// Whether timer is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Reset duration when triggered again
    #[serde(default)]
    pub can_be_refreshed: bool,

    /// Number of repeats after initial (0 = no repeat)
    #[serde(default)]
    pub repeats: u8,

    /// Timer to start when this one expires
    pub chains_to: Option<String>,

    /// Alert when this many seconds remain
    pub alert_at_secs: Option<f32>,

    /// Show on raid frames instead of timer bar
    #[serde(default)]
    pub show_on_raid_frames: bool,
}

/// Trigger types for boss timers
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum BossTimerTrigger {
    /// Combat starts
    CombatStart,

    /// Specific ability is cast
    AbilityCast {
        #[serde(default)]
        ability_ids: Vec<u64>,
    },

    /// Effect is applied
    EffectApplied {
        #[serde(default)]
        effect_ids: Vec<u64>,
    },

    /// Effect is removed
    EffectRemoved {
        #[serde(default)]
        effect_ids: Vec<u64>,
    },

    /// Another timer expires
    TimerExpires { timer_id: String },

    /// Phase is entered
    PhaseEntered { phase_id: String },

    /// Boss HP reaches threshold
    /// If `npc_id` is specified, only that NPC's HP triggers the timer (most reliable).
    /// If `boss_name` is specified, uses name matching as fallback.
    BossHpBelow {
        hp_percent: f32,
        /// NPC class/template ID (preferred)
        #[serde(default)]
        npc_id: Option<i64>,
        /// Boss name (fallback)
        #[serde(default)]
        boss_name: Option<String>,
    },

    // ─── Logical Composition ─────────────────────────────────────────────────

    /// All conditions must be met (AND logic)
    /// Triggers when ALL nested conditions fire within the same event context.
    AllOf {
        conditions: Vec<BossTimerTrigger>,
    },

    /// Any condition suffices (OR logic)
    /// Triggers when ANY of the nested conditions fires.
    AnyOf {
        conditions: Vec<BossTimerTrigger>,
    },
}

// ═══════════════════════════════════════════════════════════════════════════
// Counter Conditions
// ═══════════════════════════════════════════════════════════════════════════

/// Condition for counter-based timer activation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CounterCondition {
    /// Counter to check
    pub counter_id: String,

    /// Comparison operator
    #[serde(default)]
    pub operator: ComparisonOp,

    /// Value to compare against
    pub value: u32,
}

/// Comparison operators for counter conditions
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
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
    pub fn evaluate(&self, left: u32, right: u32) -> bool {
        match self {
            ComparisonOp::Eq => left == right,
            ComparisonOp::Lt => left < right,
            ComparisonOp::Gt => left > right,
            ComparisonOp::Lte => left <= right,
            ComparisonOp::Gte => left >= right,
            ComparisonOp::Ne => left != right,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Serde Helpers
// ═══════════════════════════════════════════════════════════════════════════

fn default_timer_color() -> [u8; 4] {
    [200, 200, 200, 255]
}

fn default_true() -> bool {
    true
}

// ═══════════════════════════════════════════════════════════════════════════
// Impl Blocks
// ═══════════════════════════════════════════════════════════════════════════

impl BossDefinition {
    /// Check if an NPC name matches this boss
    pub fn matches_npc_name(&self, name: &str) -> bool {
        self.npc_names.iter().any(|n| n.eq_ignore_ascii_case(name))
    }

    /// Check if an NPC ID matches this boss
    pub fn matches_npc_id(&self, id: i64) -> bool {
        self.npc_ids.contains(&id)
    }

    /// Get the initial phase (triggered by CombatStart)
    pub fn initial_phase(&self) -> Option<&PhaseDefinition> {
        self.phases
            .iter()
            .find(|p| matches!(p.trigger, PhaseTrigger::CombatStart))
    }

    /// Get counters that should reset on phase change
    pub fn counters_reset_on_phase(&self) -> Vec<&str> {
        self.counters
            .iter()
            .filter(|c| matches!(c.reset_on, CounterReset::PhaseChange))
            .map(|c| c.id.as_str())
            .collect()
    }
}

impl BossDefinition {
    /// Check if this boss is active for the given area
    pub fn matches_area(&self, area_name: &str) -> bool {
        self.area_name.eq_ignore_ascii_case(area_name)
    }
}
