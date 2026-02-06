//! Tauri API client module
//!
//! Provides type-safe wrappers around Tauri invoke() calls, eliminating
//! boilerplate and centralizing all backend communication.

use serde::Serialize;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

use crate::types::{AppConfig, OverlayStatus, OverlayType, SessionInfo};
use crate::utils::js_set;

// ─────────────────────────────────────────────────────────────────────────────
// Raw Tauri Bindings
// ─────────────────────────────────────────────────────────────────────────────

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"], js_name = "listen")]
    pub async fn tauri_listen(event: &str, handler: &Closure<dyn FnMut(JsValue)>) -> JsValue;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "dialog"], js_name = "open")]
    pub async fn open_dialog(options: JsValue) -> JsValue;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "dialog"], js_name = "save")]
    pub async fn save_dialog(options: JsValue) -> JsValue;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "app"], js_name = "getVersion")]
    pub async fn get_version() -> JsValue;
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper Functions
// ─────────────────────────────────────────────────────────────────────────────

/// Build a JsValue object with a single key-value pair
fn build_args<T: Serialize + ?Sized>(key: &str, value: &T) -> JsValue {
    let args = serde_wasm_bindgen::to_value(value).unwrap_or(JsValue::NULL);
    let obj = js_sys::Object::new();
    js_set(&obj, key, &args);
    obj.into()
}

/// Deserialize a JsValue into a type, returning None on failure (no console logging)
fn from_js<T: serde::de::DeserializeOwned>(value: JsValue) -> Option<T> {
    serde_wasm_bindgen::from_value(value).ok()
}

/// Invoke a Tauri command, catching any errors silently.
/// Returns JsValue on success, JsValue::NULL on failure.
/// Use this for read operations where errors can be safely ignored.
async fn invoke(cmd: &str, args: JsValue) -> JsValue {
    try_invoke(cmd, args).await.unwrap_or(JsValue::NULL)
}

/// Invoke a Tauri command that may return an error, catching the rejection.
/// Returns Ok(JsValue) on success, Err(String) on failure.
/// Use this for mutations or when you need to handle/display errors.
async fn try_invoke(cmd: &str, args: JsValue) -> Result<JsValue, String> {
    use js_sys::Promise;
    use wasm_bindgen_futures::JsFuture;

    // Get the invoke function from Tauri
    let window = web_sys::window().ok_or("No window")?;
    let tauri = js_sys::Reflect::get(&window, &JsValue::from_str("__TAURI__"))
        .map_err(|_| "No __TAURI__")?;
    let core = js_sys::Reflect::get(&tauri, &JsValue::from_str("core")).map_err(|_| "No core")?;
    let invoke_fn =
        js_sys::Reflect::get(&core, &JsValue::from_str("invoke")).map_err(|_| "No invoke")?;
    let invoke_fn: js_sys::Function = invoke_fn.dyn_into().map_err(|_| "invoke not a function")?;

    // Call invoke and get the promise
    let promise = invoke_fn
        .call2(&JsValue::NULL, &JsValue::from_str(cmd), &args)
        .map_err(|e| format!("invoke call failed: {:?}", e))?;
    let promise: Promise = promise.dyn_into().map_err(|_| "not a promise")?;

    // Await the promise, catching rejections
    JsFuture::from(promise).await.map_err(|e| {
        // Extract error message from JsValue
        e.as_string().unwrap_or_else(|| format!("{:?}", e))
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Config Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Get the current application configuration
pub async fn get_config() -> Option<AppConfig> {
    let result = invoke("get_config", JsValue::NULL).await;
    from_js(result)
}

/// Update the application configuration
pub async fn update_config(config: &AppConfig) -> Result<(), String> {
    try_invoke("update_config", build_args("config", config)).await?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Get current overlay status (running, enabled, modes)
pub async fn get_overlay_status() -> Option<OverlayStatus> {
    let result = invoke("get_overlay_status", JsValue::NULL).await;
    from_js(result)
}

/// Show an overlay (enable + spawn if visible)
pub async fn show_overlay(kind: OverlayType) -> bool {
    let result = invoke("show_overlay", build_args("kind", &kind)).await;
    result.as_bool().unwrap_or(false)
}

/// Hide an overlay (disable + shutdown if running)
pub async fn hide_overlay(kind: OverlayType) -> bool {
    let result = invoke("hide_overlay", build_args("kind", &kind)).await;
    result.as_bool().unwrap_or(false)
}

/// Toggle an overlay's enabled state
pub async fn toggle_overlay(kind: OverlayType, currently_enabled: bool) -> bool {
    if currently_enabled {
        hide_overlay(kind).await
    } else {
        show_overlay(kind).await
    }
}

/// Show all enabled overlays
pub async fn show_all_overlays() -> bool {
    let result = invoke("show_all_overlays", JsValue::NULL).await;
    result.as_bool().unwrap_or(false) || result.is_array()
}

/// Hide all running overlays
pub async fn hide_all_overlays() -> bool {
    let result = invoke("hide_all_overlays", JsValue::NULL).await;
    result.as_bool().unwrap_or(false)
}

/// Toggle visibility of all overlays
pub async fn toggle_visibility(currently_visible: bool) -> bool {
    if currently_visible {
        hide_all_overlays().await
    } else {
        show_all_overlays().await
    }
}

/// Toggle move mode for all overlays
pub async fn toggle_move_mode() -> Result<bool, String> {
    let result = invoke("toggle_move_mode", JsValue::NULL).await;
    if let Some(new_mode) = result.as_bool() {
        Ok(new_mode)
    } else if let Some(err) = result.as_string() {
        Err(err)
    } else {
        Err("Unknown error".to_string())
    }
}

/// Toggle raid rearrange mode
pub async fn toggle_raid_rearrange() -> Result<bool, String> {
    let result = invoke("toggle_raid_rearrange", JsValue::NULL).await;
    if let Some(new_mode) = result.as_bool() {
        Ok(new_mode)
    } else if let Some(err) = result.as_string() {
        Err(err)
    } else {
        Err("Unknown error".to_string())
    }
}

/// Refresh overlay settings for all running overlays
pub async fn refresh_overlay_settings() -> bool {
    let result = invoke("refresh_overlay_settings", JsValue::NULL).await;
    result.as_bool().unwrap_or(false)
}

/// Preview overlay settings without persisting (for live preview)
pub async fn preview_overlay_settings(settings: &crate::types::OverlaySettings) -> bool {
    let result = invoke("preview_overlay_settings", build_args("settings", settings)).await;
    result.as_bool().unwrap_or(false)
}

/// Clear all players from raid registry
pub async fn clear_raid_registry() {
    let _ = invoke("clear_raid_registry", JsValue::NULL).await;
}

// ─────────────────────────────────────────────────────────────────────────────
// Session Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Get current session info
pub async fn get_session_info() -> Option<SessionInfo> {
    let result = invoke("get_session_info", JsValue::NULL).await;
    from_js(result)
}

/// Get currently active file path
pub async fn get_active_file() -> Option<String> {
    let result = invoke("get_active_file", JsValue::NULL).await;
    result.as_string()
}

/// Check if directory watcher is active
pub async fn get_watching_status() -> bool {
    let result = invoke("get_watching_status", JsValue::NULL).await;
    from_js(result).unwrap_or(false)
}

/// Restart the directory watcher
pub async fn restart_watcher() {
    let _ = invoke("restart_watcher", JsValue::NULL).await;
}

/// Refresh the log file index (rebuilds from disk)
pub async fn refresh_log_index() {
    let _ = invoke("refresh_log_index", JsValue::NULL).await;
}

// ─────────────────────────────────────────────────────────────────────────────
// Boss Notes Selection
// ─────────────────────────────────────────────────────────────────────────────

/// Boss info for notes selector
#[derive(Debug, Clone, serde::Deserialize)]
pub struct BossNotesInfo {
    pub id: String,
    pub name: String,
    pub has_notes: bool,
}

/// Get list of bosses with notes status for the current area
pub async fn get_area_bosses_for_notes() -> Vec<BossNotesInfo> {
    let result = invoke("get_area_bosses_for_notes", JsValue::NULL).await;
    from_js(result).unwrap_or_default()
}

/// Send notes for a specific boss to the overlay
pub async fn select_boss_notes(boss_id: &str) -> Result<(), String> {
    let args = js_sys::Object::new();
    js_set(&args, "bossId", &JsValue::from_str(boss_id));
    let result = invoke("select_boss_notes", args.into()).await;
    if result.is_null() || result.is_undefined() {
        Ok(())
    } else if let Some(err) = result.as_string() {
        Err(err)
    } else {
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Log Management Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Get total size of all log files in bytes
pub async fn get_log_directory_size() -> u64 {
    let result = invoke("get_log_directory_size", JsValue::NULL).await;
    from_js(result).unwrap_or(0)
}

/// Get count of log files
pub async fn get_log_file_count() -> usize {
    let result = invoke("get_log_file_count", JsValue::NULL).await;
    from_js(result).unwrap_or(0)
}

/// Get list of all log files with metadata
pub async fn get_log_files() -> JsValue {
    invoke("get_log_files", JsValue::NULL).await
}

/// Clean up log files. Returns (empty_deleted, old_deleted).
pub async fn cleanup_logs(delete_empty: bool, retention_days: Option<u32>) -> (u32, u32) {
    let args = js_sys::Object::new();
    js_set(&args, "deleteEmpty", &JsValue::from_bool(delete_empty));
    if let Some(days) = retention_days {
        js_set(&args, "retentionDays", &JsValue::from_f64(days as f64));
    } else {
        js_set(&args, "retentionDays", &JsValue::NULL);
    }
    let result = invoke("cleanup_logs", args.into()).await;
    from_js(result).unwrap_or((0, 0))
}

/// Refresh file sizes in the directory index (fast stat-only)
pub async fn refresh_file_sizes() {
    let _ = invoke("refresh_file_sizes", JsValue::NULL).await;
}

// ─────────────────────────────────────────────────────────────────────────────
// File Browser Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Open a historical log file (pauses live tailing)
pub async fn open_historical_file(path: &str) -> Result<(), String> {
    try_invoke("open_historical_file", build_args("path", &path)).await?;
    Ok(())
}

/// Resume live tailing mode
pub async fn resume_live_tailing() -> Result<(), String> {
    try_invoke("resume_live_tailing", JsValue::NULL).await?;
    Ok(())
}

/// Check if in live tailing mode
pub async fn is_live_tailing() -> bool {
    let result = invoke("is_live_tailing", JsValue::NULL).await;
    from_js(result).unwrap_or(true)
}

// ─────────────────────────────────────────────────────────────────────────────
// Profile Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Get list of profile names
pub async fn get_profile_names() -> Vec<String> {
    let result = invoke("get_profile_names", JsValue::NULL).await;
    from_js(result).unwrap_or_default()
}

/// Get currently active profile name
pub async fn get_active_profile() -> Option<String> {
    let result = invoke("get_active_profile", JsValue::NULL).await;
    from_js(result).unwrap_or(None)
}

/// Save current settings to a profile
pub async fn save_profile(name: &str) -> Result<(), String> {
    try_invoke("save_profile", build_args("name", &name)).await?;
    Ok(())
}

/// Load a profile by name
pub async fn load_profile(name: &str) -> Result<(), String> {
    try_invoke("load_profile", build_args("name", &name)).await?;
    Ok(())
}

/// Delete a profile by name
pub async fn delete_profile(name: &str) -> Result<(), String> {
    try_invoke("delete_profile", build_args("name", &name)).await?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Dialog Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Open a directory picker dialog
pub async fn pick_directory(title: &str) -> Option<String> {
    let options = js_sys::Object::new();
    js_set(&options, "directory", &JsValue::TRUE);
    js_set(&options, "title", &JsValue::from_str(title));

    let result = open_dialog(options.into()).await;
    result.as_string()
}

// ─────────────────────────────────────────────────────────────────────────────
// App Info
// ─────────────────────────────────────────────────────────────────────────────

/// Get the app version from tauri.conf.json
pub async fn get_app_version() -> String {
    get_version().await.as_string().unwrap_or_default()
}

// ─────────────────────────────────────────────────────────────────────────────
// URL Opening
// ─────────────────────────────────────────────────────────────────────────────

/// Open a URL in the default browser
///
/// On Linux, uses XDG Desktop Portal for better compatibility with immutable distros.
/// Falls back to tauri-plugin-opener on other platforms or if portal fails.
pub async fn open_url(url: &str) {
    let _ = try_invoke("open_url", build_args("url", &url)).await;
}

// ─────────────────────────────────────────────────────────────────────────────
// Encounter History
// ─────────────────────────────────────────────────────────────────────────────

/// Get encounter history summaries
pub async fn get_encounter_history()
-> Option<Vec<crate::components::history_panel::EncounterSummary>> {
    let result = invoke("get_encounter_history", JsValue::NULL).await;
    from_js(result)
}

// ─────────────────────────────────────────────────────────────────────────────
// Unified Encounter Item Commands (NEW - replaces type-specific commands)
// ─────────────────────────────────────────────────────────────────────────────

use crate::types::{BossWithPath, EncounterItem};

/// Fetch all bosses for an area file with full encounter data
pub async fn fetch_area_bosses(file_path: &str) -> Option<Vec<BossWithPath>> {
    let args = build_args("filePath", file_path);
    let result = invoke("fetch_area_bosses", args).await;
    from_js(result)
}

/// Create a new encounter item (timer, phase, counter, challenge, or entity)
pub async fn create_encounter_item(
    boss_id: &str,
    file_path: &str,
    item: &EncounterItem,
) -> Result<EncounterItem, String> {
    let obj = js_sys::Object::new();
    js_set(&obj, "bossId", &JsValue::from_str(boss_id));
    js_set(&obj, "filePath", &JsValue::from_str(file_path));
    let item_js = serde_wasm_bindgen::to_value(item).unwrap_or(JsValue::NULL);
    js_set(&obj, "item", &item_js);

    let result = try_invoke("create_encounter_item", obj.into()).await?;
    from_js(result).ok_or_else(|| "Failed to deserialize created item".to_string())
}

/// Update an existing encounter item
pub async fn update_encounter_item(
    boss_id: &str,
    file_path: &str,
    item: &EncounterItem,
    original_id: Option<&str>,
) -> Result<EncounterItem, String> {
    let obj = js_sys::Object::new();
    js_set(&obj, "bossId", &JsValue::from_str(boss_id));
    js_set(&obj, "filePath", &JsValue::from_str(file_path));
    let item_js = serde_wasm_bindgen::to_value(item).unwrap_or(JsValue::NULL);
    js_set(&obj, "item", &item_js);
    if let Some(orig) = original_id {
        js_set(&obj, "originalId", &JsValue::from_str(orig));
    } else {
        js_set(&obj, "originalId", &JsValue::NULL);
    }

    let result = try_invoke("update_encounter_item", obj.into()).await?;
    from_js(result).ok_or_else(|| "Failed to deserialize updated item".to_string())
}

/// Delete an encounter item
pub async fn delete_encounter_item(
    item_type: &str,
    item_id: &str,
    boss_id: &str,
    file_path: &str,
) -> Result<(), String> {
    let obj = js_sys::Object::new();
    js_set(&obj, "itemType", &JsValue::from_str(item_type));
    js_set(&obj, "itemId", &JsValue::from_str(item_id));
    js_set(&obj, "bossId", &JsValue::from_str(boss_id));
    js_set(&obj, "filePath", &JsValue::from_str(file_path));

    try_invoke("delete_encounter_item", obj.into()).await?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Encounter Editor Commands
// ─────────────────────────────────────────────────────────────────────────────

use crate::types::{AreaListItem, BossTimerDefinition};

/// Duplicate a timer (returns DSL type, backend generates new ID)
pub async fn duplicate_encounter_timer(
    timer_id: &str,
    boss_id: &str,
    file_path: &str,
) -> Result<BossTimerDefinition, String> {
    let obj = js_sys::Object::new();
    js_set(&obj, "timerId", &JsValue::from_str(timer_id));
    js_set(&obj, "bossId", &JsValue::from_str(boss_id));
    js_set(&obj, "filePath", &JsValue::from_str(file_path));

    let result = try_invoke("duplicate_encounter_timer", obj.into()).await?;
    from_js(result).ok_or_else(|| "Failed to parse timer response".to_string())
}

/// Get area index for lazy-loading timer editor
pub async fn get_area_index() -> Option<Vec<AreaListItem>> {
    let result = invoke("get_area_index", JsValue::NULL).await;
    from_js(result)
}

use crate::types::{BossEditItem, NewAreaRequest};

/// Create a new boss in an area file
pub async fn create_boss(boss: &BossEditItem) -> Result<BossEditItem, String> {
    let args = build_args("boss", boss);
    let result = try_invoke("create_boss", args).await?;
    from_js(result).ok_or_else(|| "Failed to parse boss response".to_string())
}

/// Create a new area file
pub async fn create_area(area: &NewAreaRequest) -> Result<String, String> {
    let args = build_args("area", area);
    let result = try_invoke("create_area", args).await?;
    from_js(result).ok_or_else(|| "Failed to parse area response".to_string())
}

// ─────────────────────────────────────────────────────────────────────────────
// Encounter Export/Import
// ─────────────────────────────────────────────────────────────────────────────

use crate::types::{ExportResult, ImportPreview};

/// Export encounter definition(s) — returns TOML content and whether source is bundled
pub async fn export_encounter_toml(
    boss_id: Option<&str>,
    file_path: &str,
) -> Result<ExportResult, String> {
    let obj = js_sys::Object::new();
    match boss_id {
        Some(id) => js_set(&obj, "bossId", &JsValue::from_str(id)),
        None => js_set(&obj, "bossId", &JsValue::NULL),
    }
    js_set(&obj, "filePath", &JsValue::from_str(file_path));
    let result = try_invoke("export_encounter_toml", obj.into()).await?;
    from_js(result).ok_or_else(|| "Failed to parse export result".to_string())
}

/// Save exported content to a file path
pub async fn save_export_file(path: &str, content: &str) -> Result<(), String> {
    let obj = js_sys::Object::new();
    js_set(&obj, "path", &JsValue::from_str(path));
    js_set(&obj, "content", &JsValue::from_str(content));
    try_invoke("save_export_file", obj.into()).await?;
    Ok(())
}

/// Preview an import (parse + diff against target area)
pub async fn preview_import_encounter(
    toml_content: &str,
    target_file_path: Option<&str>,
) -> Result<ImportPreview, String> {
    let obj = js_sys::Object::new();
    js_set(&obj, "tomlContent", &JsValue::from_str(toml_content));
    match target_file_path {
        Some(p) => js_set(&obj, "targetFilePath", &JsValue::from_str(p)),
        None => js_set(&obj, "targetFilePath", &JsValue::NULL),
    }
    let result = try_invoke("preview_import_encounter", obj.into()).await?;
    from_js(result).ok_or_else(|| "Failed to parse import preview".to_string())
}

/// Execute an import (merge into target area)
pub async fn import_encounter_toml(
    toml_content: &str,
    target_file_path: Option<&str>,
) -> Result<(), String> {
    let obj = js_sys::Object::new();
    js_set(&obj, "tomlContent", &JsValue::from_str(toml_content));
    match target_file_path {
        Some(p) => js_set(&obj, "targetFilePath", &JsValue::from_str(p)),
        None => js_set(&obj, "targetFilePath", &JsValue::NULL),
    }
    try_invoke("import_encounter_toml", obj.into()).await?;
    Ok(())
}

/// Open a native save dialog, returns the selected file path or None
pub async fn save_file_dialog(default_name: &str) -> Option<String> {
    let options = js_sys::Object::new();
    js_set(&options, "defaultPath", &JsValue::from_str(default_name));

    // Add .toml filter
    let filter = js_sys::Object::new();
    js_set(&filter, "name", &JsValue::from_str("TOML files"));
    let exts = js_sys::Array::new();
    exts.push(&JsValue::from_str("toml"));
    js_set(&filter, "extensions", &exts);
    let filters = js_sys::Array::new();
    filters.push(&filter);
    js_set(&options, "filters", &filters);

    let result = save_dialog(options.into()).await;
    result.as_string()
}

/// Open a native file dialog for .toml files, returns the file path or None
pub async fn open_toml_file_dialog() -> Option<String> {
    let options = js_sys::Object::new();

    let filter = js_sys::Object::new();
    js_set(&filter, "name", &JsValue::from_str("TOML files"));
    let exts = js_sys::Array::new();
    exts.push(&JsValue::from_str("toml"));
    js_set(&filter, "extensions", &exts);
    let filters = js_sys::Array::new();
    filters.push(&filter);
    js_set(&options, "filters", &filters);

    let result = open_dialog(options.into()).await;
    result.as_string()
}

/// Read a file's text content via backend
pub async fn read_import_file(path: &str) -> Result<String, String> {
    let result = try_invoke("read_import_file", build_args("path", &path)).await?;
    result.as_string().ok_or_else(|| "Failed to read file".to_string())
}

/// Update boss notes
pub async fn update_boss_notes(
    boss_id: &str,
    file_path: &str,
    notes: Option<String>,
) -> Result<(), String> {
    let obj = js_sys::Object::new();
    js_set(&obj, "bossId", &JsValue::from_str(boss_id));
    js_set(&obj, "filePath", &JsValue::from_str(file_path));
    match notes {
        Some(ref n) => js_set(&obj, "notes", &JsValue::from_str(n)),
        None => js_set(&obj, "notes", &JsValue::NULL),
    }
    try_invoke("update_boss_notes", obj.into()).await?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Effect Editor Commands
// ─────────────────────────────────────────────────────────────────────────────

use crate::types::EffectListItem;

/// Get all effect definitions as a flat list
pub async fn get_effect_definitions() -> Option<Vec<EffectListItem>> {
    let result = invoke("get_effect_definitions", JsValue::NULL).await;
    from_js(result)
}

/// Update an existing effect
/// Returns Ok(()) on success, Err with message on failure.
pub async fn update_effect_definition(effect: &EffectListItem) -> Result<(), String> {
    let args = build_args("effect", effect);
    try_invoke("update_effect_definition", args).await?;
    Ok(())
}

/// Delete an effect
/// Returns Ok(()) on success, Err with message on failure.
pub async fn delete_effect_definition(effect_id: &str) -> Result<(), String> {
    let args = build_args("effectId", effect_id);
    try_invoke("delete_effect_definition", args).await?;
    Ok(())
}

/// Duplicate an effect
pub async fn duplicate_effect_definition(effect_id: &str) -> Result<EffectListItem, String> {
    let args = build_args("effectId", effect_id);
    let result = try_invoke("duplicate_effect_definition", args).await?;
    from_js(result).ok_or_else(|| "Failed to parse effect response".to_string())
}

/// Create a new effect
/// Returns Ok(created effect) on success, Err(message) on failure (e.g., validation error)
pub async fn create_effect_definition(effect: &EffectListItem) -> Result<EffectListItem, String> {
    let args = build_args("effect", effect);
    let result = try_invoke("create_effect_definition", args).await?;
    from_js(result).ok_or_else(|| "Failed to deserialize created effect".to_string())
}

// ─────────────────────────────────────────────────────────────────────────────
// Effect Export/Import
// ─────────────────────────────────────────────────────────────────────────────

use crate::types::EffectImportPreview;

/// Export user effect overrides as TOML string
pub async fn export_effects_toml() -> Result<String, String> {
    let result = try_invoke("export_effects_toml", JsValue::NULL).await?;
    result
        .as_string()
        .ok_or_else(|| "Failed to get export content".to_string())
}

/// Preview effects import — parse TOML and diff against existing
pub async fn preview_import_effects(toml_content: &str) -> Result<EffectImportPreview, String> {
    let result = try_invoke(
        "preview_import_effects",
        build_args("tomlContent", &toml_content),
    )
    .await?;
    from_js(result).ok_or_else(|| "Failed to parse import preview".to_string())
}

/// Import effects from TOML content, merging into user file
pub async fn import_effects_toml(toml_content: &str) -> Result<(), String> {
    try_invoke(
        "import_effects_toml",
        build_args("tomlContent", &toml_content),
    )
    .await?;
    Ok(())
}

/// Get icon preview as base64 data URL for an ability ID.
/// Returns None if the icon is not found (graceful fallback).
pub async fn get_icon_preview(ability_id: u64) -> Option<String> {
    match try_invoke("get_icon_preview", build_args("abilityId", &ability_id)).await {
        Ok(result) => from_js(result),
        Err(_) => None, // Icon not found - graceful fallback
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Parsely Upload
// ─────────────────────────────────────────────────────────────────────────────

/// Response from Parsely upload
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ParselyUploadResponse {
    pub success: bool,
    pub link: Option<String>,
    pub error: Option<String>,
}

/// Upload a log file to Parsely.io
pub async fn upload_to_parsely(
    path: &str,
    visibility: u8,
    notes: Option<String>,
) -> Result<ParselyUploadResponse, String> {
    let obj = js_sys::Object::new();
    js_set(&obj, "path", &JsValue::from_str(path));
    js_set(&obj, "visibility", &JsValue::from_f64(visibility as f64));
    if let Some(note) = notes {
        js_set(&obj, "notes", &JsValue::from_str(&note));
    } else {
        js_set(&obj, "notes", &JsValue::NULL);
    }
    let result = try_invoke("upload_to_parsely", obj.into()).await?;
    from_js(result).ok_or_else(|| "Failed to parse upload response".to_string())
}

/// Upload a specific encounter (line range) to Parsely.io
pub async fn upload_encounter_to_parsely(
    path: &str,
    start_line: u64,
    end_line: u64,
    area_entered_line: Option<u64>,
    visibility: u8,
    notes: Option<String>,
) -> Result<ParselyUploadResponse, String> {
    let obj = js_sys::Object::new();
    js_set(&obj, "path", &JsValue::from_str(path));
    js_set(&obj, "startLine", &JsValue::from_f64(start_line as f64));
    js_set(&obj, "endLine", &JsValue::from_f64(end_line as f64));
    if let Some(line) = area_entered_line {
        js_set(&obj, "areaEnteredLine", &JsValue::from_f64(line as f64));
    } else {
        js_set(&obj, "areaEnteredLine", &JsValue::NULL);
    }
    js_set(&obj, "visibility", &JsValue::from_f64(visibility as f64));
    if let Some(note) = notes {
        js_set(&obj, "notes", &JsValue::from_str(&note));
    } else {
        js_set(&obj, "notes", &JsValue::NULL);
    }
    let result = try_invoke("upload_encounter_to_parsely", obj.into()).await?;
    from_js(result).ok_or_else(|| "Failed to parse upload response".to_string())
}

/// Set the Parsely link for an encounter (persists in backend)
pub async fn set_encounter_parsely_link(encounter_id: u64, link: &str) -> Result<bool, String> {
    let obj = js_sys::Object::new();
    js_set(&obj, "encounterId", &JsValue::from_f64(encounter_id as f64));
    js_set(&obj, "link", &JsValue::from_str(link));
    let result = try_invoke("set_encounter_parsely_link", obj.into()).await?;
    from_js(result).ok_or_else(|| "Failed to parse response".to_string())
}

// ─────────────────────────────────────────────────────────────────────────────
// Audio File Picker
// ─────────────────────────────────────────────────────────────────────────────

/// Open a file picker for audio files, returns the selected path or None
pub async fn pick_audio_file() -> Option<String> {
    let result = invoke("pick_audio_file", JsValue::NULL).await;
    from_js(result).unwrap_or(None)
}

/// Open a folder picker for the log directory, returns the selected path or None.
/// This is handled on the Rust side to maintain macOS security-scoped access.
pub async fn pick_log_directory() -> Option<String> {
    let result = invoke("pick_log_directory", JsValue::NULL).await;
    from_js(result).unwrap_or(None)
}

// ─────────────────────────────────────────────────────────────────────────────
// Updater Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Install available update (downloads, installs, restarts app)
pub async fn install_update() -> Result<(), String> {
    try_invoke("install_update", JsValue::NULL).await?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Query Commands (Data Explorer)
// ─────────────────────────────────────────────────────────────────────────────

// Re-export query types from shared types crate
pub use baras_types::{
    AbilityBreakdown, BreakdownMode, CombatLogFilters, CombatLogFindMatch, CombatLogRow, DataTab,
    EffectChartData, EffectWindow, EncounterTimeline, EntityBreakdown, GroupedEntityNames,
    PhaseSegment, PlayerDeath, RaidOverviewRow, TimeRange, TimeSeriesPoint,
};

/// Query ability breakdown for an encounter and data tab.
/// Pass encounter_idx for historical, or None for live encounter.
/// entity_types filters by entity type (e.g., ["Player", "Companion"]).
/// breakdown_mode controls grouping (by ability, target type, target instance).
/// duration_secs is used for rate calculation (DPS/HPS/etc.).
pub async fn query_breakdown(
    tab: DataTab,
    encounter_idx: Option<u32>,
    entity_name: Option<&str>,
    time_range: Option<&TimeRange>,
    entity_types: Option<&[&str]>,
    breakdown_mode: Option<&BreakdownMode>,
    duration_secs: Option<f32>,
) -> Option<Vec<AbilityBreakdown>> {
    let obj = js_sys::Object::new();
    let tab_js = serde_wasm_bindgen::to_value(&tab).unwrap_or(JsValue::NULL);
    js_set(&obj, "tab", &tab_js);
    if let Some(idx) = encounter_idx {
        js_set(&obj, "encounterIdx", &JsValue::from_f64(idx as f64));
    } else {
        js_set(&obj, "encounterIdx", &JsValue::NULL);
    }
    if let Some(name) = entity_name {
        js_set(&obj, "entityName", &JsValue::from_str(name));
    } else {
        js_set(&obj, "entityName", &JsValue::NULL);
    }
    if let Some(tr) = time_range {
        let tr_js = serde_wasm_bindgen::to_value(tr).unwrap_or(JsValue::NULL);
        js_set(&obj, "timeRange", &tr_js);
    } else {
        js_set(&obj, "timeRange", &JsValue::NULL);
    }
    if let Some(types) = entity_types {
        let types_js = serde_wasm_bindgen::to_value(types).unwrap_or(JsValue::NULL);
        js_set(&obj, "entityTypes", &types_js);
    } else {
        js_set(&obj, "entityTypes", &JsValue::NULL);
    }
    if let Some(mode) = breakdown_mode {
        let mode_js = serde_wasm_bindgen::to_value(mode).unwrap_or(JsValue::NULL);
        js_set(&obj, "breakdownMode", &mode_js);
    } else {
        js_set(&obj, "breakdownMode", &JsValue::NULL);
    }
    if let Some(dur) = duration_secs {
        js_set(&obj, "durationSecs", &JsValue::from_f64(dur as f64));
    } else {
        js_set(&obj, "durationSecs", &JsValue::NULL);
    }
    let result = invoke("query_breakdown", obj.into()).await;
    from_js(result)
}

/// Query breakdown by entity for a data tab.
pub async fn query_entity_breakdown(
    tab: DataTab,
    encounter_idx: Option<u32>,
    time_range: Option<&TimeRange>,
) -> Option<Vec<EntityBreakdown>> {
    let obj = js_sys::Object::new();
    let tab_js = serde_wasm_bindgen::to_value(&tab).unwrap_or(JsValue::NULL);
    js_set(&obj, "tab", &tab_js);
    if let Some(idx) = encounter_idx {
        js_set(&obj, "encounterIdx", &JsValue::from_f64(idx as f64));
    } else {
        js_set(&obj, "encounterIdx", &JsValue::NULL);
    }
    if let Some(tr) = time_range {
        let tr_js = serde_wasm_bindgen::to_value(tr).unwrap_or(JsValue::NULL);
        js_set(&obj, "timeRange", &tr_js);
    } else {
        js_set(&obj, "timeRange", &JsValue::NULL);
    }
    let result = invoke("query_entity_breakdown", obj.into()).await;
    from_js(result)
}

/// Query raid overview - aggregated stats per player.
pub async fn query_raid_overview(
    encounter_idx: Option<u32>,
    time_range: Option<&TimeRange>,
    duration_secs: Option<f32>,
) -> Option<Vec<RaidOverviewRow>> {
    let obj = js_sys::Object::new();
    if let Some(idx) = encounter_idx {
        js_set(&obj, "encounterIdx", &JsValue::from_f64(idx as f64));
    } else {
        js_set(&obj, "encounterIdx", &JsValue::NULL);
    }
    if let Some(tr) = time_range {
        let tr_js = serde_wasm_bindgen::to_value(tr).unwrap_or(JsValue::NULL);
        js_set(&obj, "timeRange", &tr_js);
    } else {
        js_set(&obj, "timeRange", &JsValue::NULL);
    }
    if let Some(dur) = duration_secs {
        js_set(&obj, "durationSecs", &JsValue::from_f64(dur as f64));
    } else {
        js_set(&obj, "durationSecs", &JsValue::NULL);
    }
    let result = invoke("query_raid_overview", obj.into()).await;
    from_js(result)
}

/// Query DPS over time with specified bucket size.
pub async fn query_dps_over_time(
    encounter_idx: Option<u32>,
    bucket_ms: i64,
    source_name: Option<&str>,
    time_range: Option<&TimeRange>,
) -> Option<Vec<TimeSeriesPoint>> {
    let obj = js_sys::Object::new();
    if let Some(idx) = encounter_idx {
        js_set(&obj, "encounterIdx", &JsValue::from_f64(idx as f64));
    } else {
        js_set(&obj, "encounterIdx", &JsValue::NULL);
    }
    js_set(&obj, "bucketMs", &JsValue::from_f64(bucket_ms as f64));
    if let Some(name) = source_name {
        js_set(&obj, "sourceName", &JsValue::from_str(name));
    } else {
        js_set(&obj, "sourceName", &JsValue::NULL);
    }
    if let Some(tr) = time_range {
        let tr_js = serde_wasm_bindgen::to_value(tr).unwrap_or(JsValue::NULL);
        js_set(&obj, "timeRange", &tr_js);
    } else {
        js_set(&obj, "timeRange", &JsValue::NULL);
    }
    let result = invoke("query_dps_over_time", obj.into()).await;
    from_js(result)
}

/// Query encounter timeline with phase segments.
pub async fn query_encounter_timeline(encounter_idx: Option<u32>) -> Option<EncounterTimeline> {
    let obj = js_sys::Object::new();
    if let Some(idx) = encounter_idx {
        js_set(&obj, "encounterIdx", &JsValue::from_f64(idx as f64));
    } else {
        js_set(&obj, "encounterIdx", &JsValue::NULL);
    }
    let result = invoke("query_encounter_timeline", obj.into()).await;
    from_js(result)
}

/// Query HPS over time with specified bucket size.
pub async fn query_hps_over_time(
    encounter_idx: Option<u32>,
    bucket_ms: i64,
    source_name: Option<&str>,
    time_range: Option<&TimeRange>,
) -> Option<Vec<TimeSeriesPoint>> {
    let obj = js_sys::Object::new();
    if let Some(idx) = encounter_idx {
        js_set(&obj, "encounterIdx", &JsValue::from_f64(idx as f64));
    } else {
        js_set(&obj, "encounterIdx", &JsValue::NULL);
    }
    js_set(&obj, "bucketMs", &JsValue::from_f64(bucket_ms as f64));
    if let Some(name) = source_name {
        js_set(&obj, "sourceName", &JsValue::from_str(name));
    } else {
        js_set(&obj, "sourceName", &JsValue::NULL);
    }
    if let Some(tr) = time_range {
        let tr_js = serde_wasm_bindgen::to_value(tr).unwrap_or(JsValue::NULL);
        js_set(&obj, "timeRange", &tr_js);
    } else {
        js_set(&obj, "timeRange", &JsValue::NULL);
    }
    let result = invoke("query_hps_over_time", obj.into()).await;
    from_js(result)
}

/// Query DTPS over time with specified bucket size.
pub async fn query_dtps_over_time(
    encounter_idx: Option<u32>,
    bucket_ms: i64,
    target_name: Option<&str>,
    time_range: Option<&TimeRange>,
) -> Option<Vec<TimeSeriesPoint>> {
    let obj = js_sys::Object::new();
    if let Some(idx) = encounter_idx {
        js_set(&obj, "encounterIdx", &JsValue::from_f64(idx as f64));
    } else {
        js_set(&obj, "encounterIdx", &JsValue::NULL);
    }
    js_set(&obj, "bucketMs", &JsValue::from_f64(bucket_ms as f64));
    if let Some(name) = target_name {
        js_set(&obj, "targetName", &JsValue::from_str(name));
    } else {
        js_set(&obj, "targetName", &JsValue::NULL);
    }
    if let Some(tr) = time_range {
        let tr_js = serde_wasm_bindgen::to_value(tr).unwrap_or(JsValue::NULL);
        js_set(&obj, "timeRange", &tr_js);
    } else {
        js_set(&obj, "timeRange", &JsValue::NULL);
    }
    let result = invoke("query_dtps_over_time", obj.into()).await;
    from_js(result)
}

/// Query effect uptime statistics for charts panel.
pub async fn query_effect_uptime(
    encounter_idx: Option<u32>,
    target_name: Option<&str>,
    time_range: Option<&TimeRange>,
    duration_secs: f32,
) -> Option<Vec<EffectChartData>> {
    let obj = js_sys::Object::new();
    if let Some(idx) = encounter_idx {
        js_set(&obj, "encounterIdx", &JsValue::from_f64(idx as f64));
    } else {
        js_set(&obj, "encounterIdx", &JsValue::NULL);
    }
    if let Some(name) = target_name {
        js_set(&obj, "targetName", &JsValue::from_str(name));
    } else {
        js_set(&obj, "targetName", &JsValue::NULL);
    }
    if let Some(tr) = time_range {
        let tr_js = serde_wasm_bindgen::to_value(tr).unwrap_or(JsValue::NULL);
        js_set(&obj, "timeRange", &tr_js);
    } else {
        js_set(&obj, "timeRange", &JsValue::NULL);
    }
    js_set(
        &obj,
        "durationSecs",
        &JsValue::from_f64(duration_secs as f64),
    );
    let result = invoke("query_effect_uptime", obj.into()).await;
    from_js(result)
}

/// Query individual time windows for a specific effect.
pub async fn query_effect_windows(
    encounter_idx: Option<u32>,
    effect_id: i64,
    target_name: Option<&str>,
    time_range: Option<&TimeRange>,
    duration_secs: f32,
) -> Option<Vec<EffectWindow>> {
    let obj = js_sys::Object::new();
    if let Some(idx) = encounter_idx {
        js_set(&obj, "encounterIdx", &JsValue::from_f64(idx as f64));
    } else {
        js_set(&obj, "encounterIdx", &JsValue::NULL);
    }
    js_set(&obj, "effectId", &JsValue::from_f64(effect_id as f64));
    if let Some(name) = target_name {
        js_set(&obj, "targetName", &JsValue::from_str(name));
    } else {
        js_set(&obj, "targetName", &JsValue::NULL);
    }
    if let Some(tr) = time_range {
        let tr_js = serde_wasm_bindgen::to_value(tr).unwrap_or(JsValue::NULL);
        js_set(&obj, "timeRange", &tr_js);
    } else {
        js_set(&obj, "timeRange", &JsValue::NULL);
    }
    js_set(
        &obj,
        "durationSecs",
        &JsValue::from_f64(duration_secs as f64),
    );
    let result = invoke("query_effect_windows", obj.into()).await;
    from_js(result)
}

/// Query combat log rows with pagination for virtual scrolling.
pub async fn query_combat_log(
    encounter_idx: Option<u32>,
    offset: u64,
    limit: u64,
    source_filter: Option<&str>,
    target_filter: Option<&str>,
    search_filter: Option<&str>,
    time_range: Option<&TimeRange>,
    event_filters: Option<&CombatLogFilters>,
) -> Option<Vec<CombatLogRow>> {
    let obj = js_sys::Object::new();
    if let Some(idx) = encounter_idx {
        js_set(&obj, "encounterIdx", &JsValue::from_f64(idx as f64));
    } else {
        js_set(&obj, "encounterIdx", &JsValue::NULL);
    }
    js_set(&obj, "offset", &JsValue::from_f64(offset as f64));
    js_set(&obj, "limit", &JsValue::from_f64(limit as f64));
    if let Some(s) = source_filter {
        js_set(&obj, "sourceFilter", &JsValue::from_str(s));
    } else {
        js_set(&obj, "sourceFilter", &JsValue::NULL);
    }
    if let Some(t) = target_filter {
        js_set(&obj, "targetFilter", &JsValue::from_str(t));
    } else {
        js_set(&obj, "targetFilter", &JsValue::NULL);
    }
    if let Some(s) = search_filter {
        js_set(&obj, "searchFilter", &JsValue::from_str(s));
    } else {
        js_set(&obj, "searchFilter", &JsValue::NULL);
    }
    if let Some(tr) = time_range {
        let tr_js = serde_wasm_bindgen::to_value(tr).unwrap_or(JsValue::NULL);
        js_set(&obj, "timeRange", &tr_js);
    } else {
        js_set(&obj, "timeRange", &JsValue::NULL);
    }
    if let Some(ef) = event_filters {
        let ef_js = serde_wasm_bindgen::to_value(ef).unwrap_or(JsValue::NULL);
        js_set(&obj, "eventFilters", &ef_js);
    } else {
        js_set(&obj, "eventFilters", &JsValue::NULL);
    }
    let result = invoke("query_combat_log", obj.into()).await;
    from_js(result)
}

/// Get total count of combat log rows for pagination.
pub async fn query_combat_log_count(
    encounter_idx: Option<u32>,
    source_filter: Option<&str>,
    target_filter: Option<&str>,
    search_filter: Option<&str>,
    time_range: Option<&TimeRange>,
    event_filters: Option<&CombatLogFilters>,
) -> Option<u64> {
    let obj = js_sys::Object::new();
    if let Some(idx) = encounter_idx {
        js_set(&obj, "encounterIdx", &JsValue::from_f64(idx as f64));
    } else {
        js_set(&obj, "encounterIdx", &JsValue::NULL);
    }
    if let Some(s) = source_filter {
        js_set(&obj, "sourceFilter", &JsValue::from_str(s));
    } else {
        js_set(&obj, "sourceFilter", &JsValue::NULL);
    }
    if let Some(t) = target_filter {
        js_set(&obj, "targetFilter", &JsValue::from_str(t));
    } else {
        js_set(&obj, "targetFilter", &JsValue::NULL);
    }
    if let Some(s) = search_filter {
        js_set(&obj, "searchFilter", &JsValue::from_str(s));
    } else {
        js_set(&obj, "searchFilter", &JsValue::NULL);
    }
    if let Some(tr) = time_range {
        let tr_js = serde_wasm_bindgen::to_value(tr).unwrap_or(JsValue::NULL);
        js_set(&obj, "timeRange", &tr_js);
    } else {
        js_set(&obj, "timeRange", &JsValue::NULL);
    }
    if let Some(ef) = event_filters {
        let ef_js = serde_wasm_bindgen::to_value(ef).unwrap_or(JsValue::NULL);
        js_set(&obj, "eventFilters", &ef_js);
    } else {
        js_set(&obj, "eventFilters", &JsValue::NULL);
    }
    let result = invoke("query_combat_log_count", obj.into()).await;
    from_js(result)
}

/// Find matching rows in combat log (returns position and row_idx).
pub async fn query_combat_log_find(
    encounter_idx: Option<u32>,
    find_text: &str,
    source_filter: Option<&str>,
    target_filter: Option<&str>,
    time_range: Option<&TimeRange>,
    event_filters: Option<&CombatLogFilters>,
) -> Option<Vec<CombatLogFindMatch>> {
    let obj = js_sys::Object::new();
    if let Some(idx) = encounter_idx {
        js_set(&obj, "encounterIdx", &JsValue::from_f64(idx as f64));
    } else {
        js_set(&obj, "encounterIdx", &JsValue::NULL);
    }
    js_set(&obj, "findText", &JsValue::from_str(find_text));
    if let Some(s) = source_filter {
        js_set(&obj, "sourceFilter", &JsValue::from_str(s));
    } else {
        js_set(&obj, "sourceFilter", &JsValue::NULL);
    }
    if let Some(t) = target_filter {
        js_set(&obj, "targetFilter", &JsValue::from_str(t));
    } else {
        js_set(&obj, "targetFilter", &JsValue::NULL);
    }
    if let Some(tr) = time_range {
        let tr_js = serde_wasm_bindgen::to_value(tr).unwrap_or(JsValue::NULL);
        js_set(&obj, "timeRange", &tr_js);
    } else {
        js_set(&obj, "timeRange", &JsValue::NULL);
    }
    if let Some(ef) = event_filters {
        let ef_js = serde_wasm_bindgen::to_value(ef).unwrap_or(JsValue::NULL);
        js_set(&obj, "eventFilters", &ef_js);
    } else {
        js_set(&obj, "eventFilters", &JsValue::NULL);
    }
    let result = invoke("query_combat_log_find", obj.into()).await;
    from_js(result)
}

/// Get distinct source names for combat log filter dropdown, grouped by entity type.
pub async fn query_source_names(encounter_idx: Option<u32>) -> Option<GroupedEntityNames> {
    let obj = js_sys::Object::new();
    if let Some(idx) = encounter_idx {
        js_set(&obj, "encounterIdx", &JsValue::from_f64(idx as f64));
    } else {
        js_set(&obj, "encounterIdx", &JsValue::NULL);
    }
    let result = invoke("query_source_names", obj.into()).await;
    from_js(result)
}

/// Get distinct target names for combat log filter dropdown, grouped by entity type.
pub async fn query_target_names(encounter_idx: Option<u32>) -> Option<GroupedEntityNames> {
    let obj = js_sys::Object::new();
    if let Some(idx) = encounter_idx {
        js_set(&obj, "encounterIdx", &JsValue::from_f64(idx as f64));
    } else {
        js_set(&obj, "encounterIdx", &JsValue::NULL);
    }
    let result = invoke("query_target_names", obj.into()).await;
    from_js(result)
}

/// Query player deaths in an encounter.
pub async fn query_player_deaths(encounter_idx: Option<u32>) -> Option<Vec<PlayerDeath>> {
    let obj = js_sys::Object::new();
    if let Some(idx) = encounter_idx {
        js_set(&obj, "encounterIdx", &JsValue::from_f64(idx as f64));
    } else {
        js_set(&obj, "encounterIdx", &JsValue::NULL);
    }
    let result = invoke("query_player_deaths", obj.into()).await;
    from_js(result)
}

// ─────────────────────────────────────────────────────────────────────────────
// Changelog Commands
// ─────────────────────────────────────────────────────────────────────────────

use crate::types::ChangelogResponse;

/// Check if changelog should be shown and get rendered HTML content.
pub async fn get_changelog() -> Option<ChangelogResponse> {
    let result = invoke("get_changelog", JsValue::NULL).await;
    from_js(result)
}

/// Mark the changelog as viewed for the current version.
pub async fn mark_changelog_viewed() {
    invoke("mark_changelog_viewed", JsValue::NULL).await;
}
