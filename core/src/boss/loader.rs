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

use super::{AreaConfig, AreaType, BossConfig, BossEncounterDefinition};

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
pub fn load_bosses_from_fjle(path: &Path) -> Result<Vec<BossEncounterDefinition>, String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

    let config: BossConfig = toml::from_str(&content)
        .map_err(|e| format!("Failed to parse {}: {}", path.display(), e))?;

    // If file has [area] header, populate area fields on bosses that don't have them
    let mut bosses = config.bosses;
    if let Some(ref area) = config.area {
        for boss in &mut bosses {
            if boss.area_name.is_empty() {
                boss.area_name = area.name.clone();
            }
            if boss.area_id == 0 {
                boss.area_id = area.area_id;
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
    load_bosses_with_paths_recursive(dir, &mut results)?;
    Ok(results)
}

fn load_bosses_with_paths_recursive(
    current_dir: &Path,
    results: &mut Vec<BossWithPath>,
) -> Result<(), String> {
    let entries = fs::read_dir(current_dir)
        .map_err(|e| format!("Failed to read directory {}: {}", current_dir.display(), e))?;

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() {
            load_bosses_with_paths_recursive(&path, results)?;
        } else if path.extension().is_some_and(|ext| ext == "toml") {
            match load_bosses_from_file(&path) {
                Ok(file_bosses) => {
                    // Get category from [area] section in the TOML file
                    let category = load_area_config(&path)
                        .ok()
                        .flatten()
                        .map(|a| a.area_type.to_category())
                        .unwrap_or(AreaType::Other.to_category())
                        .to_string();

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

// ═══════════════════════════════════════════════════════════════════════════════
// Custom File Merging
// ═══════════════════════════════════════════════════════════════════════════════

/// Find the custom overlay file for a bundled file
/// e.g., dxun.toml -> dxun_custom.toml in user config dir
pub fn find_custom_file(bundled_path: &Path, user_dir: &Path) -> Option<PathBuf> {
    let stem = bundled_path.file_stem()?.to_str()?;
    let custom_name = format!("{}_custom.toml", stem);

    // Look in same relative path within user directory
    let relative = bundled_path.parent()?;
    if let Some(category) = relative.file_name() {
        let custom_path = user_dir.join(category).join(&custom_name);
        if custom_path.exists() {
            return Some(custom_path);
        }
    }

    // Also check user directory root
    let custom_path = user_dir.join(&custom_name);
    if custom_path.exists() {
        return Some(custom_path);
    }

    None
}

/// Load bosses from a bundled file, merging with custom overlay if present
pub fn load_bosses_with_custom(
    bundled_path: &Path,
    user_dir: Option<&Path>,
) -> Result<Vec<BossEncounterDefinition>, String> {
    // Load bundled definitions
    let mut bosses = load_bosses_from_file(bundled_path)?;

    // Look for custom overlay file
    if let Some(user_dir) = user_dir
        && let Some(custom_path) = find_custom_file(bundled_path, user_dir)
    {
        match load_bosses_from_file(&custom_path) {
            Ok(custom_bosses) => {
                eprintln!(
                    "Merging {} custom bosses from {}",
                    custom_bosses.len(),
                    custom_path.display()
                );
                bosses = merge_boss_lists(bosses, custom_bosses);
            }
            Err(e) => {
                eprintln!("Warning: Failed to load custom file {}: {}", custom_path.display(), e);
            }
        }
    }

    Ok(bosses)
}

/// Merge two lists of boss definitions by ID
/// Custom entries replace or extend bundled entries
fn merge_boss_lists(
    mut bundled: Vec<BossEncounterDefinition>,
    custom: Vec<BossEncounterDefinition>,
) -> Vec<BossEncounterDefinition> {
    for custom_boss in custom {
        if let Some(base) = bundled.iter_mut().find(|b| b.id == custom_boss.id) {
            // Merge into existing boss
            merge_boss_definition(base, custom_boss);
        } else {
            // New boss from custom file
            bundled.push(custom_boss);
        }
    }
    bundled
}

/// Merge a custom boss definition into a base (bundled) definition
/// Element-level merging: matching IDs replace, new IDs append
fn merge_boss_definition(base: &mut BossEncounterDefinition, custom: BossEncounterDefinition) {
    // Merge timers by ID
    merge_by_id(&mut base.timers, custom.timers, |t| &t.id);

    // Merge challenges by ID
    merge_by_id(&mut base.challenges, custom.challenges, |c| &c.id);

    // Merge counters by ID
    merge_by_id(&mut base.counters, custom.counters, |c| &c.id);

    // Merge phases by ID
    merge_by_id(&mut base.phases, custom.phases, |p| &p.id);

    // Merge entities by name (entities use name as ID)
    merge_by_id(&mut base.entities, custom.entities, |e| &e.name);
}

/// Generic merge helper: replace matching IDs, append new ones
fn merge_by_id<T, F>(base: &mut Vec<T>, custom: Vec<T>, get_id: F)
where
    F: Fn(&T) -> &String,
{
    for custom_item in custom {
        let custom_id = get_id(&custom_item);
        if let Some(base_item) = base.iter_mut().find(|b| get_id(b) == custom_id) {
            // Replace with custom version
            *base_item = custom_item;
        } else {
            // Append new item
            base.push(custom_item);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boss::{ChallengeMetric, ChallengeCondition};
    use crate::entity_filter::EntityFilter;

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
name = "Add Count"
increment_on = { type = "ability_cast", abilities = [12345] }
reset_on = { type = "any_phase_change" }

[[boss.timer]]
id = "test_timer"
name = "Test Timer"
trigger = { type = "ability_cast", abilities = [12345] }
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
        assert!(matches!(boss.phases[0].start_trigger, super::super::PhaseTrigger::CombatStart));
        assert!(matches!(
            boss.phases[1].start_trigger,
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

[[boss.entities]]
name = "Dread Master Bestia"
ids = [3273941900591104]
is_boss = true
is_kill_target = true

[[boss.phase]]
id = "p1"
name = "Phase 1"
trigger = { type = "combat_start" }

[[boss.phase]]
id = "burn"
name = "Burn Phase"
trigger = { type = "boss_hp_below", hp_percent = 30.0, selector = [3273941900591104] }

[[boss.counter]]
id = "dread_scream_casts"
name = "Dread Scream Casts"
increment_on = { type = "ability_cast", abilities = [3302391763959808] }

[[boss.challenge]]
id = "boss_damage"
name = "Boss Damage"
metric = "damage"
conditions = [
    { type = "target", match = "boss" }
]

[[boss.challenge]]
id = "add_damage"
name = "Add Damage"
description = "Damage to Dread Larva and Dread Monster"
metric = "damage"
conditions = [
    { type = "target", match = { selector = [3292079547482112, 3291675820556288] } }
]

[[boss.challenge]]
id = "burn_phase_dps"
name = "Burn Phase DPS"
metric = "damage"
conditions = [
    { type = "phase", phase_ids = ["burn"] },
    { type = "target", match = "boss" }
]

[[boss.challenge]]
id = "local_player_damage"
name = "Your Boss Damage"
metric = "damage"
conditions = [
    { type = "source", match = "local_player" },
    { type = "target", match = "boss" }
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
            ChallengeCondition::Target { matcher: EntityFilter::Boss }
        ));

        let add_damage = &boss.challenges[1];
        assert_eq!(add_damage.id, "add_damage");
        assert!(matches!(
            &add_damage.conditions[0],
            ChallengeCondition::Target { matcher: EntityFilter::Selector(sels) } if sels.len() == 2
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
            ChallengeCondition::Source { matcher: EntityFilter::LocalPlayer }
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

        // Entity roster (new format)
        assert_eq!(bestia.entities.len(), 4); // Bestia, Larva, Monster, Tentacle
        let boss_ids: Vec<i64> = bestia.boss_npc_ids().collect();
        assert_eq!(boss_ids, vec![3273941900591104_i64]);

        // Phases
        assert_eq!(bestia.phases.len(), 3);
        assert_eq!(bestia.phases[0].id, "monsters");
        assert_eq!(bestia.phases[1].id, "bestia");
        assert_eq!(bestia.phases[2].id, "burn");

        // Counters
        assert_eq!(bestia.counters.len(), 4); // larva_spawns, monster_spawns, monster_deaths, dread_scream

        // Challenges
        assert_eq!(bestia.challenges.len(), 5);
        assert_eq!(bestia.challenges[0].id, "boss_damage");
        assert_eq!(bestia.challenges[1].id, "add_damage");
        assert_eq!(bestia.challenges[2].id, "burn_phase_dps");
        assert_eq!(bestia.challenges[3].id, "boss_damage_taken");
        assert_eq!(bestia.challenges[4].id, "local_player_boss_damage");

        // Timers
        assert_eq!(bestia.timers.len(), 7);

        // Combat start timers
        let soft_enrage = &bestia.timers[0];
        assert_eq!(soft_enrage.id, "soft_enrage");
        assert_eq!(soft_enrage.duration_secs, 495.0);

        let a1 = &bestia.timers[1];
        assert_eq!(a1.id, "a1_tentacle");
        assert_eq!(a1.duration_secs, 15.0);

        // Chained timers
        let a2 = &bestia.timers[2];
        assert_eq!(a2.id, "a2_monster");
        assert!(matches!(
            &a2.trigger,
            crate::timers::TimerTrigger::TimerExpires { timer_id } if timer_id == "a1_tentacle"
        ));

        // Ability-based timers
        let swelling = &bestia.timers[4];
        assert_eq!(swelling.id, "swelling_despair");
        assert!(swelling.can_be_refreshed);
        assert!(matches!(
            &swelling.trigger,
            crate::timers::TimerTrigger::AbilityCast { abilities, .. }
                if abilities.len() == 1 && matches!(&abilities[0], crate::triggers::AbilitySelector::Id(3294098182111232))
        ));

        eprintln!("Successfully loaded Bestia fixture with {} timers", bestia.timers.len());
    }
}
