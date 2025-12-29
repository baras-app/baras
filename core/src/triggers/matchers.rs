//! Shared matchers for trigger conditions.
//!
//! These structs provide uniform matching logic across all trigger types
//! (timers, phases, counters) with a consistent priority order:
//! entity roster reference > NPC ID > name.

use serde::{Deserialize, Serialize};

use crate::boss::EntityDefinition;

// ═══════════════════════════════════════════════════════════════════════════
// Entity Matcher
// ═══════════════════════════════════════════════════════════════════════════

/// Matches entities by roster reference, NPC ID, or name.
///
/// Priority order (first match wins):
/// 1. `entity` - roster reference (most reliable, locale-independent)
/// 2. `npc_id` - NPC class/template ID (stable across locales)
/// 3. `name` - runtime name matching (fallback, locale-dependent)
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EntityMatcher {
    /// Entity reference from roster (preferred)
    /// e.g., "huntmaster", "adds.siege_droid"
    #[serde(default)]
    pub entity: Option<String>,

    /// NPC class/template ID (stable across locales)
    #[serde(default)]
    pub npc_id: Option<i64>,

    /// Entity name for runtime matching (locale-dependent fallback)
    #[serde(default)]
    pub name: Option<String>,
}

impl EntityMatcher {
    /// Create a matcher that matches by NPC ID.
    pub fn by_npc_id(npc_id: i64) -> Self {
        Self { npc_id: Some(npc_id), ..Default::default() }
    }

    /// Create a matcher that matches by entity roster reference.
    pub fn by_entity(entity: impl Into<String>) -> Self {
        Self { entity: Some(entity.into()), ..Default::default() }
    }

    /// Create a matcher that matches by name.
    pub fn by_name(name: impl Into<String>) -> Self {
        Self { name: Some(name.into()), ..Default::default() }
    }

    /// Returns true if no filters are set (matches nothing by design).
    pub fn is_empty(&self) -> bool {
        self.entity.is_none() && self.npc_id.is_none() && self.name.is_none()
    }

    /// Check if this matcher matches the given entity.
    ///
    /// # Arguments
    /// * `entities` - Entity definitions for resolving entity references
    /// * `npc_id` - The NPC ID of the entity being checked
    /// * `name` - The name of the entity being checked
    ///
    /// # Returns
    /// `true` if the entity matches, `false` otherwise.
    /// Empty matchers match nothing (require explicit filter).
    pub fn matches(
        &self,
        entities: &[EntityDefinition],
        npc_id: i64,
        name: Option<&str>,
    ) -> bool {
        // 1. Entity reference (highest priority)
        if let Some(ref entity_ref) = self.entity {
            // Look up entity by name in the roster
            if let Some(entity_def) = entities.iter().find(|e| e.name.eq_ignore_ascii_case(entity_ref)) {
                return entity_def.ids.contains(&npc_id);
            }
            // Entity ref specified but not found in roster - can't match
            return false;
        }

        // 2. NPC ID match
        if let Some(required_id) = self.npc_id {
            return required_id == npc_id;
        }

        // 3. Name fallback (case-insensitive)
        if let Some(ref required_name) = self.name {
            if let Some(actual_name) = name {
                return required_name.eq_ignore_ascii_case(actual_name);
            }
            return false;
        }

        // No filters = match nothing (require explicit filter)
        false
    }

    /// Check if this matcher matches by NPC ID only (ignores roster and name).
    /// Useful when roster isn't available.
    pub fn matches_npc_id(&self, npc_id: i64) -> bool {
        if let Some(required_id) = self.npc_id {
            return required_id == npc_id;
        }
        false
    }

    /// Check if this matcher matches by name only (ignores roster and NPC ID).
    /// Useful for simple name comparisons.
    pub fn matches_name(&self, name: &str) -> bool {
        if let Some(ref required_name) = self.name {
            return required_name.eq_ignore_ascii_case(name);
        }
        false
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Effect Matcher
// ═══════════════════════════════════════════════════════════════════════════

/// Matches effects/abilities by ID with optional source/target filters.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EffectMatcher {
    /// Effect/ability IDs that trigger a match (any match suffices)
    #[serde(default)]
    pub effect_ids: Vec<u64>,

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
            effect_ids: ids.into_iter().collect(),
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

    /// Check if the effect ID matches (source/target checked separately).
    pub fn matches_effect_id(&self, effect_id: u64) -> bool {
        self.effect_ids.is_empty() || self.effect_ids.contains(&effect_id)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Ability Matcher (simpler variant for ability casts)
// ═══════════════════════════════════════════════════════════════════════════

/// Matches abilities by ID with optional source filter.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AbilityMatcher {
    /// Ability IDs that trigger a match (any match suffices)
    #[serde(default)]
    pub ability_ids: Vec<u64>,

    /// Optional filter for the source entity (who cast it)
    #[serde(default)]
    pub source: EntityMatcher,
}

impl AbilityMatcher {
    /// Create a matcher for specific ability IDs.
    pub fn by_ids(ids: impl IntoIterator<Item = u64>) -> Self {
        Self {
            ability_ids: ids.into_iter().collect(),
            ..Default::default()
        }
    }

    /// Add a source filter.
    pub fn with_source(mut self, source: EntityMatcher) -> Self {
        self.source = source;
        self
    }

    /// Check if the ability ID matches (source checked separately).
    pub fn matches_ability_id(&self, ability_id: u64) -> bool {
        self.ability_ids.is_empty() || self.ability_ids.contains(&ability_id)
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
}
