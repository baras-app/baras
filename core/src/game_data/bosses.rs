//! Boss and Encounter identification data
//!
//! Provides lookup from entity IDs to boss/encounter information.
//! Data sourced from Orbs SWTOR Combat Parser.

use hashbrown::HashMap;
use std::sync::LazyLock;

use super::flashpoint_bosses::FLASHPOINT_BOSS_DATA;
use super::lair_bosses::LAIR_BOSS_DATA;
use super::raid_bosses::RAID_BOSS_DATA;
use super::world_bosses::WORLD_BOSS_DATA;

/// Lazy-initialized lookup table combining all boss data
static BOSS_LOOKUP: LazyLock<HashMap<i64, BossInfo>> = LazyLock::new(|| {
    let total = RAID_BOSS_DATA.len()
        + LAIR_BOSS_DATA.len()
        + FLASHPOINT_BOSS_DATA.len()
        + WORLD_BOSS_DATA.len();
    let mut map = HashMap::with_capacity(total);
    for (id, info) in RAID_BOSS_DATA.iter() {
        map.insert(*id, info.clone());
    }
    for (id, info) in LAIR_BOSS_DATA.iter() {
        map.insert(*id, info.clone());
    }
    for (id, info) in FLASHPOINT_BOSS_DATA.iter() {
        map.insert(*id, info.clone());
    }
    for (id, info) in WORLD_BOSS_DATA.iter() {
        map.insert(*id, info.clone());
    }
    map
});

/// Lazy-initialized lookup of area/operation names → content type
static AREA_CONTENT_LOOKUP: LazyLock<HashMap<&'static str, ContentType>> = LazyLock::new(|| {
    let mut map = HashMap::new();
    for (_, info) in RAID_BOSS_DATA.iter() {
        // Skip training dummy - "Parsing" isn't a real area
        if info.content_type != ContentType::TrainingDummy {
            map.insert(info.operation, info.content_type);
        }
    }
    for (_, info) in LAIR_BOSS_DATA.iter() {
        map.insert(info.operation, info.content_type);
    }
    for (_, info) in FLASHPOINT_BOSS_DATA.iter() {
        map.insert(info.operation, info.content_type);
    }
    for (_, info) in WORLD_BOSS_DATA.iter() {
        // Skip training dummy - "Parsing" isn't a real area
        if info.content_type != ContentType::TrainingDummy {
            map.insert(info.operation, info.content_type);
        }
    }
    map
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContentType {
    Operation,
    Flashpoint,
    LairBoss,
    TrainingDummy,
    OpenWorld,
}

impl From<crate::dsl::AreaType> for ContentType {
    fn from(area_type: crate::dsl::AreaType) -> Self {
        match area_type {
            crate::dsl::AreaType::Operation => ContentType::Operation,
            crate::dsl::AreaType::Flashpoint => ContentType::Flashpoint,
            crate::dsl::AreaType::LairBoss => ContentType::LairBoss,
            crate::dsl::AreaType::TrainingDummy => ContentType::TrainingDummy,
            crate::dsl::AreaType::OpenWorld => ContentType::OpenWorld,
        }
    }
}

/// Game difficulty IDs (language-independent)
#[allow(unused)]
pub mod difficulty_id {
    pub const STORY_8: i64 = 836045448953651;
    pub const VETERAN_8: i64 = 836045448953652;
    pub const STORY_16: i64 = 836045448953653;
    pub const VETERAN_16: i64 = 836045448953654;
    pub const MASTER_8: i64 = 836045448953655;
    pub const MASTER_16: i64 = 836045448953656;
    pub const STORY_4: i64 = 836045448953658;
    pub const VETERAN_4: i64 = 836045448953657;
    pub const MASTER_4: i64 = 836045448953659;
}

/// Difficulty mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Difficulty {
    // 4-man (Flashpoints)
    Veteran4,
    Master4,
    // 8-man
    Story8,
    Veteran8,
    Master8,
    // 16-man
    Story16,
    Veteran16,
    Master16,
}

impl Difficulty {
    /// Returns the group size (4, 8, or 16)
    pub fn group_size(&self) -> u8 {
        match self {
            Difficulty::Veteran4 | Difficulty::Master4 => 4,
            Difficulty::Story8 | Difficulty::Veteran8 | Difficulty::Master8 => 8,
            Difficulty::Story16 | Difficulty::Veteran16 | Difficulty::Master16 => 16,
        }
    }

    /// Parse from game difficulty ID (language-independent, preferred method)
    pub fn from_difficulty_id(id: i64) -> Option<Self> {
        match id {
            difficulty_id::STORY_8 => Some(Difficulty::Story8),
            difficulty_id::VETERAN_8 => Some(Difficulty::Veteran8),
            difficulty_id::STORY_16 => Some(Difficulty::Story16),
            difficulty_id::VETERAN_16 => Some(Difficulty::Veteran16),
            difficulty_id::MASTER_8 => Some(Difficulty::Master8),
            difficulty_id::MASTER_16 => Some(Difficulty::Master16),
            difficulty_id::VETERAN_4 => Some(Difficulty::Veteran4),
            difficulty_id::MASTER_4 => Some(Difficulty::Master4),
            _ => None,
        }
    }

    /// Short display label: tier + group size (e.g., "Master 8")
    pub fn short_label(&self) -> &'static str {
        match self {
            Difficulty::Veteran4 => "Veteran 4",
            Difficulty::Master4 => "Master 4",
            Difficulty::Story8 => "Story 8",
            Difficulty::Veteran8 => "Veteran 8",
            Difficulty::Master8 => "Master 8",
            Difficulty::Story16 => "Story 16",
            Difficulty::Veteran16 => "Veteran 16",
            Difficulty::Master16 => "Master 16",
        }
    }

    /// Config key for TOML serialization (e.g., "veteran", "master", "story")
    pub fn config_key(&self) -> &'static str {
        match self {
            Difficulty::Story8 | Difficulty::Story16 => "story",
            Difficulty::Veteran4 | Difficulty::Veteran8 | Difficulty::Veteran16 => "veteran",
            Difficulty::Master4 | Difficulty::Master8 | Difficulty::Master16 => "master",
        }
    }

    /// Check if this difficulty matches a config key (case-insensitive)
    ///
    /// Supports both tier-only keys and group-size-qualified keys:
    /// - `"veteran"` → matches Veteran4, Veteran8, Veteran16
    /// - `"veteran_8"` → matches only Veteran8
    /// - `"veteran_16"` → matches only Veteran16
    /// - `"story_16"` → matches only Story16
    pub fn matches_config_key(&self, key: &str) -> bool {
        let key_lower = key.to_ascii_lowercase();

        // Try compound key first (e.g., "veteran_16", "story_8")
        if let Some((tier, size_str)) = key_lower.rsplit_once('_') {
            if let Ok(size) = size_str.parse::<u8>() {
                if size == 4 || size == 8 || size == 16 {
                    return self.config_key() == tier && self.group_size() == size;
                }
            }
        }

        // Fall back to tier-only match (e.g., "veteran" matches both 8 and 16)
        self.config_key() == key_lower
    }
}

/// Information about a boss entity
#[derive(Debug, Clone)]
pub struct BossInfo {
    pub content_type: ContentType,
    pub operation: &'static str,
    pub boss: &'static str,
    pub difficulty: Option<Difficulty>,
    /// True if this entity's death marks the encounter as complete
    pub is_kill_target: bool,
}

/// Lookup boss info by entity ID
pub fn lookup_boss(entity_id: i64) -> Option<&'static BossInfo> {
    BOSS_LOOKUP.get(&entity_id)
}

/// Check if an entity ID is a known boss.
///
/// Returns true if the entity is in the dynamic registry OR the hardcoded data.
pub fn is_boss(entity_id: i64) -> bool {
    // Check dynamic registry (from loaded definitions)
    if super::boss_registry::is_registered_boss(entity_id) == Some(true) {
        return true;
    }
    // Also check hardcoded data
    BOSS_LOOKUP.contains_key(&entity_id)
}

/// Get all boss IDs for a specific operation and boss name
pub fn get_boss_ids(operation: &str, boss: &str) -> Vec<i64> {
    BOSS_LOOKUP
        .iter()
        .filter(|(_, info)| info.operation == operation && info.boss == boss)
        .map(|(id, _)| *id)
        .collect()
}

/// Lookup content type by area/operation name
/// Returns Some(ContentType) if the area is a known operation/flashpoint/lair
pub fn lookup_area_content_type(area_name: &str) -> Option<ContentType> {
    AREA_CONTENT_LOOKUP.get(area_name).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_config_key_tier_only() {
        // Plain tier keys match all group sizes
        assert!(Difficulty::Story8.matches_config_key("story"));
        assert!(Difficulty::Story16.matches_config_key("story"));
        assert!(Difficulty::Veteran8.matches_config_key("veteran"));
        assert!(Difficulty::Veteran16.matches_config_key("veteran"));
        assert!(Difficulty::Veteran4.matches_config_key("veteran"));
        assert!(Difficulty::Master8.matches_config_key("master"));
        assert!(Difficulty::Master16.matches_config_key("master"));
        assert!(Difficulty::Master4.matches_config_key("master"));

        // Non-matching tiers
        assert!(!Difficulty::Story8.matches_config_key("veteran"));
        assert!(!Difficulty::Veteran8.matches_config_key("master"));
        assert!(!Difficulty::Master8.matches_config_key("story"));
    }

    #[test]
    fn test_matches_config_key_compound_8() {
        // 8-man compound keys
        assert!(Difficulty::Story8.matches_config_key("story_8"));
        assert!(!Difficulty::Story16.matches_config_key("story_8"));

        assert!(Difficulty::Veteran8.matches_config_key("veteran_8"));
        assert!(!Difficulty::Veteran16.matches_config_key("veteran_8"));
        assert!(!Difficulty::Veteran4.matches_config_key("veteran_8"));

        assert!(Difficulty::Master8.matches_config_key("master_8"));
        assert!(!Difficulty::Master16.matches_config_key("master_8"));
        assert!(!Difficulty::Master4.matches_config_key("master_8"));
    }

    #[test]
    fn test_matches_config_key_compound_16() {
        // 16-man compound keys
        assert!(Difficulty::Story16.matches_config_key("story_16"));
        assert!(!Difficulty::Story8.matches_config_key("story_16"));

        assert!(Difficulty::Veteran16.matches_config_key("veteran_16"));
        assert!(!Difficulty::Veteran8.matches_config_key("veteran_16"));
        assert!(!Difficulty::Veteran4.matches_config_key("veteran_16"));

        assert!(Difficulty::Master16.matches_config_key("master_16"));
        assert!(!Difficulty::Master8.matches_config_key("master_16"));
        assert!(!Difficulty::Master4.matches_config_key("master_16"));
    }

    #[test]
    fn test_matches_config_key_compound_4() {
        // 4-man compound keys
        assert!(Difficulty::Veteran4.matches_config_key("veteran_4"));
        assert!(!Difficulty::Veteran8.matches_config_key("veteran_4"));
        assert!(!Difficulty::Veteran16.matches_config_key("veteran_4"));

        assert!(Difficulty::Master4.matches_config_key("master_4"));
        assert!(!Difficulty::Master8.matches_config_key("master_4"));
        assert!(!Difficulty::Master16.matches_config_key("master_4"));
    }

    #[test]
    fn test_matches_config_key_case_insensitive() {
        assert!(Difficulty::Veteran8.matches_config_key("Veteran"));
        assert!(Difficulty::Veteran8.matches_config_key("VETERAN"));
        assert!(Difficulty::Veteran16.matches_config_key("Veteran_16"));
        assert!(Difficulty::Veteran16.matches_config_key("VETERAN_16"));
    }

    #[test]
    fn test_matches_config_key_invalid_compound() {
        // Invalid suffixes should not match as compound keys
        // "story_99" has suffix 99, which is not 4/8/16, so falls back to plain match
        assert!(!Difficulty::Story8.matches_config_key("story_99"));
        // "veteran_" is not a valid compound key
        assert!(!Difficulty::Veteran8.matches_config_key("veteran_"));
    }
}
