//! Effect modifiers, alerts, entity matching, stacks and mitigation

use crate::serde_defaults::{is_false_ref, is_zero_f32_ref};
use crate::{EffectSelector, EntityFilter, Trigger};
use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// Charge Direction (for modifier triggers)
// ─────────────────────────────────────────────────────────────────────────────

/// Direction filter for charge/stack change triggers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChargeDirection {
    Increased,
    Decreased,
    #[serde(alias = "refreshed")]
    Neutral,
}

// ─────────────────────────────────────────────────────────────────────────────
// Effect Modifiers
// ─────────────────────────────────────────────────────────────────────────────

/// A reactive modifier that adjusts an active effect when a trigger fires.
///
/// Modifiers live on an `EffectDefinition` and are evaluated against incoming
/// signals while the effect is active. They can adjust duration, sync charges,
/// and are rate-limited by an optional internal cooldown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectModifier {
    /// What event triggers this modifier
    pub trigger: Trigger,

    /// Duration adjustment in seconds (positive = extend, negative = reduce)
    #[serde(default, skip_serializing_if = "is_zero_f32_ref")]
    pub adjust_duration_secs: f32,

    /// Only activate on critical hits (applies to DamageTaken/HealingTaken triggers)
    #[serde(default, skip_serializing_if = "is_false_ref")]
    pub requires_crit: bool,

    /// Reset remaining duration to the effect's base duration_secs on each proc
    #[serde(default, skip_serializing_if = "is_false_ref")]
    pub refill_duration: bool,

    /// Internal cooldown — minimum seconds between activations
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icd_secs: Option<f32>,

    /// Treat the effect's initial application as an ICD proc, so the modifier
    /// cannot fire until `icd_secs` have elapsed since `applied_at`
    #[serde(default, skip_serializing_if = "is_false_ref")]
    pub icd_from_application: bool,

    /// Scale `icd_secs` by the holder's alacrity (same formula as effect duration)
    #[serde(default, skip_serializing_if = "is_false_ref")]
    pub icd_affected_by_alacrity: bool,

    /// Maximum duration this effect can reach (ceiling)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_duration_secs: Option<f32>,

    /// Remove the effect when this modifier fires (instead of adjusting duration)
    #[serde(default, skip_serializing_if = "is_false_ref")]
    pub cancel: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// Alert Types (shared across effects and timers)
// ─────────────────────────────────────────────────────────────────────────────

/// When to trigger an alert notification.
///
/// Used by both the effect and timer systems to control when alert text
/// is displayed. For effects: OnApply = effect starts, OnExpire = effect ends.
/// For timers: OnApply = timer starts, OnExpire = timer expires.
/// `Countdown` (timers and effects) fires repeatedly during the last N
/// seconds (N configured via `alert_countdown_secs`) with live-updating
/// remaining-time text. The alerts overlay dedupes by id and auto-suppresses
/// when the carried `remaining_secs` reaches zero — no fade tail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertTrigger {
    /// No alert
    #[default]
    None,
    /// Alert on start (effect applied / timer started)
    OnApply,
    /// Alert on end (effect expired / timer expired)
    OnExpire,
    /// Live-updating alert during the last N seconds before expiry.
    Countdown,
}

impl AlertTrigger {
    /// Returns all variants for UI dropdowns.
    pub fn all() -> &'static [AlertTrigger] {
        &[Self::None, Self::OnApply, Self::OnExpire, Self::Countdown]
    }
}

/// Selector for entities - can match by NPC ID, roster alias, or name.
/// Uses untagged serde: numbers as IDs, strings as roster alias or name.
/// Priority when matching: Roster Alias → NPC ID → Name (resolved at runtime).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EntitySelector {
    Id(i64),
    Name(String),
}

impl EntitySelector {
    /// Parse from user input - tries NPC ID first, falls back to name/alias.
    pub fn from_input(input: &str) -> Self {
        match input.trim().parse::<i64>() {
            Ok(id) => Self::Id(id),
            Err(_) => Self::Name(input.trim().to_string()),
        }
    }

    /// Returns the display string for this selector.
    pub fn display(&self) -> String {
        match self {
            Self::Id(id) => id.to_string(),
            Self::Name(name) => name.clone(),
        }
    }
}

/// Wrapper for entity selectors used in source/target filters.
/// Matches the backend's EntityMatcher serialization format.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EntityMatcher {
    #[serde(default)]
    pub selector: Vec<EntitySelector>,
}

impl EntityMatcher {
    pub fn new(selector: Vec<EntitySelector>) -> Self {
        Self { selector }
    }

    pub fn is_empty(&self) -> bool {
        self.selector.is_empty()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Effect Stack Tracking (for counter-based effect stack monitoring)
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration for automatic effect stack tracking on a counter.
///
/// When a counter has this config, it bypasses normal increment/decrement
/// triggers and instead automatically updates based on game effect events
/// (ApplyEffect, ModifyCharges, RemoveEffect).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectStackConfig {
    /// Which effects to track (by ID or name)
    pub effects: Vec<EffectSelector>,

    /// Who has the effect (required — determines which entities' stacks to track)
    #[serde(default = "EntityFilter::default_any")]
    pub target: EntityFilter,

    /// How to aggregate when multiple entities match the target filter
    #[serde(default)]
    pub aggregation: StackAggregation,
}

/// How to aggregate stack counts across multiple entities
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StackAggregation {
    /// Maximum stacks on any matching entity (most common — "someone has N stacks")
    #[default]
    Max,
    /// Sum of stacks across all matching entities
    Sum,
    /// Minimum stacks on any matching entity (0 if no entity has the effect)
    Min,
}

// ─────────────────────────────────────────────────────────────────────────────
// Mitigation / Defense Type
// ─────────────────────────────────────────────────────────────────────────────

/// Defense result that reduced or negated damage.
///
/// Maps directly to the game's `defense_type_id` log field.
/// Used as an optional filter on `Trigger::DamageTaken` to fire only when a
/// specific mitigation result occurs (e.g., only on IMMUNE, only on RESIST).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MitigationType {
    Miss,
    Parry,
    Dodge,
    Immune,
    Resist,
    Deflect,
    Shield,
    Absorbed,
    Cover,
    Reflected,
}

impl MitigationType {
    /// Returns the game's numeric `defense_type_id` for this mitigation result.
    pub fn defense_type_id(self) -> i64 {
        match self {
            Self::Miss => 836045448945502,
            Self::Parry => 836045448945503,
            Self::Dodge => 836045448945505,
            Self::Immune => 836045448945506,
            Self::Resist => 836045448945507,
            Self::Deflect => 836045448945508,
            Self::Shield => 836045448945509,
            Self::Absorbed => 836045448945511,
            Self::Cover => 836045448945510,
            Self::Reflected => 836045448953649,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Miss => "Miss",
            Self::Parry => "Parry",
            Self::Dodge => "Dodge",
            Self::Immune => "Immune",
            Self::Resist => "Resist",
            Self::Deflect => "Deflect",
            Self::Shield => "Shield",
            Self::Absorbed => "Absorbed",
            Self::Cover => "Cover",
            Self::Reflected => "Reflected",
        }
    }

    pub const ALL: &'static [Self] = &[
        Self::Miss,
        Self::Parry,
        Self::Dodge,
        Self::Immune,
        Self::Resist,
        Self::Deflect,
        Self::Shield,
        Self::Absorbed,
        Self::Cover,
        Self::Reflected,
    ];
}
