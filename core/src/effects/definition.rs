//! Effect definition types
//!
//! Definitions are templates loaded from TOML config files that describe
//! what effects to track and how to display them.

use serde::{Deserialize, Serialize};

use crate::dsl::AudioConfig;
use crate::dsl::Trigger;

// Re-export from shared modules
pub use crate::dsl::EntityFilter;
pub use crate::dsl::{AbilitySelector, EffectSelector};

// ═══════════════════════════════════════════════════════════════════════════
// Effect Definitions
// ═══════════════════════════════════════════════════════════════════════════

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

/// Which overlay should display this effect.
///
/// Effects are routed to different overlays based on this setting,
/// allowing specialized displays for each use case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisplayTarget {
    /// No overlay specified - effect won't display unless show_on_raid_frames is set
    #[default]
    None,
    /// Show on raid frames overlay (HOTs on group members)
    RaidFrames,
    /// Show on personal buffs bar (procs/buffs on self)
    PersonalBuffs,
    /// Show on personal debuffs bar (debuffs on self from NPCs/bosses)
    PersonalDebuffs,
    /// Show on cooldown tracker (ability cooldowns)
    Cooldowns,
    /// Show on multi-target DOT tracker (DOTs on enemies)
    DotTracker,
    /// Show on generic effects countdown overlay (legacy)
    EffectsOverlay,
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

    // ─── Trigger ────────────────────────────────────────────────────────────
    /// What starts tracking this effect.
    /// Use EffectApplied/EffectRemoved for buff/debuff tracking,
    /// or AbilityCast for proc/cooldown tracking.
    pub trigger: Trigger,

    /// If true, ignore game EffectRemoved signals - only expire via duration_secs.
    /// Useful for tracking cooldowns that shouldn't end when the buff is consumed.
    /// Note: Cooldowns (DisplayTarget::Cooldowns) always ignore effect removed events.
    #[serde(default, alias = "fixed_duration")]
    pub ignore_effect_removed: bool,

    /// Abilities (ID or name) that can refresh this effect's duration
    #[serde(default)]
    pub refresh_abilities: Vec<AbilitySelector>,

    /// Whether or not the effect will refresh on ModifyCharges events
    #[serde(default)]
    pub is_refreshed_on_modify: bool,

    // ─── Duration ───────────────────────────────────────────────────────────
    /// Expected duration in seconds (None = indefinite/unknown)
    pub duration_secs: Option<f32>,

    /// Whether this duration/cooldown is affected by player's alacrity stat.
    /// If true, duration = base_duration / (1 + alacrity_percent/100).
    /// If false (default), duration is static.
    #[serde(default)]
    pub is_affected_by_alacrity: bool,

    /// Seconds to show "ready" state after cooldown expires (0 = disabled).
    /// When cooldown ends, shows in light-blue "ready" state for this duration.
    #[serde(default)]
    pub cooldown_ready_secs: f32,

    // ─── Display ────────────────────────────────────────────────────────────
    /// Effect category (determines default color)
    #[serde(default)]
    pub category: EffectCategory,

    /// Override color as RGBA (None = use category default)
    pub color: Option<[u8; 4]>,

    /// Show this effect on raid frames (HOTs/shields typically true, DOTs false)
    #[serde(default)]
    pub show_on_raid_frames: bool,

    /// Only show when remaining time is at or below this threshold (0 = always show)
    #[serde(default)]
    pub show_at_secs: f32,

    /// Which overlay should display this effect
    #[serde(default)]
    pub display_target: DisplayTarget,

    /// Icon ability ID for display (falls back to effect_id or trigger ability if not set)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_ability_id: Option<u64>,

    /// Whether to show the icon (true) or fall back to colored square (false)
    #[serde(default = "crate::serde_defaults::default_true")]
    pub show_icon: bool,

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

    /// Get the display text, falling back to name if not set
    pub fn display_text(&self) -> &str {
        self.display_text.as_deref().unwrap_or(&self.name)
    }

    /// Check if this is an EffectApplied trigger
    pub fn is_effect_applied_trigger(&self) -> bool {
        matches!(self.trigger, Trigger::EffectApplied { .. })
    }

    /// Check if this is an EffectRemoved trigger
    pub fn is_effect_removed_trigger(&self) -> bool {
        matches!(self.trigger, Trigger::EffectRemoved { .. })
    }

    /// Check if this is an AbilityCast trigger
    pub fn is_ability_cast_trigger(&self) -> bool {
        matches!(self.trigger, Trigger::AbilityCast { .. })
    }

    /// Check if an effect ID/name matches this definition's trigger
    pub fn matches_effect(&self, effect_id: u64, effect_name: Option<&str>) -> bool {
        match &self.trigger {
            Trigger::EffectApplied { effects, .. } | Trigger::EffectRemoved { effects, .. } => {
                !effects.is_empty() && effects.iter().any(|s| s.matches(effect_id, effect_name))
            }
            _ => false,
        }
    }

    /// Check if an ability cast matches this definition's trigger
    pub fn matches_ability_cast(&self, ability_id: u64, ability_name: Option<&str>) -> bool {
        if let Trigger::AbilityCast { abilities, .. } = &self.trigger {
            abilities.is_empty()
                || abilities
                    .iter()
                    .any(|s| s.matches(ability_id, ability_name))
        } else {
            false
        }
    }

    /// Check if an ability can refresh this effect
    pub fn can_refresh_with(&self, ability_id: u64, ability_name: Option<&str>) -> bool {
        self.refresh_abilities
            .iter()
            .any(|s| s.matches(ability_id, ability_name))
    }

    /// Get the source filter from the trigger
    pub fn source_filter(&self) -> &EntityFilter {
        match &self.trigger {
            Trigger::EffectApplied { source, .. }
            | Trigger::EffectRemoved { source, .. }
            | Trigger::AbilityCast { source, .. }
            | Trigger::DamageTaken { source, .. } => source,
            _ => &EntityFilter::Any,
        }
    }

    /// Get the target filter from the trigger
    pub fn target_filter(&self) -> &EntityFilter {
        match &self.trigger {
            Trigger::EffectApplied { target, .. }
            | Trigger::EffectRemoved { target, .. }
            | Trigger::AbilityCast { target, .. }
            | Trigger::DamageTaken { target, .. } => target,
            _ => &EntityFilter::Any,
        }
    }
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
