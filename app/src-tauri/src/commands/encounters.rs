//! Unified Encounter Definition CRUD
//!
//! Single module for managing all encounter definition items (timers, phases,
//! counters, challenges, entities) with enum dispatch instead of duplicated commands.
//!
//! Architecture:
//! - Bundled definitions in app resources (read-only)
//! - User customizations in ~/.config/baras/definitions/encounters/*_custom.toml
//! - Timer preferences (enabled/color/audio) stored separately

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, State};

use baras_core::boss::{
    AreaType, BossEncounterDefinition, BossTimerDefinition, BossWithPath, ChallengeDefinition,
    CounterDefinition, EntityDefinition, PhaseDefinition, find_custom_file, load_area_config,
    load_bosses_from_file, load_bosses_with_custom, load_bosses_with_paths, merge_boss_definition,
    save_bosses_to_file,
};
use baras_core::timers::{TimerPreferences, boss_timer_key};

use crate::service::ServiceHandle;
use tracing::debug;

// ═══════════════════════════════════════════════════════════════════════════════
// Core Types
// ═══════════════════════════════════════════════════════════════════════════════

/// Serializable response type for BossWithPath (core type uses PathBuf)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BossWithPathResponse {
    pub boss: BossEncounterDefinition,
    pub file_path: String,
    pub category: String,
}

impl From<BossWithPath> for BossWithPathResponse {
    fn from(bwp: BossWithPath) -> Self {
        Self {
            boss: bwp.boss,
            file_path: bwp.file_path.to_string_lossy().to_string(),
            category: bwp.category,
        }
    }
}

/// Unified wrapper for all encounter definition item types.
/// Uses serde tag for frontend serialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "item_type", rename_all = "snake_case")]
pub enum EncounterItem {
    Timer(BossTimerDefinition),
    Phase(PhaseDefinition),
    Counter(CounterDefinition),
    Challenge(ChallengeDefinition),
    Entity(EntityDefinition),
}

impl EncounterItem {
    /// Get the unique identifier for this item.
    /// Most types use `id` field, Entity uses `name`.
    pub fn id(&self) -> &str {
        match self {
            Self::Timer(t) => &t.id,
            Self::Phase(p) => &p.id,
            Self::Counter(c) => &c.id,
            Self::Challenge(c) => &c.id,
            Self::Entity(e) => &e.name,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tauri Path Helpers (only Tauri-specific logic lives here)
// ═══════════════════════════════════════════════════════════════════════════════

fn get_user_encounters_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("baras").join("definitions").join("encounters"))
}

fn get_bundled_encounters_dir(app_handle: &AppHandle) -> Option<PathBuf> {
    app_handle
        .path()
        .resolve(
            "definitions/encounters",
            tauri::path::BaseDirectory::Resource,
        )
        .ok()
}

fn ensure_user_dir() -> Result<PathBuf, String> {
    let dir = get_user_encounters_dir().ok_or("Could not determine user config directory")?;
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create dir: {}", e))?;
    }
    Ok(dir)
}

/// Load all bosses from bundled + user directories with custom overlays merged.
fn load_all_bosses(app_handle: &AppHandle) -> Result<Vec<BossWithPath>, String> {
    let bundled_dir = get_bundled_encounters_dir(app_handle)
        .ok_or("Could not find bundled encounter definitions")?;
    let user_dir = ensure_user_dir()?;

    // Load bundled with custom overlays merged (uses loader.rs)
    let mut results = load_bosses_with_paths(&bundled_dir)?;

    // Merge custom overlays into bundled bosses
    for bwp in &mut results {
        if let Some(custom_path) = find_custom_file(&bwp.file_path, &user_dir) {
            if let Ok(custom_bosses) = load_bosses_from_file(&custom_path) {
                for custom in custom_bosses {
                    if custom.id == bwp.boss.id {
                        merge_boss_definition(&mut bwp.boss, custom);
                    }
                }
            }
        }
    }

    // Add user-only files (not _custom.toml, no bundled counterpart)
    if user_dir.exists() {
        for bwp in load_bosses_with_paths(&user_dir)? {
            let filename = bwp
                .file_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();
            if filename.ends_with("_custom.toml") {
                continue;
            }
            // Check if this has a bundled counterpart
            if let Ok(rel) = bwp.file_path.strip_prefix(&user_dir) {
                if bundled_dir.join(rel).exists() {
                    continue;
                }
            }
            results.push(bwp);
        }
    }

    Ok(results)
}

/// Check if file is bundled. Returns Some(custom_path) if so.
fn get_custom_path_if_bundled(file_path: &Path, app_handle: &AppHandle) -> Option<PathBuf> {
    let bundled_dir = get_bundled_encounters_dir(app_handle)?;
    let user_dir = get_user_encounters_dir()?;

    let canonical_file = file_path
        .canonicalize()
        .unwrap_or_else(|_| file_path.to_path_buf());
    let canonical_bundled = bundled_dir
        .canonicalize()
        .unwrap_or_else(|_| bundled_dir.clone());

    if canonical_file.starts_with(&canonical_bundled) {
        // Build custom path: user_dir/relative_path/stem_custom.toml
        let relative = file_path.strip_prefix(&bundled_dir).ok()?;
        let stem = file_path.file_stem()?.to_string_lossy();
        let custom_name = format!("{}_custom.toml", stem);
        let custom_path = if let Some(parent) = relative.parent() {
            user_dir.join(parent).join(custom_name)
        } else {
            user_dir.join(custom_name)
        };
        Some(custom_path)
    } else {
        None
    }
}

fn generate_dsl_id(boss_id: &str, name: &str) -> String {
    let name_part: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .split('_')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_");
    format!("{}_{}", boss_id, name_part)
}

/// Load bosses from a single file with custom overlays merged.
fn load_file_with_custom(file_path: &Path) -> Result<Vec<BossWithPath>, String> {
    let user_dir = get_user_encounters_dir();
    debug!(file_path = ?file_path, user_dir = ?user_dir, "load_file_with_custom");

    let mut bosses = load_bosses_with_custom(file_path, user_dir.as_deref())?;
    debug!(count = bosses.len(), "Loaded boss definitions");
    for boss in &bosses {
        debug!(
            name = %boss.name,
            id = %boss.id,
            timer_count = boss.timers.len(),
            "Boss loaded"
        );
    }

    // Rebuild indexes after merge
    for boss in &mut bosses {
        boss.build_indexes();
    }

    // Get category from area config (same as old timers.rs logic)
    let category = load_area_config(file_path)
        .ok()
        .flatten()
        .map(|a| a.area_type.to_category())
        .unwrap_or(AreaType::OpenWorld.to_category())
        .to_string();

    Ok(bosses
        .into_iter()
        .map(|boss| BossWithPath {
            boss,
            file_path: file_path.to_path_buf(),
            category: category.clone(),
        })
        .collect())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Generic Item Operations
// ═══════════════════════════════════════════════════════════════════════════════

/// Check if an item exists in a custom overlay file by type and ID.
fn item_exists_in_custom_by_type(
    custom_path: &Path,
    boss_id: &str,
    item_type: &str,
    item_id: &str,
) -> bool {
    if !custom_path.exists() {
        return false;
    }

    load_bosses_from_file(custom_path)
        .ok()
        .map(|bosses| {
            bosses.iter().any(|b| {
                b.id == boss_id
                    && match item_type {
                        "timer" => b.timers.iter().any(|t| t.id == item_id),
                        "phase" => b.phases.iter().any(|p| p.id == item_id),
                        "counter" => b.counters.iter().any(|c| c.id == item_id),
                        "challenge" => b.challenges.iter().any(|c| c.id == item_id),
                        "entity" => b.entities.iter().any(|e| e.name == item_id),
                        _ => false,
                    }
            })
        })
        .unwrap_or(false)
}

/// Delete an item from a custom overlay file.
fn delete_item_from_custom(
    custom_path: &Path,
    boss_id: &str,
    item_type: &str,
    item_id: &str,
) -> Result<(), String> {
    let mut bosses = load_bosses_from_file(custom_path)
        .map_err(|e| format!("Failed to load custom file: {}", e))?;

    for boss in &mut bosses {
        if boss.id == boss_id {
            match item_type {
                "timer" => boss.timers.retain(|t| t.id != item_id),
                "phase" => boss.phases.retain(|p| p.id != item_id),
                "counter" => boss.counters.retain(|c| c.id != item_id),
                "challenge" => boss.challenges.retain(|c| c.id != item_id),
                "entity" => boss.entities.retain(|e| e.name != item_id),
                _ => return Err(format!("Unknown item type: {}", item_type)),
            }
        }
    }

    // Remove empty boss entries
    bosses.retain(|b| {
        !b.timers.is_empty()
            || !b.phases.is_empty()
            || !b.counters.is_empty()
            || !b.challenges.is_empty()
            || !b.entities.is_empty()
    });

    if bosses.is_empty() {
        std::fs::remove_file(custom_path)
            .map_err(|e| format!("Failed to delete empty custom file: {}", e))?;
    } else {
        save_bosses_to_file(&bosses, custom_path)?;
    }

    Ok(())
}

/// Save an item to a custom overlay file (upsert).
fn save_item_to_custom_file(
    custom_path: &Path,
    boss_id: &str,
    item: &EncounterItem,
) -> Result<(), String> {
    let mut bosses = if custom_path.exists() {
        load_bosses_from_file(custom_path).unwrap_or_default()
    } else {
        Vec::new()
    };

    // Create temp boss with just this item
    let mut temp = BossEncounterDefinition {
        id: boss_id.to_string(),
        ..Default::default()
    };
    match item {
        EncounterItem::Timer(t) => temp.timers.push(t.clone()),
        EncounterItem::Phase(p) => temp.phases.push(p.clone()),
        EncounterItem::Counter(c) => temp.counters.push(c.clone()),
        EncounterItem::Challenge(c) => temp.challenges.push(c.clone()),
        EncounterItem::Entity(e) => temp.entities.push(e.clone()),
    }

    // Merge into existing boss or add new
    if let Some(boss) = bosses.iter_mut().find(|b| b.id == boss_id) {
        merge_boss_definition(boss, temp);
    } else {
        bosses.push(temp);
    }

    if let Some(parent) = custom_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    save_bosses_to_file(&bosses, custom_path)
}

// ═══════════════════════════════════════════════════════════════════════════════
// Timer Preferences
// ═══════════════════════════════════════════════════════════════════════════════

fn timer_preferences_path() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("baras").join("timer_preferences.toml"))
}

fn load_timer_preferences() -> TimerPreferences {
    timer_preferences_path()
        .and_then(|p| TimerPreferences::load(&p).ok())
        .unwrap_or_default()
}

fn save_timer_preferences(prefs: &TimerPreferences) -> Result<(), String> {
    let path = timer_preferences_path().ok_or("Could not determine preferences path")?;
    prefs.save(&path).map_err(|e| e.to_string())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tauri Commands
// ═══════════════════════════════════════════════════════════════════════════════

/// Get bosses for an area file with timer preferences merged.
#[tauri::command]
pub async fn fetch_area_bosses(file_path: String) -> Result<Vec<BossWithPathResponse>, String> {
    let path = PathBuf::from(&file_path);

    debug!(file_path = %file_path, path_exists = path.exists(), "fetch_area_bosses called");

    if !path.exists() {
        return Err(format!("File not found: {}", file_path));
    }

    let mut bosses = load_file_with_custom(&path)?;
    debug!(count = bosses.len(), "Loaded bosses for area");

    let prefs = load_timer_preferences();

    // Merge user preferences into timers
    for bwp in &mut bosses {
        for timer in &mut bwp.boss.timers {
            let key = boss_timer_key(&bwp.boss.area_name, &bwp.boss.name, &timer.id);
            if let Some(p) = prefs.get(&key) {
                if let Some(v) = p.enabled {
                    timer.enabled = v;
                }
                if let Some(v) = p.color {
                    timer.color = v;
                }
                if let Some(v) = p.audio_enabled {
                    timer.audio.enabled = v;
                }
                if let Some(ref v) = p.audio_file {
                    timer.audio.file = Some(v.clone());
                }
            }
        }
    }

    // Convert to serializable response type
    Ok(bosses.into_iter().map(BossWithPathResponse::from).collect())
}

/// Create a new encounter item.
#[tauri::command]
pub async fn create_encounter_item(
    app_handle: AppHandle,
    service: State<'_, ServiceHandle>,
    boss_id: String,
    file_path: String,
    mut item: EncounterItem,
) -> Result<EncounterItem, String> {
    let file_path_buf = PathBuf::from(&file_path);
    let mut bosses = load_all_bosses(&app_handle)?;

    // Find the target boss
    let boss_with_path = bosses
        .iter_mut()
        .find(|b| b.boss.id == boss_id && b.file_path == file_path_buf)
        .ok_or_else(|| format!("Boss '{}' not found", boss_id))?;

    // Generate ID if empty and add to collection
    match &mut item {
        EncounterItem::Timer(t) => {
            if t.id.is_empty() {
                t.id = generate_dsl_id(&boss_id, &t.name);
            }
            if boss_with_path.boss.timers.iter().any(|x| x.id == t.id) {
                return Err(format!("Timer '{}' already exists", t.id));
            }
            boss_with_path.boss.timers.push(t.clone());
        }
        EncounterItem::Phase(p) => {
            if p.id.is_empty() {
                p.id = generate_dsl_id(&boss_id, &p.name);
            }
            if boss_with_path.boss.phases.iter().any(|x| x.id == p.id) {
                return Err(format!("Phase '{}' already exists", p.id));
            }
            boss_with_path.boss.phases.push(p.clone());
        }
        EncounterItem::Counter(c) => {
            if c.id.is_empty() {
                c.id = generate_dsl_id(&boss_id, &c.name);
            }
            if boss_with_path.boss.counters.iter().any(|x| x.id == c.id) {
                return Err(format!("Counter '{}' already exists", c.id));
            }
            boss_with_path.boss.counters.push(c.clone());
        }
        EncounterItem::Challenge(c) => {
            if c.id.is_empty() {
                c.id = generate_dsl_id(&boss_id, &c.name);
            }
            if boss_with_path.boss.challenges.iter().any(|x| x.id == c.id) {
                return Err(format!("Challenge '{}' already exists", c.id));
            }
            boss_with_path.boss.challenges.push(c.clone());
        }
        EncounterItem::Entity(e) => {
            if boss_with_path
                .boss
                .entities
                .iter()
                .any(|x| x.name == e.name)
            {
                return Err(format!("Entity '{}' already exists", e.name));
            }
            boss_with_path.boss.entities.push(e.clone());
        }
    }

    // Save to appropriate file
    if let Some(custom_path) = get_custom_path_if_bundled(&file_path_buf, &app_handle) {
        save_item_to_custom_file(&custom_path, &boss_id, &item)?;
    } else {
        let file_bosses: Vec<_> = bosses
            .iter()
            .filter(|b| b.file_path == file_path_buf)
            .map(|b| b.boss.clone())
            .collect();
        save_bosses_to_file(&file_bosses, &file_path_buf)?;
    }

    let _ = service.reload_timer_definitions().await;
    Ok(item)
}

/// Update an existing encounter item.
#[tauri::command]
pub async fn update_encounter_item(
    app_handle: AppHandle,
    service: State<'_, ServiceHandle>,
    boss_id: String,
    file_path: String,
    item: EncounterItem,
    original_id: Option<String>, // For entity rename (name is the ID)
) -> Result<EncounterItem, String> {
    let file_path_buf = PathBuf::from(&file_path);
    let bosses = load_all_bosses(&app_handle)?;

    // Find the boss
    let boss_with_path = bosses
        .iter()
        .find(|b| b.boss.id == boss_id && b.file_path == file_path_buf)
        .ok_or_else(|| format!("Boss '{}' not found", boss_id))?;

    let item_id = original_id.as_deref().unwrap_or_else(|| item.id());

    // Timer: save preference fields
    if let EncounterItem::Timer(ref t) = item {
        let mut prefs = load_timer_preferences();
        let key = boss_timer_key(
            &boss_with_path.boss.area_name,
            &boss_with_path.boss.name,
            &t.id,
        );
        prefs.update_enabled(&key, t.enabled);
        prefs.update_color(&key, t.color);
        prefs.update_audio_enabled(&key, t.audio.enabled);
        prefs.update_audio_file(&key, t.audio.file.clone());
        save_timer_preferences(&prefs)?;

        // Update live session
        if let Some(session) = service.shared.session.read().await.as_ref() {
            let session = session.read().await;
            if let Some(timer_mgr) = session.timer_manager()
                && let Ok(mut mgr) = timer_mgr.lock()
            {
                mgr.set_preferences(prefs);
            }
        }
    }

    // Save definition changes
    if let Some(custom_path) = get_custom_path_if_bundled(&file_path_buf, &app_handle) {
        save_item_to_custom_file(&custom_path, &boss_id, &item)?;
    } else {
        let mut bosses = load_all_bosses(&app_handle)?;
        let boss = bosses
            .iter_mut()
            .find(|b| b.boss.id == boss_id && b.file_path == file_path_buf)
            .ok_or_else(|| format!("Boss '{}' not found", boss_id))?;

        match &item {
            EncounterItem::Timer(t) => {
                if let Some(existing) = boss.boss.timers.iter_mut().find(|x| x.id == item_id) {
                    *existing = t.clone();
                }
            }
            EncounterItem::Phase(p) => {
                if let Some(existing) = boss.boss.phases.iter_mut().find(|x| x.id == item_id) {
                    *existing = p.clone();
                }
            }
            EncounterItem::Counter(c) => {
                if let Some(existing) = boss.boss.counters.iter_mut().find(|x| x.id == item_id) {
                    *existing = c.clone();
                }
            }
            EncounterItem::Challenge(c) => {
                if let Some(existing) = boss.boss.challenges.iter_mut().find(|x| x.id == item_id) {
                    *existing = c.clone();
                }
            }
            EncounterItem::Entity(e) => {
                if let Some(existing) = boss.boss.entities.iter_mut().find(|x| x.name == item_id) {
                    *existing = e.clone();
                }
            }
        }

        let file_bosses: Vec<_> = bosses
            .iter()
            .filter(|b| b.file_path == file_path_buf)
            .map(|b| b.boss.clone())
            .collect();
        save_bosses_to_file(&file_bosses, &file_path_buf)?;
    }

    let _ = service.reload_timer_definitions().await;
    Ok(item)
}

/// Delete an encounter item.
#[tauri::command]
pub async fn delete_encounter_item(
    app_handle: AppHandle,
    service: State<'_, ServiceHandle>,
    item_type: String,
    item_id: String,
    boss_id: String,
    file_path: String,
) -> Result<(), String> {
    let file_path_buf = PathBuf::from(&file_path);

    if let Some(custom_path) = get_custom_path_if_bundled(&file_path_buf, &app_handle) {
        // Bundled file - only delete from custom overlay
        if item_exists_in_custom_by_type(&custom_path, &boss_id, &item_type, &item_id) {
            delete_item_from_custom(&custom_path, &boss_id, &item_type, &item_id)?;
        } else {
            return Err(format!(
                "Cannot delete bundled {}s. Disable them instead.",
                item_type
            ));
        }
    } else {
        // User file - load just this file, delete, save
        let mut bosses = load_file_with_custom(&file_path_buf)?;
        let boss = bosses
            .iter_mut()
            .find(|b| b.boss.id == boss_id)
            .ok_or_else(|| format!("Boss '{}' not found", boss_id))?;

        let removed = match item_type.as_str() {
            "timer" => {
                let n = boss.boss.timers.len();
                boss.boss.timers.retain(|t| t.id != item_id);
                n != boss.boss.timers.len()
            }
            "phase" => {
                let n = boss.boss.phases.len();
                boss.boss.phases.retain(|p| p.id != item_id);
                n != boss.boss.phases.len()
            }
            "counter" => {
                let n = boss.boss.counters.len();
                boss.boss.counters.retain(|c| c.id != item_id);
                n != boss.boss.counters.len()
            }
            "challenge" => {
                let n = boss.boss.challenges.len();
                boss.boss.challenges.retain(|c| c.id != item_id);
                n != boss.boss.challenges.len()
            }
            "entity" => {
                let n = boss.boss.entities.len();
                boss.boss.entities.retain(|e| e.name != item_id);
                n != boss.boss.entities.len()
            }
            _ => return Err(format!("Unknown item type: {}", item_type)),
        };

        if !removed {
            return Err(format!("{} '{}' not found", item_type, item_id));
        }

        let file_bosses: Vec<_> = bosses.iter().map(|b| b.boss.clone()).collect();
        save_bosses_to_file(&file_bosses, &file_path_buf)?;
    }

    let _ = service.reload_timer_definitions().await;
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Area Index & Creation Commands
// ═══════════════════════════════════════════════════════════════════════════════

/// Response type for area index entries (matches frontend AreaListItem)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AreaListItem {
    pub name: String,
    pub area_id: i64,
    pub file_path: String,
    pub category: String,
    pub boss_count: usize,
    pub timer_count: usize,
}

/// Get area index - list of all encounter areas with boss/timer counts.
#[tauri::command]
pub async fn get_area_index(app_handle: AppHandle) -> Result<Vec<AreaListItem>, String> {
    let bundled_dir = get_bundled_encounters_dir(&app_handle)
        .ok_or("Could not find bundled encounter definitions")?;
    let user_dir = get_user_encounters_dir();

    let mut areas = Vec::new();

    // Scan bundled directory
    scan_areas_recursive(&bundled_dir, user_dir.as_deref(), &mut areas)?;

    // Scan user directory for custom areas (not _custom.toml overlays)
    if let Some(ref user_dir) = user_dir {
        if user_dir.exists() {
            scan_user_only_areas(user_dir, &bundled_dir, &mut areas)?;
        }
    }

    // Sort by name
    areas.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(areas)
}

fn scan_areas_recursive(
    dir: &Path,
    user_dir: Option<&Path>,
    areas: &mut Vec<AreaListItem>,
) -> Result<(), String> {
    if !dir.exists() {
        return Ok(());
    }

    let entries = std::fs::read_dir(dir).map_err(|e| format!("Failed to read directory: {}", e))?;

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() {
            scan_areas_recursive(&path, user_dir, areas)?;
        } else if path.extension().is_some_and(|ext| ext == "toml") {
            // Skip custom overlay files
            let filename = path.file_name().unwrap_or_default().to_string_lossy();
            if filename.ends_with("_custom.toml") {
                continue;
            }

            if let Ok(Some(area_config)) = load_area_config(&path) {
                // Load bosses to get counts (with custom overlays merged)
                let bosses = load_bosses_with_custom(&path, user_dir).unwrap_or_default();
                let boss_count = bosses.len();
                let timer_count: usize = bosses.iter().map(|b| b.timers.len()).sum();

                let category = area_config.area_type.to_category().to_string();

                areas.push(AreaListItem {
                    name: area_config.name,
                    area_id: area_config.area_id,
                    file_path: path.to_string_lossy().to_string(),
                    category,
                    boss_count,
                    timer_count,
                });
            }
        }
    }

    Ok(())
}

fn scan_user_only_areas(
    user_dir: &Path,
    bundled_dir: &Path,
    areas: &mut Vec<AreaListItem>,
) -> Result<(), String> {
    let entries =
        std::fs::read_dir(user_dir).map_err(|e| format!("Failed to read user directory: {}", e))?;

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() {
            // Recurse into subdirectories
            scan_user_only_areas(&path, bundled_dir, areas)?;
        } else if path.extension().is_some_and(|ext| ext == "toml") {
            let filename = path.file_name().unwrap_or_default().to_string_lossy();

            // Skip custom overlay files
            if filename.ends_with("_custom.toml") {
                continue;
            }

            // Check if this has a bundled counterpart
            if let Ok(rel) = path.strip_prefix(user_dir) {
                if bundled_dir.join(rel).exists() {
                    continue; // Already included from bundled scan
                }
            }

            // User-only area file
            if let Ok(Some(area_config)) = load_area_config(&path) {
                let bosses = load_bosses_from_file(&path).unwrap_or_default();
                let boss_count = bosses.len();
                let timer_count: usize = bosses.iter().map(|b| b.timers.len()).sum();
                let category = area_config.area_type.to_category().to_string();

                areas.push(AreaListItem {
                    name: area_config.name,
                    area_id: area_config.area_id,
                    file_path: path.to_string_lossy().to_string(),
                    category,
                    boss_count,
                    timer_count,
                });
            }
        }
    }

    Ok(())
}

/// Request to create a new area file (matches frontend NewAreaRequest)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewAreaRequest {
    pub name: String,
    pub area_id: i64,
    #[serde(default = "default_operation")]
    pub area_type: String,
}

fn default_operation() -> String {
    "operation".to_string()
}

/// Create a new area file.
#[tauri::command]
pub async fn create_area(area: NewAreaRequest) -> Result<String, String> {
    let user_dir = ensure_user_dir()?;

    // Generate filename from area name
    let filename: String = area
        .name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .split('_')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_");

    let file_path = user_dir.join(format!("{}.toml", filename));

    if file_path.exists() {
        return Err(format!("Area file already exists: {}", file_path.display()));
    }

    // Build TOML content
    let area_type = match area.area_type.as_str() {
        "operation" => AreaType::Operation,
        "flashpoint" => AreaType::Flashpoint,
        "lair_boss" => AreaType::LairBoss,
        "training_dummy" => AreaType::TrainingDummy,
        _ => AreaType::OpenWorld,
    };

    let content = format!(
        r#"[area]
name = "{}"
area_id = {}
area_type = "{}"
"#,
        area.name,
        area.area_id,
        area_type
            .to_category()
            .replace("ies", "y")
            .replace("es", "")
            .replace("s", "")
    );

    std::fs::write(&file_path, content).map_err(|e| format!("Failed to write area file: {}", e))?;

    Ok(file_path.to_string_lossy().to_string())
}

/// Request to create a new boss (matches frontend BossEditItem)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BossEditItem {
    pub id: String,
    pub name: String,
    pub area_name: String,
    pub area_id: i64,
    pub file_path: String,
    #[serde(default)]
    pub difficulties: Vec<String>,
}

/// Create a new boss in an area file.
#[tauri::command]
pub async fn create_boss(
    service: State<'_, ServiceHandle>,
    boss: BossEditItem,
) -> Result<BossEditItem, String> {
    let file_path = PathBuf::from(&boss.file_path);

    if !file_path.exists() {
        return Err(format!("Area file not found: {}", boss.file_path));
    }

    // Load existing bosses
    let mut bosses = load_bosses_from_file(&file_path)?;

    // Check for duplicate
    if bosses.iter().any(|b| b.id == boss.id) {
        return Err(format!("Boss '{}' already exists", boss.id));
    }

    // Create new boss definition
    let new_boss = BossEncounterDefinition {
        id: boss.id.clone(),
        name: boss.name.clone(),
        area_name: boss.area_name.clone(),
        area_id: boss.area_id,
        difficulties: boss.difficulties.clone(),
        ..Default::default()
    };

    bosses.push(new_boss);
    save_bosses_to_file(&bosses, &file_path)?;

    let _ = service.reload_timer_definitions().await;
    Ok(boss)
}

/// Update boss notes
#[tauri::command]
pub async fn update_boss_notes(
    service: State<'_, ServiceHandle>,
    boss_id: String,
    file_path: String,
    notes: Option<String>,
) -> Result<(), String> {
    let file_path_buf = PathBuf::from(&file_path);

    if !file_path_buf.exists() {
        return Err(format!("Area file not found: {}", file_path));
    }

    // Load existing bosses
    let mut bosses = load_bosses_from_file(&file_path_buf)?;

    // Find and update the boss
    let boss = bosses
        .iter_mut()
        .find(|b| b.id == boss_id)
        .ok_or_else(|| format!("Boss '{}' not found", boss_id))?;

    boss.notes = notes;

    // Save back to file
    save_bosses_to_file(&bosses, &file_path_buf)?;

    // Trigger definition reload
    let _ = service.reload_timer_definitions().await;

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Boss Notes Selector (for Session tab)
// ═══════════════════════════════════════════════════════════════════════════════

/// A boss with notes info for the selector dropdown
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BossNotesInfo {
    /// Boss definition ID
    pub id: String,
    /// Boss display name
    pub name: String,
    /// Whether this boss has notes
    pub has_notes: bool,
}

/// Get list of bosses with notes status for the current area
/// Returns empty list if no area is loaded
#[tauri::command]
pub async fn get_area_bosses_for_notes(
    service: State<'_, ServiceHandle>,
) -> Result<Vec<BossNotesInfo>, String> {
    service.get_area_bosses_for_notes().await
}

/// Send notes for a specific boss to the overlay
#[tauri::command]
pub async fn select_boss_notes(
    service: State<'_, ServiceHandle>,
    boss_id: String,
) -> Result<(), String> {
    service.select_boss_notes(&boss_id).await
}

// ═══════════════════════════════════════════════════════════════════════════════
// Export / Import Commands
// ═══════════════════════════════════════════════════════════════════════════════

use baras_core::boss::BossConfig;

/// Export response with TOML content and whether it came from a bundled area's custom overlay.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub toml: String,
    pub is_bundled: bool,
}

/// Export user-customized encounter definition(s) as a TOML string.
/// For bundled areas, exports only the `_custom.toml` overlay content.
/// For user areas, exports the file as-is.
/// Returns Err if there are no user customizations to export.
#[tauri::command]
pub async fn export_encounter_toml(
    app_handle: AppHandle,
    boss_id: Option<String>,
    file_path: String,
) -> Result<ExportResult, String> {
    let path = PathBuf::from(&file_path);
    if !path.exists() {
        return Err(format!("File not found: {}", file_path));
    }

    let is_bundled = get_custom_path_if_bundled(&path, &app_handle).is_some();

    let mut bosses = if let Some(custom_path) = get_custom_path_if_bundled(&path, &app_handle) {
        // Bundled area — only export custom overlay content
        if !custom_path.exists() {
            return Err("No custom definitions to export".to_string());
        }
        load_bosses_from_file(&custom_path)?
    } else {
        // User area — export file directly
        load_bosses_from_file(&path)?
    };

    if let Some(ref id) = boss_id {
        bosses.retain(|b| b.id == *id);
        if bosses.is_empty() {
            return Err("No custom definitions to export for this boss".to_string());
        }
    }

    if bosses.is_empty() {
        return Err("No custom definitions to export".to_string());
    }

    let area = load_area_config(&path).ok().flatten();
    let config = BossConfig { area, bosses };

    let toml = toml::to_string(&config)
        .map_err(|e| format!("Failed to serialize: {}", e))?;
    Ok(ExportResult { toml, is_bundled })
}

/// Read a file's text content (for import flow — frontend needs file content after dialog).
#[tauri::command]
pub async fn read_import_file(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read {}: {}", path, e))
}

/// Write exported TOML content to a file path (chosen by save dialog on frontend).
#[tauri::command]
pub async fn save_export_file(path: String, content: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }
    std::fs::write(&p, content).map_err(|e| format!("Failed to write file: {}", e))
}

/// Preview item for import diff
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportItemDiff {
    pub item_type: String,
    pub name: String,
    pub id: String,
}

/// Per-boss preview for import
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportBossPreview {
    pub boss_id: String,
    pub boss_name: String,
    pub is_new_boss: bool,
    pub items_to_replace: Vec<ImportItemDiff>,
    pub items_to_add: Vec<ImportItemDiff>,
    pub items_unchanged: usize,
}

/// Full import preview response
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub source_area_name: Option<String>,
    pub bosses: Vec<ImportBossPreview>,
    pub is_new_area: bool,
    pub errors: Vec<String>,
}

/// Preview an import — parse the TOML and compute diffs against the target area.
#[tauri::command]
pub async fn preview_import_encounter(
    toml_content: String,
    target_file_path: Option<String>,
) -> Result<ImportPreview, String> {
    let config: BossConfig = toml::from_str(&toml_content)
        .map_err(|e| format!("Invalid TOML: {}", e))?;

    let source_area_name = config.area.as_ref().map(|a| a.name.clone());

    // Validate: must contain at least one boss
    let mut errors = Vec::new();
    if config.bosses.is_empty() {
        errors.push("No boss definitions found in file".to_string());
    }
    for boss in &config.bosses {
        if boss.id.is_empty() {
            errors.push(format!("Boss missing ID: '{}'", boss.name));
        }
    }

    // Check area_id mismatch when importing into an existing area
    if let Some(ref tp) = target_file_path {
        let target_path = PathBuf::from(tp);
        if let Some(source_area) = config.area.as_ref() {
            if let Ok(Some(target_area)) = load_area_config(&target_path) {
                if source_area.area_id != 0
                    && target_area.area_id != 0
                    && source_area.area_id != target_area.area_id
                {
                    errors.push(format!(
                        "Area mismatch: source is \"{}\" (ID {}) but target is \"{}\" (ID {})",
                        source_area.name, source_area.area_id,
                        target_area.name, target_area.area_id,
                    ));
                }
            }
        }
    }

    let is_new_area = target_file_path.is_none();

    // Load existing bosses from target for diff computation
    let existing_bosses: Vec<BossEncounterDefinition> = if let Some(ref tp) = target_file_path {
        let path = PathBuf::from(tp);
        if path.exists() {
            load_file_with_custom(&path)?
                .into_iter()
                .map(|b| b.boss)
                .collect()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    let boss_previews: Vec<ImportBossPreview> = config
        .bosses
        .iter()
        .map(|imported| {
            let existing = existing_bosses.iter().find(|b| b.id == imported.id);
            // Resolve display name: imported name → existing name → boss ID
            let display_name = if !imported.name.is_empty() {
                imported.name.clone()
            } else if let Some(e) = existing {
                if !e.name.is_empty() { e.name.clone() } else { imported.id.clone() }
            } else {
                imported.id.clone()
            };

            if let Some(existing) = existing {
                // Existing boss — compute item-level diffs
                let mut to_replace = Vec::new();
                let mut to_add = Vec::new();
                let mut unchanged = 0usize;

                diff_items(&imported.timers, &existing.timers,
                    |t| &t.id, |t| &t.name, "timer",
                    &mut to_replace, &mut to_add, &mut unchanged);
                diff_items(&imported.phases, &existing.phases,
                    |p| &p.id, |p| &p.name, "phase",
                    &mut to_replace, &mut to_add, &mut unchanged);
                diff_items(&imported.counters, &existing.counters,
                    |c| &c.id, |c| &c.name, "counter",
                    &mut to_replace, &mut to_add, &mut unchanged);
                diff_items(&imported.challenges, &existing.challenges,
                    |c| &c.id, |c| &c.name, "challenge",
                    &mut to_replace, &mut to_add, &mut unchanged);
                diff_items(&imported.entities, &existing.entities,
                    |e| &e.name, |e| &e.name, "entity",
                    &mut to_replace, &mut to_add, &mut unchanged);

                ImportBossPreview {
                    boss_id: imported.id.clone(),
                    boss_name: display_name,
                    is_new_boss: false,
                    items_to_replace: to_replace,
                    items_to_add: to_add,
                    items_unchanged: unchanged,
                }
            } else {
                // New boss — all items are adds
                let mut to_add = Vec::new();
                for t in &imported.timers {
                    to_add.push(ImportItemDiff { item_type: "timer".into(), name: t.name.clone(), id: t.id.clone() });
                }
                for p in &imported.phases {
                    to_add.push(ImportItemDiff { item_type: "phase".into(), name: p.name.clone(), id: p.id.clone() });
                }
                for c in &imported.counters {
                    to_add.push(ImportItemDiff { item_type: "counter".into(), name: c.name.clone(), id: c.id.clone() });
                }
                for c in &imported.challenges {
                    to_add.push(ImportItemDiff { item_type: "challenge".into(), name: c.name.clone(), id: c.id.clone() });
                }
                for e in &imported.entities {
                    to_add.push(ImportItemDiff { item_type: "entity".into(), name: e.name.clone(), id: e.name.clone() });
                }

                ImportBossPreview {
                    boss_id: imported.id.clone(),
                    boss_name: display_name,
                    is_new_boss: true,
                    items_to_replace: Vec::new(),
                    items_to_add: to_add,
                    items_unchanged: 0,
                }
            }
        })
        .collect();

    Ok(ImportPreview {
        source_area_name,
        bosses: boss_previews,
        is_new_area,
        errors,
    })
}

/// Diff helper: categorize imported items as replace/add/unchanged vs existing.
fn diff_items<T>(
    imported: &[T],
    existing: &[T],
    get_id: impl Fn(&T) -> &String,
    get_name: impl Fn(&T) -> &String,
    item_type: &str,
    to_replace: &mut Vec<ImportItemDiff>,
    to_add: &mut Vec<ImportItemDiff>,
    unchanged: &mut usize,
) {
    for item in imported {
        let id = get_id(item);
        if existing.iter().any(|e| get_id(e) == id) {
            to_replace.push(ImportItemDiff {
                item_type: item_type.to_string(),
                name: get_name(item).clone(),
                id: id.clone(),
            });
        } else {
            to_add.push(ImportItemDiff {
                item_type: item_type.to_string(),
                name: get_name(item).clone(),
                id: id.clone(),
            });
        }
    }
    // Count existing items NOT in the import
    for item in existing {
        let id = get_id(item);
        if !imported.iter().any(|i| get_id(i) == id) {
            *unchanged += 1;
        }
    }
}

/// Execute an import — merge imported bosses into target area file.
#[tauri::command]
pub async fn import_encounter_toml(
    app_handle: AppHandle,
    service: State<'_, ServiceHandle>,
    toml_content: String,
    target_file_path: Option<String>,
) -> Result<(), String> {
    let config: BossConfig = toml::from_str(&toml_content)
        .map_err(|e| format!("Invalid TOML: {}", e))?;

    if config.bosses.is_empty() {
        return Err("No boss definitions found".to_string());
    }

    match target_file_path {
        None => {
            // New area — write a new file in user encounters dir
            let area = config.area.as_ref()
                .ok_or("Import file has no [area] header — cannot create new area")?;
            let user_dir = ensure_user_dir()?;
            let filename: String = area.name
                .to_lowercase()
                .chars()
                .map(|c| if c.is_alphanumeric() { c } else { '_' })
                .collect::<String>()
                .split('_')
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("_");
            let file_path = user_dir.join(format!("{}.toml", filename));

            let content = toml::to_string(&config)
                .map_err(|e| format!("Failed to serialize: {}", e))?;
            std::fs::write(&file_path, content)
                .map_err(|e| format!("Failed to write file: {}", e))?;
        }
        Some(ref tp) => {
            let file_path = PathBuf::from(tp);
            let write_path = if let Some(custom_path) = get_custom_path_if_bundled(&file_path, &app_handle) {
                // Bundled area → merge into _custom.toml
                custom_path
            } else {
                file_path.clone()
            };

            // Load existing bosses from write target
            let mut existing_bosses = if write_path.exists() {
                load_bosses_from_file(&write_path).unwrap_or_default()
            } else {
                Vec::new()
            };

            // Merge each imported boss
            for imported_boss in config.bosses {
                if let Some(existing) = existing_bosses.iter_mut().find(|b| b.id == imported_boss.id) {
                    merge_boss_definition(existing, imported_boss);
                } else {
                    existing_bosses.push(imported_boss);
                }
            }

            if let Some(parent) = write_path.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            save_bosses_to_file(&existing_bosses, &write_path)?;
        }
    }

    let _ = service.reload_timer_definitions().await;
    Ok(())
}
