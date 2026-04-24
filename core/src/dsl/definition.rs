//! Boss encounter definition types
//!
//! Definitions are loaded from TOML config files and describe boss encounters
//! with their phases, counters, timers, and challenges.

use hashbrown::HashSet;
use serde::{Deserialize, Deserializer, Serialize};

use super::{
    ChallengeDefinition, Condition, CounterCondition, CounterDefinition, PhaseDefinition, Trigger,
};
use crate::dsl::audio::AudioConfig;
use crate::game_data::Difficulty;
use baras_types::AlertTrigger;

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

/// HP threshold marker for visual display on boss health bar
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HpMarker {
    /// HP percentage where this marker appears (0.0-100.0)
    pub hp_percent: f32,
    /// Short label (e.g., "Burn", "Adds")
    pub label: String,
    /// Difficulty tiers this marker applies to (e.g., ["veteran", "master"]).
    /// Empty = all difficulties.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub difficulties: Vec<String>,
    /// Group size this marker applies to (None = all sizes, Some(8) = 8-man only, etc.).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_size: Option<u8>,
    /// Encounter state conditions that must ALL be true for this marker to appear.
    /// Empty = always shown (subject to difficulty/group_size filters above).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<Condition>,
}

/// Per-difficulty HP entry for a shield.
///
/// Entries are evaluated in order; the first match wins.
/// Both `difficulties` and `group_size` must match when specified.
/// Omitting a field means "any" for that dimension.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShieldHpEntry {
    /// Difficulty tiers this entry applies to (e.g., ["veteran", "master"]).
    /// Supports compound keys like "veteran_8". Empty = all difficulties.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub difficulties: Vec<String>,

    /// Group size this entry applies to (None = all sizes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_size: Option<u8>,

    /// Shield HP for this difficulty/group combination.
    pub total: i64,
}

/// Shield mechanic definition for a boss entity.
///
/// Define HP values per difficulty/group-size using the `hp` vec.
/// Entries are evaluated in order; first match wins.
///
/// Example TOML:
/// ```toml
/// [[boss.entities.shields]]
/// label = "Voltinator"
/// start_trigger = { type = "effect_applied", effects = [4310755595780426] }
/// end_trigger   = { type = "effect_removed", effects = [4310755595780426] }
///
/// [[boss.entities.shields.hp]]
/// total = 1428519   # applies to all difficulties/sizes (no filter)
///
/// [[boss.entities.shields.hp]]
/// difficulties = ["veteran", "master"]
/// group_size = 16
/// total = 2100000
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShieldDefinition {
    /// Display label.
    pub label: String,

    /// Trigger that activates the shield.
    /// Supports the full Trigger enum including AnyOf.
    pub start_trigger: Trigger,

    /// Trigger that deactivates the shield.
    /// Supports the full Trigger enum including AnyOf.
    pub end_trigger: Trigger,

    /// Fallback shield HP when no `hp` entry matches (defaults to 0).
    /// Prefer defining explicit `hp` entries instead of relying on this.
    #[serde(default, skip_serializing_if = "crate::serde_defaults::is_zero_i64")]
    pub total: i64,

    /// Per-difficulty/group-size HP definitions.
    /// Evaluated in order; first matching entry wins.
    /// Falls back to `total` (default 0) if no entry matches.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hp: Vec<ShieldHpEntry>,
}

impl ShieldDefinition {
    /// Resolve the effective shield HP for the given encounter difficulty.
    ///
    /// Checks `hp` entries in order (first match wins) before falling back
    /// to `total`. Matching mirrors the timer difficulty/group_size logic:
    /// both `difficulties` and `group_size` must agree when specified.
    pub fn effective_total(&self, difficulty: Option<Difficulty>) -> i64 {
        let Some(diff) = difficulty else {
            return self.total;
        };
        for entry in &self.hp {
            if let Some(req_size) = entry.group_size {
                if diff.group_size() != req_size {
                    continue;
                }
            }
            if !entry.difficulties.is_empty()
                && !entry
                    .difficulties
                    .iter()
                    .any(|d| diff.matches_config_key(d))
            {
                continue;
            }
            return entry.total;
        }
        self.total
    }
}

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

    /// HP threshold markers for visual display on the health bar
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hp_markers: Vec<HpMarker>,

    /// Shield mechanic definitions
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "deserialize_shields_lossy"
    )]
    pub shields: Vec<ShieldDefinition>,

    /// HP percentage at which this entity is "pushed" out of combat.
    /// The entity doesn't die but is no longer participating — its health bar
    /// is removed from the Boss HP overlay when HP drops to or below this %.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pushes_at: Option<f32>,
}

/// Deserialize a `Vec<ShieldDefinition>` lossily: entries that fail to parse
/// are warned about and skipped rather than failing the entire file.
/// This provides forward/backward compatibility when the shield format changes.
fn deserialize_shields_lossy<'de, D>(deserializer: D) -> Result<Vec<ShieldDefinition>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw: Vec<toml::Value> = Vec::deserialize(deserializer)?;
    let mut shields = Vec::with_capacity(raw.len());
    for value in raw {
        match ShieldDefinition::deserialize(value.clone()) {
            Ok(shield) => shields.push(shield),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "Skipping shield entry that failed to parse (old format or invalid config)"
                );
            }
        }
    }
    Ok(shields)
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BossEncounterDefinition {
    /// Unique identifier (e.g., "apex_vanguard")
    pub id: String,

    /// Display name
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,

    /// Whether this boss definition is enabled.
    /// Disabled bosses are skipped for encounter detection and timer loading.
    /// Useful in custom overlays to completely disable a bundled boss definition.
    #[serde(
        default = "crate::serde_defaults::default_true",
        skip_serializing_if = "crate::serde_defaults::is_true"
    )]
    pub enabled: bool,

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

    /// Optional trigger that replaces entity-ID-based encounter detection.
    /// Supported variants: effect_applied, effect_removed, ability_cast, damage_taken, threat_modified.
    /// When absent, detection falls back to entity class ID matching as usual.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encounter_trigger: Option<super::triggers::Trigger>,

    /// Activates this boss if no boss has been detected within this many seconds of combat start.
    /// Useful for encounters that can only be identified by the absence of another trigger
    /// (e.g. Dxun 3 add rush fires only when Dxun 2's specific threat_modified value never appears).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encounter_trigger_fallback_secs: Option<f32>,

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

    // ─── Notes ────────────────────────────────────────────────────────────────
    /// User notes for this encounter (Markdown formatted)
    /// Displayed on the Notes overlay when this encounter is active
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,

    // ─── Victory Trigger (for special encounters) ────────────────────────────
    /// Whether this boss requires an explicit victory trigger before ExitCombat is honored.
    /// Used for encounters like Coratanni where the boss doesn't die but an ability signals victory.
    #[serde(default, skip_serializing_if = "crate::serde_defaults::is_false")]
    pub has_victory_trigger: bool,

    /// The trigger that signals victory for has_victory_trigger encounters.
    /// When this fires, subsequent ExitCombat events will end the encounter as a success.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub victory_trigger: Option<super::triggers::Trigger>,

    /// Difficulties the victory trigger applies to (empty = all difficulties).
    /// Used for encounters like Trandoshan Squad where victory conditions differ by difficulty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub victory_trigger_difficulties: Vec<String>,

    /// State conditions that must be satisfied for the victory trigger to fire.
    /// Implicitly AND'd — all conditions must be true.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub victory_conditions: Vec<Condition>,

    // ─── Final Boss ──────────────────────────────────────────────────────────
    /// Whether this is the final boss of an operation.
    /// When the final boss is killed, the operations timer is automatically stopped.
    #[serde(default, skip_serializing_if = "crate::serde_defaults::is_false")]
    pub is_final_boss: bool,

    #[serde(skip)]
    pub all_npc_ids: HashSet<i64>,
}

fn is_zero(v: &i64) -> bool {
    *v == 0
}

impl Default for BossEncounterDefinition {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            enabled: true,
            area_name: String::new(),
            area_id: 0,
            area_type: AreaType::default(),
            difficulties: Vec::new(),
            entities: Vec::new(),
            encounter_trigger: None,
            encounter_trigger_fallback_secs: None,
            phases: Vec::new(),
            counters: Vec::new(),
            timers: Vec::new(),
            challenges: Vec::new(),
            notes: None,
            has_victory_trigger: false,
            victory_trigger: None,
            victory_trigger_difficulties: Vec::new(),
            victory_conditions: Vec::new(),
            is_final_boss: false,
            all_npc_ids: HashSet::new(),
        }
    }
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

    /// When to fire an alert: on timer start, on timer expire, or never
    #[serde(
        default,
        skip_serializing_if = "crate::serde_defaults::is_alert_trigger_none"
    )]
    pub alert_on: AlertTrigger,

    /// Custom alert text (None = use timer name)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alert_text: Option<String>,

    /// Display color [R, G, B, A]
    #[serde(
        default = "crate::serde_defaults::default_timer_color",
        skip_serializing_if = "crate::serde_defaults::is_default_timer_color"
    )]
    pub color: [u8; 4],

    /// Optional ability ID for icon display on the timer bar.
    /// When set, the corresponding ability icon is shown at the left of the bar.
    /// Not auto-filled from trigger because the trigger ability often differs
    /// from the ability the timer represents.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_ability_id: Option<u64>,

    /// State conditions that must be satisfied for this timer to be active.
    /// Implicitly AND'd — all conditions must be true.
    /// Replaces the old `phases` and `counter_condition` fields (which are still
    /// accepted for backward compatibility but merged into conditions at runtime).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<Condition>,

    /// DEPRECATED: Use `conditions` with `phase_active` instead.
    /// Only active during these phases (empty = all phases).
    /// Kept for backward compatibility — merged into conditions at runtime.
    #[serde(default, skip_serializing_if = "crate::serde_defaults::is_empty_vec")]
    pub phases: Vec<String>,

    /// DEPRECATED: Use `conditions` with `counter_compare` instead.
    /// Only active when counter meets condition.
    /// Kept for backward compatibility — merged into conditions at runtime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counter_condition: Option<CounterCondition>,

    /// Difficulties this timer applies to
    #[serde(default, skip_serializing_if = "crate::serde_defaults::is_empty_vec")]
    pub difficulties: Vec<String>,

    /// Group size filter (None = all sizes, Some(8) = 8-man only, Some(16) = 16-man only)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_size: Option<u8>,

    /// Whether timer is enabled
    #[serde(
        default = "crate::serde_defaults::default_true",
        skip_serializing_if = "crate::serde_defaults::is_true"
    )]
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

    /// Which overlay should display this timer (defaults to TimersA)
    #[serde(
        default,
        skip_serializing_if = "crate::serde_defaults::is_default_display_target"
    )]
    pub display_target: crate::timers::TimerDisplayTarget,

    // ─── Audio ───────────────────────────────────────────────────────────────
    /// Audio configuration (alerts, countdown, custom sounds)
    #[serde(default, skip_serializing_if = "AudioConfig::is_default")]
    pub audio: AudioConfig,

    // ─── Advanced ────────────────────────────────────────────────────────────
    /// If true, create separate timer instances per target (e.g., for tracking
    /// individual player debuffs). Defaults to false for boss timers, meaning
    /// only one instance can be active at a time regardless of target.
    #[serde(default, skip_serializing_if = "crate::serde_defaults::is_false")]
    pub per_target: bool,

    /// Role filter for API responses (populated from preferences, not saved to TOML)
    #[serde(default)]
    pub roles: Vec<String>,

    // ─── Ability Queue ────────────────────────────────────────────────────────
    /// Creates a GCD countdown bar when this timer fires (ability queue overlay only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gcd_secs: Option<f32>,

    /// If true, timer holds at zero as "queued/ready" after expiring.
    #[serde(default, skip_serializing_if = "crate::serde_defaults::is_false")]
    pub queue_on_expire: bool,

    /// Sort priority for queued entries (higher = shown first, 0–255).
    #[serde(default, skip_serializing_if = "crate::serde_defaults::is_zero_u8")]
    pub queue_priority: u8,

    /// Trigger that clears a queued/ready entry from the ability queue.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue_remove_trigger: Option<crate::timers::TimerTrigger>,

    /// Names of other timers in the same encounter that block this ability
    /// queue entry from appearing as "next to cast" when any of them is
    /// currently active (OR semantics).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queue_blocking_timers: Vec<String>,

    /// When true, render this timer's ability-queue row as a trickling-down
    /// bar (full → empty as cooldown elapses). Only applies when
    /// `display_target = AbilityQueue`.
    #[serde(default)]
    pub queue_countdown_bar: bool,

    /// When true, this timer is never a "next cast" candidate. Used for
    /// display-only entries (incoming debuffs, lockout windows, mechanic
    /// timers) that aren't castable abilities.
    #[serde(default)]
    pub queue_hide_from_next: bool,
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
        boss_id: &str,
    ) -> crate::timers::TimerDefinition {
        crate::timers::TimerDefinition {
            id: self.id.clone(),
            name: self
                .display_text
                .clone()
                .unwrap_or_else(|| self.name.clone()),
            enabled: self.enabled,
            trigger: self.trigger.clone(),
            duration_secs: self.duration_secs,
            is_alert: self.is_alert,
            can_be_refreshed: self.can_be_refreshed,
            repeats: self.repeats,
            color: self.color,
            show_on_raid_frames: self.show_on_raid_frames,
            show_at_secs: self.show_at_secs,
            display_target: self.display_target,
            alert_on: self.alert_on,
            alert_at_secs: self.alert_at_secs,
            alert_text: self.alert_text.clone(),
            audio: self.audio.clone(),
            triggers_timer: self.chains_to.clone(),
            cancel_trigger: self.cancel_trigger.clone(),
            // Context from parent boss encounter
            area_ids: vec![area_id],
            encounters: vec![area_name.to_string()], // Kept for logging/legacy
            boss: Some(boss_name.to_string()),
            boss_definition_id: Some(boss_id.to_string()),
            difficulties: self
                .difficulties
                .iter()
                .map(|d| strip_group_size_suffix(d).to_string())
                .collect(),
            group_size: self
                .group_size
                .or_else(|| infer_group_size_from_difficulties(&self.difficulties)),
            conditions: self.conditions.clone(),
            phases: self.phases.clone(),
            counter_condition: self.counter_condition.clone(),
            // Boss timers default to single-instance (per_target = false)
            per_target: self.per_target,
            icon_ability_id: self.icon_ability_id,
            gcd_secs: self.gcd_secs,
            queue_on_expire: self.queue_on_expire,
            queue_priority: self.queue_priority,
            queue_remove_trigger: self.queue_remove_trigger.clone(),
            queue_blocking_timers: self.queue_blocking_timers.clone(),
            queue_countdown_bar: self.queue_countdown_bar,
            queue_hide_from_next: self.queue_hide_from_next,
        }
    }
}

/// Strip group size suffix from a difficulty key (e.g., "veteran_8" -> "veteran")
fn strip_group_size_suffix(key: &str) -> &str {
    for suffix in ["_8", "_16", "_4"] {
        if let Some(base) = key.strip_suffix(suffix) {
            if matches!(base, "story" | "veteran" | "master") {
                return base;
            }
        }
    }
    key
}

/// Infer group_size from legacy compound difficulty keys (e.g., ["veteran_8"] -> Some(8))
fn infer_group_size_from_difficulties(difficulties: &[String]) -> Option<u8> {
    let mut found: Option<u8> = None;
    for d in difficulties {
        for (suffix, size) in [("_8", 8u8), ("_16", 16), ("_4", 4)] {
            if d.ends_with(suffix) {
                let base = &d[..d.len() - suffix.len()];
                if matches!(base, "story" | "veteran" | "master") {
                    if found.is_some() && found != Some(size) {
                        return None; // Mixed sizes, can't infer
                    }
                    found = Some(size);
                }
            }
        }
    }
    found
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
        self.validate_triggers();
    }

    /// Validate that triggers are used in supported contexts.
    /// Logs warnings for trigger/system combinations that will silently never fire.
    fn validate_triggers(&self) {
        // Validate counter triggers
        for counter in &self.counters {
            let triggers: &[(&str, &crate::dsl::Trigger)] = &[
                ("increment_on", &counter.increment_on),
                ("reset_on", &counter.reset_on),
            ];
            for (field, trigger) in triggers {
                if let Some(kind) = trigger.contains_unsupported_for_counters_phases() {
                    tracing::warn!(
                        boss = %self.id,
                        counter = %counter.id,
                        field = %field,
                        trigger_type = %kind,
                        "Counter trigger type is not supported (only works for timers) and will never fire"
                    );
                }
            }
            if let Some(ref dec_trigger) = counter.decrement_on {
                if let Some(kind) = dec_trigger.contains_unsupported_for_counters_phases() {
                    tracing::warn!(
                        boss = %self.id,
                        counter = %counter.id,
                        field = "decrement_on",
                        trigger_type = %kind,
                        "Counter trigger type is not supported (only works for timers) and will never fire"
                    );
                }
            }
        }

        // Validate phase triggers
        for phase in &self.phases {
            if let Some(kind) = phase
                .start_trigger
                .contains_unsupported_for_counters_phases()
            {
                tracing::warn!(
                    boss = %self.id,
                    phase = %phase.id,
                    field = "start_trigger",
                    trigger_type = %kind,
                    "Phase trigger type is not supported (only works for timers) and will never fire"
                );
            }
            if let Some(ref end_trigger) = phase.end_trigger {
                if let Some(kind) = end_trigger.contains_unsupported_for_counters_phases() {
                    tracing::warn!(
                        boss = %self.id,
                        phase = %phase.id,
                        field = "end_trigger",
                        trigger_type = %kind,
                        "Phase trigger type is not supported (only works for timers) and will never fire"
                    );
                }
            }
        }

        // Validate victory trigger
        if let Some(ref trigger) = self.victory_trigger {
            if let Some(kind) = trigger.contains_unsupported_for_victory() {
                tracing::warn!(
                    boss = %self.id,
                    field = "victory_trigger",
                    trigger_type = %kind,
                    "Victory trigger type is not supported and will never fire"
                );
            }
        }

        // Validate shield triggers
        for entity in &self.entities {
            for shield in &entity.shields {
                for (field, trigger) in [
                    ("start_trigger", &shield.start_trigger),
                    ("end_trigger", &shield.end_trigger),
                ] {
                    if let Some(kind) = trigger.contains_unsupported_for_shields() {
                        tracing::warn!(
                            boss = %self.id,
                            entity = %entity.name,
                            shield = %shield.label,
                            field = %field,
                            trigger_type = %kind,
                            "Shield trigger type will never fire (CombatStart/TimeElapsed are not evaluated for shields)"
                        );
                    }
                }
            }
        }
    }

    /// Check if any entity in this encounter has the given NPC class ID
    pub fn matches_npc_id(&self, npc_id: i64) -> bool {
        self.all_npc_ids.contains(&npc_id)
    }

    /// Returns true if any condition on timers, phases, or victory uses `TimerTimeRemaining`.
    pub fn needs_timer_snapshot(&self) -> bool {
        let timer_conds = self.timers.iter().flat_map(|t| t.conditions.iter());
        let phase_conds = self.phases.iter().flat_map(|p| p.conditions.iter());
        let victory_conds = self.victory_conditions.iter();

        timer_conds
            .chain(phase_conds)
            .chain(victory_conds)
            .any(|c| c.uses_timer_time_remaining())
    }

    /// Returns the set of timer IDs needed for phase/counter trigger evaluation.
    ///
    /// This computes the transitive closure: starting from timer IDs directly referenced
    /// by phase triggers, counter triggers, and victory triggers, it follows timer chains
    /// (timer_expires/timer_started in timer triggers and cancel_triggers, plus chains_to)
    /// until no new timer IDs are discovered.
    ///
    /// Also includes timer IDs from `TimerTimeRemaining` conditions on phases and counters.
    pub fn phase_relevant_timer_ids(&self) -> std::collections::HashSet<String> {
        use std::collections::HashSet;

        let mut relevant: HashSet<String> = HashSet::new();

        // Seed: collect timer IDs from phase start/end triggers
        for phase in &self.phases {
            phase.start_trigger.collect_timer_refs(&mut relevant);
            if let Some(ref end) = phase.end_trigger {
                end.collect_timer_refs(&mut relevant);
            }
        }

        // Seed: collect timer IDs from counter triggers
        for counter in &self.counters {
            counter.increment_on.collect_timer_refs(&mut relevant);
            if let Some(ref dec) = counter.decrement_on {
                dec.collect_timer_refs(&mut relevant);
            }
            counter.reset_on.collect_timer_refs(&mut relevant);
        }

        // Seed: collect timer IDs from victory trigger
        if let Some(ref vt) = self.victory_trigger {
            vt.collect_timer_refs(&mut relevant);
        }

        // Seed: collect timer IDs from TimerTimeRemaining conditions
        for phase in &self.phases {
            for cond in &phase.conditions {
                Self::collect_condition_timer_refs(cond, &mut relevant);
            }
        }
        for counter_cond in self.victory_conditions.iter() {
            Self::collect_condition_timer_refs(counter_cond, &mut relevant);
        }

        // Transitive closure: follow timer trigger/cancel/chain references,
        // and timer conditions (e.g. TimerTimeRemaining guards on another
        // timer's active state — that timer must also be loaded or its
        // condition will always fail in the parse worker).
        let mut prev_size = 0;
        while relevant.len() != prev_size {
            prev_size = relevant.len();
            let mut new_refs: HashSet<String> = HashSet::new();

            for timer in &self.timers {
                if relevant.contains(&timer.id) {
                    timer.trigger.collect_timer_refs(&mut new_refs);
                    if let Some(ref cancel) = timer.cancel_trigger {
                        cancel.collect_timer_refs(&mut new_refs);
                    }
                    if let Some(ref chain) = timer.chains_to {
                        new_refs.insert(chain.clone());
                    }
                    for cond in &timer.conditions {
                        Self::collect_condition_timer_refs(cond, &mut new_refs);
                    }
                }
            }

            relevant.extend(new_refs);
        }

        relevant
    }

    /// Extract timer IDs from TimerTimeRemaining conditions (recursive).
    fn collect_condition_timer_refs(cond: &Condition, out: &mut std::collections::HashSet<String>) {
        match cond {
            Condition::TimerTimeRemaining { timer_id, .. } => {
                out.insert(timer_id.clone());
            }
            Condition::AllOf { conditions } | Condition::AnyOf { conditions } => {
                for c in conditions {
                    Self::collect_condition_timer_refs(c, out);
                }
            }
            Condition::Not { condition } => {
                Self::collect_condition_timer_refs(condition, out);
            }
            _ => {}
        }
    }
}
