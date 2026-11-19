//! Effect/ability selectors and refresh abilities

use crate::serde_defaults::is_false_ref;
use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// Selectors (unified ID-or-Name matching)
// ─────────────────────────────────────────────────────────────────────────────

/// Selector for effects - can match by ID or name.
/// Uses untagged serde for clean serialization: numbers as IDs, strings as names.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EffectSelector {
    Id(u64),
    Name(String),
}

impl EffectSelector {
    /// Parse from user input - tries ID first, falls back to name.
    pub fn from_input(input: &str) -> Self {
        match input.trim().parse::<u64>() {
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

    /// Check if this selector matches the given ID or name.
    pub fn matches(&self, id: u64, name: Option<&str>) -> bool {
        match self {
            Self::Id(expected) => *expected == id,
            Self::Name(expected) => name.map(|n| n.eq_ignore_ascii_case(expected)).unwrap_or(false),
        }
    }
}

/// Selector for abilities - can match by ID or name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AbilitySelector {
    Id(u64),
    Name(String),
}

impl AbilitySelector {
    /// Parse from user input - tries ID first, falls back to name.
    pub fn from_input(input: &str) -> Self {
        match input.trim().parse::<u64>() {
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

    /// Check if this selector matches the given ID or name.
    pub fn matches(&self, id: u64, name: Option<&str>) -> bool {
        match self {
            Self::Id(expected) => *expected == id,
            Self::Name(expected) => name.map(|n| n.eq_ignore_ascii_case(expected)).unwrap_or(false),
        }
    }
}

/// Scoping for effect refresh/dedup logic.
///
/// Controls which axis of the (source, target) pair is used to identify
/// "the same effect instance" for refresh and `ignore_refreshes` checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefreshScope {
    /// Per-(source, target) pair (default — preserves existing behavior).
    #[default]
    Both,
    /// Per-source only — collapses across targets. Useful when a single
    /// caster shouldn't trigger multiple instances when retargeting
    /// (e.g., HealingTaken triggers on different raid members).
    Source,
    /// Per-target only — collapses across sources. Useful for global
    /// debuffs that get overwritten when any caster reapplies the ability.
    Target,
}

impl RefreshScope {
    /// Returns all variants for UI dropdowns.
    pub fn all() -> &'static [RefreshScope] {
        &[Self::Both, Self::Source, Self::Target]
    }
}

/// When an ability can trigger a refresh
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefreshTrigger {
    /// Refresh on ability activation (default)
    #[default]
    Activation,
    /// Refresh on heal completion (for abilities with cast time)
    Heal,
    /// Refresh on any damage dealt by the ability
    Damage,
}

/// An ability that can refresh an effect, with optional conditions.
/// Supports both simple syntax (just ability ID/name) and conditional syntax.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RefreshAbility {
    /// Simple ability selector (backward compatible): just ID or name
    Simple(AbilitySelector),
    /// Ability with conditions
    Conditional {
        /// The ability that triggers refresh
        ability: AbilitySelector,
        /// Minimum stacks required for refresh (None = any stack count)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min_stacks: Option<u8>,
        /// When the refresh triggers
        #[serde(default)]
        trigger: RefreshTrigger,
        /// For the `Damage` trigger: ignore immune/resist damage events
        /// (they do not refresh the effect). No effect for other triggers.
        #[serde(default, skip_serializing_if = "is_false_ref")]
        ignore_immune_resist: bool,
        /// For the `Activation` trigger: wait for the first damage event from
        /// this ability (after the cast) before refreshing, instead of
        /// refreshing immediately on cast. `None` (unspecified) uses the
        /// per-effect default — DoT-tracker effects defer to first damage,
        /// all others refresh on cast — preserving legacy behavior.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        on_first_damage: Option<bool>,
    },
}

impl RefreshAbility {
    /// Get the ability selector
    pub fn ability(&self) -> &AbilitySelector {
        match self {
            Self::Simple(selector) => selector,
            Self::Conditional { ability, .. } => ability,
        }
    }

    /// Get minimum stacks requirement (None = any)
    pub fn min_stacks(&self) -> Option<u8> {
        match self {
            Self::Simple(_) => None,
            Self::Conditional { min_stacks, .. } => *min_stacks,
        }
    }

    /// Get the trigger type
    pub fn trigger(&self) -> RefreshTrigger {
        match self {
            Self::Simple(_) => RefreshTrigger::Activation,
            Self::Conditional { trigger, .. } => *trigger,
        }
    }

    /// Whether immune/resist damage events should be ignored (Damage trigger only)
    pub fn ignore_immune_resist(&self) -> bool {
        match self {
            Self::Simple(_) => false,
            Self::Conditional {
                ignore_immune_resist, ..
            } => *ignore_immune_resist,
        }
    }

    /// Explicit first-damage override, if set (None = use the per-effect default)
    pub fn on_first_damage(&self) -> Option<bool> {
        match self {
            Self::Simple(_) => None,
            Self::Conditional { on_first_damage, .. } => *on_first_damage,
        }
    }

    /// Whether this refresh should wait for the first damage event from the
    /// ability instead of firing on cast. `is_dot_tracker` supplies the default
    /// when no explicit `on_first_damage` override is set, so DoT-tracker
    /// effects keep deferring to first damage while everything else fires on cast.
    pub fn refresh_on_first_damage(&self, is_dot_tracker: bool) -> bool {
        self.on_first_damage().unwrap_or(is_dot_tracker)
    }

    /// Check if this ability matches the given ID or name
    pub fn matches(&self, id: u64, name: Option<&str>) -> bool {
        self.ability().matches(id, name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_refresh_ability_simple_parsing() {
        // Test simple ability ID
        let toml = r#"value = 814832605462528"#;

        #[derive(Deserialize, Debug)]
        struct Test {
            value: RefreshAbility,
        }

        let parsed: Test = toml::from_str(toml).unwrap();
        println!("Parsed simple: {:?}", parsed.value);

        assert!(matches!(
            parsed.value,
            RefreshAbility::Simple(AbilitySelector::Id(814832605462528))
        ));
        assert_eq!(parsed.value.min_stacks(), None);
        assert_eq!(parsed.value.trigger(), RefreshTrigger::Activation);
    }

    #[test]
    fn test_refresh_ability_conditional_parsing() {
        // Test conditional with min_stacks and trigger
        let toml = r#"
            [value]
            ability = 1014376786034688
            min_stacks = 2
            trigger = "heal"
        "#;

        #[derive(Deserialize, Debug)]
        struct Test {
            value: RefreshAbility,
        }

        let parsed: Test = toml::from_str(toml).unwrap();
        println!("Parsed conditional: {:?}", parsed.value);

        assert!(matches!(parsed.value, RefreshAbility::Conditional { .. }));
        assert_eq!(parsed.value.min_stacks(), Some(2));
        assert_eq!(parsed.value.trigger(), RefreshTrigger::Heal);
    }

    #[test]
    fn test_refresh_ability_array_parsing() {
        // Test array with mixed simple and conditional - exactly like the TOML config
        let toml = r#"
            refresh_abilities = [
                814832605462528,
                { ability = 1014376786034688, min_stacks = 2, trigger = "heal" },
                { ability = 815240627355648, min_stacks = 2, trigger = "heal" },
            ]
        "#;

        #[derive(Deserialize, Debug)]
        struct Test {
            refresh_abilities: Vec<RefreshAbility>,
        }

        let parsed: Test = toml::from_str(toml).unwrap();
        println!("Parsed array: {:?}", parsed.refresh_abilities);

        // First entry: Simple
        assert!(matches!(parsed.refresh_abilities[0], RefreshAbility::Simple(_)));
        assert_eq!(parsed.refresh_abilities[0].min_stacks(), None);
        assert_eq!(parsed.refresh_abilities[0].trigger(), RefreshTrigger::Activation);

        // Second entry: Conditional with min_stacks = 2, trigger = heal
        assert!(matches!(
            parsed.refresh_abilities[1],
            RefreshAbility::Conditional { .. }
        ));
        assert_eq!(parsed.refresh_abilities[1].min_stacks(), Some(2));
        assert_eq!(parsed.refresh_abilities[1].trigger(), RefreshTrigger::Heal);

        // Third entry: Conditional with min_stacks = 2, trigger = heal
        assert!(matches!(
            parsed.refresh_abilities[2],
            RefreshAbility::Conditional { .. }
        ));
        assert_eq!(parsed.refresh_abilities[2].min_stacks(), Some(2));
        assert_eq!(parsed.refresh_abilities[2].trigger(), RefreshTrigger::Heal);
    }

    #[test]
    fn test_refresh_on_first_damage_resolution() {
        // Simple selector: no override → falls back to the per-effect default.
        let simple = RefreshAbility::Simple(AbilitySelector::from_input("123"));
        assert_eq!(simple.on_first_damage(), None);
        assert!(simple.refresh_on_first_damage(true)); // DotTracker → first damage
        assert!(!simple.refresh_on_first_damage(false)); // others → on cast

        // Explicit override wins regardless of display target.
        let toml = r#"
            refresh_abilities = [
                { ability = 1, on_first_damage = true },
                { ability = 2, on_first_damage = false },
                { ability = 3 },
            ]
        "#;
        #[derive(Deserialize, Debug)]
        struct Test {
            refresh_abilities: Vec<RefreshAbility>,
        }
        let parsed: Test = toml::from_str(toml).unwrap();

        // Some(true): first damage even for a non-DotTracker effect.
        assert_eq!(parsed.refresh_abilities[0].on_first_damage(), Some(true));
        assert!(parsed.refresh_abilities[0].refresh_on_first_damage(false));

        // Some(false): on cast even for a DotTracker effect.
        assert_eq!(parsed.refresh_abilities[1].on_first_damage(), Some(false));
        assert!(!parsed.refresh_abilities[1].refresh_on_first_damage(true));

        // Unspecified conditional: still defers to the per-effect default.
        assert_eq!(parsed.refresh_abilities[2].on_first_damage(), None);
        assert!(parsed.refresh_abilities[2].refresh_on_first_damage(true));
    }

    #[test]
    fn test_refresh_ability_round_trip_omits_default_first_damage() {
        // A conditional that only sets on_first_damage should serialize that field
        // and nothing else; an unset override must not appear (backward compat).
        let ability = RefreshAbility::Conditional {
            ability: AbilitySelector::from_input("123"),
            min_stacks: None,
            trigger: RefreshTrigger::Activation,
            ignore_immune_resist: false,
            on_first_damage: None,
        };
        let serialized = toml::to_string(&Wrap { value: ability }).unwrap();
        assert!(!serialized.contains("on_first_damage"));

        #[derive(Serialize, Deserialize)]
        struct Wrap {
            value: RefreshAbility,
        }
    }
}
