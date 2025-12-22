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

/// Check if an entity ID is a known boss
pub fn is_boss(entity_id: i64) -> bool {
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


