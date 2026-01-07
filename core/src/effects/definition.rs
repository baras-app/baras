//! Effect definition types
//!
//! Definitions are templates loaded from TOML config files that describe
//! what effects to track and how to display them.

use serde::{Deserialize, Serialize};

use crate::dsl::AudioConfig;
use crate::dsl::Trigger;

// Re-export EntityFilter from shared module
pub use crate::dsl::EntityFilter;
pub use crate::dsl::{AbilitySelector, EffectSelector};

// ═══════════════════════════════════════════════════════════════════════════
// Effect Definitions
// ═══════════════════════════════════════════════════════════════════════════

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

/// How an effect should be categorized and displayed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectCategory {
    /// Heal over Time (default green)
    #[default]
    Hot,
    /// Absorb shield/barrier (yellow/gold)
    Shield,
    /// Beneficial buff (blue)
    Buff,
    /// Harmful debuff (red)
    Debuff,
    /// Dispellable/cleansable effect (purple)
    Cleansable,
    /// Temporary proc (cyan)
    Proc,
    /// Boss mechanic on player (orange)
    Mechanic,
}

impl EffectCategory {
    /// Default RGBA color for this category
    pub fn default_color(&self) -> [u8; 4] {
        match self {
            Self::Hot => [80, 200, 80, 255],         // Green
            Self::Shield => [220, 180, 50, 255],     // Yellow/Gold
            Self::Buff => [80, 140, 220, 255],       // Blue
            Self::Debuff => [200, 60, 60, 255],      // Red
            Self::Cleansable => [180, 80, 200, 255], // Purple
            Self::Proc => [80, 200, 220, 255],       // Cyan
            Self::Mechanic => [255, 140, 60, 255],   // Orange
        }
    }
}

/// Definition of an effect to track (loaded from config)
///
/// This is the "template" that describes what game effect to watch for
/// and how to display it. Multiple `ActiveEffect` instances may be
/// created from a single definition (one per affected player).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectDefinition {
    /// Unique identifier for this definition (e.g., "kolto_probe")
    pub id: String,

    /// Display name shown in overlays
    pub name: String,

    /// Optional in-game display text (defaults to name if not set)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_text: Option<String>,

    /// Whether this definition is currently enabled
    #[serde(default = "crate::serde_defaults::default_true")]
    pub enabled: bool,

    // ─── Matching ───────────────────────────────────────────────────────────
    /// Effect selectors (ID or name) that match this definition.
    /// Used when trigger is EffectApplied/EffectRemoved (or not set).
    #[serde(default)]
    pub effects: Vec<EffectSelector>,

    /// What starts tracking. Defaults to EffectApplied matching `effects`.
    /// Use AbilityCast for proc/cooldown tracking independent of game effects.
    #[serde(default)]
    pub trigger: EffectTriggerMode,

    /// Optional explicit trigger (AbilityCast, EffectApplied, EffectRemoved).
    /// When set, overrides `trigger` and `effects` fields.
    #[serde(default, rename = "start_trigger")]
    pub start_trigger: Option<Trigger>,

    /// If true, ignore game EffectRemoved signals - only expire via duration_secs.
    /// Useful for tracking cooldowns that shouldn't end when the buff is consumed.
    #[serde(default)]
    pub fixed_duration: bool,

    /// Abilities (ID or name) that can apply or refresh this effect
    #[serde(default)]
    pub refresh_abilities: Vec<AbilitySelector>,

    // ─── Filtering ──────────────────────────────────────────────────────────
    /// Who must apply the effect for it to be tracked
    #[serde(default)]
    pub source: EntityFilter,

    /// Who must receive the effect for it to be tracked
    #[serde(default)]
    pub target: EntityFilter,

    // ─── Duration ───────────────────────────────────────────────────────────
    /// Expected duration in seconds (None = indefinite/unknown)
    pub duration_secs: Option<f32>,

    /// Can this effect be refreshed by reapplication?
    #[serde(default = "crate::serde_defaults::default_true")]
    pub can_be_refreshed: bool,

    /// Whether or not the effect will refresh on ModifyCharges events
    #[serde(default)]
    pub is_refreshed_on_modify: bool,

    // ─── Display ────────────────────────────────────────────────────────────
    /// Effect category (determines default color)
    #[serde(default)]
    pub category: EffectCategory,

    /// Override color as RGBA (None = use category default)
    pub color: Option<[u8; 4]>,

    /// Maximum stacks to display (0 = don't show stacks)
    #[serde(default)]
    pub max_stacks: u8,

    /// Show this effect on raid frames (HOTs/shields typically true, DOTs false)
    #[serde(default)]
    pub show_on_raid_frames: bool,

    /// Show this effect on the effects overlay (countdown display)
    #[serde(default)]
    pub show_on_effects_overlay: bool,

    /// Only show when remaining time is at or below this threshold (0 = always show)
    #[serde(default)]
    pub show_at_secs: f32,

    // ─── Behavior ───────────────────────────────────────────────────────────
    /// Should this effect persist after target dies?
    #[serde(default)]
    pub persist_past_death: bool,

    /// Track this effect outside of combat?
    #[serde(default = "crate::serde_defaults::default_true")]
    pub track_outside_combat: bool,

    // ─── Timer Integration ──────────────────────────────────────────────────
    /// Timer ID to start when this effect is applied
    pub on_apply_trigger_timer: Option<String>,

    /// Timer ID to start when this effect expires/is removed
    pub on_expire_trigger_timer: Option<String>,

    // ─── Context ────────────────────────────────────────────────────────────
    /// Only track in specific encounters (empty = all encounters)
    #[serde(default)]
    pub encounters: Vec<String>,

    // ─── Alerts ─────────────────────────────────────────────────────────────
    /// Show visual warning when effect is about to expire
    #[serde(default)]
    pub alert_near_expiration: bool,

    /// Seconds before expiration to show warning
    #[serde(default = "default_alert_threshold")]
    pub alert_threshold_secs: f32,

    // ─── Audio ─────────────────────────────────────────────────────────────────

    /// Audio configuration (alerts, custom sounds)
    #[serde(default)]
    pub audio: AudioConfig,
}

impl EffectDefinition {
    /// Get the effective color (override or category default)
    pub fn effective_color(&self) -> [u8; 4] {
        self.color.unwrap_or_else(|| self.category.default_color())
    }

    /// Check if an effect ID/name matches this definition
    pub fn matches_effect(&self, effect_id: u64, effect_name: Option<&str>) -> bool {
        self.effects.iter().any(|s| s.matches(effect_id, effect_name))
    }

    /// Check if an ability can refresh this effect
    pub fn can_refresh_with(&self, ability_id: u64, ability_name: Option<&str>) -> bool {
        self.refresh_abilities.iter().any(|s| s.matches(ability_id, ability_name))
    }

    /// Check if this definition uses an AbilityCast start trigger
    pub fn has_ability_cast_trigger(&self) -> bool {
        matches!(self.start_trigger, Some(Trigger::AbilityCast { .. }))
    }

    /// Check if an ability cast matches this definition's start_trigger
    pub fn matches_ability_cast(&self, ability_id: u64, ability_name: Option<&str>) -> bool {
        if let Some(Trigger::AbilityCast { ref abilities, .. }) = self.start_trigger {
            abilities.is_empty() || abilities.iter().any(|s| s.matches(ability_id, ability_name))
        } else {
            false
        }
    }

    /// Get the source filter from start_trigger (if AbilityCast)
    pub fn ability_cast_source_filter(&self) -> Option<&EntityFilter> {
        if let Some(Trigger::AbilityCast { ref source, .. }) = self.start_trigger {
            Some(source)
        } else {
            None
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Serde Helpers
// ═══════════════════════════════════════════════════════════════════════════

fn default_alert_threshold() -> f32 {
    3.0
}

// ═══════════════════════════════════════════════════════════════════════════
// Config File Structure
// ═══════════════════════════════════════════════════════════════════════════

/// Root structure for effect config files (TOML)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DefinitionConfig {
    /// Effect definitions in this file
    #[serde(default, rename = "effect")]
    pub effects: Vec<EffectDefinition>,
}
