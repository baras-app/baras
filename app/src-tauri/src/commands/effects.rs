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
use baras_core::effects::{
    DefinitionConfig, EffectCategory, EffectDefinition, EffectSelector, EffectTriggerMode,
    EntityFilter,
};
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
    pub trigger: EffectTriggerMode,
    pub start_trigger: Option<Trigger>,
    pub fixed_duration: bool,
    pub effects: Vec<EffectSelector>,
    pub refresh_abilities: Vec<AbilitySelector>,
    pub source: EntityFilter,
    pub target: EntityFilter,
    pub duration_secs: Option<f32>,
    pub can_be_refreshed: bool,
    pub is_refreshed_on_modify: bool,
    pub max_stacks: u8,
    pub color: Option<[u8; 4]>,
    pub show_on_raid_frames: bool,
    pub show_on_effects_overlay: bool,
    pub show_at_secs: f32,

    // Behavior
    pub persist_past_death: bool,
    pub track_outside_combat: bool,

    // Timer integration
    pub on_apply_trigger_timer: Option<String>,
    pub on_expire_trigger_timer: Option<String>,

    // Context
    pub encounters: Vec<String>,

    // Alerts
    pub alert_near_expiration: bool,
    pub alert_threshold_secs: f32,

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
            trigger: def.trigger,
            start_trigger: def.start_trigger.clone(),
            fixed_duration: def.fixed_duration,
            effects: def.effects.clone(),
            refresh_abilities: def.refresh_abilities.clone(),
            source: def.source.clone(),
            target: def.target.clone(),
            duration_secs: def.duration_secs,
            can_be_refreshed: def.can_be_refreshed,
            is_refreshed_on_modify: def.is_refreshed_on_modify,
            max_stacks: def.max_stacks,
            color: def.color,
            show_on_raid_frames: def.show_on_raid_frames,
            show_on_effects_overlay: def.show_on_effects_overlay,
            show_at_secs: def.show_at_secs,
            persist_past_death: def.persist_past_death,
            track_outside_combat: def.track_outside_combat,
            on_apply_trigger_timer: def.on_apply_trigger_timer.clone(),
            on_expire_trigger_timer: def.on_expire_trigger_timer.clone(),
            encounters: def.encounters.clone(),
            alert_near_expiration: def.alert_near_expiration,
            alert_threshold_secs: def.alert_threshold_secs,
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
            trigger: self.trigger,
            start_trigger: self.start_trigger.clone(),
            fixed_duration: self.fixed_duration,
            effects: self.effects.clone(),
            refresh_abilities: self.refresh_abilities.clone(),
            source: self.source.clone(),
            target: self.target.clone(),
            duration_secs: self.duration_secs,
            can_be_refreshed: self.can_be_refreshed,
            is_refreshed_on_modify: self.is_refreshed_on_modify,
            max_stacks: self.max_stacks,
            color: self.color,
            show_on_raid_frames: self.show_on_raid_frames,
            show_on_effects_overlay: self.show_on_effects_overlay,
            show_at_secs: self.show_at_secs,
            persist_past_death: self.persist_past_death,
            track_outside_combat: self.track_outside_combat,
            on_apply_trigger_timer: self.on_apply_trigger_timer.clone(),
            on_expire_trigger_timer: self.on_expire_trigger_timer.clone(),
            encounters: self.encounters.clone(),
            alert_near_expiration: self.alert_near_expiration,
            alert_threshold_secs: self.alert_threshold_secs,
            audio: self.audio.clone(),
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
    // (start_trigger like AbilityCast can also trigger effects)
    if effect.effects.is_empty()
        && effect.refresh_abilities.is_empty()
        && effect.start_trigger.is_none()
    {
        return Err(
            "Effect must have at least one effect ID, refresh ability, or start_trigger to match against. \
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
    // (start_trigger like AbilityCast can also trigger effects)
    if effect.effects.is_empty()
        && effect.refresh_abilities.is_empty()
        && effect.start_trigger.is_none()
    {
        return Err(
            "Effect must have at least one effect ID, refresh ability, or start_trigger to match against. \
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
