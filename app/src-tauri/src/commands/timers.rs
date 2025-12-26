//! Timer editor Tauri commands
//!
//! CRUD operations for encounter timers displayed in the timer editor UI.
//!
//! Architecture:
//! - Default encounter definitions are bundled with the app (read-only)
//! - On first launch, defaults are copied to user config dir (~/.config/baras/encounters/)
//! - All edits are made to the user config copy, never the bundled defaults
//! - User can reset to defaults by deleting the user config dir

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, State};

use baras_core::boss::{
    load_bosses_with_paths, save_bosses_to_file, BossTimerDefinition,
    BossTimerTrigger, BossWithPath,
};

use crate::service::ServiceHandle;

// ─────────────────────────────────────────────────────────────────────────────
// Types for Frontend
// ─────────────────────────────────────────────────────────────────────────────

/// Flattened timer item for the frontend list view
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimerListItem {
    // Identity
    pub timer_id: String,
    pub boss_id: String,
    pub boss_name: String,
    pub area_name: String,
    pub category: String,
    pub file_path: String,

    // Timer data
    pub name: String,
    pub enabled: bool,
    pub duration_secs: f32,
    pub color: [u8; 4],
    pub phases: Vec<String>,
    pub difficulties: Vec<String>,

    // Trigger info (serialized for frontend)
    pub trigger: BossTimerTrigger,

    // Optional fields
    pub can_be_refreshed: bool,
    pub repeats: u8,
    pub chains_to: Option<String>,
    pub alert_at_secs: Option<f32>,
    pub show_on_raid_frames: bool,
}

impl TimerListItem {
    /// Convert a BossWithPath + timer index to a flattened list item
    fn from_boss_timer(boss_with_path: &BossWithPath, timer: &BossTimerDefinition) -> Self {
        Self {
            timer_id: timer.id.clone(),
            boss_id: boss_with_path.boss.id.clone(),
            boss_name: boss_with_path.boss.name.clone(),
            area_name: boss_with_path.boss.area_name.clone(),
            category: boss_with_path.category.clone(),
            file_path: boss_with_path.file_path.to_string_lossy().to_string(),

            name: timer.name.clone(),
            enabled: timer.enabled,
            duration_secs: timer.duration_secs,
            color: timer.color,
            phases: timer.phases.clone(),
            difficulties: timer.difficulties.clone(),

            trigger: timer.trigger.clone(),

            can_be_refreshed: timer.can_be_refreshed,
            repeats: timer.repeats,
            chains_to: timer.chains_to.clone(),
            alert_at_secs: timer.alert_at_secs,
            show_on_raid_frames: timer.show_on_raid_frames,
        }
    }

    /// Convert back to a BossTimerDefinition
    fn to_timer_definition(&self) -> BossTimerDefinition {
        BossTimerDefinition {
            id: self.timer_id.clone(),
            name: self.name.clone(),
            trigger: self.trigger.clone(),
            duration_secs: self.duration_secs,
            color: self.color,
            phases: self.phases.clone(),
            counter_condition: None, // TODO: Add to UI if needed
            difficulties: self.difficulties.clone(),
            enabled: self.enabled,
            can_be_refreshed: self.can_be_refreshed,
            repeats: self.repeats,
            chains_to: self.chains_to.clone(),
            alert_at_secs: self.alert_at_secs,
            show_on_raid_frames: self.show_on_raid_frames,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper Functions
// ─────────────────────────────────────────────────────────────────────────────

/// Get the user's encounters config directory
/// Returns ~/.config/baras/encounters/ (or equivalent on Windows/Mac)
fn get_user_encounters_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("baras").join("encounters"))
}

/// Get the bundled default encounters directory
fn get_bundled_encounters_dir(app_handle: &AppHandle) -> Option<PathBuf> {
    app_handle
        .path()
        .resolve("definitions/encounters", tauri::path::BaseDirectory::Resource)
        .ok()
}

/// Ensure user encounters directory exists and has defaults copied
/// This is called before any timer operations to guarantee the user dir is ready
fn ensure_user_encounters_dir(app_handle: &AppHandle) -> Result<PathBuf, String> {
    let user_dir = get_user_encounters_dir()
        .ok_or_else(|| "Could not determine user config directory".to_string())?;

    // If user dir already exists with content, use it as-is
    if user_dir.exists() {
        let has_content = std::fs::read_dir(&user_dir)
            .map(|mut entries| entries.next().is_some())
            .unwrap_or(false);

        if has_content {
            eprintln!("[TIMERS] Using existing user encounters dir: {:?}", user_dir);
            return Ok(user_dir);
        }
    }

    // User dir is empty or doesn't exist - copy defaults
    let bundled_dir = get_bundled_encounters_dir(app_handle)
        .ok_or_else(|| "Could not find bundled encounter definitions".to_string())?;

    if !bundled_dir.exists() {
        return Err(format!(
            "Bundled encounters directory does not exist: {:?}",
            bundled_dir
        ));
    }

    eprintln!(
        "[TIMERS] Copying default encounters from {:?} to {:?}",
        bundled_dir, user_dir
    );

    copy_dir_recursive(&bundled_dir, &user_dir)?;

    eprintln!("[TIMERS] Successfully copied default encounters to user dir");
    Ok(user_dir)
}

/// Recursively copy a directory
fn copy_dir_recursive(src: &PathBuf, dst: &PathBuf) -> Result<(), String> {
    std::fs::create_dir_all(dst)
        .map_err(|e| format!("Failed to create directory {:?}: {}", dst, e))?;

    let entries = std::fs::read_dir(src)
        .map_err(|e| format!("Failed to read directory {:?}: {}", src, e))?;

    for entry in entries.flatten() {
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)
                .map_err(|e| format!("Failed to copy {:?} to {:?}: {}", src_path, dst_path, e))?;
        }
    }

    Ok(())
}

/// Load all bosses from the user config directory
/// Ensures defaults are copied first if needed
fn load_user_bosses(app_handle: &AppHandle) -> Result<Vec<BossWithPath>, String> {
    let user_dir = ensure_user_encounters_dir(app_handle)?;

    load_bosses_with_paths(&user_dir)
        .map_err(|e| format!("Failed to load bosses from user dir: {}", e))
}


// ─────────────────────────────────────────────────────────────────────────────
// Tauri Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Get all encounter timers as a flat list
#[tauri::command]
pub async fn get_encounter_timers(app_handle: AppHandle) -> Result<Vec<TimerListItem>, String> {
    let bosses = load_user_bosses(&app_handle)?;

    let mut items = Vec::new();
    for boss_with_path in &bosses {
        for timer in &boss_with_path.boss.timers {
            items.push(TimerListItem::from_boss_timer(boss_with_path, timer));
        }
    }

    // Sort by boss name, then timer name
    items.sort_by(|a, b| {
        a.boss_name
            .cmp(&b.boss_name)
            .then(a.name.cmp(&b.name))
    });

    Ok(items)
}

/// Update an existing timer
#[tauri::command]
pub async fn update_encounter_timer(
    app_handle: AppHandle,
    service: State<'_, ServiceHandle>,
    timer: TimerListItem,
) -> Result<(), String> {
    let mut bosses = load_user_bosses(&app_handle)?;

    // Find the boss and update the timer
    let mut found = false;
    for boss_with_path in &mut bosses {
        if boss_with_path.boss.id == timer.boss_id
            && boss_with_path.file_path.to_string_lossy() == timer.file_path
        {
            for existing_timer in &mut boss_with_path.boss.timers {
                if existing_timer.id == timer.timer_id {
                    *existing_timer = timer.to_timer_definition();
                    found = true;
                    break;
                }
            }
            break;
        }
    }

    if !found {
        return Err(format!(
            "Timer '{}' not found in boss '{}'",
            timer.timer_id, timer.boss_id
        ));
    }

    // Save the modified file
    let file_path = PathBuf::from(&timer.file_path);
    let file_bosses: Vec<_> = bosses
        .iter()
        .filter(|b| b.file_path == file_path)
        .map(|b| b.boss.clone())
        .collect();

    save_bosses_to_file(&file_bosses, &file_path)?;

    // Reload definitions into the running session
    let _ = service.reload_timer_definitions().await;

    Ok(())
}

/// Create a new timer for a boss
#[tauri::command]
pub async fn create_encounter_timer(
    app_handle: AppHandle,
    service: State<'_, ServiceHandle>,
    timer: TimerListItem,
) -> Result<TimerListItem, String> {
    let mut bosses = load_user_bosses(&app_handle)?;
    let file_path_buf = PathBuf::from(&timer.file_path);
    let boss_id = &timer.boss_id;

    // Generate a unique timer ID if not provided (prefixed with boss_id)
    let timer_id = if timer.timer_id.is_empty() {
        generate_timer_id(boss_id, &timer.name)
    } else {
        timer.timer_id.clone()
    };

    // Convert to BossTimerDefinition
    let new_timer = BossTimerDefinition {
        id: timer_id.clone(),
        name: timer.name.clone(),
        trigger: timer.trigger.clone(),
        duration_secs: timer.duration_secs,
        color: timer.color,
        phases: timer.phases.clone(),
        counter_condition: None,
        difficulties: timer.difficulties.clone(),
        enabled: timer.enabled,
        can_be_refreshed: timer.can_be_refreshed,
        repeats: timer.repeats,
        chains_to: timer.chains_to.clone(),
        alert_at_secs: timer.alert_at_secs,
        show_on_raid_frames: timer.show_on_raid_frames,
    };

    // Check for duplicate ID across ALL bosses (not just the target boss)
    for boss_with_path in &bosses {
        if boss_with_path.boss.timers.iter().any(|t| t.id == timer_id) {
            return Err(format!(
                "Timer with ID '{}' already exists in boss '{}'. Timer IDs must be globally unique.",
                timer_id, boss_with_path.boss.name
            ));
        }
    }

    // Find the boss and add the timer
    let mut created_item: Option<TimerListItem> = None;
    for boss_with_path in &mut bosses {
        if boss_with_path.boss.id == *boss_id && boss_with_path.file_path == file_path_buf {
            boss_with_path.boss.timers.push(new_timer.clone());
            created_item = Some(TimerListItem::from_boss_timer(boss_with_path, &new_timer));
            break;
        }
    }

    let item = created_item.ok_or_else(|| format!("Boss '{}' not found", boss_id))?;

    // Save the modified file
    let file_bosses: Vec<_> = bosses
        .iter()
        .filter(|b| b.file_path == file_path_buf)
        .map(|b| b.boss.clone())
        .collect();

    save_bosses_to_file(&file_bosses, &file_path_buf)?;

    // Reload definitions into the running session
    let _ = service.reload_timer_definitions().await;

    Ok(item)
}

/// Generate a timer ID from boss ID and timer name (snake_case, safe for TOML)
/// Format: {boss_id}_{timer_name_snake_case}
fn generate_timer_id(boss_id: &str, name: &str) -> String {
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

/// Delete a timer
#[tauri::command]
pub async fn delete_encounter_timer(
    app_handle: AppHandle,
    service: State<'_, ServiceHandle>,
    timer_id: String,
    boss_id: String,
    file_path: String,
) -> Result<(), String> {
    let mut bosses = load_user_bosses(&app_handle)?;
    let file_path_buf = PathBuf::from(&file_path);

    // Find the boss and remove the timer
    let mut found = false;
    for boss_with_path in &mut bosses {
        if boss_with_path.boss.id == boss_id && boss_with_path.file_path == file_path_buf {
            let original_len = boss_with_path.boss.timers.len();
            boss_with_path.boss.timers.retain(|t| t.id != timer_id);
            found = boss_with_path.boss.timers.len() < original_len;
            break;
        }
    }

    if !found {
        return Err(format!(
            "Timer '{}' not found in boss '{}'",
            timer_id, boss_id
        ));
    }

    // Save the modified file
    let file_bosses: Vec<_> = bosses
        .iter()
        .filter(|b| b.file_path == file_path_buf)
        .map(|b| b.boss.clone())
        .collect();

    save_bosses_to_file(&file_bosses, &file_path_buf)?;

    // Reload definitions into the running session
    let _ = service.reload_timer_definitions().await;

    Ok(())
}

/// Duplicate a timer with a new ID
#[tauri::command]
pub async fn duplicate_encounter_timer(
    app_handle: AppHandle,
    service: State<'_, ServiceHandle>,
    timer_id: String,
    boss_id: String,
    file_path: String,
) -> Result<TimerListItem, String> {
    let mut bosses = load_user_bosses(&app_handle)?;
    let file_path_buf = PathBuf::from(&file_path);

    // Find the timer to duplicate
    let mut new_timer: Option<BossTimerDefinition> = None;

    for boss_with_path in &bosses {
        if boss_with_path.boss.id == boss_id && boss_with_path.file_path == file_path_buf {
            if let Some(timer) = boss_with_path.boss.timers.iter().find(|t| t.id == timer_id) {
                let mut cloned = timer.clone();

                // Generate unique ID (check globally across ALL bosses)
                let mut suffix = 1;
                loop {
                    let new_id = format!("{}_copy{}", timer_id, suffix);
                    let exists_globally = bosses.iter().any(|b| {
                        b.boss.timers.iter().any(|t| t.id == new_id)
                    });
                    if !exists_globally {
                        cloned.id = new_id;
                        cloned.name = format!("{} (Copy)", timer.name);
                        break;
                    }
                    suffix += 1;
                }

                new_timer = Some(cloned);
            }
            break;
        }
    }

    let timer = new_timer.ok_or_else(|| format!("Timer '{}' not found", timer_id))?;

    // Add the duplicated timer
    for boss_with_path in &mut bosses {
        if boss_with_path.boss.id == boss_id && boss_with_path.file_path == file_path_buf {
            boss_with_path.boss.timers.push(timer.clone());
            break;
        }
    }

    // Get the item before saving (need to find the boss again after mutation)
    let item = bosses
        .iter()
        .find(|b| b.boss.id == boss_id && b.file_path == file_path_buf)
        .map(|b| TimerListItem::from_boss_timer(b, &timer))
        .ok_or_else(|| "Failed to create timer item".to_string())?;

    // Save the modified file
    let file_bosses: Vec<_> = bosses
        .iter()
        .filter(|b| b.file_path == file_path_buf)
        .map(|b| b.boss.clone())
        .collect();

    save_bosses_to_file(&file_bosses, &file_path_buf)?;

    // Reload definitions into the running session
    let _ = service.reload_timer_definitions().await;

    Ok(item)
}

/// Get list of all bosses (for "New Timer" dropdown)
#[tauri::command]
pub async fn get_encounter_bosses(
    app_handle: AppHandle,
) -> Result<Vec<BossListItem>, String> {
    let bosses = load_user_bosses(&app_handle)?;

    let items: Vec<_> = bosses
        .iter()
        .map(|b| BossListItem {
            id: b.boss.id.clone(),
            name: b.boss.name.clone(),
            area_name: b.boss.area_name.clone(),
            category: b.category.clone(),
            file_path: b.file_path.to_string_lossy().to_string(),
        })
        .collect();

    Ok(items)
}

/// Minimal boss info for the "New Timer" dropdown
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BossListItem {
    pub id: String,
    pub name: String,
    pub area_name: String,
    pub category: String,
    pub file_path: String,
}

/// Area summary for the lazy-loading area index
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

/// Get the area index for lazy loading the timer editor
/// Returns list of areas with summary info (boss count, timer count)
#[tauri::command]
pub async fn get_area_index(app_handle: AppHandle) -> Result<Vec<AreaListItem>, String> {
    eprintln!("[TIMERS] get_area_index called");

    let user_dir = ensure_user_encounters_dir(&app_handle)?;
    eprintln!("[TIMERS] User encounters dir: {:?}", user_dir);

    let mut areas = Vec::new();
    collect_areas_recursive(&user_dir, &mut areas)?;

    eprintln!("[TIMERS] Found {} areas", areas.len());

    // Sort by category then name
    areas.sort_by(|a, b| a.category.cmp(&b.category).then(a.name.cmp(&b.name)));

    Ok(areas)
}

/// Recursively collect area files with summary stats
fn collect_areas_recursive(
    current_dir: &PathBuf,
    areas: &mut Vec<AreaListItem>,
) -> Result<(), String> {
    use baras_core::boss::{load_area_config, load_bosses_from_file};

    let entries = std::fs::read_dir(current_dir)
        .map_err(|e| format!("Failed to read directory: {}", e))?;

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() {
            collect_areas_recursive( &path, areas)?;
        } else if path.extension().is_some_and(|ext| ext == "toml") {
            // Try to load area config for metadata
            match load_area_config(&path) {
                Ok(Some(area_config)) => {
                    // Load bosses to get counts
                    let (boss_count, timer_count) = match load_bosses_from_file(&path) {
                        Ok(bosses) => {
                            let timers: usize = bosses.iter().map(|b| b.timers.len()).sum();
                            (bosses.len(), timers)
                        }
                        Err(e) => {
                            eprintln!("[TIMERS] Failed to load bosses from {:?}: {}", path, e);
                            (0, 0)
                        }
                    };

                    // Use category from TOML if provided, otherwise try to determine from path
                    let category = if !area_config.category.is_empty() {
                        area_config.category
                    } else {
                        determine_category(&path)
                    };

                    areas.push(AreaListItem {
                        name: area_config.name,
                        area_id: area_config.area_id,
                        file_path: path.to_string_lossy().to_string(),
                        category,
                        boss_count,
                        timer_count,
                    });
                }
                Ok(None) => {
                    eprintln!("[TIMERS] No [area] section in {:?}", path);
                }
                Err(e) => {
                    eprintln!("[TIMERS] Failed to parse {:?}: {}", path, e);
                }
            }
        }
    }

    Ok(())
}

/// Determine category from file path or area name
fn determine_category(file_path: &Path) -> String {
    let path_str = file_path.to_string_lossy().to_lowercase();

    if path_str.contains("/operations/") || path_str.contains("\\operations\\") {
        return "operations".to_string();
    }
    if path_str.contains("/flashpoints/") || path_str.contains("\\flashpoints\\") {
        return "flashpoints".to_string();
    }
    if path_str.contains("/lair_bosses/") || path_str.contains("\\lair_bosses\\") {
        return "lair_bosses".to_string();
    }

    // Fallback: determine from filename (known operations/flashpoints)
    let filename = file_path.file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    // Known operations
    const OPERATIONS: &[&str] = &[
        "dxun", "r4", "eternity_vault", "karagga_s_palace", "explosive_conflict",
        "terror_from_beyond", "scum_and_villainy", "dread_fortress", "dread_palace",
        "ravagers", "temple_of_sacrifice", "gods_from_the_machine", "toborro_s_palace",
    ];

    // Known flashpoints
    const FLASHPOINTS: &[&str] = &[
        "athiss", "hammer_station", "mandalorian_raiders", "cademimu", "boarding_party",
        "the_foundry", "maelstrom_prison", "kaon_under_siege", "lost_island",
        "czerka_corporate_labs", "czerka_core_meltdown", "korriban_incursion",
        "assault_on_tython", "depths_of_manaan", "legacy_of_the_rakata", "blood_hunt",
        "battle_of_rishi", "crisis_on_umbara", "a_traitor_among_the_chiss",
        "the_nathema_conspiracy", "objective_meridian", "spirit_of_vengeance",
        "secrets_of_the_enclave", "ruins_of_nul", "the_red_reaper", "directive_7",
        "false_emperor", "the_esseles", "the_black_talon", "propagator_core",
    ];

    if OPERATIONS.iter().any(|op| filename.contains(op)) {
        return "operations".to_string();
    }
    if FLASHPOINTS.iter().any(|fp| filename.contains(fp)) {
        return "flashpoints".to_string();
    }

    "other".to_string()
}

/// Get timers for a specific area file (lazy loading)
#[tauri::command]
pub async fn get_timers_for_area(
    file_path: String,
) -> Result<Vec<TimerListItem>, String> {
    let path = PathBuf::from(&file_path);

    if !path.exists() {
        return Err(format!("File not found: {}", file_path));
    }

    // Load bosses from this specific file
    let bosses = load_bosses_with_paths(path.parent().unwrap_or(&path))
        .map_err(|e| format!("Failed to load bosses: {}", e))?;

    // Filter to only bosses from this file and flatten timers
    let mut items = Vec::new();
    for boss_with_path in &bosses {
        if boss_with_path.file_path == path {
            for timer in &boss_with_path.boss.timers {
                items.push(TimerListItem::from_boss_timer(boss_with_path, timer));
            }
        }
    }

    // Sort by boss name, then timer name
    items.sort_by(|a, b| a.boss_name.cmp(&b.boss_name).then(a.name.cmp(&b.name)));

    Ok(items)
}

/// Get bosses for a specific area file (for "New Timer" dropdown within an area)
#[tauri::command]
pub async fn get_bosses_for_area(file_path: String) -> Result<Vec<BossListItem>, String> {
    let path = PathBuf::from(&file_path);

    if !path.exists() {
        return Err(format!("File not found: {}", file_path));
    }

    let bosses = load_bosses_with_paths(path.parent().unwrap_or(&path))
        .map_err(|e| format!("Failed to load bosses: {}", e))?;

    let items: Vec<_> = bosses
        .iter()
        .filter(|b| b.file_path == path)
        .map(|b| BossListItem {
            id: b.boss.id.clone(),
            name: b.boss.name.clone(),
            area_name: b.boss.area_name.clone(),
            category: b.category.clone(),
            file_path: b.file_path.to_string_lossy().to_string(),
        })
        .collect();

    Ok(items)
}
