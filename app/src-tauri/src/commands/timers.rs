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
    load_bosses_with_paths, save_bosses_to_file, BossTimerDefinition, BossWithPath,
};
use baras_core::effects::EntityFilter;
use baras_core::timers::TimerTrigger;

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
    pub trigger: TimerTrigger,

    // Entity filters (preserved for round-trip, not yet editable in UI)
    pub source: EntityFilter,
    pub target: EntityFilter,

    // Alert fields
    pub is_alert: bool,
    pub alert_text: Option<String>,

    // Optional fields
    pub can_be_refreshed: bool,
    pub repeats: u8,
    pub chains_to: Option<String>,
    pub alert_at_secs: Option<f32>,
    pub show_on_raid_frames: bool,

    // Audio
    pub audio_enabled: bool,
    pub audio_file: Option<String>,
    pub audio_offset: u8,
    pub countdown_start: u8,
    pub countdown_voice: Option<String>,
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
            source: timer.source.clone(),
            target: timer.target.clone(),

            is_alert: timer.is_alert,
            alert_text: timer.alert_text.clone(),

            can_be_refreshed: timer.can_be_refreshed,
            repeats: timer.repeats,
            chains_to: timer.chains_to.clone(),
            alert_at_secs: timer.alert_at_secs,
            show_on_raid_frames: timer.show_on_raid_frames,

            audio_enabled: timer.audio_enabled,
            audio_file: timer.audio_file.clone(),
            audio_offset: timer.audio_offset,
            countdown_start: timer.countdown_start,
            countdown_voice: timer.countdown_voice.clone(),
        }
    }

    /// Convert back to a BossTimerDefinition
    fn to_timer_definition(&self) -> BossTimerDefinition {
        BossTimerDefinition {
            id: self.timer_id.clone(),
            name: self.name.clone(),
            display_text: None,
            trigger: self.trigger.clone(),
            source: self.source.clone(),
            target: self.target.clone(),
            duration_secs: self.duration_secs,
            is_alert: self.is_alert,
            alert_text: self.alert_text.clone(),
            color: self.color,
            phases: self.phases.clone(),
            counter_condition: None, // TODO: Add to UI if needed
            difficulties: self.difficulties.clone(),
            enabled: self.enabled,
            can_be_refreshed: self.can_be_refreshed,
            repeats: self.repeats,
            chains_to: self.chains_to.clone(),
            cancel_trigger: None, // TODO: Add to UI if needed
            alert_at_secs: self.alert_at_secs,
            show_on_raid_frames: self.show_on_raid_frames,
            audio_enabled: self.audio_enabled,
            audio_file: self.audio_file.clone(),
            audio_offset: self.audio_offset,
            countdown_start: self.countdown_start,
            countdown_voice: self.countdown_voice.clone(),
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
        generate_dsl_id(boss_id, &timer.name)
    } else {
        timer.timer_id.clone()
    };

    // Convert to BossTimerDefinition
    let new_timer = BossTimerDefinition {
        id: timer_id.clone(),
        name: timer.name.clone(),
        display_text: None,
        trigger: timer.trigger.clone(),
        source: timer.source.clone(),
        target: timer.target.clone(),
        duration_secs: timer.duration_secs,
        is_alert: timer.is_alert,
        alert_text: timer.alert_text.clone(),
        color: timer.color,
        phases: timer.phases.clone(),
        counter_condition: None,
        difficulties: timer.difficulties.clone(),
        enabled: timer.enabled,
        can_be_refreshed: timer.can_be_refreshed,
        repeats: timer.repeats,
        chains_to: timer.chains_to.clone(),
        cancel_trigger: None,
        alert_at_secs: timer.alert_at_secs,
        show_on_raid_frames: timer.show_on_raid_frames,
        audio_enabled: timer.audio_enabled,
        audio_file: timer.audio_file.clone(),
        audio_offset: timer.audio_offset,
        countdown_start: timer.countdown_start,
        countdown_voice: timer.countdown_voice.clone(),
    };

    // Check for duplicate ID within the target boss only (per-encounter uniqueness)
    for boss_with_path in &bosses {
        if boss_with_path.boss.id == timer.boss_id && boss_with_path.boss.timers.iter().any(|t| t.id == timer_id) {
            return Err(format!(
                "Timer with ID '{}' already exists in this encounter. Timer IDs must be unique within each encounter.",
                timer_id
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

/// Generate a DSL object ID from boss ID and object name (snake_case, safe for TOML)
/// Used for timers, phases, counters, and challenges.
/// Format: {boss_id}_{name_snake_case}
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

    // Canonicalize paths for reliable comparison
    let file_path_buf = PathBuf::from(&file_path);
    let canonical_path = file_path_buf.canonicalize().unwrap_or_else(|_| file_path_buf.clone());

    // Find the boss and remove the timer
    let mut found = false;
    let mut matched_file_path: Option<PathBuf> = None;

    for boss_with_path in &mut bosses {
        let boss_canonical = boss_with_path.file_path.canonicalize()
            .unwrap_or_else(|_| boss_with_path.file_path.clone());

        if boss_with_path.boss.id == boss_id && boss_canonical == canonical_path {
            let original_len = boss_with_path.boss.timers.len();
            boss_with_path.boss.timers.retain(|t| t.id != timer_id);
            found = boss_with_path.boss.timers.len() < original_len;
            matched_file_path = Some(boss_with_path.file_path.clone());
            break;
        }
    }

    if !found {
        return Err(format!(
            "Timer '{}' not found in boss '{}'",
            timer_id, boss_id
        ));
    }

    // Use the actual file path from the matched boss (ensures consistency)
    let save_path = matched_file_path.unwrap_or(file_path_buf);

    // Save the modified file
    let file_bosses: Vec<_> = bosses
        .iter()
        .filter(|b| {
            let b_canonical = b.file_path.canonicalize().unwrap_or_else(|_| b.file_path.clone());
            b_canonical == canonical_path
        })
        .map(|b| b.boss.clone())
        .collect();

    save_bosses_to_file(&file_bosses, &save_path)?;

    // Reload definitions into the running session (propagate errors)
    service.reload_timer_definitions().await
        .map_err(|e| format!("Failed to reload after delete: {}", e))?;

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

                    // Determine category from file path
                    let category = determine_category(&path);

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

// ─────────────────────────────────────────────────────────────────────────────
// Boss & Area Creation Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Request to create a new boss
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BossEditItem {
    pub id: String,
    pub name: String,
    pub area_name: String,
    pub area_id: i64,
    pub file_path: String,
    pub difficulties: Vec<String>,
}

/// Request to create a new area file
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewAreaRequest {
    pub name: String,
    pub area_id: i64,
    pub area_type: String,
}

/// Create a new boss in an existing area file
#[tauri::command]
pub async fn create_boss(
    app_handle: AppHandle,
    service: State<'_, ServiceHandle>,
    boss: BossEditItem,
) -> Result<BossEditItem, String> {
    use baras_core::boss::{load_bosses_from_file, BossEncounterDefinition};

    let file_path = PathBuf::from(&boss.file_path);

    if !file_path.exists() {
        return Err(format!("Area file not found: {}", boss.file_path));
    }

    // Load existing bosses from the file
    let mut bosses = load_bosses_from_file(&file_path)
        .map_err(|e| format!("Failed to load bosses: {}", e))?;

    // Check for duplicate boss ID
    if bosses.iter().any(|b| b.id == boss.id) {
        return Err(format!("Boss with ID '{}' already exists in this area", boss.id));
    }

    // Create new boss definition
    #[allow(deprecated)]
    let new_boss = BossEncounterDefinition {
        id: boss.id.clone(),
        name: boss.name.clone(),
        area_name: boss.area_name.clone(),
        area_id: boss.area_id,
        difficulties: boss.difficulties.clone(),
        timers: vec![],
        phases: vec![],
        counters: vec![],
        challenges: vec![],
        entities: vec![],
    };

    bosses.push(new_boss);

    // Save back to file
    save_bosses_to_file(&bosses, &file_path)?;

    // Reload definitions
    let _ = service.reload_timer_definitions().await;

    Ok(boss)
}

/// Create a new area file
#[tauri::command]
pub async fn create_area(
    app_handle: AppHandle,
    area: NewAreaRequest,
) -> Result<String, String> {
    let user_dir = ensure_user_encounters_dir(&app_handle)?;

    // Determine subdirectory based on area type
    let subdir = match area.area_type.as_str() {
        "operation" => "operations",
        "flashpoint" => "flashpoints",
        "lair_boss" => "lair_bosses",
        _ => "other",
    };

    let target_dir = user_dir.join(subdir);
    std::fs::create_dir_all(&target_dir)
        .map_err(|e| format!("Failed to create directory: {}", e))?;

    // Generate filename from area name (snake_case)
    let filename: String = area.name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .split('_')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_");

    let file_path = target_dir.join(format!("{}.toml", filename));

    if file_path.exists() {
        return Err(format!("Area file already exists: {:?}", file_path));
    }

    // Create minimal TOML content with area config
    let content = format!(
        r#"# {}
# Area type: {}

[area]
name = "{}"
area_id = {}

# Add bosses below using [[boss]] sections
"#,
        area.name, area.area_type, area.name, area.area_id
    );

    std::fs::write(&file_path, content)
        .map_err(|e| format!("Failed to write area file: {}", e))?;

    Ok(file_path.to_string_lossy().to_string())
}

// ─────────────────────────────────────────────────────────────────────────────
// Audio File Picker
// ─────────────────────────────────────────────────────────────────────────────

/// Open a file picker dialog to select an audio file
#[tauri::command]
pub async fn pick_audio_file(app_handle: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let file_path = app_handle
        .dialog()
        .file()
        .add_filter("Audio Files", &["mp3", "wav", "ogg", "flac", "m4a"])
        .blocking_pick_file();

    match file_path {
        Some(path) => Ok(Some(path.to_string())),
        None => Ok(None), // User cancelled
    }
}

/// List bundled alert sounds (excludes voice pack directories)
#[tauri::command]
pub async fn list_bundled_sounds(app_handle: AppHandle) -> Result<Vec<String>, String> {
    // In release: bundled resources. In dev: fall back to source directory
    let sounds_dir = app_handle
        .path()
        .resolve("definitions/sounds", tauri::path::BaseDirectory::Resource)
        .ok()
        .filter(|p| p.exists())
        .unwrap_or_else(|| {
            // Dev fallback: relative to project root
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("core/definitions/sounds")
        });

    let mut sounds = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&sounds_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            // Only include files (not directories like voice packs)
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    let ext = ext.to_string_lossy().to_lowercase();
                    if ext == "mp3" || ext == "wav" {
                        if let Some(name) = path.file_name() {
                            sounds.push(name.to_string_lossy().to_string());
                        }
                    }
                }
            }
        }
    }

    sounds.sort();
    Ok(sounds)
}

// ─────────────────────────────────────────────────────────────────────────────
// Phase CRUD
// ─────────────────────────────────────────────────────────────────────────────

use baras_core::boss::{CounterCondition, PhaseDefinition, PhaseTrigger};

/// Flattened phase item for the frontend list view
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhaseListItem {
    pub id: String,
    pub name: String,
    pub boss_id: String,
    pub boss_name: String,
    pub file_path: String,
    pub start_trigger: PhaseTrigger,
    pub end_trigger: Option<PhaseTrigger>,
    pub preceded_by: Option<String>,
    pub counter_condition: Option<CounterCondition>,
    pub resets_counters: Vec<String>,
}

impl PhaseListItem {
    fn from_boss_phase(boss_with_path: &BossWithPath, phase: &PhaseDefinition) -> Self {
        Self {
            id: phase.id.clone(),
            name: phase.name.clone(),
            boss_id: boss_with_path.boss.id.clone(),
            boss_name: boss_with_path.boss.name.clone(),
            file_path: boss_with_path.file_path.to_string_lossy().to_string(),
            start_trigger: phase.start_trigger.clone(),
            end_trigger: phase.end_trigger.clone(),
            preceded_by: phase.preceded_by.clone(),
            counter_condition: phase.counter_condition.clone(),
            resets_counters: phase.resets_counters.clone(),
        }
    }

    fn to_phase_definition(&self) -> PhaseDefinition {
        PhaseDefinition {
            id: self.id.clone(),
            name: self.name.clone(),
            display_text: None,
            start_trigger: self.start_trigger.clone(),
            end_trigger: self.end_trigger.clone(),
            preceded_by: self.preceded_by.clone(),
            counter_condition: self.counter_condition.clone(),
            resets_counters: self.resets_counters.clone(),
        }
    }
}

/// Get phases for a specific area file
#[tauri::command]
pub async fn get_phases_for_area(file_path: String) -> Result<Vec<PhaseListItem>, String> {
    let path = PathBuf::from(&file_path);

    if !path.exists() {
        return Err(format!("File not found: {}", file_path));
    }

    let bosses = load_bosses_with_paths(path.parent().unwrap_or(&path))
        .map_err(|e| format!("Failed to load bosses: {}", e))?;

    let mut items = Vec::new();
    for boss_with_path in &bosses {
        if boss_with_path.file_path == path {
            for phase in &boss_with_path.boss.phases {
                items.push(PhaseListItem::from_boss_phase(boss_with_path, phase));
            }
        }
    }

    items.sort_by(|a, b| a.boss_name.cmp(&b.boss_name).then(a.name.cmp(&b.name)));

    Ok(items)
}

/// Update an existing phase
#[tauri::command]
pub async fn update_phase(
    app_handle: AppHandle,
    service: State<'_, ServiceHandle>,
    phase: PhaseListItem,
) -> Result<PhaseListItem, String> {
    let mut bosses = load_user_bosses(&app_handle)?;
    let file_path_buf = PathBuf::from(&phase.file_path);

    let mut updated_item = None;

    for boss_with_path in &mut bosses {
        if boss_with_path.boss.id == phase.boss_id && boss_with_path.file_path == file_path_buf {
            if let Some(existing) = boss_with_path.boss.phases.iter_mut().find(|p| p.id == phase.id)
            {
                *existing = phase.to_phase_definition();
                updated_item = Some(phase.clone());
            }
            break;
        }
    }

    let item = updated_item.ok_or_else(|| format!("Phase '{}' not found", phase.id))?;

    let file_bosses: Vec<_> = bosses
        .iter()
        .filter(|b| b.file_path == file_path_buf)
        .map(|b| b.boss.clone())
        .collect();

    save_bosses_to_file(&file_bosses, &file_path_buf)?;
    let _ = service.reload_timer_definitions().await;

    Ok(item)
}

/// Create a new phase
#[tauri::command]
pub async fn create_phase(
    app_handle: AppHandle,
    service: State<'_, ServiceHandle>,
    phase: PhaseListItem,
) -> Result<PhaseListItem, String> {
    let mut bosses = load_user_bosses(&app_handle)?;
    let file_path_buf = PathBuf::from(&phase.file_path);
    let boss_id = phase.boss_id.clone();

    // Generate phase ID
    let phase_id = generate_dsl_id(&phase.boss_id, &phase.name);
    let mut new_phase = phase.to_phase_definition();
    new_phase.id = phase_id.clone();

    let mut created_item = None;

    for boss_with_path in &mut bosses {
        if boss_with_path.boss.id == boss_id && boss_with_path.file_path == file_path_buf {
            boss_with_path.boss.phases.push(new_phase.clone());
            created_item = Some(PhaseListItem::from_boss_phase(boss_with_path, &new_phase));
            break;
        }
    }

    let item = created_item.ok_or_else(|| format!("Boss '{}' not found", boss_id))?;

    let file_bosses: Vec<_> = bosses
        .iter()
        .filter(|b| b.file_path == file_path_buf)
        .map(|b| b.boss.clone())
        .collect();

    save_bosses_to_file(&file_bosses, &file_path_buf)?;
    let _ = service.reload_timer_definitions().await;

    Ok(item)
}

/// Delete a phase
#[tauri::command]
pub async fn delete_phase(
    app_handle: AppHandle,
    service: State<'_, ServiceHandle>,
    phase_id: String,
    boss_id: String,
    file_path: String,
) -> Result<(), String> {
    let mut bosses = load_user_bosses(&app_handle)?;
    let file_path_buf = PathBuf::from(&file_path);
    let canonical_path = file_path_buf.canonicalize().unwrap_or_else(|_| file_path_buf.clone());

    let mut found = false;
    let mut matched_file_path: Option<PathBuf> = None;

    for boss_with_path in &mut bosses {
        let boss_canonical = boss_with_path.file_path.canonicalize()
            .unwrap_or_else(|_| boss_with_path.file_path.clone());

        if boss_with_path.boss.id == boss_id && boss_canonical == canonical_path {
            let original_len = boss_with_path.boss.phases.len();
            boss_with_path.boss.phases.retain(|p| p.id != phase_id);
            found = boss_with_path.boss.phases.len() < original_len;
            matched_file_path = Some(boss_with_path.file_path.clone());
            break;
        }
    }

    if !found {
        return Err(format!("Phase '{}' not found in boss '{}'", phase_id, boss_id));
    }

    let save_path = matched_file_path.unwrap_or(file_path_buf);
    let file_bosses: Vec<_> = bosses
        .iter()
        .filter(|b| {
            let b_canonical = b.file_path.canonicalize().unwrap_or_else(|_| b.file_path.clone());
            b_canonical == canonical_path
        })
        .map(|b| b.boss.clone())
        .collect();

    save_bosses_to_file(&file_bosses, &save_path)?;
    service.reload_timer_definitions().await
        .map_err(|e| format!("Failed to reload after delete: {}", e))?;

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Counter CRUD
// ─────────────────────────────────────────────────────────────────────────────

use baras_core::boss::{
    ChallengeCondition, ChallengeDefinition, ChallengeMetric, CounterDefinition, CounterTrigger,
    EntityDefinition,
};

/// Flattened counter item for the frontend list view
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CounterListItem {
    pub id: String,
    /// Display name (used for ID generation if id is empty)
    pub name: String,
    /// Optional in-game display text (defaults to name if not set)
    #[serde(default)]
    pub display_text: Option<String>,
    pub boss_id: String,
    pub boss_name: String,
    pub file_path: String,
    pub increment_on: CounterTrigger,
    pub reset_on: CounterTrigger,
    #[serde(default)]
    pub initial_value: u32,
    #[serde(default)]
    pub decrement: bool,
    #[serde(default)]
    pub set_value: Option<u32>,
}

impl CounterListItem {
    fn from_boss_counter(boss_with_path: &BossWithPath, counter: &CounterDefinition) -> Self {
        Self {
            id: counter.id.clone(),
            name: counter.name.clone(),
            display_text: counter.display_text.clone(),
            boss_id: boss_with_path.boss.id.clone(),
            boss_name: boss_with_path.boss.name.clone(),
            file_path: boss_with_path.file_path.to_string_lossy().to_string(),
            increment_on: counter.increment_on.clone(),
            reset_on: counter.reset_on.clone(),
            initial_value: counter.initial_value,
            decrement: counter.decrement,
            set_value: counter.set_value,
        }
    }

    fn to_counter_definition(&self) -> CounterDefinition {
        CounterDefinition {
            id: self.id.clone(),
            name: self.name.clone(),
            display_text: self.display_text.clone(),
            increment_on: self.increment_on.clone(),
            reset_on: self.reset_on.clone(),
            initial_value: self.initial_value,
            decrement: self.decrement,
            set_value: self.set_value,
        }
    }
}

/// Get counters for a specific area file
#[tauri::command]
pub async fn get_counters_for_area(file_path: String) -> Result<Vec<CounterListItem>, String> {
    let path = PathBuf::from(&file_path);

    if !path.exists() {
        return Err(format!("File not found: {}", file_path));
    }

    let bosses = load_bosses_with_paths(path.parent().unwrap_or(&path))
        .map_err(|e| format!("Failed to load bosses: {}", e))?;

    let mut items = Vec::new();
    for boss_with_path in &bosses {
        if boss_with_path.file_path == path {
            for counter in &boss_with_path.boss.counters {
                items.push(CounterListItem::from_boss_counter(boss_with_path, counter));
            }
        }
    }

    items.sort_by(|a, b| a.boss_name.cmp(&b.boss_name).then(a.id.cmp(&b.id)));

    Ok(items)
}

/// Update an existing counter
#[tauri::command]
pub async fn update_counter(
    app_handle: AppHandle,
    service: State<'_, ServiceHandle>,
    counter: CounterListItem,
) -> Result<CounterListItem, String> {
    let mut bosses = load_user_bosses(&app_handle)?;
    let file_path_buf = PathBuf::from(&counter.file_path);

    let mut updated_item = None;

    for boss_with_path in &mut bosses {
        if boss_with_path.boss.id == counter.boss_id && boss_with_path.file_path == file_path_buf {
            if let Some(existing) = boss_with_path
                .boss
                .counters
                .iter_mut()
                .find(|c| c.id == counter.id)
            {
                *existing = counter.to_counter_definition();
                updated_item = Some(counter.clone());
            }
            break;
        }
    }

    let item = updated_item.ok_or_else(|| format!("Counter '{}' not found", counter.id))?;

    let file_bosses: Vec<_> = bosses
        .iter()
        .filter(|b| b.file_path == file_path_buf)
        .map(|b| b.boss.clone())
        .collect();

    save_bosses_to_file(&file_bosses, &file_path_buf)?;
    let _ = service.reload_timer_definitions().await;

    Ok(item)
}

/// Create a new counter
#[tauri::command]
pub async fn create_counter(
    app_handle: AppHandle,
    service: State<'_, ServiceHandle>,
    counter: CounterListItem,
) -> Result<CounterListItem, String> {
    let mut bosses = load_user_bosses(&app_handle)?;
    let file_path_buf = PathBuf::from(&counter.file_path);
    let boss_id = counter.boss_id.clone();

    // Generate ID from name if id is empty
    let counter_id = if counter.id.is_empty() {
        generate_dsl_id(&boss_id, &counter.name)
    } else {
        counter.id.clone()
    };

    let mut new_counter = counter.to_counter_definition();
    new_counter.id = counter_id;
    let mut created_item = None;

    for boss_with_path in &mut bosses {
        if boss_with_path.boss.id == boss_id && boss_with_path.file_path == file_path_buf {
            boss_with_path.boss.counters.push(new_counter.clone());
            created_item = Some(CounterListItem::from_boss_counter(boss_with_path, &new_counter));
            break;
        }
    }

    let item = created_item.ok_or_else(|| format!("Boss '{}' not found", boss_id))?;

    let file_bosses: Vec<_> = bosses
        .iter()
        .filter(|b| b.file_path == file_path_buf)
        .map(|b| b.boss.clone())
        .collect();

    save_bosses_to_file(&file_bosses, &file_path_buf)?;
    let _ = service.reload_timer_definitions().await;

    Ok(item)
}

/// Delete a counter
#[tauri::command]
pub async fn delete_counter(
    app_handle: AppHandle,
    service: State<'_, ServiceHandle>,
    counter_id: String,
    boss_id: String,
    file_path: String,
) -> Result<(), String> {
    let mut bosses = load_user_bosses(&app_handle)?;
    let file_path_buf = PathBuf::from(&file_path);
    let canonical_path = file_path_buf.canonicalize().unwrap_or_else(|_| file_path_buf.clone());

    let mut found = false;
    let mut matched_file_path: Option<PathBuf> = None;

    for boss_with_path in &mut bosses {
        let boss_canonical = boss_with_path.file_path.canonicalize()
            .unwrap_or_else(|_| boss_with_path.file_path.clone());

        if boss_with_path.boss.id == boss_id && boss_canonical == canonical_path {
            let original_len = boss_with_path.boss.counters.len();
            boss_with_path.boss.counters.retain(|c| c.id != counter_id);
            found = boss_with_path.boss.counters.len() < original_len;
            matched_file_path = Some(boss_with_path.file_path.clone());
            break;
        }
    }

    if !found {
        return Err(format!(
            "Counter '{}' not found in boss '{}'",
            counter_id, boss_id
        ));
    }

    let save_path = matched_file_path.unwrap_or(file_path_buf);
    let file_bosses: Vec<_> = bosses
        .iter()
        .filter(|b| {
            let b_canonical = b.file_path.canonicalize().unwrap_or_else(|_| b.file_path.clone());
            b_canonical == canonical_path
        })
        .map(|b| b.boss.clone())
        .collect();

    save_bosses_to_file(&file_bosses, &save_path)?;
    service.reload_timer_definitions().await
        .map_err(|e| format!("Failed to reload after delete: {}", e))?;

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Challenge CRUD
// ─────────────────────────────────────────────────────────────────────────────


/// Flattened challenge item for the frontend list view
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChallengeListItem {
    pub id: String,
    pub name: String,
    /// Optional in-game display text (defaults to name if not set)
    #[serde(default)]
    pub display_text: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub boss_id: String,
    pub boss_name: String,
    pub file_path: String,
    pub metric: ChallengeMetric,
    #[serde(default)]
    pub conditions: Vec<ChallengeCondition>,
}

impl ChallengeListItem {
    fn from_boss_challenge(boss_with_path: &BossWithPath, challenge: &ChallengeDefinition) -> Self {
        Self {
            id: challenge.id.clone(),
            name: challenge.name.clone(),
            display_text: challenge.display_text.clone(),
            description: challenge.description.clone(),
            boss_id: boss_with_path.boss.id.clone(),
            boss_name: boss_with_path.boss.name.clone(),
            file_path: boss_with_path.file_path.to_string_lossy().to_string(),
            metric: challenge.metric,
            conditions: challenge.conditions.clone(),
        }
    }

    fn to_challenge_definition(&self) -> ChallengeDefinition {
        ChallengeDefinition {
            id: self.id.clone(),
            name: self.name.clone(),
            display_text: self.display_text.clone(),
            description: self.description.clone(),
            metric: self.metric,
            conditions: self.conditions.clone(),
        }
    }
}

/// Get challenges for a specific area file
#[tauri::command]
pub async fn get_challenges_for_area(file_path: String) -> Result<Vec<ChallengeListItem>, String> {
    let path = PathBuf::from(&file_path);

    if !path.exists() {
        return Err(format!("File not found: {}", file_path));
    }

    let bosses = load_bosses_with_paths(path.parent().unwrap_or(&path))
        .map_err(|e| format!("Failed to load bosses: {}", e))?;

    let mut items = Vec::new();
    for boss_with_path in &bosses {
        if boss_with_path.file_path == path {
            for challenge in &boss_with_path.boss.challenges {
                items.push(ChallengeListItem::from_boss_challenge(boss_with_path, challenge));
            }
        }
    }

    items.sort_by(|a, b| a.boss_name.cmp(&b.boss_name).then(a.id.cmp(&b.id)));

    Ok(items)
}

/// Update an existing challenge
#[tauri::command]
pub async fn update_challenge(
    app_handle: AppHandle,
    service: State<'_, ServiceHandle>,
    challenge: ChallengeListItem,
) -> Result<ChallengeListItem, String> {
    let mut bosses = load_user_bosses(&app_handle)?;
    let file_path_buf = PathBuf::from(&challenge.file_path);

    let mut updated_item = None;

    for boss_with_path in &mut bosses {
        if boss_with_path.boss.id == challenge.boss_id && boss_with_path.file_path == file_path_buf {
            if let Some(existing) = boss_with_path
                .boss
                .challenges
                .iter_mut()
                .find(|c| c.id == challenge.id)
            {
                *existing = challenge.to_challenge_definition();
                updated_item = Some(challenge.clone());
            }
            break;
        }
    }

    let item = updated_item.ok_or_else(|| format!("Challenge '{}' not found", challenge.id))?;

    let file_bosses: Vec<_> = bosses
        .iter()
        .filter(|b| b.file_path == file_path_buf)
        .map(|b| b.boss.clone())
        .collect();

    save_bosses_to_file(&file_bosses, &file_path_buf)?;
    let _ = service.reload_timer_definitions().await;

    Ok(item)
}

/// Create a new challenge
#[tauri::command]
pub async fn create_challenge(
    app_handle: AppHandle,
    service: State<'_, ServiceHandle>,
    challenge: ChallengeListItem,
) -> Result<ChallengeListItem, String> {
    let mut bosses = load_user_bosses(&app_handle)?;
    let file_path_buf = PathBuf::from(&challenge.file_path);
    let boss_id = challenge.boss_id.clone();

    // Generate ID from name if id is empty
    let challenge_id = if challenge.id.is_empty() {
        generate_dsl_id(&boss_id, &challenge.name)
    } else {
        challenge.id.clone()
    };

    let mut new_challenge = challenge.to_challenge_definition();
    new_challenge.id = challenge_id;
    let mut created_item = None;

    for boss_with_path in &mut bosses {
        if boss_with_path.boss.id == boss_id && boss_with_path.file_path == file_path_buf {
            boss_with_path.boss.challenges.push(new_challenge.clone());
            created_item = Some(ChallengeListItem::from_boss_challenge(boss_with_path, &new_challenge));
            break;
        }
    }

    let item = created_item.ok_or_else(|| format!("Boss '{}' not found", boss_id))?;

    let file_bosses: Vec<_> = bosses
        .iter()
        .filter(|b| b.file_path == file_path_buf)
        .map(|b| b.boss.clone())
        .collect();

    save_bosses_to_file(&file_bosses, &file_path_buf)?;
    let _ = service.reload_timer_definitions().await;

    Ok(item)
}

/// Delete a challenge
#[tauri::command]
pub async fn delete_challenge(
    app_handle: AppHandle,
    service: State<'_, ServiceHandle>,
    challenge_id: String,
    boss_id: String,
    file_path: String,
) -> Result<(), String> {
    let mut bosses = load_user_bosses(&app_handle)?;
    let file_path_buf = PathBuf::from(&file_path);
    let canonical_path = file_path_buf.canonicalize().unwrap_or_else(|_| file_path_buf.clone());

    let mut found = false;
    let mut matched_file_path: Option<PathBuf> = None;

    for boss_with_path in &mut bosses {
        let boss_canonical = boss_with_path.file_path.canonicalize()
            .unwrap_or_else(|_| boss_with_path.file_path.clone());

        if boss_with_path.boss.id == boss_id && boss_canonical == canonical_path {
            let original_len = boss_with_path.boss.challenges.len();
            boss_with_path.boss.challenges.retain(|c| c.id != challenge_id);
            found = boss_with_path.boss.challenges.len() < original_len;
            matched_file_path = Some(boss_with_path.file_path.clone());
            break;
        }
    }

    if !found {
        return Err(format!(
            "Challenge '{}' not found in boss '{}'",
            challenge_id, boss_id
        ));
    }

    let save_path = matched_file_path.unwrap_or(file_path_buf);
    let file_bosses: Vec<_> = bosses
        .iter()
        .filter(|b| {
            let b_canonical = b.file_path.canonicalize().unwrap_or_else(|_| b.file_path.clone());
            b_canonical == canonical_path
        })
        .map(|b| b.boss.clone())
        .collect();

    save_bosses_to_file(&file_bosses, &save_path)?;
    service.reload_timer_definitions().await
        .map_err(|e| format!("Failed to reload after delete: {}", e))?;

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Entity CRUD
// ─────────────────────────────────────────────────────────────────────────────

/// Flattened entity item for the frontend list view
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityListItem {
    pub name: String,
    pub boss_id: String,
    pub boss_name: String,
    pub file_path: String,
    #[serde(default)]
    pub ids: Vec<i64>,
    #[serde(default)]
    pub is_boss: bool,
    #[serde(default)]
    pub triggers_encounter: bool,
    #[serde(default)]
    pub is_kill_target: bool,
    #[serde(default)]
    pub show_on_hp_overlay: bool,
}

impl EntityListItem {
    fn from_boss_entity(boss_with_path: &BossWithPath, entity: &EntityDefinition) -> Self {
        Self {
            name: entity.name.clone(),
            boss_id: boss_with_path.boss.id.clone(),
            boss_name: boss_with_path.boss.name.clone(),
            file_path: boss_with_path.file_path.to_string_lossy().to_string(),
            ids: entity.ids.clone(),
            is_boss: entity.is_boss,
            triggers_encounter: entity.triggers_encounter(),
            is_kill_target: entity.is_kill_target,
            show_on_hp_overlay: entity.shows_on_hp_overlay(),
        }
    }

    fn to_entity_definition(&self) -> EntityDefinition {
        EntityDefinition {
            name: self.name.clone(),
            ids: self.ids.clone(),
            is_boss: self.is_boss,
            triggers_encounter: Some(self.triggers_encounter),
            is_kill_target: self.is_kill_target,
            show_on_hp_overlay: Some(self.show_on_hp_overlay),
        }
    }
}

/// Get entities for a specific area file
#[tauri::command]
pub async fn get_entities_for_area(file_path: String) -> Result<Vec<EntityListItem>, String> {
    let path = PathBuf::from(&file_path);

    if !path.exists() {
        return Err(format!("File not found: {}", file_path));
    }

    let bosses = load_bosses_with_paths(path.parent().unwrap_or(&path))
        .map_err(|e| format!("Failed to load bosses: {}", e))?;

    let mut items = Vec::new();
    for boss_with_path in &bosses {
        if boss_with_path.file_path == path {
            for entity in &boss_with_path.boss.entities {
                items.push(EntityListItem::from_boss_entity(boss_with_path, entity));
            }
        }
    }

    items.sort_by(|a, b| a.boss_name.cmp(&b.boss_name).then(a.name.cmp(&b.name)));

    Ok(items)
}

/// Update an existing entity
#[tauri::command]
pub async fn update_entity(
    app_handle: AppHandle,
    service: State<'_, ServiceHandle>,
    entity: EntityListItem,
    original_name: String,
) -> Result<EntityListItem, String> {
    let mut bosses = load_user_bosses(&app_handle)?;
    let file_path_buf = PathBuf::from(&entity.file_path);

    let mut updated_item = None;

    for boss_with_path in &mut bosses {
        if boss_with_path.boss.id == entity.boss_id && boss_with_path.file_path == file_path_buf {
            if let Some(existing) = boss_with_path
                .boss
                .entities
                .iter_mut()
                .find(|e| e.name == original_name)
            {
                *existing = entity.to_entity_definition();
                updated_item = Some(entity.clone());
            }
            break;
        }
    }

    let item = updated_item.ok_or_else(|| format!("Entity '{}' not found", original_name))?;

    let file_bosses: Vec<_> = bosses
        .iter()
        .filter(|b| b.file_path == file_path_buf)
        .map(|b| b.boss.clone())
        .collect();

    save_bosses_to_file(&file_bosses, &file_path_buf)?;
    let _ = service.reload_timer_definitions().await;

    Ok(item)
}

/// Create a new entity
#[tauri::command]
pub async fn create_entity(
    app_handle: AppHandle,
    service: State<'_, ServiceHandle>,
    entity: EntityListItem,
) -> Result<EntityListItem, String> {
    let mut bosses = load_user_bosses(&app_handle)?;
    let file_path_buf = PathBuf::from(&entity.file_path);
    let boss_id = entity.boss_id.clone();

    let new_entity = entity.to_entity_definition();
    let mut created_item = None;

    for boss_with_path in &mut bosses {
        if boss_with_path.boss.id == boss_id && boss_with_path.file_path == file_path_buf {
            // Check for duplicate name
            if boss_with_path.boss.entities.iter().any(|e| e.name == entity.name) {
                return Err(format!("Entity '{}' already exists in this boss", entity.name));
            }
            boss_with_path.boss.entities.push(new_entity.clone());
            created_item = Some(EntityListItem::from_boss_entity(boss_with_path, &new_entity));
            break;
        }
    }

    let item = created_item.ok_or_else(|| format!("Boss '{}' not found", boss_id))?;

    let file_bosses: Vec<_> = bosses
        .iter()
        .filter(|b| b.file_path == file_path_buf)
        .map(|b| b.boss.clone())
        .collect();

    save_bosses_to_file(&file_bosses, &file_path_buf)?;
    let _ = service.reload_timer_definitions().await;

    Ok(item)
}

/// Delete an entity
#[tauri::command]
pub async fn delete_entity(
    app_handle: AppHandle,
    service: State<'_, ServiceHandle>,
    entity_name: String,
    boss_id: String,
    file_path: String,
) -> Result<(), String> {
    let mut bosses = load_user_bosses(&app_handle)?;
    let file_path_buf = PathBuf::from(&file_path);

    let mut found = false;
    for boss_with_path in &mut bosses {
        if boss_with_path.boss.id == boss_id && boss_with_path.file_path == file_path_buf {
            let original_len = boss_with_path.boss.entities.len();
            boss_with_path.boss.entities.retain(|e| e.name != entity_name);
            found = boss_with_path.boss.entities.len() < original_len;
            break;
        }
    }

    if !found {
        return Err(format!(
            "Entity '{}' not found in boss '{}'",
            entity_name, boss_id
        ));
    }

    let file_bosses: Vec<_> = bosses
        .iter()
        .filter(|b| b.file_path == file_path_buf)
        .map(|b| b.boss.clone())
        .collect();

    save_bosses_to_file(&file_bosses, &file_path_buf)?;
    let _ = service.reload_timer_definitions().await;

    Ok(())
}
