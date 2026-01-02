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

use baras_core::dsl::AudioConfig;
use baras_core::boss::{
    BossTimerDefinition, BossWithPath, load_bosses_with_paths, save_bosses_to_file,
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
    pub display_text: Option<String>,
    pub enabled: bool,
    pub duration_secs: f32,
    pub color: [u8; 4],
    pub phases: Vec<String>,
    pub difficulties: Vec<String>,

    // Trigger info
    pub trigger: TimerTrigger,

    // Entity filters (from trigger)
    pub source: EntityFilter,
    pub target: EntityFilter,

    // Counter guard condition
    pub counter_condition: Option<CounterCondition>,

    // Alert fields
    pub is_alert: bool,
    pub alert_text: Option<String>,

    // Cancel trigger
    pub cancel_trigger: Option<TimerTrigger>,

    // Behavior
    pub can_be_refreshed: bool,
    pub repeats: u8,
    pub chains_to: Option<String>,
    pub alert_at_secs: Option<f32>,
    pub show_on_raid_frames: bool,
    pub show_at_secs: f32,

    // Audio
    pub audio: AudioConfig,
}

impl TimerListItem {
    /// Convert a BossWithPath + timer to a flattened list item, merging preferences
    fn from_boss_timer(
        boss_with_path: &BossWithPath,
        timer: &BossTimerDefinition,
        prefs: &TimerPreferences,
    ) -> Self {
        // Extract source/target from trigger
        let (source, target) = timer.trigger.source_target_filters();

        // Generate preference key
        let pref_key = boss_timer_key(
            &boss_with_path.boss.area_name,
            &boss_with_path.boss.name,
            &timer.id,
        );
        let pref = prefs.get(&pref_key);

        // Merge preferences over definition defaults
        let enabled = pref.and_then(|p| p.enabled).unwrap_or(timer.enabled);
        let color = pref.and_then(|p| p.color).unwrap_or(timer.color);
        let audio = AudioConfig {
            enabled: pref
                .and_then(|p| p.audio_enabled)
                .unwrap_or(timer.audio.enabled),
            file: pref
                .and_then(|p| p.audio_file.clone())
                .or_else(|| timer.audio.file.clone()),
            offset: timer.audio.offset,
            countdown_start: timer.audio.countdown_start,
            countdown_voice: timer.audio.countdown_voice.clone(),
        };

        Self {
            timer_id: timer.id.clone(),
            boss_id: boss_with_path.boss.id.clone(),
            boss_name: boss_with_path.boss.name.clone(),
            area_name: boss_with_path.boss.area_name.clone(),
            category: boss_with_path.category.clone(),
            file_path: boss_with_path.file_path.to_string_lossy().to_string(),

            name: timer.name.clone(),
            display_text: timer.display_text.clone(),
            enabled,
            duration_secs: timer.duration_secs,
            color,
            phases: timer.phases.clone(),
            difficulties: timer.difficulties.clone(),

            trigger: timer.trigger.clone(),
            source,
            target,

            counter_condition: timer.counter_condition.clone(),

            is_alert: timer.is_alert,
            alert_text: timer.alert_text.clone(),

            cancel_trigger: timer.cancel_trigger.clone(),

            can_be_refreshed: timer.can_be_refreshed,
            repeats: timer.repeats,
            chains_to: timer.chains_to.clone(),
            alert_at_secs: timer.alert_at_secs,
            show_on_raid_frames: timer.show_on_raid_frames,
            show_at_secs: timer.show_at_secs,

            audio,
        }
    }

    /// Convert back to a BossTimerDefinition (excludes preference fields)
    /// Note: enabled, color, and audio are NOT included in definition output
    /// because they should be saved to preferences, not the definition file.
    fn to_timer_definition(&self) -> BossTimerDefinition {
        // Rebuild trigger with source/target filters
        let trigger = self
            .trigger
            .clone()
            .with_source_target(self.source.clone(), self.target.clone());

        BossTimerDefinition {
            id: self.timer_id.clone(),
            name: self.name.clone(),
            display_text: self.display_text.clone(),
            trigger,
            duration_secs: self.duration_secs,
            is_alert: self.is_alert,
            alert_text: self.alert_text.clone(),
            color: self.color,
            phases: self.phases.clone(),
            counter_condition: self.counter_condition.clone(),
            difficulties: self.difficulties.clone(),
            enabled: self.enabled,
            can_be_refreshed: self.can_be_refreshed,
            repeats: self.repeats,
            chains_to: self.chains_to.clone(),
            cancel_trigger: self.cancel_trigger.clone(),
            alert_at_secs: self.alert_at_secs,
            show_on_raid_frames: self.show_on_raid_frames,
            show_at_secs: self.show_at_secs,
            audio: self.audio.clone(),
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
        .resolve(
            "definitions/encounters",
            tauri::path::BaseDirectory::Resource,
        )
        .ok()
}

/// Ensure user encounters directory exists (for custom overlay files)
/// Does NOT copy bundled defaults - those are loaded directly and never copied
fn ensure_user_encounters_dir(_app_handle: &AppHandle) -> Result<PathBuf, String> {
    let user_dir = get_user_encounters_dir()
        .ok_or_else(|| "Could not determine user config directory".to_string())?;

    // Just create the directory if it doesn't exist
    if !user_dir.exists() {
        std::fs::create_dir_all(&user_dir)
            .map_err(|e| format!("Failed to create user encounters dir: {}", e))?;
        eprintln!("[TIMERS] Created user encounters dir: {:?}", user_dir);
    }

    Ok(user_dir)
}

/// Load all bosses: bundled defaults merged with user custom overlays
/// - Bundled files are read-only defaults
/// - Custom files (*_custom.toml) in user dir contain user modifications
/// - Files in user dir with same name as bundled are user overrides
fn load_merged_bosses(app_handle: &AppHandle) -> Result<Vec<BossWithPath>, String> {
    let bundled_dir = get_bundled_encounters_dir(app_handle)
        .ok_or_else(|| "Could not find bundled encounter definitions".to_string())?;

    let user_dir = ensure_user_encounters_dir(app_handle)?;

    eprintln!("[TIMERS] Loading bundled from {:?}", bundled_dir);
    eprintln!("[TIMERS] Loading custom overlays from {:?}", user_dir);

    load_bosses_merged(&bundled_dir, &user_dir)
}

/// Load bosses from bundled dir, merging with custom overlays from user dir
fn load_bosses_merged(bundled_dir: &Path, user_dir: &Path) -> Result<Vec<BossWithPath>, String> {
    let mut results = Vec::new();

    // Load from bundled directory
    if bundled_dir.exists() {
        load_bosses_merged_recursive(bundled_dir, bundled_dir, user_dir, &mut results)?;
    }

    // Also load any user-created files (not overlays, but entirely new files)
    if user_dir.exists() {
        load_user_only_files(user_dir, user_dir, bundled_dir, &mut results)?;
    }

    Ok(results)
}

/// Recursively load bundled bosses, merging with custom overlays
fn load_bosses_merged_recursive(
    base_dir: &Path,
    current_dir: &Path,
    user_dir: &Path,
    results: &mut Vec<BossWithPath>,
) -> Result<(), String> {
    use baras_core::boss::load_bosses_with_custom;

    let entries = std::fs::read_dir(current_dir)
        .map_err(|e| format!("Failed to read directory {}: {}", current_dir.display(), e))?;

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() {
            load_bosses_merged_recursive(base_dir, &path, user_dir, results)?;
        } else if path.extension().is_some_and(|ext| ext == "toml") {
            // Load bundled + merge with custom overlay
            match load_bosses_with_custom(&path, Some(user_dir)) {
                Ok(file_bosses) => {
                    // Get category from [area] section in the TOML file
                    let category = baras_core::boss::load_area_config(&path)
                        .ok()
                        .flatten()
                        .map(|a| a.area_type.to_category())
                        .unwrap_or("other")
                        .to_string();

                    for boss in file_bosses {
                        results.push(BossWithPath {
                            boss,
                            file_path: path.clone(), // Points to bundled source
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

/// Load user-created files that don't have a bundled counterpart
fn load_user_only_files(
    base_dir: &Path,
    current_dir: &Path,
    bundled_dir: &Path,
    results: &mut Vec<BossWithPath>,
) -> Result<(), String> {
    use baras_core::boss::load_bosses_from_file;

    let entries = std::fs::read_dir(current_dir)
        .map_err(|e| format!("Failed to read directory {}: {}", current_dir.display(), e))?;

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() {
            load_user_only_files(base_dir, &path, bundled_dir, results)?;
        } else if path.extension().is_some_and(|ext| ext == "toml") {
            let filename = path.file_name().unwrap_or_default().to_string_lossy();

            // Skip *_custom.toml files (these are overlays, already merged)
            if filename.ends_with("_custom.toml") {
                continue;
            }

            // Skip if there's a matching bundled file
            if has_bundled_counterpart(&path, base_dir, bundled_dir) {
                continue;
            }

            // This is a user-created file
            match load_bosses_from_file(&path) {
                Ok(file_bosses) => {
                    // Get category from [area] section in the TOML file
                    let category = baras_core::boss::load_area_config(&path)
                        .ok()
                        .flatten()
                        .map(|a| a.area_type.to_category())
                        .unwrap_or("other")
                        .to_string();
                    eprintln!("[TIMERS] Loaded user file: {:?}", path);

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

/// Check if a user file has a corresponding bundled file
fn has_bundled_counterpart(user_file: &Path, user_base: &Path, bundled_dir: &Path) -> bool {
    if let Ok(relative) = user_file.strip_prefix(user_base) {
        let bundled_path = bundled_dir.join(relative);
        return bundled_path.exists();
    }
    false
}

/// Get the custom file path for saving edits to a bundled boss
fn get_custom_file_path(bundled_path: &Path, bundled_dir: &Path, user_dir: &Path) -> PathBuf {
    // Get relative path from bundled dir
    let relative = bundled_path
        .strip_prefix(bundled_dir)
        .unwrap_or(bundled_path);

    // Build custom file name: foo.toml -> foo_custom.toml
    let stem = bundled_path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    let custom_name = format!("{}_custom.toml", stem);

    // Put in same relative directory within user dir
    if let Some(parent) = relative.parent() {
        user_dir.join(parent).join(custom_name)
    } else {
        user_dir.join(custom_name)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Get all encounter timers as a flat list
#[tauri::command]
pub async fn get_encounter_timers(app_handle: AppHandle) -> Result<Vec<TimerListItem>, String> {
    let bosses = load_merged_bosses(&app_handle)?;
    let prefs = load_timer_preferences();

    let mut items = Vec::new();
    for boss_with_path in &bosses {
        for timer in &boss_with_path.boss.timers {
            items.push(TimerListItem::from_boss_timer(
                boss_with_path,
                timer,
                &prefs,
            ));
        }
    }

    // Sort by boss name, then timer name
    items.sort_by(|a, b| a.boss_name.cmp(&b.boss_name).then(a.name.cmp(&b.name)));

    Ok(items)
}

/// Update an existing timer
/// Preference fields (enabled, color, audio) go to preferences file
/// Definition fields go to custom overlay (for bundled) or user file
#[tauri::command]
pub async fn update_encounter_timer(
    app_handle: AppHandle,
    service: State<'_, ServiceHandle>,
    timer: TimerListItem,
) -> Result<(), String> {
    let file_path = PathBuf::from(&timer.file_path);
    let bosses = load_merged_bosses(&app_handle)?;

    // Find the original timer definition to compare
    let original = bosses
        .iter()
        .find(|b| b.boss.id == timer.boss_id && b.file_path == file_path)
        .and_then(|b| b.boss.timers.iter().find(|t| t.id == timer.timer_id))
        .ok_or_else(|| format!("Timer '{}' not found", timer.timer_id))?;

    // Check if preference fields changed
    let prefs_changed = timer.enabled != original.enabled
        || timer.color != original.color
        || timer.audio.enabled != original.audio.enabled
        || timer.audio.file != original.audio.file;

    // Check if definition fields changed (everything except preferences)
    let def_changed = timer.name != original.name
        || timer.display_text != original.display_text
        || timer.trigger != original.trigger
        || timer.duration_secs != original.duration_secs
        || timer.phases != original.phases
        || timer.difficulties != original.difficulties
        || timer.is_alert != original.is_alert
        || timer.alert_text != original.alert_text
        || timer.counter_condition != original.counter_condition
        || timer.cancel_trigger != original.cancel_trigger
        || timer.can_be_refreshed != original.can_be_refreshed
        || timer.repeats != original.repeats
        || timer.chains_to != original.chains_to
        || timer.alert_at_secs != original.alert_at_secs
        || timer.show_on_raid_frames != original.show_on_raid_frames
        || timer.show_at_secs != original.show_at_secs;

    // Save preference changes to preferences file
    if prefs_changed {
        let mut prefs = load_timer_preferences();
        let pref_key = boss_timer_key(&timer.area_name, &timer.boss_name, &timer.timer_id);

        // Only set preference if it differs from definition default
        if timer.enabled != original.enabled {
            prefs.update_enabled(&pref_key, timer.enabled);
        }
        if timer.color != original.color {
            prefs.update_color(&pref_key, timer.color);
        }
        if timer.audio.enabled != original.audio.enabled {
            prefs.update_audio_enabled(&pref_key, timer.audio.enabled);
        }
        if timer.audio.file != original.audio.file {
            prefs.update_audio_file(&pref_key, timer.audio.file.clone());
        }

        save_timer_preferences(&prefs)?;
        eprintln!("[TIMERS] Saved preferences for timer '{}'", timer.timer_id);

        // Update the live session's preferences (Live mode only)
        if let Some(session) = service.shared.session.read().await.as_ref() {
            let session = session.read().await;
            if let Some(timer_mgr) = session.timer_manager() {
                if let Ok(mut mgr) = timer_mgr.lock() {
                    mgr.set_preferences(prefs);
                }
            }
        }
    }

    // Save definition changes to file
    if def_changed {
        let bundled_dir = get_bundled_encounters_dir(&app_handle);
        let user_dir = get_user_encounters_dir();

        let timer_def = timer.to_timer_definition();

        let is_bundled = bundled_dir
            .as_ref()
            .is_some_and(|bd| file_path.starts_with(bd));

        if is_bundled {
            // Bundled file - save to custom overlay
            let custom_path = get_custom_file_path(
                &file_path,
                bundled_dir.as_ref().unwrap(),
                user_dir.as_ref().ok_or("No user dir")?,
            );
            save_timer_to_custom_file(&custom_path, &timer.boss_id, &timer_def)?;
        } else {
            // User file - save directly
            let mut bosses = load_merged_bosses(&app_handle)?;

            for boss_with_path in &mut bosses {
                if boss_with_path.boss.id == timer.boss_id && boss_with_path.file_path == file_path
                    && let Some(existing) = boss_with_path
                        .boss
                        .timers
                        .iter_mut()
                        .find(|t| t.id == timer.timer_id)
                    {
                        *existing = timer_def;
                        break;
                    }
            }

            let file_bosses: Vec<_> = bosses
                .iter()
                .filter(|b| b.file_path == file_path)
                .map(|b| b.boss.clone())
                .collect();

            save_bosses_to_file(&file_bosses, &file_path)?;
        }

        // Reload definitions into the running session
        let _ = service.reload_timer_definitions().await;
    }

    Ok(())
}

/// Save a single timer modification to a custom overlay file
/// Creates or updates the custom file with just the modified timer
fn save_timer_to_custom_file(
    custom_path: &Path,
    boss_id: &str,
    timer: &BossTimerDefinition,
) -> Result<(), String> {
    use baras_core::boss::{BossEncounterDefinition, load_bosses_from_file};

    // Load existing custom file if present
    let mut bosses = if custom_path.exists() {
        load_bosses_from_file(custom_path).unwrap_or_default()
    } else {
        Vec::new()
    };

    // Find or create the boss entry
    let boss = if let Some(b) = bosses.iter_mut().find(|b| b.id == boss_id) {
        b
    } else {
        // Create minimal boss entry for the overlay
        #[allow(deprecated)]
        bosses.push(BossEncounterDefinition {
            id: boss_id.to_string(),
            name: String::new(), // Will be merged from bundled
            area_name: String::new(),
            area_id: 0,
            difficulties: vec![],
            timers: vec![],
            phases: vec![],
            counters: vec![],
            challenges: vec![],
            entities: vec![],
        });
        bosses.last_mut().unwrap()
    };

    // Update or add the timer
    if let Some(existing) = boss.timers.iter_mut().find(|t| t.id == timer.id) {
        *existing = timer.clone();
    } else {
        boss.timers.push(timer.clone());
    }

    // Ensure parent directory exists
    if let Some(parent) = custom_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    save_bosses_to_file(&bosses, custom_path)?;

    eprintln!(
        "[TIMERS] Saved timer '{}' to custom file {:?}",
        timer.id, custom_path
    );
    Ok(())
}

/// Create a new timer for a boss
#[tauri::command]
pub async fn create_encounter_timer(
    app_handle: AppHandle,
    service: State<'_, ServiceHandle>,
    timer: TimerListItem,
) -> Result<TimerListItem, String> {
    let mut bosses = load_merged_bosses(&app_handle)?;
    let prefs = load_timer_preferences();
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
        show_at_secs: timer.show_at_secs,
        audio: timer.audio.clone(),
    };

    // Check for duplicate ID within the target boss only (per-encounter uniqueness)
    for boss_with_path in &bosses {
        if boss_with_path.boss.id == timer.boss_id
            && boss_with_path.boss.timers.iter().any(|t| t.id == timer_id)
        {
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
            created_item = Some(TimerListItem::from_boss_timer(
                boss_with_path,
                &new_timer,
                &prefs,
            ));
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
    let mut bosses = load_merged_bosses(&app_handle)?;

    // Canonicalize paths for reliable comparison
    let file_path_buf = PathBuf::from(&file_path);
    let canonical_path = file_path_buf
        .canonicalize()
        .unwrap_or_else(|_| file_path_buf.clone());

    // Find the boss and remove the timer
    let mut found = false;
    let mut matched_file_path: Option<PathBuf> = None;

    for boss_with_path in &mut bosses {
        let boss_canonical = boss_with_path
            .file_path
            .canonicalize()
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
            let b_canonical = b
                .file_path
                .canonicalize()
                .unwrap_or_else(|_| b.file_path.clone());
            b_canonical == canonical_path
        })
        .map(|b| b.boss.clone())
        .collect();

    save_bosses_to_file(&file_bosses, &save_path)?;

    // Reload definitions into the running session (propagate errors)
    service
        .reload_timer_definitions()
        .await
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
    let mut bosses = load_merged_bosses(&app_handle)?;
    let prefs = load_timer_preferences();
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
                    let exists_globally = bosses
                        .iter()
                        .any(|b| b.boss.timers.iter().any(|t| t.id == new_id));
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
        .map(|b| TimerListItem::from_boss_timer(b, &timer, &prefs))
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
pub async fn get_encounter_bosses(app_handle: AppHandle) -> Result<Vec<BossListItem>, String> {
    let bosses = load_merged_bosses(&app_handle)?;

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
/// Loads from bundled directory (with custom overlay counts merged) plus user-created areas
#[tauri::command]
pub async fn get_area_index(app_handle: AppHandle) -> Result<Vec<AreaListItem>, String> {
    eprintln!("[TIMERS] get_area_index called");

    let bundled_dir = get_bundled_encounters_dir(&app_handle)
        .ok_or_else(|| "Could not find bundled encounter definitions".to_string())?;
    let user_dir = ensure_user_encounters_dir(&app_handle)?;

    eprintln!("[TIMERS] Bundled dir: {:?}", bundled_dir);
    eprintln!("[TIMERS] User dir: {:?}", user_dir);

    let mut areas = Vec::new();

    // Collect bundled areas (with custom overlay counts merged)
    collect_areas_from_bundled(&bundled_dir, &user_dir, &mut areas)?;

    // Collect user-created areas (that aren't just overlays to bundled ones)
    let bundled_area_ids: std::collections::HashSet<_> = areas.iter().map(|a| a.area_id).collect();
    collect_user_areas(&user_dir, &bundled_area_ids, &mut areas)?;

    eprintln!("[TIMERS] Found {} areas", areas.len());

    // Sort by category then name
    areas.sort_by(|a, b| a.category.cmp(&b.category).then(a.name.cmp(&b.name)));

    Ok(areas)
}

/// Recursively collect area files from bundled directory with merged custom data
fn collect_areas_from_bundled(
    bundled_dir: &Path,
    user_dir: &Path,
    areas: &mut Vec<AreaListItem>,
) -> Result<(), String> {
    collect_areas_from_bundled_recursive(bundled_dir, bundled_dir, user_dir, areas)
}

fn collect_areas_from_bundled_recursive(
    base_dir: &Path,
    current_dir: &Path,
    user_dir: &Path,
    areas: &mut Vec<AreaListItem>,
) -> Result<(), String> {
    use baras_core::boss::{load_area_config, load_bosses_with_custom};

    let entries =
        std::fs::read_dir(current_dir).map_err(|e| format!("Failed to read directory: {}", e))?;

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() {
            collect_areas_from_bundled_recursive(base_dir, &path, user_dir, areas)?;
        } else if path.extension().is_some_and(|ext| ext == "toml") {
            // Try to load area config for metadata
            match load_area_config(&path) {
                Ok(Some(area_config)) => {
                    // Load bosses with custom overlay merged to get accurate counts
                    let (boss_count, timer_count) =
                        match load_bosses_with_custom(&path, Some(user_dir)) {
                            Ok(bosses) => {
                                let timers: usize = bosses.iter().map(|b| b.timers.len()).sum();
                                (bosses.len(), timers)
                            }
                            Err(e) => {
                                eprintln!("[TIMERS] Failed to load bosses from {:?}: {}", path, e);
                                (0, 0)
                            }
                        };

                    areas.push(AreaListItem {
                        name: area_config.name,
                        area_id: area_config.area_id,
                        file_path: path.to_string_lossy().to_string(),
                        category: area_config.area_type.to_category().to_string(),
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

/// Collect user-created areas that aren't overlays of bundled areas
fn collect_user_areas(
    user_dir: &Path,
    bundled_area_ids: &std::collections::HashSet<i64>,
    areas: &mut Vec<AreaListItem>,
) -> Result<(), String> {
    if !user_dir.exists() {
        return Ok(());
    }

    collect_user_areas_recursive(user_dir, bundled_area_ids, areas)
}

fn collect_user_areas_recursive(
    dir: &Path,
    bundled_area_ids: &std::collections::HashSet<i64>,
    areas: &mut Vec<AreaListItem>,
) -> Result<(), String> {
    use baras_core::boss::{load_area_config, load_bosses_from_file};

    let entries = std::fs::read_dir(dir).map_err(|e| format!("Failed to read directory: {}", e))?;

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() {
            collect_user_areas_recursive(&path, bundled_area_ids, areas)?;
        } else if path.extension().is_some_and(|ext| ext == "toml") {
            // Skip overlay files (those that end with _custom.toml)
            if path
                .file_stem()
                .is_some_and(|s| s.to_string_lossy().ends_with("_custom"))
            {
                continue;
            }

            match load_area_config(&path) {
                Ok(Some(area_config)) => {
                    // Skip if this area already exists in bundled (based on area_id)
                    if bundled_area_ids.contains(&area_config.area_id) {
                        continue;
                    }

                    // Count bosses and timers
                    let (boss_count, timer_count) = match load_bosses_from_file(&path) {
                        Ok(bosses) => {
                            let timers: usize = bosses.iter().map(|b| b.timers.len()).sum();
                            (bosses.len(), timers)
                        }
                        Err(_) => (0, 0),
                    };

                    areas.push(AreaListItem {
                        name: area_config.name,
                        area_id: area_config.area_id,
                        file_path: path.to_string_lossy().to_string(),
                        category: area_config.area_type.to_category().to_string(),
                        boss_count,
                        timer_count,
                    });
                }
                Ok(None) => {}
                Err(_) => {}
            }
        }
    }

    Ok(())
}

/// Get timers for a specific area file (lazy loading)
#[tauri::command]
pub async fn get_timers_for_area(file_path: String) -> Result<Vec<TimerListItem>, String> {
    let path = PathBuf::from(&file_path);

    if !path.exists() {
        return Err(format!("File not found: {}", file_path));
    }

    // Load bosses from this specific file
    let bosses = load_bosses_with_paths(path.parent().unwrap_or(&path))
        .map_err(|e| format!("Failed to load bosses: {}", e))?;
    let prefs = load_timer_preferences();

    // Filter to only bosses from this file and flatten timers
    let mut items = Vec::new();
    for boss_with_path in &bosses {
        if boss_with_path.file_path == path {
            for timer in &boss_with_path.boss.timers {
                items.push(TimerListItem::from_boss_timer(
                    boss_with_path,
                    timer,
                    &prefs,
                ));
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
    service: State<'_, ServiceHandle>,
    boss: BossEditItem,
) -> Result<BossEditItem, String> {
    use baras_core::boss::{BossEncounterDefinition, load_bosses_from_file};

    let file_path = PathBuf::from(&boss.file_path);

    if !file_path.exists() {
        return Err(format!("Area file not found: {}", boss.file_path));
    }

    // Load existing bosses from the file
    let mut bosses =
        load_bosses_from_file(&file_path).map_err(|e| format!("Failed to load bosses: {}", e))?;

    // Check for duplicate boss ID
    if bosses.iter().any(|b| b.id == boss.id) {
        return Err(format!(
            "Boss with ID '{}' already exists in this area",
            boss.id
        ));
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
pub async fn create_area(app_handle: AppHandle, area: NewAreaRequest) -> Result<String, String> {
    let user_dir = ensure_user_encounters_dir(&app_handle)?;

    // Generate filename from area name (snake_case)
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

    // Save directly in user directory (category comes from area_type in file)
    let file_path = user_dir.join(format!("{}.toml", filename));

    if file_path.exists() {
        return Err(format!("Area file already exists: {:?}", file_path));
    }

    // Create minimal TOML content with area config including area_type
    let content = format!(
        r#"# {}

[area]
name = "{}"
area_id = {}
area_type = "{}"

# Add bosses below using [[boss]] sections
"#,
        area.name, area.name, area.area_id, area.area_type
    );

    std::fs::write(&file_path, content).map_err(|e| format!("Failed to write area file: {}", e))?;

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
            if path.is_file()
                && let Some(ext) = path.extension() {
                    let ext = ext.to_string_lossy().to_lowercase();
                    if (ext == "mp3" || ext == "wav")
                        && let Some(name) = path.file_name() {
                            sounds.push(name.to_string_lossy().to_string());
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
    let mut bosses = load_merged_bosses(&app_handle)?;
    let file_path_buf = PathBuf::from(&phase.file_path);

    let mut updated_item = None;

    for boss_with_path in &mut bosses {
        if boss_with_path.boss.id == phase.boss_id && boss_with_path.file_path == file_path_buf {
            if let Some(existing) = boss_with_path
                .boss
                .phases
                .iter_mut()
                .find(|p| p.id == phase.id)
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
    let mut bosses = load_merged_bosses(&app_handle)?;
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
    let mut bosses = load_merged_bosses(&app_handle)?;
    let file_path_buf = PathBuf::from(&file_path);
    let canonical_path = file_path_buf
        .canonicalize()
        .unwrap_or_else(|_| file_path_buf.clone());

    let mut found = false;
    let mut matched_file_path: Option<PathBuf> = None;

    for boss_with_path in &mut bosses {
        let boss_canonical = boss_with_path
            .file_path
            .canonicalize()
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
        return Err(format!(
            "Phase '{}' not found in boss '{}'",
            phase_id, boss_id
        ));
    }

    let save_path = matched_file_path.unwrap_or(file_path_buf);
    let file_bosses: Vec<_> = bosses
        .iter()
        .filter(|b| {
            let b_canonical = b
                .file_path
                .canonicalize()
                .unwrap_or_else(|_| b.file_path.clone());
            b_canonical == canonical_path
        })
        .map(|b| b.boss.clone())
        .collect();

    save_bosses_to_file(&file_bosses, &save_path)?;
    service
        .reload_timer_definitions()
        .await
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
use baras_core::context::ChallengeColumns;

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decrement_on: Option<CounterTrigger>,
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
            decrement_on: counter.decrement_on.clone(),
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
            decrement_on: self.decrement_on.clone(),
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
    let mut bosses = load_merged_bosses(&app_handle)?;
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
    let mut bosses = load_merged_bosses(&app_handle)?;
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
            created_item = Some(CounterListItem::from_boss_counter(
                boss_with_path,
                &new_counter,
            ));
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
    let mut bosses = load_merged_bosses(&app_handle)?;
    let file_path_buf = PathBuf::from(&file_path);
    let canonical_path = file_path_buf
        .canonicalize()
        .unwrap_or_else(|_| file_path_buf.clone());

    let mut found = false;
    let mut matched_file_path: Option<PathBuf> = None;

    for boss_with_path in &mut bosses {
        let boss_canonical = boss_with_path
            .file_path
            .canonicalize()
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
            let b_canonical = b
                .file_path
                .canonicalize()
                .unwrap_or_else(|_| b.file_path.clone());
            b_canonical == canonical_path
        })
        .map(|b| b.boss.clone())
        .collect();

    save_bosses_to_file(&file_bosses, &save_path)?;
    service
        .reload_timer_definitions()
        .await
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
    /// Whether this challenge is enabled for display
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Bar color [r, g, b, a] (optional, uses overlay default if None)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<[u8; 4]>,
    /// Which columns to display
    #[serde(default)]
    pub columns: ChallengeColumns,
}

fn default_enabled() -> bool { true }

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
            enabled: challenge.enabled,
            color: challenge.color,
            columns: challenge.columns,
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
            enabled: self.enabled,
            color: self.color,
            columns: self.columns,
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
                items.push(ChallengeListItem::from_boss_challenge(
                    boss_with_path,
                    challenge,
                ));
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
    let mut bosses = load_merged_bosses(&app_handle)?;
    let file_path_buf = PathBuf::from(&challenge.file_path);

    let mut updated_item = None;

    for boss_with_path in &mut bosses {
        if boss_with_path.boss.id == challenge.boss_id && boss_with_path.file_path == file_path_buf
        {
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
    let mut bosses = load_merged_bosses(&app_handle)?;
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
            created_item = Some(ChallengeListItem::from_boss_challenge(
                boss_with_path,
                &new_challenge,
            ));
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
    let mut bosses = load_merged_bosses(&app_handle)?;
    let file_path_buf = PathBuf::from(&file_path);
    let canonical_path = file_path_buf
        .canonicalize()
        .unwrap_or_else(|_| file_path_buf.clone());

    let mut found = false;
    let mut matched_file_path: Option<PathBuf> = None;

    for boss_with_path in &mut bosses {
        let boss_canonical = boss_with_path
            .file_path
            .canonicalize()
            .unwrap_or_else(|_| boss_with_path.file_path.clone());

        if boss_with_path.boss.id == boss_id && boss_canonical == canonical_path {
            let original_len = boss_with_path.boss.challenges.len();
            boss_with_path
                .boss
                .challenges
                .retain(|c| c.id != challenge_id);
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
            let b_canonical = b
                .file_path
                .canonicalize()
                .unwrap_or_else(|_| b.file_path.clone());
            b_canonical == canonical_path
        })
        .map(|b| b.boss.clone())
        .collect();

    save_bosses_to_file(&file_bosses, &save_path)?;
    service
        .reload_timer_definitions()
        .await
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
    let mut bosses = load_merged_bosses(&app_handle)?;
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
    let mut bosses = load_merged_bosses(&app_handle)?;
    let file_path_buf = PathBuf::from(&entity.file_path);
    let boss_id = entity.boss_id.clone();

    let new_entity = entity.to_entity_definition();
    let mut created_item = None;

    for boss_with_path in &mut bosses {
        if boss_with_path.boss.id == boss_id && boss_with_path.file_path == file_path_buf {
            // Check for duplicate name
            if boss_with_path
                .boss
                .entities
                .iter()
                .any(|e| e.name == entity.name)
            {
                return Err(format!(
                    "Entity '{}' already exists in this boss",
                    entity.name
                ));
            }
            boss_with_path.boss.entities.push(new_entity.clone());
            created_item = Some(EntityListItem::from_boss_entity(
                boss_with_path,
                &new_entity,
            ));
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
    let mut bosses = load_merged_bosses(&app_handle)?;
    let file_path_buf = PathBuf::from(&file_path);

    let mut found = false;
    for boss_with_path in &mut bosses {
        if boss_with_path.boss.id == boss_id && boss_with_path.file_path == file_path_buf {
            let original_len = boss_with_path.boss.entities.len();
            boss_with_path
                .boss
                .entities
                .retain(|e| e.name != entity_name);
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

// ─────────────────────────────────────────────────────────────────────────────
// Timer Preferences Commands
// ─────────────────────────────────────────────────────────────────────────────

use baras_core::timers::{TimerPreference, TimerPreferences, boss_timer_key};

/// Get the timer preferences file path
fn timer_preferences_path() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("baras").join("timer_preferences.toml"))
}

/// Load timer preferences from disk
fn load_timer_preferences() -> TimerPreferences {
    timer_preferences_path()
        .and_then(|p| TimerPreferences::load(&p).ok())
        .unwrap_or_default()
}

/// Save timer preferences to disk
fn save_timer_preferences(prefs: &TimerPreferences) -> Result<(), String> {
    let path = timer_preferences_path().ok_or("Could not determine preferences path")?;
    prefs.save(&path).map_err(|e| e.to_string())
}

/// Get preference for a specific timer
#[tauri::command]
pub fn get_timer_preference(
    area_name: String,
    boss_name: String,
    timer_id: String,
) -> Option<TimerPreference> {
    let prefs = load_timer_preferences();
    let key = boss_timer_key(&area_name, &boss_name, &timer_id);
    prefs.get(&key).cloned()
}

/// Set enabled preference for a timer
#[tauri::command]
pub async fn set_timer_enabled(
    service: State<'_, ServiceHandle>,
    area_name: String,
    boss_name: String,
    timer_id: String,
    enabled: bool,
) -> Result<(), String> {
    let mut prefs = load_timer_preferences();
    let key = boss_timer_key(&area_name, &boss_name, &timer_id);
    prefs.update_enabled(&key, enabled);
    save_timer_preferences(&prefs)?;

    // Update the live session's timer manager preferences (Live mode only)
    if let Some(session) = service.shared.session.read().await.as_ref() {
        let session = session.read().await;
        if let Some(timer_mgr) = session.timer_manager() {
            if let Ok(mut mgr) = timer_mgr.lock() {
                mgr.set_preferences(prefs);
            }
        }
    }

    Ok(())
}

/// Set audio preference for a timer
#[tauri::command]
pub async fn set_timer_audio(
    service: State<'_, ServiceHandle>,
    area_name: String,
    boss_name: String,
    timer_id: String,
    audio_enabled: Option<bool>,
    audio_file: Option<String>,
) -> Result<(), String> {
    let mut prefs = load_timer_preferences();
    let key = boss_timer_key(&area_name, &boss_name, &timer_id);

    if let Some(enabled) = audio_enabled {
        prefs.update_audio_enabled(&key, enabled);
    }
    if audio_file.is_some() {
        prefs.update_audio_file(&key, audio_file);
    }

    save_timer_preferences(&prefs)?;

    // Update live session (Live mode only)
    if let Some(session) = service.shared.session.read().await.as_ref() {
        let session = session.read().await;
        if let Some(timer_mgr) = session.timer_manager() {
            if let Ok(mut mgr) = timer_mgr.lock() {
                mgr.set_preferences(prefs);
            }
        }
    }

    Ok(())
}

/// Set color preference for a timer
#[tauri::command]
pub async fn set_timer_color(
    service: State<'_, ServiceHandle>,
    area_name: String,
    boss_name: String,
    timer_id: String,
    color: [u8; 4],
) -> Result<(), String> {
    let mut prefs = load_timer_preferences();
    let key = boss_timer_key(&area_name, &boss_name, &timer_id);
    prefs.update_color(&key, color);
    save_timer_preferences(&prefs)?;

    // Update live session (Live mode only)
    if let Some(session) = service.shared.session.read().await.as_ref() {
        let session = session.read().await;
        if let Some(timer_mgr) = session.timer_manager() {
            if let Ok(mut mgr) = timer_mgr.lock() {
                mgr.set_preferences(prefs);
            }
        }
    }

    Ok(())
}

/// Reset all preferences for a timer back to defaults
#[tauri::command]
pub async fn reset_timer_preference(
    service: State<'_, ServiceHandle>,
    area_name: String,
    boss_name: String,
    timer_id: String,
) -> Result<(), String> {
    let mut prefs = load_timer_preferences();
    let key = boss_timer_key(&area_name, &boss_name, &timer_id);
    prefs.clear(&key);
    save_timer_preferences(&prefs)?;

    // Update live session (Live mode only)
    if let Some(session) = service.shared.session.read().await.as_ref() {
        let session = session.read().await;
        if let Some(timer_mgr) = session.timer_manager() {
            if let Ok(mut mgr) = timer_mgr.lock() {
                mgr.set_preferences(prefs);
            }
        }
    }

    Ok(())
}
