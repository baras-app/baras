//! Effect editor Tauri commands
//!
//! CRUD operations for effect definitions displayed in the effect editor UI.
//!
//! Architecture:
//! - Default effect definitions are bundled with the app (read-only)
//! - On first launch, defaults are copied to user config dir (~/.config/baras/effects/)
//! - All edits are made to the user config copy, never the bundled defaults

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, State};

use baras_core::dsl::{AudioConfig, Trigger};
use baras_core::effects::{DefinitionConfig, DisplayTarget, EffectCategory, EffectDefinition};
use baras_types::AbilitySelector;

use crate::service::ServiceHandle;

// ─────────────────────────────────────────────────────────────────────────────
// Types for Frontend
// ─────────────────────────────────────────────────────────────────────────────

/// Effect item for the frontend list view
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectListItem {
    // Identity
    pub id: String,
    pub name: String,
    pub display_text: Option<String>,
    pub file_path: String,

    // Effect data
    pub enabled: bool,
    pub category: EffectCategory,
    pub trigger: Trigger,
    pub ignore_effect_removed: bool,
    pub refresh_abilities: Vec<AbilitySelector>,
    pub duration_secs: Option<f32>,
    pub is_refreshed_on_modify: bool,
    pub color: Option<[u8; 4]>,
    pub show_at_secs: f32,

    // Display routing
    pub display_target: DisplayTarget,
    pub icon_ability_id: Option<u64>,
    pub show_icon: bool,

    // Duration modifiers
    pub is_affected_by_alacrity: bool,
    pub cooldown_ready_secs: f32,

    // Behavior
    pub persist_past_death: bool,
    pub track_outside_combat: bool,

    // Timer integration
    pub on_apply_trigger_timer: Option<String>,
    pub on_expire_trigger_timer: Option<String>,

    // Audio
    pub audio: AudioConfig,
}

impl EffectListItem {
    fn from_definition(def: &EffectDefinition, file_path: &Path) -> Self {
        Self {
            id: def.id.clone(),
            name: def.name.clone(),
            display_text: def.display_text.clone(),
            file_path: file_path.to_string_lossy().to_string(),
            enabled: def.enabled,
            category: def.category,
            trigger: def.trigger.clone(),
            ignore_effect_removed: def.ignore_effect_removed,
            refresh_abilities: def.refresh_abilities.clone(),
            duration_secs: def.duration_secs,
            is_refreshed_on_modify: def.is_refreshed_on_modify,
            color: def.color,
            show_at_secs: def.show_at_secs,
            display_target: def.display_target,
            icon_ability_id: def.icon_ability_id,
            show_icon: def.show_icon,
            is_affected_by_alacrity: def.is_affected_by_alacrity,
            cooldown_ready_secs: def.cooldown_ready_secs,
            persist_past_death: def.persist_past_death,
            track_outside_combat: def.track_outside_combat,
            on_apply_trigger_timer: def.on_apply_trigger_timer.clone(),
            on_expire_trigger_timer: def.on_expire_trigger_timer.clone(),
            audio: def.audio.clone(),
        }
    }

    fn to_definition(&self) -> EffectDefinition {
        EffectDefinition {
            id: self.id.clone(),
            name: self.name.clone(),
            display_text: self.display_text.clone(),
            enabled: self.enabled,
            category: self.category,
            trigger: self.trigger.clone(),
            ignore_effect_removed: self.ignore_effect_removed,
            refresh_abilities: self.refresh_abilities.clone(),
            duration_secs: self.duration_secs,
            is_refreshed_on_modify: self.is_refreshed_on_modify,
            color: self.color,
            show_on_raid_frames: self.display_target == DisplayTarget::RaidFrames,
            show_at_secs: self.show_at_secs,
            persist_past_death: self.persist_past_death,
            track_outside_combat: self.track_outside_combat,
            on_apply_trigger_timer: self.on_apply_trigger_timer.clone(),
            on_expire_trigger_timer: self.on_expire_trigger_timer.clone(),
            audio: self.audio.clone(),
            display_target: self.display_target,
            icon_ability_id: self.icon_ability_id,
            is_affected_by_alacrity: self.is_affected_by_alacrity,
            cooldown_ready_secs: self.cooldown_ready_secs,
            show_icon: self.show_icon,
        }
    }

    /// Check if this effect has a valid trigger configuration
    fn has_valid_trigger(&self) -> bool {
        match &self.trigger {
            Trigger::EffectApplied { effects, .. } | Trigger::EffectRemoved { effects, .. } => {
                !effects.is_empty() || !self.refresh_abilities.is_empty()
            }
            Trigger::AbilityCast { abilities, .. } => {
                !abilities.is_empty() || !self.refresh_abilities.is_empty()
            }
            _ => false,
        }
    }
}

/// Effect with its source file path for loading/saving
struct EffectWithPath {
    effect: EffectDefinition,
    file_path: PathBuf,
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper Functions
// ─────────────────────────────────────────────────────────────────────────────

/// Get the user's effects config directory
fn get_user_effects_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("baras").join("definitions").join("effects"))
}

/// Get the bundled default effects directory
fn get_bundled_effects_dir(app_handle: &AppHandle) -> Option<PathBuf> {
    app_handle
        .path()
        .resolve("definitions/effects", tauri::path::BaseDirectory::Resource)
        .ok()
}

/// Ensure user effects directory exists and has defaults synced
fn ensure_user_effects_dir(app_handle: &AppHandle) -> Result<PathBuf, String> {
    let user_dir = get_user_effects_dir()
        .ok_or_else(|| "Could not determine user config directory".to_string())?;

    let bundled_dir = get_bundled_effects_dir(app_handle)
        .ok_or_else(|| "Could not find bundled effect definitions".to_string())?;

    if !bundled_dir.exists() {
        return Err(format!(
            "Bundled effects directory does not exist: {:?}",
            bundled_dir
        ));
    }

    // Ensure user dir exists
    std::fs::create_dir_all(&user_dir)
        .map_err(|e| format!("Failed to create user effects dir: {}", e))?;

    // Sync missing files from bundled to user dir (don't overwrite existing)
    sync_missing_files(&bundled_dir, &user_dir)?;

    Ok(user_dir)
}

/// Copy files from src to dst that don't already exist in dst
fn sync_missing_files(src: &PathBuf, dst: &Path) -> Result<(), String> {
    let entries =
        std::fs::read_dir(src).map_err(|e| format!("Failed to read directory {:?}: {}", src, e))?;

    for entry in entries.flatten() {
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        // Only copy if destination doesn't exist (preserve user edits)
        if !dst_path.exists() {
            if src_path.is_dir() {
                copy_dir_recursive(&src_path, &dst_path)?;
                eprintln!(
                    "[EFFECTS] Synced missing directory: {:?}",
                    entry.file_name()
                );
            } else {
                std::fs::copy(&src_path, &dst_path)
                    .map_err(|e| format!("Failed to copy {:?}: {}", src_path, e))?;
                eprintln!("[EFFECTS] Synced missing file: {:?}", entry.file_name());
            }
        }
    }

    Ok(())
}

/// Recursively copy a directory
fn copy_dir_recursive(src: &PathBuf, dst: &PathBuf) -> Result<(), String> {
    std::fs::create_dir_all(dst)
        .map_err(|e| format!("Failed to create directory {:?}: {}", dst, e))?;

    let entries =
        std::fs::read_dir(src).map_err(|e| format!("Failed to read directory {:?}: {}", src, e))?;

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

/// Load all effects from the user config directory with file paths
fn load_user_effects(app_handle: &AppHandle) -> Result<Vec<EffectWithPath>, String> {
    let user_dir = ensure_user_effects_dir(app_handle)?;
    load_effects_with_paths(&user_dir)
}

/// Load effects from a directory with their file paths
fn load_effects_with_paths(dir: &PathBuf) -> Result<Vec<EffectWithPath>, String> {
    let mut results = Vec::new();

    if !dir.exists() {
        return Ok(results);
    }

    let entries =
        std::fs::read_dir(dir).map_err(|e| format!("Failed to read directory {:?}: {}", dir, e))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "toml") {
            match std::fs::read_to_string(&path) {
                Ok(contents) => {
                    if let Ok(config) = toml::from_str::<DefinitionConfig>(&contents) {
                        for effect in config.effects {
                            results.push(EffectWithPath {
                                effect,
                                file_path: path.clone(),
                            });
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[EFFECTS] Failed to read {:?}: {}", path, e);
                }
            }
        }
    }

    Ok(results)
}

/// Save effects to a TOML file
fn save_effects_to_file(effects: &[EffectDefinition], path: &PathBuf) -> Result<(), String> {
    let config = DefinitionConfig {
        effects: effects.to_vec(),
    };

    let content = toml::to_string_pretty(&config)
        .map_err(|e| format!("Failed to serialize effect config: {}", e))?;

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory {:?}: {}", parent, e))?;
    }

    std::fs::write(path, content).map_err(|e| format!("Failed to write {:?}: {}", path, e))?;

    Ok(())
}

/// Get the path to custom.toml for user-created/modified effects
fn get_custom_effects_path() -> Option<PathBuf> {
    get_user_effects_dir().map(|p| p.join("custom.toml"))
}

/// Load effects from custom.toml
fn load_custom_effects() -> Vec<EffectDefinition> {
    let Some(custom_path) = get_custom_effects_path() else {
        return vec![];
    };

    if !custom_path.exists() {
        return vec![];
    }

    match std::fs::read_to_string(&custom_path) {
        Ok(contents) => toml::from_str::<DefinitionConfig>(&contents)
            .map(|c| c.effects)
            .unwrap_or_default(),
        Err(_) => vec![],
    }
}

/// Save effects to custom.toml
fn save_custom_effects(effects: &[EffectDefinition]) -> Result<(), String> {
    let Some(custom_path) = get_custom_effects_path() else {
        return Err("Cannot determine custom effects path".to_string());
    };
    save_effects_to_file(effects, &custom_path)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Get all effect definitions as a flat list
#[tauri::command]
pub async fn get_effect_definitions(app_handle: AppHandle) -> Result<Vec<EffectListItem>, String> {
    let effects = load_user_effects(&app_handle)?;

    let items: Vec<_> = effects
        .iter()
        .map(|e| EffectListItem::from_definition(&e.effect, &e.file_path))
        .collect();

    Ok(items)
}

/// Update an existing effect (modifications saved to custom.toml)
#[tauri::command]
pub async fn update_effect_definition(
    service: State<'_, ServiceHandle>,
    effect: EffectListItem,
) -> Result<(), String> {
    // Validate effect has at least one way to match
    if !effect.has_valid_trigger() {
        return Err(
            "Effect must have at least one effect ID, ability, or refresh ability to match against. \
            Without these, the effect will never trigger."
                .to_string(),
        );
    }

    let custom_path = get_custom_effects_path().ok_or("Cannot determine custom effects path")?;
    let is_from_custom = effect.file_path == custom_path.to_string_lossy();

    // Load custom effects
    let mut custom_effects = load_custom_effects();

    if is_from_custom {
        // Effect is already in custom.toml - update in place
        let mut found = false;
        for existing in &mut custom_effects {
            if existing.id == effect.id {
                *existing = effect.to_definition();
                found = true;
                break;
            }
        }

        if !found {
            return Err(format!("Effect '{}' not found in custom.toml", effect.id));
        }
    } else {
        // Effect is from a default file - add/update in custom.toml as an override
        // This creates a new entry that will override the default when loaded
        let existing_idx = custom_effects.iter().position(|e| e.id == effect.id);

        if let Some(idx) = existing_idx {
            custom_effects[idx] = effect.to_definition();
        } else {
            custom_effects.push(effect.to_definition());
        }
    }

    save_custom_effects(&custom_effects)?;

    // Reload definitions in the running service
    let _ = service.reload_effect_definitions().await;

    Ok(())
}

/// Create a new effect (always saved to custom.toml)
#[tauri::command]
pub async fn create_effect_definition(
    app_handle: AppHandle,
    service: State<'_, ServiceHandle>,
    mut effect: EffectListItem,
) -> Result<EffectListItem, String> {
    // Validate effect has at least one way to match
    if !effect.has_valid_trigger() {
        return Err(
            "Effect must have at least one effect ID, ability, or refresh ability to match against. \
            Without these, the effect will never trigger."
                .to_string(),
        );
    }

    // Generate ID from name if not provided
    if effect.id.is_empty() {
        effect.id = generate_effect_id(&effect.name);
    }

    let effects = load_user_effects(&app_handle)?;

    // Check for duplicate ID across all files
    if effects.iter().any(|e| e.effect.id == effect.id) {
        return Err(format!("Effect with ID '{}' already exists", effect.id));
    }

    // Load existing custom effects and add the new one
    let mut custom_effects = load_custom_effects();
    custom_effects.push(effect.to_definition());

    // Save to custom.toml
    save_custom_effects(&custom_effects)?;

    // Update file_path to reflect where it was saved
    let custom_path = get_custom_effects_path().ok_or("Cannot determine custom effects path")?;
    effect.file_path = custom_path.to_string_lossy().to_string();

    // Reload definitions in the running service
    let _ = service.reload_effect_definitions().await;

    Ok(effect)
}

/// Delete an effect
#[tauri::command]
pub async fn delete_effect_definition(
    app_handle: AppHandle,
    service: State<'_, ServiceHandle>,
    effect_id: String,
    file_path: String,
) -> Result<(), String> {
    let effects = load_user_effects(&app_handle)?;
    let file_path_buf = PathBuf::from(&file_path);

    // Get all effects from the same file
    let file_effects: Vec<EffectDefinition> = effects
        .iter()
        .filter(|e| e.file_path == file_path_buf)
        .map(|e| e.effect.clone())
        .collect();

    // Remove the effect
    let new_effects: Vec<_> = file_effects
        .into_iter()
        .filter(|e| e.id != effect_id)
        .collect();

    if new_effects.len()
        == effects
            .iter()
            .filter(|e| e.file_path == file_path_buf)
            .count()
    {
        return Err(format!("Effect '{}' not found", effect_id));
    }

    save_effects_to_file(&new_effects, &file_path_buf)?;

    // Reload definitions in the running service
    let _ = service.reload_effect_definitions().await;

    Ok(())
}

/// Duplicate an effect with a new ID
#[tauri::command]
pub async fn duplicate_effect_definition(
    app_handle: AppHandle,
    service: State<'_, ServiceHandle>,
    effect_id: String,
    file_path: String,
) -> Result<EffectListItem, String> {
    let effects = load_user_effects(&app_handle)?;
    let file_path_buf = PathBuf::from(&file_path);

    // Find the effect to duplicate
    let source = effects
        .iter()
        .find(|e| e.effect.id == effect_id && e.file_path == file_path_buf)
        .ok_or_else(|| format!("Effect '{}' not found", effect_id))?;

    // Generate unique ID
    let mut new_effect = source.effect.clone();
    let mut suffix = 1;
    loop {
        let new_id = format!("{}_copy{}", effect_id, suffix);
        if !effects.iter().any(|e| e.effect.id == new_id) {
            new_effect.id = new_id;
            new_effect.name = format!("{} (Copy)", source.effect.name);
            break;
        }
        suffix += 1;
    }

    // Get all effects from the target file and add the new one
    let mut file_effects: Vec<EffectDefinition> = effects
        .iter()
        .filter(|e| e.file_path == file_path_buf)
        .map(|e| e.effect.clone())
        .collect();

    file_effects.push(new_effect.clone());

    save_effects_to_file(&file_effects, &file_path_buf)?;

    // Reload definitions in the running service
    let _ = service.reload_effect_definitions().await;

    Ok(EffectListItem::from_definition(&new_effect, &file_path_buf))
}

/// Get list of available effect files (for "New Effect" file selection)
#[tauri::command]
pub async fn get_effect_files(app_handle: AppHandle) -> Result<Vec<String>, String> {
    let user_dir = ensure_user_effects_dir(&app_handle)?;

    let mut files = Vec::new();

    if user_dir.exists() {
        let entries =
            std::fs::read_dir(&user_dir).map_err(|e| format!("Failed to read directory: {}", e))?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "toml") {
                files.push(path.to_string_lossy().to_string());
            }
        }
    }

    Ok(files)
}

/// Generate an effect ID from name (snake_case, safe for TOML)
fn generate_effect_id(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .split('_')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

// ─────────────────────────────────────────────────────────────────────────────
// Icon Preview
// ─────────────────────────────────────────────────────────────────────────────

use std::collections::HashMap;
use std::io::Read;
use std::sync::Mutex;
use zip::ZipArchive;

/// Lazy-loaded icon name lookup cache
static ICON_NAME_CACHE: std::sync::OnceLock<Mutex<HashMap<u64, String>>> = std::sync::OnceLock::new();

/// Get or load the icon name mapping from CSV
fn get_icon_name_mapping(app_handle: &AppHandle) -> Option<&Mutex<HashMap<u64, String>>> {
    Some(ICON_NAME_CACHE.get_or_init(|| {
        let icons_dir = app_handle
            .path()
            .resolve("icons", tauri::path::BaseDirectory::Resource)
            .ok()
            .filter(|p| p.exists())
            .unwrap_or_else(|| {
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .join("icons")
            });

        let csv_path = icons_dir.join("icons.csv");
        let mut map = HashMap::new();

        if let Ok(content) = std::fs::read_to_string(&csv_path) {
            for line in content.lines().skip(1) {
                let line = line.trim_start_matches('\u{feff}');
                if line.is_empty() || line.starts_with("ability_id") {
                    continue;
                }
                let parts: Vec<&str> = line.splitn(3, ',').collect();
                if parts.len() >= 3 {
                    if let Ok(ability_id) = parts[0].parse::<u64>() {
                        let icon_name = parts[2].trim().to_lowercase();
                        if !icon_name.is_empty() {
                            map.insert(ability_id, icon_name);
                        }
                    }
                }
            }
        }

        Mutex::new(map)
    }))
}

/// Get icon PNG data as base64 for a given ability ID
#[tauri::command]
pub async fn get_icon_preview(app_handle: AppHandle, ability_id: u64) -> Result<String, String> {
    // Look up icon name
    let icon_name = {
        let cache = get_icon_name_mapping(&app_handle).ok_or("Failed to load icon mapping")?;
        let map = cache.lock().map_err(|_| "Lock poisoned")?;
        map.get(&ability_id).cloned()
    };

    let icon_name = icon_name.ok_or_else(|| format!("No icon mapping for ability {}", ability_id))?;

    // Get icons directory
    let icons_dir = app_handle
        .path()
        .resolve("icons", tauri::path::BaseDirectory::Resource)
        .ok()
        .filter(|p| p.exists())
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("icons")
        });

    // Try to load from ZIP files
    let zip_paths = [
        icons_dir.join("icons.zip"),
        icons_dir.join("icons2.zip"),
    ];

    let filename = format!("{}.png", icon_name);

    for zip_path in &zip_paths {
        if let Ok(file) = std::fs::File::open(zip_path) {
            let reader = std::io::BufReader::new(file);
            if let Ok(mut archive) = ZipArchive::new(reader) {
                if let Ok(mut zip_file) = archive.by_name(&filename) {
                    let mut png_data = Vec::new();
                    if zip_file.read_to_end(&mut png_data).is_ok() {
                        // Return base64 encoded PNG
                        use base64::Engine;
                        let base64_data = base64::engine::general_purpose::STANDARD.encode(&png_data);
                        return Ok(format!("data:image/png;base64,{}", base64_data));
                    }
                }
            }
        }
    }

    Err(format!("Icon '{}' not found in ZIP archives", icon_name))
}
