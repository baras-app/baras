//! Effect editor Tauri commands
//!
//! CRUD operations for effect definitions displayed in the effect editor UI.
//!
//! Architecture (delta-based):
//! - Bundled effect definitions are read from app resources (read-only)
//! - User overrides stored in single file: ~/.config/baras/definitions/effects.toml
//! - User effects with matching IDs completely replace bundled effects
//! - Version field enforces DSL compatibility - mismatched versions cause user file deletion

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};

use baras_core::dsl::{AudioConfig, Trigger};
use baras_core::effects::{
    AlertTrigger, DefinitionConfig, DisplayTarget, EFFECTS_DSL_VERSION, EffectDefinition,
};
use baras_types::AbilitySelector;

use crate::service::ServiceHandle;
use tracing::warn;

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
    /// Whether this effect has a user override (vs bundled-only)
    pub is_user_override: bool,

    // Effect data
    pub enabled: bool,
    pub trigger: Trigger,
    pub ignore_effect_removed: bool,
    pub refresh_abilities: Vec<AbilitySelector>,
    pub duration_secs: Option<f32>,
    pub is_refreshed_on_modify: bool,
    pub default_charges: Option<u8>,
    pub color: Option<[u8; 4]>,
    pub show_at_secs: f32,

    // Display routing
    pub display_target: DisplayTarget,
    pub icon_ability_id: Option<u64>,
    pub show_icon: bool,
    pub display_source: bool,

    // Duration modifiers
    pub is_affected_by_alacrity: bool,
    pub cooldown_ready_secs: f32,

    // Behavior
    pub persist_past_death: bool,
    pub track_outside_combat: bool,

    // Timer integration
    pub on_apply_trigger_timer: Option<String>,
    pub on_expire_trigger_timer: Option<String>,

    // Alerts
    pub alert_text: Option<String>,
    pub alert_on: AlertTrigger,

    // Audio
    pub audio: AudioConfig,
}

impl EffectListItem {
    fn from_definition(def: &EffectDefinition, is_user_override: bool) -> Self {
        Self {
            id: def.id.clone(),
            name: def.name.clone(),
            display_text: def.display_text.clone(),
            is_user_override,
            enabled: def.enabled,
            trigger: def.trigger.clone(),
            ignore_effect_removed: def.ignore_effect_removed,
            refresh_abilities: def.refresh_abilities.clone(),
            duration_secs: def.duration_secs,
            is_refreshed_on_modify: def.is_refreshed_on_modify,
            default_charges: def.default_charges,
            color: def.color,
            show_at_secs: def.show_at_secs,
            display_target: def.display_target,
            icon_ability_id: def.icon_ability_id,
            show_icon: def.show_icon,
            display_source: def.display_source,
            is_affected_by_alacrity: def.is_affected_by_alacrity,
            cooldown_ready_secs: def.cooldown_ready_secs,
            persist_past_death: def.persist_past_death,
            track_outside_combat: def.track_outside_combat,
            on_apply_trigger_timer: def.on_apply_trigger_timer.clone(),
            on_expire_trigger_timer: def.on_expire_trigger_timer.clone(),
            alert_text: def.alert_text.clone(),
            alert_on: def.alert_on,
            audio: def.audio.clone(),
        }
    }

    fn to_definition(&self) -> EffectDefinition {
        EffectDefinition {
            id: self.id.clone(),
            name: self.name.clone(),
            display_text: self.display_text.clone(),
            enabled: self.enabled,
            trigger: self.trigger.clone(),
            ignore_effect_removed: self.ignore_effect_removed,
            refresh_abilities: self.refresh_abilities.clone(),
            duration_secs: self.duration_secs,
            is_refreshed_on_modify: self.is_refreshed_on_modify,
            default_charges: self.default_charges,
            color: self.color,
            show_at_secs: self.show_at_secs,
            persist_past_death: self.persist_past_death,
            track_outside_combat: self.track_outside_combat,
            on_apply_trigger_timer: self.on_apply_trigger_timer.clone(),
            on_expire_trigger_timer: self.on_expire_trigger_timer.clone(),
            alert_text: self.alert_text.clone(),
            alert_on: self.alert_on,
            audio: self.audio.clone(),
            display_target: self.display_target,
            icon_ability_id: self.icon_ability_id,
            is_affected_by_alacrity: self.is_affected_by_alacrity,
            cooldown_ready_secs: self.cooldown_ready_secs,
            show_icon: self.show_icon,
            display_source: self.display_source,
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

// ─────────────────────────────────────────────────────────────────────────────
// Helper Functions
// ─────────────────────────────────────────────────────────────────────────────

/// Get the user effects config file path
fn get_user_effects_path() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("baras").join("definitions").join("effects.toml"))
}

/// Get the bundled effects directory
fn get_bundled_effects_dir(app_handle: &AppHandle) -> Option<PathBuf> {
    app_handle
        .path()
        .resolve("definitions/effects", tauri::path::BaseDirectory::Resource)
        .ok()
}

/// Load bundled effect definitions from app resources
fn load_bundled_effects(app_handle: &AppHandle) -> HashMap<String, EffectDefinition> {
    let mut effects = HashMap::new();

    let Some(bundled_dir) = get_bundled_effects_dir(app_handle).filter(|p| p.exists()) else {
        return effects;
    };

    let Ok(entries) = std::fs::read_dir(&bundled_dir) else {
        return effects;
    };

    let files: Vec<_> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|ext| ext == "toml")
                && !p.file_name().is_some_and(|n| n == "custom.toml") // Skip template
        })
        .collect();

    for path in files {
        if let Ok(contents) = std::fs::read_to_string(&path)
            && let Ok(config) = toml::from_str::<DefinitionConfig>(&contents)
        {
            for effect in config.effects {
                effects.insert(effect.id.clone(), effect);
            }
        }
    }

    effects
}

/// Load user effect overrides from single config file
fn load_user_effects_file() -> Option<(u32, Vec<EffectDefinition>)> {
    let path = get_user_effects_path()?;
    if !path.exists() {
        return None;
    }

    let contents = std::fs::read_to_string(&path).ok()?;
    let config = toml::from_str::<DefinitionConfig>(&contents).ok()?;

    Some((config.version, config.effects))
}

/// Save user effects to the config file
fn save_user_effects(effects: &[EffectDefinition]) -> Result<(), String> {
    let path = get_user_effects_path().ok_or("Cannot determine user effects path")?;

    let config = DefinitionConfig {
        version: EFFECTS_DSL_VERSION,
        effects: effects.to_vec(),
    };

    let content = toml::to_string_pretty(&config)
        .map_err(|e| format!("Failed to serialize effect config: {}", e))?;

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory {:?}: {}", parent, e))?;
    }

    std::fs::write(&path, content).map_err(|e| format!("Failed to write {:?}: {}", path, e))?;

    Ok(())
}

/// Load merged effects (bundled + user overrides) for UI display
fn load_all_effects(app_handle: &AppHandle) -> Vec<(EffectDefinition, bool)> {
    // Load bundled effects
    let bundled = load_bundled_effects(app_handle);

    // Load user overrides (if version matches)
    let user_effects: HashMap<String, EffectDefinition> =
        if let Some((version, effects)) = load_user_effects_file() {
            if version == EFFECTS_DSL_VERSION {
                effects.into_iter().map(|e| (e.id.clone(), e)).collect()
            } else {
                // Version mismatch - delete user file
                if let Some(path) = get_user_effects_path() {
                    warn!(
                        file_version = version,
                        expected_version = EFFECTS_DSL_VERSION,
                        "User effects version mismatch, deleting file"
                    );
                    let _ = std::fs::remove_file(path);
                }
                HashMap::new()
            }
        } else {
            HashMap::new()
        };

    // Track which IDs are user overrides
    let user_ids: std::collections::HashSet<_> = user_effects.keys().cloned().collect();

    // Merge: start with bundled, overlay with user
    let mut merged: HashMap<String, EffectDefinition> = bundled;
    for (id, effect) in user_effects {
        merged.insert(id, effect);
    }

    // Convert to list with is_user_override flag
    merged
        .into_iter()
        .map(|(id, effect)| {
            let is_user = user_ids.contains(&id);
            (effect, is_user)
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Get all effect definitions as a flat list
#[tauri::command]
pub async fn get_effect_definitions(app_handle: AppHandle) -> Result<Vec<EffectListItem>, String> {
    let effects = load_all_effects(&app_handle);

    let items: Vec<_> = effects
        .iter()
        .map(|(effect, is_user)| EffectListItem::from_definition(effect, *is_user))
        .collect();

    Ok(items)
}

/// Update an existing effect (always saved to user file)
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

    // Load current user effects
    let mut user_effects: Vec<EffectDefinition> = load_user_effects_file()
        .filter(|(v, _)| *v == EFFECTS_DSL_VERSION)
        .map(|(_, e)| e)
        .unwrap_or_default();

    // Update or add the effect
    if let Some(existing) = user_effects.iter_mut().find(|e| e.id == effect.id) {
        *existing = effect.to_definition();
    } else {
        user_effects.push(effect.to_definition());
    }

    save_user_effects(&user_effects)?;

    // Reload definitions in the running service
    let _ = service.reload_effect_definitions().await;

    Ok(())
}

/// Create a new effect (saved to user file)
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

    // Check for duplicate ID across all effects
    let all_effects = load_all_effects(&app_handle);
    if all_effects.iter().any(|(e, _)| e.id == effect.id) {
        return Err(format!("Effect with ID '{}' already exists", effect.id));
    }

    // Load current user effects and add the new one
    let mut user_effects: Vec<EffectDefinition> = load_user_effects_file()
        .filter(|(v, _)| *v == EFFECTS_DSL_VERSION)
        .map(|(_, e)| e)
        .unwrap_or_default();

    user_effects.push(effect.to_definition());
    save_user_effects(&user_effects)?;

    // Mark as user override
    effect.is_user_override = true;

    // Reload definitions in the running service
    let _ = service.reload_effect_definitions().await;

    Ok(effect)
}

/// Delete an effect
/// - If it's a user-only effect (not in bundled), removes it completely
/// - If it's a user override of bundled, removes the override (reverts to bundled)
#[tauri::command]
pub async fn delete_effect_definition(
    app_handle: AppHandle,
    service: State<'_, ServiceHandle>,
    effect_id: String,
) -> Result<(), String> {
    // Load bundled to check if this is an override
    let bundled = load_bundled_effects(&app_handle);
    let is_bundled = bundled.contains_key(&effect_id);

    // Load current user effects
    let mut user_effects: Vec<EffectDefinition> = load_user_effects_file()
        .filter(|(v, _)| *v == EFFECTS_DSL_VERSION)
        .map(|(_, e)| e)
        .unwrap_or_default();

    let original_len = user_effects.len();
    user_effects.retain(|e| e.id != effect_id);

    if user_effects.len() == original_len && !is_bundled {
        return Err(format!("Effect '{}' not found", effect_id));
    }

    // Save updated user effects (or remove file if empty)
    if user_effects.is_empty() {
        if let Some(path) = get_user_effects_path() {
            let _ = std::fs::remove_file(path);
        }
    } else {
        save_user_effects(&user_effects)?;
    }

    // Reload definitions in the running service
    let _ = service.reload_effect_definitions().await;

    Ok(())
}

/// Duplicate an effect with a new ID (always saved to user file)
#[tauri::command]
pub async fn duplicate_effect_definition(
    app_handle: AppHandle,
    service: State<'_, ServiceHandle>,
    effect_id: String,
) -> Result<EffectListItem, String> {
    let all_effects = load_all_effects(&app_handle);

    // Find the effect to duplicate
    let source = all_effects
        .iter()
        .find(|(e, _)| e.id == effect_id)
        .ok_or_else(|| format!("Effect '{}' not found", effect_id))?;

    // Generate unique ID
    let mut new_effect = source.0.clone();
    let mut suffix = 1;
    loop {
        let new_id = format!("{}_copy{}", effect_id, suffix);
        if !all_effects.iter().any(|(e, _)| e.id == new_id) {
            new_effect.id = new_id;
            new_effect.name = format!("{} (Copy)", source.0.name);
            break;
        }
        suffix += 1;
    }

    // Load current user effects and add the duplicate
    let mut user_effects: Vec<EffectDefinition> = load_user_effects_file()
        .filter(|(v, _)| *v == EFFECTS_DSL_VERSION)
        .map(|(_, e)| e)
        .unwrap_or_default();

    user_effects.push(new_effect.clone());
    save_user_effects(&user_effects)?;

    // Reload definitions in the running service
    let _ = service.reload_effect_definitions().await;

    Ok(EffectListItem::from_definition(&new_effect, true))
}

// ─────────────────────────────────────────────────────────────────────────────
// Export/Import Types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectImportDiff {
    pub id: String,
    pub name: String,
    pub display_target: DisplayTarget,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectImportPreview {
    pub effects_to_replace: Vec<EffectImportDiff>,
    pub effects_to_add: Vec<EffectImportDiff>,
    pub effects_unchanged: usize,
    pub errors: Vec<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Export/Import Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Export user effect overrides as TOML
#[tauri::command]
pub async fn export_effects_toml() -> Result<String, String> {
    let effects = load_user_effects_file()
        .filter(|(v, _)| *v == EFFECTS_DSL_VERSION)
        .map(|(_, effects)| effects)
        .ok_or("No custom effect definitions to export")?;

    if effects.is_empty() {
        return Err("No custom effect definitions to export".to_string());
    }

    let config = DefinitionConfig {
        version: EFFECTS_DSL_VERSION,
        effects,
    };

    toml::to_string_pretty(&config)
        .map_err(|e| format!("Failed to serialize effects: {}", e))
}

/// Preview effects import — parse TOML and diff against existing
#[tauri::command]
pub async fn preview_import_effects(
    app_handle: AppHandle,
    toml_content: String,
) -> Result<EffectImportPreview, String> {
    let config: DefinitionConfig = toml::from_str(&toml_content)
        .map_err(|e| format!("Failed to parse TOML: {}", e))?;

    let mut errors = Vec::new();

    if config.version != EFFECTS_DSL_VERSION {
        errors.push(format!(
            "Version mismatch: file has version {}, expected {}",
            config.version, EFFECTS_DSL_VERSION
        ));
    }

    if config.effects.is_empty() {
        errors.push("No effects found in file".to_string());
    }

    // Load current merged effects for diff
    let current_effects = load_all_effects(&app_handle);
    let current_ids: std::collections::HashSet<String> =
        current_effects.iter().map(|(e, _)| e.id.clone()).collect();
    let imported_ids: std::collections::HashSet<String> =
        config.effects.iter().map(|e| e.id.clone()).collect();

    let mut effects_to_replace = Vec::new();
    let mut effects_to_add = Vec::new();

    for effect in &config.effects {
        let diff = EffectImportDiff {
            id: effect.id.clone(),
            name: effect.name.clone(),
            display_target: effect.display_target,
        };

        if current_ids.contains(&effect.id) {
            effects_to_replace.push(diff);
        } else {
            effects_to_add.push(diff);
        }
    }

    let effects_unchanged = current_ids
        .iter()
        .filter(|id| !imported_ids.contains(*id))
        .count();

    Ok(EffectImportPreview {
        effects_to_replace,
        effects_to_add,
        effects_unchanged,
        errors,
    })
}

/// Import effects from TOML, merging into user file
#[tauri::command]
pub async fn import_effects_toml(
    service: State<'_, ServiceHandle>,
    toml_content: String,
) -> Result<(), String> {
    let config: DefinitionConfig = toml::from_str(&toml_content)
        .map_err(|e| format!("Failed to parse TOML: {}", e))?;

    if config.version != EFFECTS_DSL_VERSION {
        return Err(format!(
            "Version mismatch: file has version {}, expected {}",
            config.version, EFFECTS_DSL_VERSION
        ));
    }

    // Load current user effects and merge
    let mut user_effects: Vec<EffectDefinition> = load_user_effects_file()
        .filter(|(v, _)| *v == EFFECTS_DSL_VERSION)
        .map(|(_, e)| e)
        .unwrap_or_default();

    for imported in config.effects {
        if let Some(existing) = user_effects.iter_mut().find(|e| e.id == imported.id) {
            *existing = imported;
        } else {
            user_effects.push(imported);
        }
    }

    save_user_effects(&user_effects)?;
    let _ = service.reload_effect_definitions().await;

    Ok(())
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

use std::io::Read;
use std::sync::Mutex;
use zip::ZipArchive;

/// Lazy-loaded icon name lookup cache
static ICON_NAME_CACHE: std::sync::OnceLock<Mutex<HashMap<u64, String>>> =
    std::sync::OnceLock::new();

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
                    .ancestors()
                    .nth(2)
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| PathBuf::from("."))
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

    let icon_name =
        icon_name.ok_or_else(|| format!("No icon mapping for ability {}", ability_id))?;

    // Get icons directory
    let icons_dir = app_handle
        .path()
        .resolve("icons", tauri::path::BaseDirectory::Resource)
        .ok()
        .filter(|p| p.exists())
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .nth(2)
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."))
                .join("icons")
        });

    // Try to load from ZIP files
    let zip_paths = [icons_dir.join("icons.zip"), icons_dir.join("icons2.zip")];

    let filename = format!("{}.png", icon_name);

    for zip_path in &zip_paths {
        if let Ok(file) = std::fs::File::open(zip_path) {
            let reader = std::io::BufReader::new(file);
            if let Ok(mut archive) = ZipArchive::new(reader)
                && let Ok(mut zip_file) = archive.by_name(&filename)
            {
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

    Err(format!("Icon '{}' not found in ZIP archives", icon_name))
}
