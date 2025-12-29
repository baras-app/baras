//! Shared matchers for trigger conditions.
//!
//! These structs provide uniform matching logic across all trigger types
//! (timers, phases, counters) with a consistent priority order:
//! entity roster reference > NPC ID > name.

use serde::{Deserialize, Serialize};

use crate::boss::EntityDefinition;

// Re-export selectors from the shared types crate
pub use baras_types::{AbilitySelector, EffectSelector, EntitySelector};

// ═══════════════════════════════════════════════════════════════════════════
// Entity Selector Extension (core-specific matching with EntityDefinition)
// ═══════════════════════════════════════════════════════════════════════════

/// Extension trait for EntitySelector that provides core-specific matching.
/// These methods need EntityDefinition which lives in core.
pub trait EntitySelectorExt {
    /// Check if this selector matches the given entity.
    ///
    /// For `Name` selectors, resolution priority is:
    /// 1. Roster alias match (if entities provided and name matches a roster entry)
    /// 2. Direct name match (case-insensitive)
    fn matches_with_roster(
        &self,
        entities: &[EntityDefinition],
        npc_id: i64,
        entity_name: Option<&str>,
    ) -> bool;

    /// Check if this selector matches by NPC ID only (ignores roster and name).
    fn matches_npc_id(&self, npc_id: i64) -> bool;

    /// Check if this selector matches by name only (ignores roster and NPC ID).
    fn matches_name_only(&self, name: &str) -> bool;
}

impl EntitySelectorExt for EntitySelector {
    fn matches_with_roster(
        &self,
        entities: &[EntityDefinition],
        npc_id: i64,
        entity_name: Option<&str>,
    ) -> bool {
        match self {
            Self::Id(expected_id) => *expected_id == npc_id,
            Self::Name(name) => {
                // Priority 1: Try roster alias lookup
                if let Some(entity_def) = entities.iter().find(|e| e.name.eq_ignore_ascii_case(name)) {
                    return entity_def.ids.contains(&npc_id);
                }
                // Priority 2: Fall back to name matching
                entity_name
                    .map(|n| n.eq_ignore_ascii_case(name))
                    .unwrap_or(false)
            }
        }
    }

    fn matches_npc_id(&self, npc_id: i64) -> bool {
        match self {
            Self::Id(expected_id) => *expected_id == npc_id,
            Self::Name(_) => false,
        }
    }

    fn matches_name_only(&self, name: &str) -> bool {
        match self {
            Self::Id(_) => false,
            Self::Name(expected) => expected.eq_ignore_ascii_case(name),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Entity Matcher
// ═══════════════════════════════════════════════════════════════════════════

/// Matches entities using a list of selectors.
///
/// Any selector matching means the entity matches (OR logic).
/// Empty selector list matches nothing (require explicit filter).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EntityMatcher {
    /// Entity selectors - any match suffices.
    /// Supports NPC IDs and names/roster aliases.
    #[serde(default)]
    pub entities: Vec<EntitySelector>,
}

impl EntityMatcher {
    /// Create a matcher that matches by NPC ID.
    pub fn by_npc_id(npc_id: i64) -> Self {
        Self { entities: vec![EntitySelector::Id(npc_id)] }
    }

    /// Create a matcher that matches by entity roster reference or name.
    pub fn by_entity(entity: impl Into<String>) -> Self {
        Self { entities: vec![EntitySelector::Name(entity.into())] }
    }

    /// Create a matcher that matches by name (same as by_entity for unified API).
    pub fn by_name(name: impl Into<String>) -> Self {
        Self::by_entity(name)
    }

    /// Create a matcher from multiple selectors.
    pub fn by_selectors(selectors: impl IntoIterator<Item = EntitySelector>) -> Self {
        Self { entities: selectors.into_iter().collect() }
    }

    /// Returns true if no filters are set (matches nothing by design).
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    /// Check if this matcher matches the given entity.
    ///
    /// Returns true if any selector matches, false if none match.
    /// Empty matchers match nothing (require explicit filter).
    pub fn matches(
        &self,
        entities: &[EntityDefinition],
        npc_id: i64,
        name: Option<&str>,
    ) -> bool {
        if self.entities.is_empty() {
            return false;
        }
        self.entities.iter().any(|s| s.matches_with_roster(entities, npc_id, name))
    }

    /// Check if this matcher matches by NPC ID only (ignores roster and name).
    /// Useful when roster isn't available.
    pub fn matches_npc_id(&self, npc_id: i64) -> bool {
        self.entities.iter().any(|s| s.matches_npc_id(npc_id))
    }

    /// Check if this matcher matches by name only (ignores roster and NPC ID).
    /// Useful for simple name comparisons.
    pub fn matches_name(&self, name: &str) -> bool {
        self.entities.iter().any(|s| s.matches_name_only(name))
    }

    /// Check if this matcher has "local_player" as a filter.
    /// This is a special value used to match player entities.
    pub fn is_local_player_filter(&self) -> bool {
        self.entities.iter().any(|s| {
            matches!(s, EntitySelector::Name(name) if name.eq_ignore_ascii_case("local_player"))
        })
    }

    /// Get the first name selector (if any) for display purposes.
    pub fn first_name(&self) -> Option<&str> {
        self.entities.iter().find_map(|s| match s {
            EntitySelector::Name(name) => Some(name.as_str()),
            EntitySelector::Id(_) => None,
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Effect Matcher
// ═══════════════════════════════════════════════════════════════════════════

/// Matches effects by ID or name with optional source/target filters.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EffectMatcher {
    /// Effect selectors that trigger a match (any match suffices).
    #[serde(default)]
    pub effects: Vec<EffectSelector>,

    /// Optional filter for the source entity
    #[serde(default)]
    pub source: EntityMatcher,

    /// Optional filter for the target entity
    #[serde(default)]
    pub target: EntityMatcher,
}

impl EffectMatcher {
    /// Create a matcher for specific effect IDs.
    pub fn by_ids(ids: impl IntoIterator<Item = u64>) -> Self {
        Self {
            effects: ids.into_iter().map(EffectSelector::Id).collect(),
            ..Default::default()
        }
    }

    /// Create a matcher for specific effect selectors.
    pub fn by_selectors(selectors: impl IntoIterator<Item = EffectSelector>) -> Self {
        Self {
            effects: selectors.into_iter().collect(),
            ..Default::default()
        }
    }

    /// Add a source filter.
    pub fn with_source(mut self, source: EntityMatcher) -> Self {
        self.source = source;
        self
    }

    /// Add a target filter.
    pub fn with_target(mut self, target: EntityMatcher) -> Self {
        self.target = target;
        self
    }

    /// Check if the effect matches by ID only.
    pub fn matches_effect_id(&self, effect_id: u64) -> bool {
        self.matches_effect(effect_id, None)
    }

    /// Check if the effect matches by ID and/or name.
    pub fn matches_effect(&self, effect_id: u64, effect_name: Option<&str>) -> bool {
        self.effects.is_empty() || self.effects.iter().any(|s| s.matches(effect_id, effect_name))
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Ability Matcher (simpler variant for ability casts)
// ═══════════════════════════════════════════════════════════════════════════

/// Matches abilities by ID or name with optional source filter.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AbilityMatcher {
    /// Ability selectors that trigger a match (any match suffices).
    #[serde(default)]
    pub abilities: Vec<AbilitySelector>,

    /// Optional filter for the source entity (who cast it)
    #[serde(default)]
    pub source: EntityMatcher,
}

impl AbilityMatcher {
    /// Create a matcher for specific ability IDs.
    pub fn by_ids(ids: impl IntoIterator<Item = u64>) -> Self {
        Self {
            abilities: ids.into_iter().map(AbilitySelector::Id).collect(),
            ..Default::default()
        }
    }

    /// Create a matcher for specific ability selectors.
    pub fn by_selectors(selectors: impl IntoIterator<Item = AbilitySelector>) -> Self {
        Self {
            abilities: selectors.into_iter().collect(),
            ..Default::default()
        }
    }

    /// Add a source filter.
    pub fn with_source(mut self, source: EntityMatcher) -> Self {
        self.source = source;
        self
    }

    /// Check if the ability matches by ID only.
    pub fn matches_ability_id(&self, ability_id: u64) -> bool {
        self.matches_ability(ability_id, None)
    }

    /// Check if the ability matches by ID and/or name.
    pub fn matches_ability(&self, ability_id: u64, ability_name: Option<&str>) -> bool {
        self.abilities.is_empty() || self.abilities.iter().any(|s| s.matches(ability_id, ability_name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_matcher_empty_matches_nothing() {
        let matcher = EntityMatcher::default();
        assert!(!matcher.matches(&[], 12345, Some("Boss")));
    }

    #[test]
    fn entity_matcher_by_npc_id() {
        let matcher = EntityMatcher::by_npc_id(12345);
        assert!(matcher.matches(&[], 12345, Some("Boss")));
        assert!(!matcher.matches(&[], 99999, Some("Boss")));
    }

    #[test]
    fn entity_matcher_by_name_case_insensitive() {
        let matcher = EntityMatcher::by_name("Huntmaster");
        assert!(matcher.matches(&[], 12345, Some("huntmaster")));
        assert!(matcher.matches(&[], 12345, Some("HUNTMASTER")));
        assert!(!matcher.matches(&[], 12345, Some("Other Boss")));
    }

    #[test]
    fn entity_matcher_with_entity_roster() {
        let entities = vec![
            EntityDefinition {
                name: "Huntmaster".to_string(),
                ids: vec![1001, 1002, 1003],
                is_boss: true,
                triggers_encounter: None,
                is_kill_target: true,
            },
        ];
        let matcher = EntityMatcher::by_entity("Huntmaster");
        assert!(matcher.matches(&entities, 1001, None));
        assert!(matcher.matches(&entities, 1002, Some("whatever")));
        assert!(!matcher.matches(&entities, 9999, None));
    }

    #[test]
    fn effect_matcher_by_ids() {
        let matcher = EffectMatcher::by_ids([100, 200, 300]);
        assert!(matcher.matches_effect_id(200));
        assert!(!matcher.matches_effect_id(999));
    }

    #[test]
    fn effect_selector_from_input_parses_id() {
        let selector = EffectSelector::from_input("12345");
        assert_eq!(selector, EffectSelector::Id(12345));
    }

    #[test]
    fn effect_selector_from_input_parses_name() {
        let selector = EffectSelector::from_input("Burn");
        assert_eq!(selector, EffectSelector::Name("Burn".to_string()));
    }

    #[test]
    fn effect_matcher_matches_by_name() {
        let matcher = EffectMatcher::by_selectors([
            EffectSelector::Name("Burn".to_string()),
        ]);
        assert!(matcher.matches_effect(999, Some("Burn")));
        assert!(matcher.matches_effect(999, Some("burn"))); // case insensitive
        assert!(!matcher.matches_effect(999, Some("Freeze")));
        assert!(!matcher.matches_effect(999, None));
    }

    #[test]
    fn effect_matcher_matches_mixed() {
        let matcher = EffectMatcher::by_selectors([
            EffectSelector::Id(100),
            EffectSelector::Name("Burn".to_string()),
        ]);
        // Matches by ID
        assert!(matcher.matches_effect(100, None));
        // Matches by name
        assert!(matcher.matches_effect(999, Some("Burn")));
        // Neither matches
        assert!(!matcher.matches_effect(999, Some("Freeze")));
    }

    #[test]
    fn ability_selector_from_input() {
        assert_eq!(
            AbilitySelector::from_input("12345"),
            AbilitySelector::Id(12345)
        );
        assert_eq!(
            AbilitySelector::from_input("Force Lightning"),
            AbilitySelector::Name("Force Lightning".to_string())
        );
    }

    #[test]
    fn ability_matcher_matches_by_name() {
        let matcher = AbilityMatcher::by_selectors([
            AbilitySelector::Name("Force Lightning".to_string()),
        ]);
        assert!(matcher.matches_ability(999, Some("Force Lightning")));
        assert!(matcher.matches_ability(999, Some("force lightning")));
        assert!(!matcher.matches_ability(999, Some("Saber Strike")));
    }

    #[test]
    fn entity_selector_from_input_parses_id() {
        let selector = EntitySelector::from_input("12345");
        assert_eq!(selector, EntitySelector::Id(12345));
    }

    #[test]
    fn entity_selector_from_input_parses_name() {
        let selector = EntitySelector::from_input("Huntmaster");
        assert_eq!(selector, EntitySelector::Name("Huntmaster".to_string()));
    }

    #[test]
    fn entity_selector_resolves_roster_before_name() {
        let entities = vec![
            EntityDefinition {
                name: "Boss".to_string(),
                ids: vec![1001, 1002],
                is_boss: true,
                triggers_encounter: None,
                is_kill_target: true,
            },
        ];

        // "Boss" should match via roster (ID 1001)
        let selector = EntitySelector::Name("Boss".to_string());
        assert!(selector.matches_with_roster(&entities, 1001, Some("Different Name")));
        assert!(selector.matches_with_roster(&entities, 1002, None));
        // Doesn't match other IDs even if name matches
        assert!(!selector.matches_with_roster(&entities, 9999, Some("Boss")));
    }

    #[test]
    fn entity_selector_falls_back_to_name_match() {
        // When roster is empty, falls back to name matching
        let selector = EntitySelector::Name("Boss".to_string());
        assert!(selector.matches_with_roster(&[], 9999, Some("Boss")));
        assert!(selector.matches_with_roster(&[], 9999, Some("boss"))); // case insensitive
        assert!(!selector.matches_with_roster(&[], 9999, Some("Other")));
    }

    #[test]
    fn entity_matcher_with_multiple_selectors() {
        let matcher = EntityMatcher::by_selectors([
            EntitySelector::Id(1001),
            EntitySelector::Name("Boss".to_string()),
        ]);
        // Matches by ID
        assert!(matcher.matches(&[], 1001, None));
        // Matches by name
        assert!(matcher.matches(&[], 9999, Some("Boss")));
        // Neither matches
        assert!(!matcher.matches(&[], 9999, Some("Other")));
    }
}
