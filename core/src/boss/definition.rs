//! Boss encounter definition types
//!
//! Definitions are loaded from TOML config files and describe boss encounters
//! with their phases, counters, timers, and challenges.

use serde::{Deserialize, Serialize};

use super::ChallengeDefinition;

// ═══════════════════════════════════════════════════════════════════════════
// Root Config Structure
// ═══════════════════════════════════════════════════════════════════════════

/// Type of content area (used for UI grouping and boss DPS tracking)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AreaType {
    /// Raid/operation (8 or 16 player)
    #[default]
    Operation,
    /// Flashpoint (4 player)
    Flashpoint,
    /// World boss / lair boss
    LairBoss,
    /// Training dummy (parsing area)
    TrainingDummy,
    /// Other/unknown content
    Other,
}

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

    /// Type of content (operation, flashpoint, lair_boss, etc.)
    /// Used for UI grouping and determining if NPCs count as "bosses"
    #[serde(default)]
    pub area_type: AreaType,
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

    /// Boss encounter definitions in this file
    #[serde(default, rename = "boss")]
    pub bosses: Vec<BossEncounterDefinition>,
}

// ═══════════════════════════════════════════════════════════════════════════
// Entity Definition (NPCs in the encounter)
// ═══════════════════════════════════════════════════════════════════════════

/// Definition of an NPC entity in the encounter (boss or add).
/// Entities are defined once and referenced by name in triggers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityDefinition {
    /// Display name (also used for trigger references)
    pub name: String,

    /// NPC class IDs across all difficulty modes
    /// Include all variants: SM8, HM8, SM16, HM16/NiM
    #[serde(default)]
    pub ids: Vec<i64>,

    /// Whether this is a boss entity (for detection, health bars, DPS tracking)
    /// Only `is_boss = true` entities trigger encounter detection
    #[serde(default)]
    pub is_boss: bool,

    /// Whether killing this entity ends the encounter
    #[serde(default)]
    pub is_kill_target: bool,
}

impl EntityDefinition {
    /// Check if an NPC ID matches this entity
    pub fn matches_id(&self, id: i64) -> bool {
        self.ids.contains(&id)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Boss Encounter Definition
// ═══════════════════════════════════════════════════════════════════════════

/// Definition of a boss encounter (e.g., "Dread Guard", "Brontes")
/// Uses an entity roster pattern: define NPCs once, reference by name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BossEncounterDefinition {
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

    /// Entity roster: all NPCs relevant to this encounter
    /// Define once with IDs, reference by name in triggers
    #[serde(default, rename = "entities")]
    pub entities: Vec<EntityDefinition>,

    // ─── Legacy fields (deprecated, use entities instead) ────────────────────

    /// NPC names that identify this boss (for detection, fallback)
    #[deprecated(note = "Use entities with is_boss = true instead")]
    #[serde(default)]
    pub npc_names: Vec<String>,

    /// NPC class IDs for precise detection (preferred over names)
    #[deprecated(note = "Use entities with is_boss = true instead")]
    #[serde(default)]
    pub npc_ids: Vec<i64>,

    // ─── Mechanics ───────────────────────────────────────────────────────────

    /// Phase definitions
    #[serde(default, rename = "phase")]
    pub phases: Vec<PhaseDefinition>,

    /// Counter definitions
    #[serde(default, rename = "counter")]
    pub counters: Vec<CounterDefinition>,

    /// Boss-specific timers
    #[serde(default, rename = "timer")]
    pub timers: Vec<BossTimerDefinition>,

    /// Challenge definitions for tracking metrics
    #[serde(default, rename = "challenge")]
    pub challenges: Vec<ChallengeDefinition>,
}

// ═══════════════════════════════════════════════════════════════════════════
// Phase Definitions
// ═══════════════════════════════════════════════════════════════════════════

/// A phase within a boss encounter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseDefinition {
    /// Phase identifier (e.g., "p1", "walker_1", "kephess_2", "burn")
    pub id: String,

    /// Display name
    pub name: String,

    /// What triggers this phase to start
    #[serde(alias = "trigger")]
    pub start_trigger: PhaseTrigger,

    /// What triggers this phase to end (optional - otherwise ends when another phase starts)
    #[serde(default)]
    pub end_trigger: Option<PhaseTrigger>,

    /// Phase that must immediately precede this one (guard condition)
    /// e.g., walker_2 has preceded_by = "kephess_1" so it only fires after kephess_1
    #[serde(default)]
    pub preceded_by: Option<String>,

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
    /// Priority: entity > npc_id > boss_name > any boss
    BossHpBelow {
        hp_percent: f32,
        /// Entity reference from roster (preferred)
        #[serde(default)]
        entity: Option<String>,
        /// NPC class/template ID (legacy/fallback)
        #[serde(default)]
        npc_id: Option<i64>,
        /// Boss name (fallback - may vary by locale)
        #[serde(default)]
        boss_name: Option<String>,
    },

    /// Boss HP rises above threshold
    /// Priority: entity > npc_id > boss_name > any boss
    BossHpAbove {
        hp_percent: f32,
        /// Entity reference from roster (preferred)
        #[serde(default)]
        entity: Option<String>,
        /// NPC class/template ID (legacy/fallback)
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

    /// Entity is first seen (add spawn)
    EntityFirstSeen {
        /// Entity reference from roster (preferred)
        #[serde(default)]
        entity: Option<String>,
        /// NPC ID to watch for (legacy/fallback)
        #[serde(default)]
        npc_id: Option<i64>,
        /// Entity name fallback (runtime matching)
        #[serde(default)]
        entity_name: Option<String>,
    },

    /// Entity dies
    EntityDeath {
        /// Entity reference from roster (preferred)
        #[serde(default)]
        entity: Option<String>,
        /// NPC ID to watch for (legacy/fallback)
        #[serde(default)]
        npc_id: Option<i64>,
        /// Entity name fallback (runtime matching)
        #[serde(default)]
        entity_name: Option<String>,
    },

    /// Another phase's end_trigger fired
    PhaseEnded {
        /// Single phase ID (convenience)
        #[serde(default)]
        phase_id: Option<String>,
        /// Multiple phase IDs (any match triggers)
        #[serde(default)]
        phase_ids: Vec<String>,
    },

    // ─── Logical Composition ─────────────────────────────────────────────────

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
    /// NPC is first seen (add spawn)
    EntityFirstSeen {
        /// Entity reference from roster (preferred)
        #[serde(default)]
        entity: Option<String>,
        /// NPC ID (legacy/fallback)
        #[serde(default)]
        npc_id: Option<i64>,
        /// Entity name fallback (runtime matching)
        #[serde(default)]
        entity_name: Option<String>,
    },
    /// Entity dies
    EntityDeath {
        /// Entity reference from roster (preferred)
        #[serde(default)]
        entity: Option<String>,
        /// NPC ID (legacy/fallback)
        #[serde(default)]
        npc_id: Option<i64>,
        /// Entity name fallback (runtime matching)
        #[serde(default)]
        entity_name: Option<String>,
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
    pub trigger: crate::timers::TimerTrigger,

    /// Duration in seconds (0 = instant, use with is_alert)
    #[serde(default)]
    pub duration_secs: f32,

    /// If true, fires as instant alert (no countdown bar)
    #[serde(default)]
    pub is_alert: bool,

    /// Display color [R, G, B, A]
    #[serde(default = "crate::serde_defaults::default_timer_color")]
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
    #[serde(default = "crate::serde_defaults::default_true")]
    pub enabled: bool,

    /// Reset duration when triggered again
    #[serde(default)]
    pub can_be_refreshed: bool,

    /// Number of repeats after initial (0 = no repeat)
    #[serde(default)]
    pub repeats: u8,

    /// Timer to start when this one expires
    pub chains_to: Option<String>,

    /// Cancel this timer when the referenced timer starts
    pub cancel_on_timer: Option<String>,

    /// Alert when this many seconds remain
    pub alert_at_secs: Option<f32>,

    /// Show on raid frames instead of timer bar
    #[serde(default)]
    pub show_on_raid_frames: bool,
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
// Impl Blocks
// ═══════════════════════════════════════════════════════════════════════════

impl BossEncounterDefinition {
    // ─── Entity Roster Methods ───────────────────────────────────────────────

    /// Get an entity by name (case-insensitive)
    pub fn entity_by_name(&self, name: &str) -> Option<&EntityDefinition> {
        self.entities
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case(name))
    }

    /// Get the entity that contains a given NPC ID
    pub fn entity_for_id(&self, id: i64) -> Option<&EntityDefinition> {
        self.entities.iter().find(|e| e.ids.contains(&id))
    }

    /// Get all boss entities (is_boss = true)
    pub fn boss_entities(&self) -> impl Iterator<Item = &EntityDefinition> {
        self.entities.iter().filter(|e| e.is_boss)
    }

    /// Get all NPC IDs for boss entities only (for registry/detection)
    pub fn boss_npc_ids(&self) -> impl Iterator<Item = i64> + '_ {
        self.entities
            .iter()
            .filter(|e| e.is_boss)
            .flat_map(|e| e.ids.iter().copied())
    }

    /// Get all NPC IDs from all entities (for trigger matching)
    pub fn all_entity_ids(&self) -> impl Iterator<Item = i64> + '_ {
        self.entities.iter().flat_map(|e| e.ids.iter().copied())
    }

    /// Resolve an entity reference to its NPC IDs
    /// Returns None if entity not found
    pub fn resolve_entity_ids(&self, entity_name: &str) -> Option<Vec<i64>> {
        self.entity_by_name(entity_name).map(|e| e.ids.clone())
    }

    /// Get kill target entities
    pub fn kill_targets(&self) -> impl Iterator<Item = &EntityDefinition> {
        self.entities.iter().filter(|e| e.is_kill_target)
    }

    // ─── Legacy Compatibility ────────────────────────────────────────────────

    /// Check if an NPC name matches any boss in this encounter
    /// Checks both entity names and legacy npc_names
    #[allow(deprecated)]
    pub fn matches_npc_name(&self, name: &str) -> bool {
        // Check entity roster first
        if self.entities.iter().any(|e| e.is_boss && e.name.eq_ignore_ascii_case(name)) {
            return true;
        }
        // Fall back to legacy field
        self.npc_names.iter().any(|n| n.eq_ignore_ascii_case(name))
    }

    /// Check if an NPC ID matches any boss in this encounter
    /// Checks both entity roster and legacy npc_ids
    #[allow(deprecated)]
    pub fn matches_npc_id(&self, id: i64) -> bool {
        // Check entity roster first (boss entities only)
        if self.entities.iter().any(|e| e.is_boss && e.ids.contains(&id)) {
            return true;
        }
        // Fall back to legacy field
        self.npc_ids.contains(&id)
    }

    // ─── Phase/Counter Methods ───────────────────────────────────────────────

    /// Get the initial phase (triggered by CombatStart)
    pub fn initial_phase(&self) -> Option<&PhaseDefinition> {
        self.phases
            .iter()
            .find(|p| matches!(p.start_trigger, PhaseTrigger::CombatStart))
    }

    /// Get counters that should reset on phase change
    pub fn counters_reset_on_phase(&self) -> Vec<&str> {
        self.counters
            .iter()
            .filter(|c| matches!(c.reset_on, CounterReset::PhaseChange))
            .map(|c| c.id.as_str())
            .collect()
    }

    /// Check if this encounter is for the given area
    pub fn matches_area(&self, area_name: &str) -> bool {
        self.area_name.eq_ignore_ascii_case(area_name)
    }
}

/// Type alias for backward compatibility
#[deprecated(note = "Use BossEncounterDefinition instead")]
pub type BossDefinition = BossEncounterDefinition;
