//! Boss encounter definition types
//!
//! Definitions are loaded from TOML config files and describe boss encounters
//! with their phases, counters, timers, and challenges.

use hashbrown::HashSet;
use serde::{Deserialize, Serialize};

use super::{
    ChallengeDefinition, CounterCondition, CounterDefinition, CounterTrigger, PhaseDefinition,
};
use crate::dsl::audio::AudioConfig;

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
    /// Open world content (heroics, dailies, etc.)
    OpenWorld,
}

impl AreaType {
    /// Convert to category string for UI grouping
    pub fn to_category(&self) -> &'static str {
        match self {
            AreaType::Operation => "operations",
            AreaType::Flashpoint => "flashpoints",
            AreaType::LairBoss => "lair_bosses",
            AreaType::TrainingDummy => "other",
            AreaType::OpenWorld => "open_world",
        }
    }
}

/// Area header for consolidated encounter files
/// Contains area metadata for indexing and lazy loading
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AreaConfig {
    /// Display name of the area (e.g., "Dxun", "The Ravagers")
    pub name: String,

    /// SWTOR area ID for this operation/flashpoint
    /// Used to match AreaEntered signals for lazy loading
    #[serde(default, alias = "id")]
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

    /// Whether this is a boss entity (for health bars, DPS tracking)
    #[serde(default)]
    pub is_boss: bool,

    /// Whether this entity triggers encounter detection when seen.
    /// Defaults to `is_boss` value if not specified.
    /// Use `triggers_encounter = true` with `is_boss = false` for entities
    /// that should load the encounter but not show on the health bar.
    #[serde(default)]
    pub triggers_encounter: Option<bool>,

    /// Whether killing this entity ends the encounter
    #[serde(default)]
    pub is_kill_target: bool,

    /// Whether to show this entity on the Boss HP overlay.
    /// Defaults to `is_boss` value if not specified.
    /// Use to hide invincible boss phases or show important non-boss adds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_on_hp_overlay: Option<bool>,
}

impl EntityDefinition {
    /// Check if an NPC ID matches this entity
    pub fn matches_id(&self, id: i64) -> bool {
        self.ids.contains(&id)
    }

    /// Whether this entity triggers encounter detection.
    /// Defaults to `is_boss` if not explicitly set.
    pub fn triggers_encounter(&self) -> bool {
        self.triggers_encounter.unwrap_or(self.is_boss)
    }

    /// Whether this entity should show on the Boss HP overlay.
    /// Defaults to `is_boss` if not explicitly set.
    pub fn shows_on_hp_overlay(&self) -> bool {
        self.show_on_hp_overlay.unwrap_or(self.is_boss)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Boss Encounter Definition
// ═══════════════════════════════════════════════════════════════════════════

/// Definition of a boss encounter (e.g., "Dread Guard", "Brontes")
/// Uses an entity roster pattern: define NPCs once, reference by name.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BossEncounterDefinition {
    /// Unique identifier (e.g., "apex_vanguard")
    pub id: String,

    /// Display name
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,

    /// Area name as it appears in the game log (for display/logging)
    /// E.g., "Dxun - The CI-004 Facility", "Blood Hunt"
    /// In consolidated format, this is populated from the [area] header
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub area_name: String,

    /// Area ID from game (primary matching key - more reliable than name)
    /// In consolidated format, this is populated from the [area] header
    #[serde(default, skip_serializing_if = "is_zero")]
    pub area_id: i64,

    /// Content type for this encounter (Operation, Flashpoint, etc.)
    /// In consolidated format, this is populated from the [area] header
    #[serde(default)]
    pub area_type: AreaType,

    /// Difficulties this boss config applies to (empty = all)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub difficulties: Vec<String>,

    /// Entity roster: all NPCs relevant to this encounter
    /// Define once with IDs, reference by name in triggers
    #[serde(default, alias = "entity", skip_serializing_if = "Vec::is_empty")]
    pub entities: Vec<EntityDefinition>,

    // ─── Mechanics ───────────────────────────────────────────────────────────
    /// Phase definitions
    #[serde(default, alias = "phase", skip_serializing_if = "Vec::is_empty")]
    pub phases: Vec<PhaseDefinition>,

    /// Counter definitions
    #[serde(default, alias = "counter", skip_serializing_if = "Vec::is_empty")]
    pub counters: Vec<CounterDefinition>,

    /// Boss-specific timers
    #[serde(default, rename = "timer", skip_serializing_if = "Vec::is_empty")]
    pub timers: Vec<BossTimerDefinition>,

    /// Challenge definitions for tracking metrics
    #[serde(default, alias = "challenge", skip_serializing_if = "Vec::is_empty")]
    pub challenges: Vec<ChallengeDefinition>,

    #[serde(skip)]
    pub all_npc_ids: HashSet<i64>,
}

fn is_zero(v: &i64) -> bool {
    *v == 0
}

// ═══════════════════════════════════════════════════════════════════════════
// Boss Timer Definition
// ═══════════════════════════════════════════════════════════════════════════

/// Timer definition embedded in boss configs.
///
/// This is a thin wrapper around TimerDefinition with different serde defaults:
/// - `source` and `target` default to `Any` (boss abilities come from NPCs)
/// - `encounters` and `boss` are implicit from parent context
///
/// Use `to_timer_definition()` to convert with full context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BossTimerDefinition {
    /// Unique identifier (auto-generated from name if empty)
    pub id: String,

    /// Display name (used for ID generation, must be unique within encounter)
    pub name: String,

    /// Optional in-game display text (defaults to name if not set)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_text: Option<String>,

    /// What triggers this timer (includes source/target filters)
    pub trigger: crate::timers::TimerTrigger,

    /// Duration in seconds (0 = instant, use with is_alert)
    #[serde(default, skip_serializing_if = "crate::serde_defaults::is_zero_f32")]
    pub duration_secs: f32,

    /// If true, fires as instant alert (no countdown bar)
    #[serde(default, skip_serializing_if = "crate::serde_defaults::is_false")]
    pub is_alert: bool,

    /// Custom alert text (None = use timer name)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alert_text: Option<String>,

    /// Display color [R, G, B, A]
    #[serde(default = "crate::serde_defaults::default_timer_color")]
    pub color: [u8; 4],

    /// Only active during these phases (empty = all phases)
    #[serde(default, skip_serializing_if = "crate::serde_defaults::is_empty_vec")]
    pub phases: Vec<String>,

    /// Only active when counter meets condition
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counter_condition: Option<CounterCondition>,

    /// Difficulties this timer applies to
    #[serde(default, skip_serializing_if = "crate::serde_defaults::is_empty_vec")]
    pub difficulties: Vec<String>,

    /// Whether timer is enabled
    #[serde(default = "crate::serde_defaults::default_true")]
    pub enabled: bool,

    /// Reset duration when triggered again
    #[serde(default, skip_serializing_if = "crate::serde_defaults::is_false")]
    pub can_be_refreshed: bool,

    /// Number of repeats after initial (0 = no repeat)
    #[serde(default, skip_serializing_if = "crate::serde_defaults::is_zero_u8")]
    pub repeats: u8,

    /// Timer to start when this one expires
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chains_to: Option<String>,

    /// Cancel this timer when this trigger fires
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancel_trigger: Option<crate::timers::TimerTrigger>,

    /// Alert when this many seconds remain
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alert_at_secs: Option<f32>,

    /// Show on raid frames instead of timer bar
    #[serde(default, skip_serializing_if = "crate::serde_defaults::is_false")]
    pub show_on_raid_frames: bool,

    /// Only show when remaining time is at or below this threshold (0 = always show)
    #[serde(default, skip_serializing_if = "crate::serde_defaults::is_zero_f32")]
    pub show_at_secs: f32,

    // ─── Audio ───────────────────────────────────────────────────────────────
    /// Audio configuration (alerts, countdown, custom sounds)
    #[serde(default)]
    pub audio: AudioConfig,
}

impl BossTimerDefinition {
    /// Convert to a full TimerDefinition with boss context.
    ///
    /// Fills in the `area_ids` and `boss` fields from the parent encounter.
    /// Uses area_id for reliable matching (area_name kept for logging/fallback).
    pub fn to_timer_definition(
        &self,
        area_id: i64,
        area_name: &str,
        boss_name: &str,
    ) -> crate::timers::TimerDefinition {
        crate::timers::TimerDefinition {
            id: self.id.clone(),
            name: self.display_text.clone().unwrap_or_else(|| self.name.clone()),
            enabled: self.enabled,
            trigger: self.trigger.clone(),
            duration_secs: self.duration_secs,
            is_alert: self.is_alert,
            can_be_refreshed: self.can_be_refreshed,
            repeats: self.repeats,
            color: self.color,
            show_on_raid_frames: self.show_on_raid_frames,
            show_at_secs: self.show_at_secs,
            alert_at_secs: self.alert_at_secs,
            alert_text: self.alert_text.clone(),
            audio: self.audio.clone(),
            triggers_timer: self.chains_to.clone(),
            cancel_trigger: self.cancel_trigger.clone(),
            // Context from parent boss encounter
            area_ids: vec![area_id],
            encounters: vec![area_name.to_string()], // Kept for logging/legacy
            boss: Some(boss_name.to_string()),
            difficulties: self.difficulties.clone(),
            phases: self.phases.clone(),
            counter_condition: self.counter_condition.clone(),
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

    /// Get all boss entities (is_boss = true) for health bar display
    pub fn boss_entities(&self) -> impl Iterator<Item = &EntityDefinition> {
        self.entities.iter().filter(|e| e.is_boss)
    }

    /// Get all entities that should show on the HP overlay
    pub fn hp_overlay_entities(&self) -> impl Iterator<Item = &EntityDefinition> {
        self.entities.iter().filter(|e| e.shows_on_hp_overlay())
    }

    /// Get all NPC IDs that trigger encounter detection
    pub fn encounter_trigger_ids(&self) -> impl Iterator<Item = i64> + '_ {
        self.entities
            .iter()
            .filter(|e| e.triggers_encounter())
            .flat_map(|e| e.ids.iter().copied())
    }

    /// Get all NPC IDs for boss entities only (for health bar tracking)
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

    // ─── Phase/Counter Methods ───────────────────────────────────────────────

    /// Get the initial phase (triggered by CombatStart)
    pub fn initial_phase(&self) -> Option<&PhaseDefinition> {
        self.phases
            .iter()
            .find(|p| p.start_trigger.contains_combat_start())
    }

    /// Get counters that should reset on any phase change
    pub fn counters_reset_on_phase(&self) -> Vec<&str> {
        self.counters
            .iter()
            .filter(|c| matches!(c.reset_on, CounterTrigger::AnyPhaseChange))
            .map(|c| c.id.as_str())
            .collect()
    }

    /// Get counters that reset on a specific phase
    pub fn counters_reset_on_specific_phase(&self, phase_id: &str) -> Vec<&str> {
        self.counters
            .iter()
            .filter(|c| matches!(&c.reset_on, CounterTrigger::PhaseEntered { phase_id: p } if p == phase_id))
            .map(|c| c.id.as_str())
            .collect()
    }

    /// Get counters that reset when a specific timer expires
    pub fn counters_reset_on_timer(&self, timer_id: &str) -> Vec<&str> {
        self.counters
            .iter()
            .filter(|c| matches!(&c.reset_on, CounterTrigger::TimerExpires { timer_id: t } if t == timer_id))
            .map(|c| c.id.as_str())
            .collect()
    }

    /// Check if this encounter is for the given area
    pub fn matches_area(&self, area_name: &str) -> bool {
        self.area_name.eq_ignore_ascii_case(area_name)
    }

    pub fn build_indexes(&mut self) {
        self.all_npc_ids = self
            .entities
            .iter()
            .flat_map(|e| e.ids.iter().copied())
            .collect();
    }

    /// Check if any entity in this encounter has the given NPC class ID
    pub fn matches_npc_id(&self, npc_id: i64) -> bool {
        self.all_npc_ids.contains(&npc_id)
    }
}
