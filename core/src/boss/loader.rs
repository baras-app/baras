//! Boss definition loading and saving
//!
//! Load and save boss encounter definitions from/to TOML files.
//!
//! Supports two formats:
//! - Legacy: Individual boss files with area_name on each boss
//! - Consolidated: Area files with `[area]` header and multiple `[[boss]]` entries

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::{AreaConfig, BossConfig, BossEncounterDefinition};

/// Boss definition with its source file path for saving back
#[derive(Debug, Clone)]
pub struct BossWithPath {
    pub boss: BossEncounterDefinition,
    pub file_path: PathBuf,
    pub category: String, // "operations", "flashpoints", "lair_bosses"
}

/// Lightweight area index entry for lazy loading
/// Only contains metadata needed to find the right file
#[derive(Debug, Clone)]
pub struct AreaIndexEntry {
    pub name: String,
    pub area_id: i64,
    pub file_path: PathBuf,
}

/// Index mapping area_id -> file path for lazy loading
pub type AreaIndex = HashMap<i64, AreaIndexEntry>;

/// Load boss definitions from a single TOML file
/// Handles both legacy format (area_name on each boss) and new consolidated format
pub fn load_bosses_from_file(path: &Path) -> Result<Vec<BossEncounterDefinition>, String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

    let config: BossConfig = toml::from_str(&content)
        .map_err(|e| format!("Failed to parse {}: {}", path.display(), e))?;

    // If file has [area] header, populate area_name on bosses that don't have it
    let mut bosses = config.bosses;
    if let Some(ref area) = config.area {
        for boss in &mut bosses {
            if boss.area_name.is_empty() {
                boss.area_name = area.name.clone();
            }
        }
    }

    Ok(bosses)
}

/// Load just the area config from a file (lightweight, for indexing)
pub fn load_area_config(path: &Path) -> Result<Option<AreaConfig>, String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

    let config: BossConfig = toml::from_str(&content)
        .map_err(|e| format!("Failed to parse {}: {}", path.display(), e))?;

    Ok(config.area)
}

/// Build an area index from a directory of encounter files
/// This is lightweight - only reads [area] headers, not full boss definitions
pub fn build_area_index(dir: &Path) -> Result<AreaIndex, String> {
    let mut index = HashMap::new();

    if !dir.exists() {
        return Ok(index);
    }

    build_area_index_recursive(dir, &mut index)?;
    Ok(index)
}

fn build_area_index_recursive(dir: &Path, index: &mut AreaIndex) -> Result<(), String> {
    let entries = fs::read_dir(dir)
        .map_err(|e| format!("Failed to read directory {}: {}", dir.display(), e))?;

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() {
            build_area_index_recursive(&path, index)?;
        } else if path.extension().is_some_and(|ext| ext == "toml")
            && let Ok(Some(area)) = load_area_config(&path)
                && area.area_id != 0 {
                    index.insert(area.area_id, AreaIndexEntry {
                        name: area.name,
                        area_id: area.area_id,
                        file_path: path,
                    });
        }
    }

    Ok(())
}

/// Load all boss definitions from a directory (recursive)
pub fn load_bosses_from_dir(dir: &Path) -> Result<Vec<BossEncounterDefinition>, String> {
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut bosses = Vec::new();
    load_bosses_recursive(dir, &mut bosses)?;
    Ok(bosses)
}

/// Load all boss definitions with their file paths and categories
/// This is used by the timer editor to know where to save changes back
pub fn load_bosses_with_paths(dir: &Path) -> Result<Vec<BossWithPath>, String> {
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut results = Vec::new();
    load_bosses_with_paths_recursive(dir, dir, &mut results)?;
    Ok(results)
}

fn load_bosses_with_paths_recursive(
    base_dir: &Path,
    current_dir: &Path,
    results: &mut Vec<BossWithPath>,
) -> Result<(), String> {
    let entries = fs::read_dir(current_dir)
        .map_err(|e| format!("Failed to read directory {}: {}", current_dir.display(), e))?;

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() {
            load_bosses_with_paths_recursive(base_dir, &path, results)?;
        } else if path.extension().is_some_and(|ext| ext == "toml") {
            match load_bosses_from_file(&path) {
                Ok(file_bosses) => {
                    // Determine category from path relative to base
                    let category = determine_category(base_dir, &path);

                    for boss in file_bosses {
                        results.push(BossWithPath {
                            boss,
                            file_path: path.clone(),
                            category: category.clone(),
                        });
                    }
                }
                Err(e) => {
                    eprintln!("Warning: {}", e);
                }
            }
        }
    }

    Ok(())
}

/// Determine category (operations/flashpoints/lair_bosses) from file path
/// Looks for known category names anywhere in the path for robustness
fn determine_category(base_dir: &Path, file_path: &Path) -> String {
    let path_str = file_path.to_string_lossy().to_lowercase();

    // Check for known category names in the path
    if path_str.contains("/operations/") || path_str.contains("\\operations\\") {
        return "operations".to_string();
    }
    if path_str.contains("/flashpoints/") || path_str.contains("\\flashpoints\\") {
        return "flashpoints".to_string();
    }
    if path_str.contains("/lair_bosses/") || path_str.contains("\\lair_bosses\\") {
        return "lair_bosses".to_string();
    }

    // Fallback: try relative path extraction
    if let Ok(relative) = file_path.strip_prefix(base_dir) {
        let parts: Vec<_> = relative.components().collect();
        // Need at least 2 parts (category/subdir or category/file.toml)
        if parts.len() >= 2
            && let std::path::Component::Normal(first) = parts[0] {
                let cat = first.to_string_lossy().to_string();
                if !cat.ends_with(".toml") {
                    return cat;
                }
        }
    }

    "unknown".to_string()
}

fn load_bosses_recursive(dir: &Path, bosses: &mut Vec<BossEncounterDefinition>) -> Result<(), String> {
    let entries = fs::read_dir(dir)
        .map_err(|e| format!("Failed to read directory {}: {}", dir.display(), e))?;

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() {
            load_bosses_recursive(&path, bosses)?;
        } else if path.extension().is_some_and(|ext| ext == "toml") {
            match load_bosses_from_file(&path) {
                Ok(file_bosses) => {
                    for boss in &file_bosses {
                        eprintln!(
                            "Loaded boss: {} (area: {})",
                            boss.name, boss.area_name
                        );
                    }
                    bosses.extend(file_bosses);
                }
                Err(e) => {
                    eprintln!("Warning: {}", e);
                }
            }
        }
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Saving
// ═══════════════════════════════════════════════════════════════════════════════

/// Save a single boss definition to a TOML file
pub fn save_boss_to_file(boss: &BossEncounterDefinition, path: &Path) -> Result<(), String> {
    save_bosses_to_file(std::slice::from_ref(boss), path)
}

/// Save multiple boss definitions to a single TOML file
/// Preserves the existing [area] header if present
pub fn save_bosses_to_file(bosses: &[BossEncounterDefinition], path: &Path) -> Result<(), String> {
    // Read existing file to preserve [area] section
    let existing_area = if path.exists() {
        fs::read_to_string(path)
            .ok()
            .and_then(|content| toml::from_str::<BossConfig>(&content).ok())
            .and_then(|config| config.area)
    } else {
        None
    };

    let config = BossConfig {
        area: existing_area,
        bosses: bosses.to_vec(),
    };

    let content = toml::to_string_pretty(&config)
        .map_err(|e| format!("Failed to serialize boss config: {}", e))?;

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory {}: {}", parent.display(), e))?;
    }

    fs::write(path, content)
        .map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boss::{ChallengeMetric, ChallengeCondition, EntityMatcher};

    #[test]
    fn test_parse_boss_config() {
        let toml = r#"
[[boss]]
id = "test_boss"
name = "Test Boss"
area_name = "Test Area"
npc_names = ["Test Boss NPC"]

[[boss.phase]]
id = "p1"
name = "Phase 1"
trigger = { type = "combat_start" }

[[boss.phase]]
id = "p2"
name = "Phase 2"
trigger = { type = "boss_hp_below", hp_percent = 50.0 }
resets_counters = ["add_count"]

[[boss.counter]]
id = "add_count"
increment_on = { type = "ability_cast", ability_ids = [12345] }
reset_on = "phase_change"

[[boss.timer]]
id = "test_timer"
name = "Test Timer"
trigger = { type = "ability_cast", ability_ids = [12345] }
duration_secs = 30.0
phases = ["p1"]
"#;

        let config: BossConfig = toml::from_str(toml).expect("Failed to parse TOML");
        assert_eq!(config.bosses.len(), 1);

        let boss = &config.bosses[0];
        assert_eq!(boss.id, "test_boss");
        assert_eq!(boss.area_name, "Test Area");
        assert_eq!(boss.phases.len(), 2);
        assert_eq!(boss.counters.len(), 1);
        assert_eq!(boss.timers.len(), 1);

        // Check phase trigger parsing
        assert!(matches!(boss.phases[0].trigger, super::super::PhaseTrigger::CombatStart));
        assert!(matches!(
            boss.phases[1].trigger,
            super::super::PhaseTrigger::BossHpBelow { hp_percent, .. } if (hp_percent - 50.0).abs() < 0.01
        ));
    }

    #[test]
    fn test_parse_boss_with_challenges() {
        let toml = r#"
[area]
name = "Dread Palace"
area_id = 833575842743088
category = "operations"

[[boss]]
id = "bestia"
name = "Dread Master Bestia"
npc_ids = [3273941900591104]

[[boss.phase]]
id = "p1"
name = "Phase 1"
trigger = { type = "combat_start" }

[[boss.phase]]
id = "burn"
name = "Burn Phase"
trigger = { type = "boss_hp_below", hp_percent = 30.0, npc_id = 3273941900591104 }

[[boss.counter]]
id = "dread_scream_casts"
increment_on = { type = "ability_cast", ability_ids = [3302391763959808] }

[[boss.challenge]]
id = "boss_damage"
name = "Boss Damage"
metric = "damage"
conditions = [
    { type = "target", match = "any_boss" }
]

[[boss.challenge]]
id = "add_damage"
name = "Add Damage"
description = "Damage to Dread Larva and Dread Monster"
metric = "damage"
conditions = [
    { type = "target", match = { npc_ids = [3292079547482112, 3291675820556288] } }
]

[[boss.challenge]]
id = "burn_phase_dps"
name = "Burn Phase DPS"
metric = "damage"
conditions = [
    { type = "phase", phase_ids = ["burn"] },
    { type = "target", match = "any_boss" }
]

[[boss.challenge]]
id = "local_player_damage"
name = "Your Boss Damage"
metric = "damage"
conditions = [
    { type = "source", match = "local_player" },
    { type = "target", match = "any_boss" }
]
"#;

        let config: BossConfig = toml::from_str(toml).expect("Failed to parse TOML");
        assert_eq!(config.bosses.len(), 1);

        let boss = &config.bosses[0];
        assert_eq!(boss.id, "bestia");
        assert_eq!(boss.phases.len(), 2);
        assert_eq!(boss.counters.len(), 1);
        assert_eq!(boss.challenges.len(), 4);

        // Verify area populated
        let area = config.area.as_ref().expect("Area should be present");
        assert_eq!(area.name, "Dread Palace");
        assert_eq!(area.area_id, 833575842743088);

        // Check challenge parsing
        let boss_damage = &boss.challenges[0];
        assert_eq!(boss_damage.id, "boss_damage");
        assert_eq!(boss_damage.metric, ChallengeMetric::Damage);
        assert_eq!(boss_damage.conditions.len(), 1);
        assert!(matches!(
            &boss_damage.conditions[0],
            ChallengeCondition::Target { matcher: EntityMatcher::AnyBoss }
        ));

        let add_damage = &boss.challenges[1];
        assert_eq!(add_damage.id, "add_damage");
        assert!(matches!(
            &add_damage.conditions[0],
            ChallengeCondition::Target { matcher: EntityMatcher::NpcIds(ids) } if ids.len() == 2
        ));

        let burn_dps = &boss.challenges[2];
        assert_eq!(burn_dps.id, "burn_phase_dps");
        assert_eq!(burn_dps.conditions.len(), 2);
        assert!(matches!(
            &burn_dps.conditions[0],
            ChallengeCondition::Phase { phase_ids } if phase_ids == &["burn"]
        ));

        let local_player = &boss.challenges[3];
        assert!(matches!(
            &local_player.conditions[0],
            ChallengeCondition::Source { matcher: EntityMatcher::LocalPlayer }
        ));
    }

    #[test]
    fn test_load_bestia_fixture() {
        // Load the actual Dread Palace fixture file
        let path = std::path::Path::new("../test-log-files/fixtures/config/dread_palace.toml");
        if !path.exists() {
            eprintln!("Fixture file not found, skipping test");
            return;
        }

        let bosses = load_bosses_from_file(path).expect("Failed to load fixture");
        assert_eq!(bosses.len(), 1);

        let bestia = &bosses[0];
        assert_eq!(bestia.id, "bestia");
        assert_eq!(bestia.name, "Dread Master Bestia");
        assert_eq!(bestia.npc_ids, vec![3273941900591104_i64]);

        // Phases
        assert_eq!(bestia.phases.len(), 2);
        assert_eq!(bestia.phases[0].id, "p1");
        assert_eq!(bestia.phases[1].id, "burn");

        // Counters
        assert_eq!(bestia.counters.len(), 3);

        // Challenges
        assert_eq!(bestia.challenges.len(), 5);
        assert_eq!(bestia.challenges[0].id, "boss_damage");
        assert_eq!(bestia.challenges[1].id, "add_damage");
        assert_eq!(bestia.challenges[2].id, "burn_phase_dps");
        assert_eq!(bestia.challenges[3].id, "boss_damage_taken");
        assert_eq!(bestia.challenges[4].id, "local_player_boss_damage");

        eprintln!("Successfully loaded Bestia fixture with {} challenges", bestia.challenges.len());
    }
}
