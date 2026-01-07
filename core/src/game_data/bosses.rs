//! Boss and Encounter identification data
//!
//! Provides lookup from entity IDs to boss/encounter information.
//! Data sourced from Orbs SWTOR Combat Parser.

use hashbrown::HashMap;
use std::sync::LazyLock;

use super::flashpoint_bosses::FLASHPOINT_BOSS_DATA;
use super::lair_bosses::LAIR_BOSS_DATA;
use super::raid_bosses::RAID_BOSS_DATA;

/// Lazy-initialized lookup table combining all boss data
static BOSS_LOOKUP: LazyLock<HashMap<i64, BossInfo>> = LazyLock::new(|| {
    let total = RAID_BOSS_DATA.len() + LAIR_BOSS_DATA.len() + FLASHPOINT_BOSS_DATA.len();
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

    /// Returns the difficulty tier (Story, Veteran, Master)
    pub fn tier(&self) -> &'static str {
        match self {
            Difficulty::Story8 | Difficulty::Story16 => "Story",
            Difficulty::Veteran4 | Difficulty::Veteran8 | Difficulty::Veteran16 => "Veteran",
            Difficulty::Master4 | Difficulty::Master8 | Difficulty::Master16 => "Master",
        }
    }

    /// Short display name (e.g., "SM 8", "HM 16", "NiM 8")
    pub fn short_name(&self) -> &'static str {
        match self {
            Difficulty::Veteran4 => "Vet",
            Difficulty::Master4 => "MM",
            Difficulty::Story8 => "SM 8",
            Difficulty::Story16 => "SM 16",
            Difficulty::Veteran8 => "HM 8",
            Difficulty::Veteran16 => "HM 16",
            Difficulty::Master8 => "NiM 8",
            Difficulty::Master16 => "NiM 16",
        }
    }

    /// Parse from game difficulty string (e.g., "4 Player Veteran", "8 Player Story Mode")
    pub fn from_game_string(s: &str) -> Option<Self> {
        let s_lower = s.to_ascii_lowercase();

        // Determine group size from string
        let size = if s_lower.contains("16") {
            16
        } else if s_lower.contains("8") {
            8
        } else if s_lower.contains("4") {
            4
        } else {
            return None;
        };

        // Determine tier from string
        let is_master = s_lower.contains("master") || s_lower.contains("nightmare");
        let is_veteran = s_lower.contains("veteran") || s_lower.contains("hard");
        let is_story = s_lower.contains("story");

        match (size, is_master, is_veteran, is_story) {
            (4, true, _, _) => Some(Difficulty::Master4),
            (4, _, true, _) => Some(Difficulty::Veteran4),
            (4, _, _, _) => Some(Difficulty::Veteran4), // Default 4-man to Veteran
            (8, true, _, _) => Some(Difficulty::Master8),
            (8, _, true, _) => Some(Difficulty::Veteran8),
            (8, _, _, _) => Some(Difficulty::Story8), // Default 8-man to Story
            (16, true, _, _) => Some(Difficulty::Master16),
            (16, _, true, _) => Some(Difficulty::Veteran16),
            (16, _, _, _) => Some(Difficulty::Story16), // Default 16-man to Story
            _ => None,
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
    /// Handles both exact matches ("veteran") and tier-only matches
    pub fn matches_config_key(&self, key: &str) -> bool {
        let key_lower = key.to_ascii_lowercase();
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

impl BossInfo {
    /// Format as encounter name (e.g., "Eternity Vault: Soa (HM 8)")
    pub fn encounter_name(&self) -> String {
        match self.difficulty {
            Some(d) => format!("{}: {} ({})", self.operation, self.boss, d.short_name()),
            None => format!("{}: {}", self.operation, self.boss),
        }
    }

    /// Format as short name (e.g., "Soa HM 8")
    pub fn short_name(&self) -> String {
        match self.difficulty {
            Some(d) => format!("{} {}", self.boss, d.short_name()),
            None => self.boss.to_string(),
        }
    }
}

/// Lookup boss info by entity ID
pub fn lookup_boss(entity_id: i64) -> Option<&'static BossInfo> {
    BOSS_LOOKUP.get(&entity_id)
}

/// Check if an entity ID is a known boss.
///
/// Checks dynamic registry (from loaded TOML definitions) first,
/// then falls back to hardcoded data.
pub fn is_boss(entity_id: i64) -> bool {
    // Check dynamic registry first (from loaded definitions)
    if let Some(is_registered) = super::boss_registry::is_registered_boss(entity_id) {
        return is_registered;
    }
    // Fall back to hardcoded data
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
