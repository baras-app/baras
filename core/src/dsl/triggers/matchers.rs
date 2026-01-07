//! Shared matchers for trigger conditions.
//!
//! This module provides extension traits for EntitySelector matching
//! with support for entity roster lookups.

use crate::dsl::EntityDefinition;

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

    /// Get the first name selector (if any) for display or lookup purposes.
    fn first_name(&self) -> Option<&str>;
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
                if let Some(entity_def) =
                    entities.iter().find(|e| e.name.eq_ignore_ascii_case(name))
                {
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

    fn first_name(&self) -> Option<&str> {
        match self {
            Self::Id(_) => None,
            Self::Name(name) => Some(name.as_str()),
        }
    }
}

/// Extension trait for slices of EntitySelector.
/// Provides OR-matching: any selector matching means success.
impl EntitySelectorExt for [EntitySelector] {
    fn matches_with_roster(
        &self,
        entities: &[EntityDefinition],
        npc_id: i64,
        entity_name: Option<&str>,
    ) -> bool {
        self.iter()
            .any(|s| s.matches_with_roster(entities, npc_id, entity_name))
    }

    fn matches_npc_id(&self, npc_id: i64) -> bool {
        self.iter().any(|s| s.matches_npc_id(npc_id))
    }

    fn matches_name_only(&self, name: &str) -> bool {
        self.iter().any(|s| s.matches_name_only(name))
    }

    fn first_name(&self) -> Option<&str> {
        self.iter().find_map(|s| s.first_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn entity_selector_matches_npc_id() {
        let selector = EntitySelector::Id(12345);
        assert!(selector.matches_npc_id(12345));
        assert!(!selector.matches_npc_id(99999));
    }

    #[test]
    fn entity_selector_matches_name_case_insensitive() {
        let selector = EntitySelector::Name("Huntmaster".to_string());
        assert!(selector.matches_name_only("huntmaster"));
        assert!(selector.matches_name_only("HUNTMASTER"));
        assert!(!selector.matches_name_only("Other Boss"));
    }

    #[test]
    fn entity_selector_resolves_roster_before_name() {
        let entities = vec![EntityDefinition {
            name: "Boss".to_string(),
            ids: vec![1001, 1002],
            is_boss: true,
            triggers_encounter: None,
            is_kill_target: true,
            show_on_hp_overlay: None,
        }];

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
    fn entity_selector_slice_or_matching() {
        let selectors = vec![
            EntitySelector::Id(1001),
            EntitySelector::Name("Boss".to_string()),
        ];
        // Matches by ID
        assert!(selectors.as_slice().matches_npc_id(1001));
        assert!(!selectors.as_slice().matches_npc_id(9999));
        // Matches by name
        assert!(selectors.as_slice().matches_name_only("Boss"));
        assert!(!selectors.as_slice().matches_name_only("Other"));
    }

    #[test]
    fn effect_selector_from_input() {
        assert_eq!(
            EffectSelector::from_input("12345"),
            EffectSelector::Id(12345)
        );
        assert_eq!(
            EffectSelector::from_input("Burn"),
            EffectSelector::Name("Burn".to_string())
        );
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
}
