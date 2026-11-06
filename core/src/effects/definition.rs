//! Effect definition types
//!
//! Definitions are templates loaded from TOML config files that describe
//! what effects to track and how to display them.

use serde::{Deserialize, Deserializer, Serialize};

use crate::dsl::AudioConfig;
use crate::dsl::Trigger;
use crate::game_data::{Discipline, DisciplineFilter};

// Re-export from shared modules
pub use crate::dsl::EntityFilter;
pub use crate::dsl::{AbilitySelector, EffectSelector};
pub use baras_types::{AlertTrigger, RefreshAbility, RefreshScope, RefreshTrigger};

// ═══════════════════════════════════════════════════════════════════════════
// Effect Definitions
// ═══════════════════════════════════════════════════════════════════════════

/// Default RGBA color for effects without explicit color
const DEFAULT_EFFECT_COLOR: [u8; 4] = [128, 128, 128, 255];

/// Which overlay should display this effect.
///
/// Effects are routed to different overlays based on this setting,
/// allowing specialized displays for each use case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisplayTarget {
    /// No overlay specified - effect won't display
    #[default]
    None,
    /// Show on raid frames overlay (HOTs on group members)
    RaidFrames,
    /// Show on Effects A overlay (personal effects)
    #[serde(alias = "personal_buffs")]
    EffectsA,
    /// Show on Effects B overlay (personal effects)
    #[serde(alias = "personal_debuffs")]
    EffectsB,
    /// Show on Effects C overlay (personal effects)
    EffectsC,
    /// Show on cooldown tracker (ability cooldowns)
    Cooldowns,
    /// Show on cooldown tracker B (ability cooldowns)
    CooldownsB,
    /// Show on multi-target DOT tracker (DOTs on enemies)
    DotTracker,
    /// Show on boss HP overlay (icons below the relevant boss bar)
    BossHealth,
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
    #[serde(
        default = "crate::serde_defaults::default_true",
        skip_serializing_if = "crate::serde_defaults::is_true"
    )]
    pub enabled: bool,

    // ─── Trigger ────────────────────────────────────────────────────────────
    /// What starts tracking this effect.
    /// Use EffectApplied/EffectRemoved for buff/debuff tracking,
    /// or AbilityCast for proc/cooldown tracking.
    pub trigger: Trigger,

    /// If true, ignore game EffectRemoved signals - only expire via duration_secs.
    /// Useful for tracking cooldowns that shouldn't end when the buff is consumed.
    /// Note: Cooldowns (DisplayTarget::Cooldowns) always ignore effect removed events.
    #[serde(
        default,
        alias = "fixed_duration",
        skip_serializing_if = "crate::serde_defaults::is_false"
    )]
    pub ignore_effect_removed: bool,

    /// Abilities (ID or name) that can refresh this effect's duration.
    /// Supports both simple selectors and conditional refresh with min_stacks/trigger.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refresh_abilities: Vec<RefreshAbility>,

    /// Whether refresh abilities on this effect use AoE damage correlation.
    /// When true, the tracker uses damage events to detect multi-target refreshes
    /// instead of requiring individual ApplyEffect signals per target.
    #[serde(default, skip_serializing_if = "crate::serde_defaults::is_false")]
    pub is_aoe_refresh: bool,

    /// When true, AoE refresh uses immediate mode: any damage from the ability
    /// after activation refreshes the target without anchor/window scoping.
    /// When false (default), uses strict DOT mode: anchors on primary target
    /// then collects hits within ±10ms to prevent dot ticks from false-refreshing.
    #[serde(default, skip_serializing_if = "crate::serde_defaults::is_false")]
    pub aoe_refresh_immediate: bool,

    /// Whether or not the effect will refresh on ModifyCharges events
    #[serde(default, skip_serializing_if = "crate::serde_defaults::is_false")]
    pub is_refreshed_on_modify: bool,

    /// Default charges when creating via late registration (ability activation)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_charges: Option<u8>,

    /// Game maintains at most one instance of this buff per target across all
    /// sources (e.g. Kolto Shell, Trauma Probe); any caster's reapply refreshes
    /// the original caster's instance, which retains credit.
    #[serde(default, skip_serializing_if = "crate::serde_defaults::is_false")]
    pub single_instance_per_target: bool,

    // ─── Duration ───────────────────────────────────────────────────────────
    /// Expected duration in seconds (None = indefinite/unknown)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<f32>,

    /// Whether this duration/cooldown is affected by player's alacrity stat.
    /// If true, duration = base_duration / (1 + alacrity_percent/100).
    /// If false (default), duration is static.
    #[serde(default, skip_serializing_if = "crate::serde_defaults::is_false")]
    pub is_affected_by_alacrity: bool,

    /// Seconds to show "ready" state after cooldown expires (0 = disabled).
    /// When cooldown ends, shows in light-blue "ready" state for this duration.
    #[serde(default, skip_serializing_if = "crate::serde_defaults::is_zero_f32")]
    pub cooldown_ready_secs: f32,

    // ─── Display ────────────────────────────────────────────────────────────
    /// Effect color as RGBA
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<[u8; 4]>,

    /// Only show when remaining time is at or below this threshold (0 = always show)
    #[serde(default, skip_serializing_if = "crate::serde_defaults::is_zero_f32")]
    pub show_at_secs: f32,

    /// Which overlays should display this effect.
    ///
    /// A single effect can be shown on multiple overlays simultaneously
    /// (e.g. a HoT can appear on both RaidFrames and EffectsA).
    ///
    /// Backwards-compatible with the legacy `display_target = "..."` single-value
    /// form via a custom deserializer.
    #[serde(
        default,
        alias = "display_target",
        deserialize_with = "deserialize_display_targets",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub display_targets: Vec<DisplayTarget>,

    /// Icon ability ID for display (falls back to effect_id or trigger ability if not set)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_ability_id: Option<u64>,

    /// Whether to show the icon (true) or fall back to colored square (false)
    #[serde(
        default = "crate::serde_defaults::default_true",
        skip_serializing_if = "crate::serde_defaults::is_true"
    )]
    pub show_icon: bool,

    /// Whether to display the source entity name on personal overlays
    /// (Cooldowns, PersonalBuffs, PersonalDebuffs)
    #[serde(default, skip_serializing_if = "crate::serde_defaults::is_false")]
    pub display_source: bool,

    /// Whether to display the target entity name on personal overlays
    /// (e.g. taunt/guard target). Suppressed when the target is the source.
    /// Named `show_target` because `display_target` is a legacy alias of
    /// `display_targets`.
    #[serde(default, skip_serializing_if = "crate::serde_defaults::is_false")]
    pub show_target: bool,

    /// Uptime tracking: pin this effect's icon on its overlay whenever the
    /// local player's discipline matches, rendered desaturated while the
    /// effect is not active (e.g. tank guard, sniper cover). Requires a
    /// known discipline — nothing shows before discipline detection.
    #[serde(default, skip_serializing_if = "crate::serde_defaults::is_false")]
    pub track_uptime: bool,

    /// With `track_uptime`: suppress the desaturated placeholder until the
    /// effect has been applied at least once this session.
    #[serde(default, skip_serializing_if = "crate::serde_defaults::is_false")]
    pub track_uptime_after_first: bool,

    /// Emphasize stacks for this effect on Effects A/B: stack count drawn
    /// large and centered, countdown in the corner. ORed with the overlay's
    /// own stack_priority setting.
    #[serde(default, skip_serializing_if = "crate::serde_defaults::is_false")]
    pub stack_priority: bool,

    /// Show the countdown text for this effect on Effects A/B. Only applies
    /// while the overlay's own show_countdown setting is also on.
    #[serde(
        default = "crate::serde_defaults::default_true",
        skip_serializing_if = "crate::serde_defaults::is_true"
    )]
    pub show_countdown: bool,

    /// Always display a stack count for this effect, floored at 1: an active
    /// non-stacking (or single-stack) instance shows "1", real counts show
    /// through as they climb. Applies to Effects A/B and raid frames (which
    /// otherwise hide counts below 2).
    #[serde(default, skip_serializing_if = "crate::serde_defaults::is_false")]
    pub show_single_stack: bool,

    // ─── Discipline Scoping ────────────────────────────────────────────────
    /// Disciplines this effect is restricted to. Empty = all disciplines.
    /// When set, the effect only activates if the local player's discipline
    /// is in this list. Does not affect NPCs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disciplines: Vec<Discipline>,

    /// Source entity must be a player of one of these disciplines/roles.
    /// Empty = no constraint. When non-empty, NPCs, companions, and players
    /// whose discipline is unknown never match.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_disciplines: Vec<DisciplineFilter>,

    /// Target entity must be a player of one of these disciplines/roles.
    /// Same semantics as `source_disciplines`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_disciplines: Vec<DisciplineFilter>,

    // ─── Behavior ───────────────────────────────────────────────────────────
    /// If true, retriggering the effect while it is already active is ignored
    /// (the existing duration is preserved instead of being refreshed). Useful
    /// for triggers like DamageTaken/HealingTaken where reapplication shouldn't
    /// reset the timer. Refresh abilities still operate normally.
    #[serde(default, skip_serializing_if = "crate::serde_defaults::is_false")]
    pub ignore_refreshes: bool,

    /// Scoping for refresh/dedup logic. Controls which axis of the
    /// (source, target) pair is used to identify "the same effect instance".
    /// Default (`Both`) preserves the existing per-(source, target) behavior.
    /// `Source` collapses across targets; `Target` collapses across sources.
    #[serde(default, skip_serializing_if = "crate::serde_defaults::is_default_refresh_scope")]
    pub refresh_scope: RefreshScope,

    /// Should this effect persist after target dies?
    #[serde(default, skip_serializing_if = "crate::serde_defaults::is_false")]
    pub persist_past_death: bool,

    /// Track this effect outside of combat?
    #[serde(
        default = "crate::serde_defaults::default_true",
        skip_serializing_if = "crate::serde_defaults::is_true"
    )]
    pub track_outside_combat: bool,

    // ─── Timer Integration ──────────────────────────────────────────────────
    /// Timer ID to start when this effect is applied
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_apply_trigger_timer: Option<String>,

    /// Timer ID to start when this effect expires/is removed
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_expire_trigger_timer: Option<String>,

    // ─── Alerts ────────────────────────────────────────────────────────────────
    /// If true, fires as instant alert (no active effect created).
    /// Only shows alert text and plays audio on trigger — no duration tracking.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_alert: bool,

    /// Text to display in the alerts overlay
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alert_text: Option<String>,

    /// When to trigger the alert
    #[serde(
        default,
        skip_serializing_if = "crate::serde_defaults::is_alert_trigger_none"
    )]
    pub alert_on: AlertTrigger,

    /// When `alert_on == Countdown`, the trailing window (in seconds, 0..10)
    /// during which the live-updating alert is shown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alert_countdown_secs: Option<f32>,

    // ─── Audio ─────────────────────────────────────────────────────────────────
    /// Audio configuration (alerts, custom sounds)
    #[serde(default, skip_serializing_if = "AudioConfig::is_default")]
    pub audio: AudioConfig,

    // ─── Modifiers ────────────────────────────────────────────────────────────
    /// Reactive modifiers that adjust this effect when triggers fire.
    /// Evaluated against incoming signals while the effect is active.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modifiers: Vec<baras_types::EffectModifier>,
}

/// Deserialize `display_targets` accepting either a single bare value
/// (legacy `display_target = "raid_frames"`) or an array
/// (`display_targets = ["raid_frames", "effects_a"]`).
/// `None` values are filtered out so empty/unset = no overlay display.
fn deserialize_display_targets<'de, D>(deserializer: D) -> Result<Vec<DisplayTarget>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::{self, SeqAccess, Visitor};
    use std::fmt;

    struct OneOrMany;

    impl<'de> Visitor<'de> for OneOrMany {
        type Value = Vec<DisplayTarget>;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a display target string or a list of display target strings")
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
            let t = DisplayTarget::deserialize(de::value::StrDeserializer::<E>::new(v))?;
            Ok(if matches!(t, DisplayTarget::None) {
                Vec::new()
            } else {
                vec![t]
            })
        }

        fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
            self.visit_str(&v)
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut out = Vec::new();
            while let Some(item) = seq.next_element::<DisplayTarget>()? {
                if !matches!(item, DisplayTarget::None) && !out.contains(&item) {
                    out.push(item);
                }
            }
            Ok(out)
        }
    }

    deserializer.deserialize_any(OneOrMany)
}

impl EffectDefinition {
    /// Get the effective color (explicit color or default gray)
    pub fn effective_color(&self) -> [u8; 4] {
        self.color.unwrap_or(DEFAULT_EFFECT_COLOR)
    }

    /// Check whether this effect should appear on the given overlay target.
    pub fn displays_on(&self, target: DisplayTarget) -> bool {
        self.display_targets.contains(&target)
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

    /// Check if this is a DamageTaken trigger
    pub fn is_damage_taken_trigger(&self) -> bool {
        matches!(self.trigger, Trigger::DamageTaken { .. })
    }

    /// Check if this is a HealingTaken trigger
    pub fn is_healing_taken_trigger(&self) -> bool {
        matches!(self.trigger, Trigger::HealingTaken { .. })
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
            .any(|r| r.matches(ability_id, ability_name))
    }

    /// Find the RefreshAbility entry that matches the given ability.
    /// Returns None if no match found.
    pub fn find_refresh_ability(
        &self,
        ability_id: u64,
        ability_name: Option<&str>,
    ) -> Option<&RefreshAbility> {
        self.refresh_abilities
            .iter()
            .find(|r| r.matches(ability_id, ability_name))
    }

    /// Check if this effect should activate for the given discipline.
    /// Returns true if no discipline filter is set (empty list) or if the
    /// discipline matches one of the configured disciplines.
    /// When discipline is None (unknown/not yet detected), returns true
    /// to avoid hiding effects before discipline detection.
    pub fn matches_discipline(&self, discipline: Option<&Discipline>) -> bool {
        if self.disciplines.is_empty() {
            return true;
        }
        discipline
            .map(|d| self.disciplines.contains(d))
            .unwrap_or(true)
    }

    /// Icon ability ID for uptime placeholders: explicit icon_ability_id,
    /// falling back to the first ID selector in the trigger.
    pub fn uptime_icon_id(&self) -> u64 {
        if let Some(id) = self.icon_ability_id {
            return id;
        }
        match &self.trigger {
            Trigger::EffectApplied { effects, .. } | Trigger::EffectRemoved { effects, .. } => {
                effects.iter().find_map(|s| match s {
                    EffectSelector::Id(id) => Some(*id),
                    EffectSelector::Name(_) => None,
                })
            }
            Trigger::AbilityCast { abilities, .. }
            | Trigger::DamageTaken { abilities, .. }
            | Trigger::HealingTaken { abilities, .. } => {
                abilities.iter().find_map(|s| match s {
                    AbilitySelector::Id(id) => Some(*id),
                    AbilitySelector::Name(_) => None,
                })
            }
            _ => None,
        }
        .unwrap_or(0)
    }

    /// Get the source filter from the trigger
    pub fn source_filter(&self) -> &EntityFilter {
        match &self.trigger {
            Trigger::EffectApplied { source, .. }
            | Trigger::EffectRemoved { source, .. }
            | Trigger::AbilityCast { source, .. }
            | Trigger::DamageTaken { source, .. }
            | Trigger::HealingTaken { source, .. } => source,
            _ => &EntityFilter::Any,
        }
    }

    /// Get the target filter from the trigger
    pub fn target_filter(&self) -> &EntityFilter {
        match &self.trigger {
            Trigger::EffectApplied { target, .. }
            | Trigger::EffectRemoved { target, .. }
            | Trigger::AbilityCast { target, .. }
            | Trigger::DamageTaken { target, .. }
            | Trigger::HealingTaken { target, .. } => target,
            _ => &EntityFilter::Any,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Config File Structure
// ═══════════════════════════════════════════════════════════════════════════

/// Current DSL version for effect definitions.
/// Increment this when making breaking changes to the DSL format.
/// User config files with mismatched versions will be deleted on startup.
pub const EFFECTS_DSL_VERSION: u32 = 1;

/// Root structure for effect config files (TOML)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DefinitionConfig {
    /// DSL version - used to detect and handle breaking changes.
    /// If user file version != EFFECTS_DSL_VERSION, the user file is deleted.
    #[serde(default)]
    pub version: u32,

    /// Effect definitions in this file
    #[serde(default, rename = "effect")]
    pub effects: Vec<EffectDefinition>,
}
