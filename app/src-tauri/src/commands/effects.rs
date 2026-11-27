//! Effect editor Tauri commands
//!
//! CRUD operations for effect definitions displayed in the effect editor UI.
//!
//! Bundled definitions are read-only; user edits live in `definitions/effects.toml`
//! as sparse overrides of bundled effects or full custom definitions (`UserEffects`).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};

use baras_core::dsl::{AudioConfig, Trigger};
use baras_core::effects::{
    AlertTrigger, DefinitionConfig, DefinitionSet, DisplayTarget, EFFECTS_DSL_VERSION, EffectDefinition, UserEffects,
    eq_ignoring_toggles,
};
use baras_core::game_data::{Discipline, DisciplineFilter};
use baras_types::{RefreshAbility, RefreshScope};

use crate::service::{ServiceHandle, load_bundled_effects};

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
    #[serde(default)]
    pub note: Option<String>,
    /// Whether this effect has a user override (vs bundled-only)
    pub is_user_override: bool,
    /// Whether this effect exists in the bundled defaults
    pub is_bundled: bool,

    // Effect data
    pub enabled: bool,
    pub trigger: Trigger,
    pub ignore_effect_removed: bool,
    pub refresh_abilities: Vec<RefreshAbility>,
    pub is_aoe_refresh: bool,
    #[serde(default)]
    pub aoe_refresh_immediate: bool,
    pub duration_secs: Option<f32>,
    pub is_refreshed_on_modify: bool,
    #[serde(default)]
    pub single_instance_per_target: bool,
    pub default_charges: Option<u8>,
    pub color: Option<[u8; 4]>,
    pub show_at_secs: f32,

    // Display routing
    #[serde(default, alias = "display_target")]
    pub display_targets: Vec<DisplayTarget>,
    pub icon_ability_id: Option<u64>,
    pub show_icon: bool,
    pub display_source: bool,
    #[serde(default)]
    pub show_target: bool,
    #[serde(default)]
    pub track_uptime: bool,
    #[serde(default)]
    pub track_uptime_after_first: bool,
    #[serde(default)]
    pub stack_priority: bool,
    #[serde(default = "baras_core::serde_defaults::default_true")]
    pub show_countdown: bool,
    #[serde(default)]
    pub show_single_stack: bool,

    // Duration modifiers
    pub is_affected_by_alacrity: bool,
    pub cooldown_ready_secs: f32,

    // Discipline scoping
    /// Disciplines this effect is scoped to (empty = all)
    #[serde(default)]
    pub disciplines: Vec<String>,
    /// Source entity discipline/role filter (empty = no constraint)
    #[serde(default)]
    pub source_disciplines: Vec<String>,
    /// Target entity discipline/role filter (empty = no constraint)
    #[serde(default)]
    pub target_disciplines: Vec<String>,

    // Behavior
    #[serde(default)]
    pub ignore_refreshes: bool,
    #[serde(default)]
    pub refresh_scope: RefreshScope,
    pub persist_past_death: bool,
    pub track_outside_combat: bool,

    // Timer integration
    pub on_apply_trigger_timer: Option<String>,
    pub on_expire_trigger_timer: Option<String>,

    // Alerts
    #[serde(default)]
    pub is_alert: bool,
    pub alert_text: Option<String>,
    pub alert_on: AlertTrigger,
    #[serde(default)]
    pub alert_countdown_secs: Option<f32>,

    // Audio
    pub audio: AudioConfig,

    // Modifiers
    #[serde(default)]
    pub modifiers: Vec<baras_types::EffectModifier>,
}

impl EffectListItem {
    fn from_definition(def: &EffectDefinition, is_user_override: bool, is_bundled: bool) -> Self {
        Self {
            id: def.id.clone(),
            name: def.name.clone(),
            display_text: def.display_text.clone(),
            note: def.note.clone(),
            is_user_override,
            is_bundled,
            enabled: def.enabled,
            trigger: def.trigger.clone(),
            ignore_effect_removed: def.ignore_effect_removed,
            refresh_abilities: def.refresh_abilities.clone(),
            is_aoe_refresh: def.is_aoe_refresh,
            aoe_refresh_immediate: def.aoe_refresh_immediate,
            duration_secs: def.duration_secs,
            is_refreshed_on_modify: def.is_refreshed_on_modify,
            single_instance_per_target: def.single_instance_per_target,
            default_charges: def.default_charges,
            color: def.color,
            show_at_secs: def.show_at_secs,
            display_targets: def.display_targets.clone(),
            icon_ability_id: def.icon_ability_id,
            show_icon: def.show_icon,
            display_source: def.display_source,
            show_target: def.show_target,
            track_uptime: def.track_uptime,
            track_uptime_after_first: def.track_uptime_after_first,
            stack_priority: def.stack_priority,
            show_countdown: def.show_countdown,
            show_single_stack: def.show_single_stack,
            is_affected_by_alacrity: def.is_affected_by_alacrity,
            cooldown_ready_secs: def.cooldown_ready_secs,
            disciplines: def.disciplines.iter().map(|d| d.name().to_string()).collect(),
            source_disciplines: def.source_disciplines.iter().map(|d| d.name().to_string()).collect(),
            target_disciplines: def.target_disciplines.iter().map(|d| d.name().to_string()).collect(),
            ignore_refreshes: def.ignore_refreshes,
            refresh_scope: def.refresh_scope,
            persist_past_death: def.persist_past_death,
            track_outside_combat: def.track_outside_combat,
            on_apply_trigger_timer: def.on_apply_trigger_timer.clone(),
            on_expire_trigger_timer: def.on_expire_trigger_timer.clone(),
            is_alert: def.is_alert,
            alert_text: def.alert_text.clone(),
            alert_on: def.alert_on,
            alert_countdown_secs: def.alert_countdown_secs,
            audio: def.audio.clone(),
            modifiers: def.modifiers.clone(),
        }
    }

    fn to_definition(&self) -> EffectDefinition {
        EffectDefinition {
            id: self.id.clone(),
            name: self.name.clone(),
            display_text: self.display_text.clone(),
            note: self.note.clone(),
            enabled: self.enabled,
            trigger: self.trigger.clone(),
            ignore_effect_removed: self.ignore_effect_removed,
            refresh_abilities: self.refresh_abilities.clone(),
            is_aoe_refresh: self.is_aoe_refresh,
            aoe_refresh_immediate: self.aoe_refresh_immediate,
            duration_secs: self.duration_secs,
            is_refreshed_on_modify: self.is_refreshed_on_modify,
            single_instance_per_target: self.single_instance_per_target,
            default_charges: self.default_charges,
            color: self.color,
            show_at_secs: self.show_at_secs,
            disciplines: self
                .disciplines
                .iter()
                .filter_map(|name| Discipline::from_name(name))
                .collect(),
            source_disciplines: self
                .source_disciplines
                .iter()
                .filter_map(|name| DisciplineFilter::from_name(name))
                .collect(),
            target_disciplines: self
                .target_disciplines
                .iter()
                .filter_map(|name| DisciplineFilter::from_name(name))
                .collect(),
            ignore_refreshes: self.ignore_refreshes,
            refresh_scope: self.refresh_scope,
            persist_past_death: self.persist_past_death,
            track_outside_combat: self.track_outside_combat,
            on_apply_trigger_timer: self.on_apply_trigger_timer.clone(),
            on_expire_trigger_timer: self.on_expire_trigger_timer.clone(),
            is_alert: self.is_alert,
            alert_text: self.alert_text.clone(),
            alert_on: self.alert_on,
            alert_countdown_secs: self.alert_countdown_secs,
            audio: self.audio.clone(),
            display_targets: self.display_targets.clone(),
            icon_ability_id: self.icon_ability_id,
            is_affected_by_alacrity: self.is_affected_by_alacrity,
            cooldown_ready_secs: self.cooldown_ready_secs,
            show_icon: self.show_icon,
            display_source: self.display_source,
            show_target: self.show_target,
            track_uptime: self.track_uptime,
            track_uptime_after_first: self.track_uptime_after_first,
            stack_priority: self.stack_priority,
            show_countdown: self.show_countdown,
            show_single_stack: self.show_single_stack,
            modifiers: self.modifiers.clone(),
        }
    }

    /// Check if this effect has a valid trigger configuration
    fn has_valid_trigger(&self) -> bool {
        match &self.trigger {
            Trigger::EffectApplied { effects, .. } | Trigger::EffectRemoved { effects, .. } => {
                !effects.is_empty() || !self.refresh_abilities.is_empty()
            }
            Trigger::AbilityCast { abilities, .. }
            | Trigger::DamageTaken { abilities, .. }
            | Trigger::HealingTaken { abilities, .. } => !abilities.is_empty() || !self.refresh_abilities.is_empty(),
            _ => false,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper Functions
// ─────────────────────────────────────────────────────────────────────────────

const NO_TRIGGER_ERR: &str = "Effect must have at least one effect ID, ability, or refresh ability to match against. \
    Without these, the effect will never trigger.";

/// Bundled definitions and the user's entries (v1 files are migrated here).
/// A load failure is an error, never an empty set: saving over it would wipe the file.
fn load_effects(app_handle: &AppHandle) -> Result<(DefinitionSet, UserEffects), String> {
    let bundled = load_bundled_effects(app_handle);
    let user = match UserEffects::default_path() {
        Some(p) => UserEffects::load(&p, &bundled).map_err(|e| e.to_string())?,
        None => UserEffects::default(),
    };
    Ok((bundled, user))
}

fn save_user_effects(user: &UserEffects) -> Result<(), String> {
    let path = UserEffects::default_path().ok_or("Cannot determine user effects path")?;
    user.save(&path).map_err(|e| e.to_string())
}

/// A bundled effect only counts as overridden when it differs beyond the toggles
fn list_item(def: &EffectDefinition, bundled: &DefinitionSet) -> EffectListItem {
    match bundled.get(&def.id) {
        Some(b) => EffectListItem::from_definition(def, !eq_ignoring_toggles(def, b), true),
        None => EffectListItem::from_definition(def, true, false),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Get all effect definitions as a flat list
#[tauri::command]
pub async fn get_effect_definitions(app_handle: AppHandle) -> Result<Vec<EffectListItem>, String> {
    let (bundled, user) = load_effects(&app_handle)?;
    Ok(user.apply(&bundled).iter().map(|e| list_item(e, &bundled)).collect())
}

/// Update an existing effect (bundled effects store only the changed fields)
#[tauri::command]
pub async fn update_effect_definition(
    app_handle: AppHandle,
    service: State<'_, ServiceHandle>,
    effect: EffectListItem,
) -> Result<(), String> {
    if !effect.has_valid_trigger() {
        return Err(NO_TRIGGER_ERR.to_string());
    }

    let (bundled, mut user) = load_effects(&app_handle)?;
    user.upsert(&effect.to_definition(), bundled.get(&effect.id));
    save_user_effects(&user)?;

    let _ = service.reload_effect_definitions().await;
    Ok(())
}

/// Set the enabled and / or audio toggle on many effects: one save, one reload
#[tauri::command]
pub async fn set_effect_toggles(
    app_handle: AppHandle,
    service: State<'_, ServiceHandle>,
    effect_ids: Vec<String>,
    enabled: Option<bool>,
    audio: Option<bool>,
) -> Result<(), String> {
    let (bundled, mut user) = load_effects(&app_handle)?;
    let merged = user.apply(&bundled);
    for id in &effect_ids {
        let Some(mut def) = merged.get(id).cloned() else {
            continue;
        };
        if let Some(v) = enabled {
            def.enabled = v;
        }
        if let Some(v) = audio {
            def.audio.enabled = v;
        }
        user.upsert(&def, bundled.get(id));
    }
    save_user_effects(&user)?;

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
    if !effect.has_valid_trigger() {
        return Err(NO_TRIGGER_ERR.to_string());
    }

    if effect.id.is_empty() {
        effect.id = generate_effect_id(&effect.name);
    }

    let (bundled, mut user) = load_effects(&app_handle)?;
    if bundled.get(&effect.id).is_some() || user.contains(&effect.id) {
        return Err(format!("Effect with ID '{}' already exists", effect.id));
    }

    user.upsert(&effect.to_definition(), None);
    save_user_effects(&user)?;
    effect.is_user_override = true;

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
    let (bundled, mut user) = load_effects(&app_handle)?;
    if !user.remove(&effect_id) && bundled.get(&effect_id).is_none() {
        return Err(format!("Effect '{}' not found", effect_id));
    }
    save_user_effects(&user)?;

    let _ = service.reload_effect_definitions().await;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Export/Import Types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectImportDiff {
    pub id: String,
    pub name: String,
    pub display_targets: Vec<DisplayTarget>,
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

/// Export user effects as full (merged) definitions.
/// If `effect_id` is provided, exports only that single effect; otherwise exports all.
#[tauri::command]
pub async fn export_effects_toml(app_handle: AppHandle, effect_ids: Option<Vec<String>>) -> Result<String, String> {
    let (bundled, user) = load_effects(&app_handle)?;
    // No ids: the user's own definitions, minus toggle-only overrides (the list
    // shows those built-ins as unmodified). With ids: those effects as they
    // stand, built-ins included
    let effects: Vec<EffectDefinition> = match &effect_ids {
        None => user
            .resolve(&bundled)
            .into_iter()
            .filter(|def| bundled.get(&def.id).is_none_or(|b| !eq_ignoring_toggles(def, b)))
            .collect(),
        Some(ids) => {
            let merged = user.apply(&bundled);
            ids.iter().filter_map(|id| merged.get(id).cloned()).collect()
        }
    };
    if effects.is_empty() {
        return Err(match effect_ids {
            Some(_) => "No matching effect definitions to export".to_string(),
            None => "No custom effect definitions to export".to_string(),
        });
    }

    let config = DefinitionConfig {
        version: EFFECTS_DSL_VERSION,
        effects,
    };
    toml::to_string_pretty(&config).map_err(|e| format!("Failed to serialize effects: {}", e))
}

/// Exports hold full definitions in every version so far, so any known version imports
fn version_error(version: u32) -> Option<String> {
    (!(1..=EFFECTS_DSL_VERSION).contains(&version)).then(|| {
        format!(
            "Version mismatch: file has version {}, expected 1–{}",
            version, EFFECTS_DSL_VERSION
        )
    })
}

fn parse_import(toml_content: &str) -> Result<DefinitionConfig, String> {
    toml::from_str(toml_content).map_err(|e| format!("Failed to parse TOML: {}", e))
}

/// Preview effects import — parse TOML and diff against existing
#[tauri::command]
pub async fn preview_import_effects(
    app_handle: AppHandle,
    toml_content: String,
) -> Result<EffectImportPreview, String> {
    let config = parse_import(&toml_content)?;
    let mut errors: Vec<String> = version_error(config.version).into_iter().collect();

    if config.effects.is_empty() {
        errors.push("No effects found in file".to_string());
    }

    let (bundled, user) = load_effects(&app_handle)?;
    let current = user.apply(&bundled);
    let imported_ids: std::collections::HashSet<&str> = config.effects.iter().map(|e| e.id.as_str()).collect();

    let (mut effects_to_replace, mut effects_to_add) = (Vec::new(), Vec::new());
    for effect in &config.effects {
        let diff = EffectImportDiff {
            id: effect.id.clone(),
            name: effect.name.clone(),
            display_targets: effect.display_targets.clone(),
        };
        if current.get(&effect.id).is_some() {
            effects_to_replace.push(diff);
        } else {
            effects_to_add.push(diff);
        }
    }

    let effects_unchanged = current.iter().filter(|e| !imported_ids.contains(e.id.as_str())).count();

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
    app_handle: AppHandle,
    service: State<'_, ServiceHandle>,
    toml_content: String,
) -> Result<(), String> {
    let config = parse_import(&toml_content)?;
    if let Some(e) = version_error(config.version) {
        return Err(e);
    }

    let (bundled, mut user) = load_effects(&app_handle)?;
    for imported in &config.effects {
        user.upsert(imported, bundled.get(&imported.id));
    }
    save_user_effects(&user)?;

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
static ICON_NAME_CACHE: std::sync::OnceLock<Mutex<HashMap<u64, String>>> = std::sync::OnceLock::new();

/// Pre-extracted icon PNG data: lowercase filename → PNG bytes
static ICON_PNG_CACHE: std::sync::OnceLock<HashMap<String, Vec<u8>>> = std::sync::OnceLock::new();

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
                if parts.len() >= 3
                    && let Ok(ability_id) = parts[0].parse::<u64>()
                {
                    let icon_name = parts[2].trim().to_string();
                    if !icon_name.is_empty() {
                        map.insert(ability_id, icon_name);
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
                .ancestors()
                .nth(2)
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."))
                .join("icons")
        });

    // Extract all PNGs from zip archives on first call
    let zip_paths = [icons_dir.join("icons.zip"), icons_dir.join("icons2.zip")];
    let cache = ICON_PNG_CACHE.get_or_init(|| {
        let mut map = HashMap::new();
        for zip_path in &zip_paths {
            if let Ok(file) = std::fs::File::open(zip_path) {
                let reader = std::io::BufReader::new(file);
                if let Ok(mut archive) = ZipArchive::new(reader) {
                    for i in 0..archive.len() {
                        if let Ok(mut entry) = archive.by_index(i) {
                            let Ok(name) = entry.name().map(|n| n.to_string()) else {
                                continue;
                            };
                            if name.ends_with(".png") {
                                let mut data = Vec::new();
                                if entry.read_to_end(&mut data).is_ok() {
                                    map.insert(name.to_lowercase(), data);
                                }
                            }
                        }
                    }
                }
            }
        }
        map
    });

    let lookup_key = format!("{}.png", icon_name.to_lowercase());
    if let Some(png_data) = cache.get(&lookup_key) {
        use base64::Engine;
        let base64_data = base64::engine::general_purpose::STANDARD.encode(png_data);
        return Ok(format!("data:image/png;base64,{}", base64_data));
    }

    Err(format!("Icon '{}' not found in ZIP archives", icon_name))
}
