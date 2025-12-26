//! Tauri API client module
//!
//! Provides type-safe wrappers around Tauri invoke() calls, eliminating
//! boilerplate and centralizing all backend communication.

use wasm_bindgen::prelude::*;
use serde::Serialize;

use crate::types::{AppConfig, OverlayStatus, OverlayType, SessionInfo};

// ─────────────────────────────────────────────────────────────────────────────
// Raw Tauri Bindings
// ─────────────────────────────────────────────────────────────────────────────

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    async fn invoke(cmd: &str, args: JsValue) -> JsValue;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"], js_name = "listen")]
    pub async fn tauri_listen(event: &str, handler: &Closure<dyn FnMut(JsValue)>) -> JsValue;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "dialog"], js_name = "open")]
    pub async fn open_dialog(options: JsValue) -> JsValue;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "app"], js_name = "getVersion")]
    pub async fn get_version() -> JsValue;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "opener"], js_name = "openUrl")]
    pub async fn open_url(url: &str) -> JsValue;
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper Functions
// ─────────────────────────────────────────────────────────────────────────────

/// Build a JsValue object with a single key-value pair
fn build_args<T: Serialize + ?Sized>(key: &str, value: &T) -> JsValue {
    let args = serde_wasm_bindgen::to_value(value).unwrap_or(JsValue::NULL);
    let obj = js_sys::Object::new();
    js_sys::Reflect::set(&obj, &JsValue::from_str(key), &args).unwrap();
    obj.into()
}

/// Deserialize a JsValue into a type, returning None on failure
fn from_js<T: serde::de::DeserializeOwned>(value: JsValue) -> Option<T> {
    serde_wasm_bindgen::from_value(value).ok()
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
pub async fn update_config(config: &AppConfig) -> bool {
    let result = invoke("update_config", build_args("config", config)).await;
    result.is_undefined() || result.is_null()
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
    js_sys::Reflect::set(&args, &JsValue::from_str("deleteEmpty"), &JsValue::from_bool(delete_empty)).unwrap();
    if let Some(days) = retention_days {
        js_sys::Reflect::set(&args, &JsValue::from_str("retentionDays"), &JsValue::from_f64(days as f64)).unwrap();
    } else {
        js_sys::Reflect::set(&args, &JsValue::from_str("retentionDays"), &JsValue::NULL).unwrap();
    }
    let result = invoke("cleanup_logs", args.into()).await;
    from_js(result).unwrap_or((0, 0))
}

// ─────────────────────────────────────────────────────────────────────────────
// File Browser Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Open a historical log file (pauses live tailing)
pub async fn open_historical_file(path: &str) -> bool {
    let result = invoke("open_historical_file", build_args("path", &path)).await;
    result.is_undefined() || result.is_null()
}

/// Resume live tailing mode
pub async fn resume_live_tailing() -> bool {
    let result = invoke("resume_live_tailing", JsValue::NULL).await;
    result.is_undefined() || result.is_null()
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
pub async fn save_profile(name: &str) -> bool {
    let result = invoke("save_profile", build_args("name", &name)).await;
    result.is_undefined() || result.is_null()
}

/// Load a profile by name
pub async fn load_profile(name: &str) -> bool {
    let result = invoke("load_profile", build_args("name", &name)).await;
    result.is_undefined() || result.is_null()
}

/// Delete a profile by name
pub async fn delete_profile(name: &str) -> bool {
    let result = invoke("delete_profile", build_args("name", &name)).await;
    result.is_undefined() || result.is_null()
}

// ─────────────────────────────────────────────────────────────────────────────
// Dialog Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Open a directory picker dialog
pub async fn pick_directory(title: &str) -> Option<String> {
    let options = js_sys::Object::new();
    js_sys::Reflect::set(&options, &JsValue::from_str("directory"), &JsValue::TRUE).unwrap();
    js_sys::Reflect::set(&options, &JsValue::from_str("title"), &JsValue::from_str(title)).unwrap();

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
// Encounter History
// ─────────────────────────────────────────────────────────────────────────────

/// Get encounter history summaries
pub async fn get_encounter_history() -> JsValue {
    invoke("get_encounter_history", JsValue::NULL).await
}

// ─────────────────────────────────────────────────────────────────────────────
// Timer Editor Commands
// ─────────────────────────────────────────────────────────────────────────────

use crate::types::{AreaListItem, BossListItem, TimerListItem};

/// Update an existing timer
pub async fn update_encounter_timer(timer: &TimerListItem) -> bool {
    let args = build_args("timer", timer);
    let result = invoke("update_encounter_timer", args).await;
    !result.is_null() && !result.is_undefined()
}

/// Delete a timer
pub async fn delete_encounter_timer(timer_id: &str, boss_id: &str, file_path: &str) -> bool {
    let obj = js_sys::Object::new();
    js_sys::Reflect::set(&obj, &JsValue::from_str("timerId"), &JsValue::from_str(timer_id)).unwrap();
    js_sys::Reflect::set(&obj, &JsValue::from_str("bossId"), &JsValue::from_str(boss_id)).unwrap();
    js_sys::Reflect::set(&obj, &JsValue::from_str("filePath"), &JsValue::from_str(file_path)).unwrap();

    let result = invoke("delete_encounter_timer", obj.into()).await;
    !result.is_null() && !result.is_undefined()
}

/// Duplicate a timer
pub async fn duplicate_encounter_timer(timer_id: &str, boss_id: &str, file_path: &str) -> Option<TimerListItem> {
    let obj = js_sys::Object::new();
    js_sys::Reflect::set(&obj, &JsValue::from_str("timerId"), &JsValue::from_str(timer_id)).unwrap();
    js_sys::Reflect::set(&obj, &JsValue::from_str("bossId"), &JsValue::from_str(boss_id)).unwrap();
    js_sys::Reflect::set(&obj, &JsValue::from_str("filePath"), &JsValue::from_str(file_path)).unwrap();

    let result = invoke("duplicate_encounter_timer", obj.into()).await;
    from_js(result)
}

/// Create a new timer
pub async fn create_encounter_timer(timer: &TimerListItem) -> Option<TimerListItem> {
    let args = build_args("timer", timer);
    let result = invoke("create_encounter_timer", args).await;
    from_js(result)
}

/// Get area index for lazy-loading timer editor
pub async fn get_area_index() -> Option<Vec<AreaListItem>> {
    let result = invoke("get_area_index", JsValue::NULL).await;
    from_js(result)
}

/// Get timers for a specific area file
pub async fn get_timers_for_area(file_path: &str) -> Option<Vec<TimerListItem>> {
    let args = build_args("filePath", file_path);
    let result = invoke("get_timers_for_area", args).await;
    from_js(result)
}

/// Get bosses for a specific area file
pub async fn get_bosses_for_area(file_path: &str) -> Option<Vec<BossListItem>> {
    let args = build_args("filePath", file_path);
    let result = invoke("get_bosses_for_area", args).await;
    from_js(result)
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
/// Returns true on success. Tauri commands returning Result<(), String> serialize Ok(()) as null.
pub async fn update_effect_definition(effect: &EffectListItem) -> bool {
    let args = build_args("effect", effect);
    let result = invoke("update_effect_definition", args).await;
    // Ok(()) serializes to null, Err would throw - so null means success
    result.is_null() || result.is_undefined()
}

/// Delete an effect
/// Returns true on success. Tauri commands returning Result<(), String> serialize Ok(()) as null.
pub async fn delete_effect_definition(effect_id: &str, file_path: &str) -> bool {
    let obj = js_sys::Object::new();
    js_sys::Reflect::set(&obj, &JsValue::from_str("effectId"), &JsValue::from_str(effect_id)).unwrap();
    js_sys::Reflect::set(&obj, &JsValue::from_str("filePath"), &JsValue::from_str(file_path)).unwrap();

    let result = invoke("delete_effect_definition", obj.into()).await;
    // Ok(()) serializes to null, Err would throw - so null means success
    result.is_null() || result.is_undefined()
}

/// Duplicate an effect
pub async fn duplicate_effect_definition(effect_id: &str, file_path: &str) -> Option<EffectListItem> {
    let obj = js_sys::Object::new();
    js_sys::Reflect::set(&obj, &JsValue::from_str("effectId"), &JsValue::from_str(effect_id)).unwrap();
    js_sys::Reflect::set(&obj, &JsValue::from_str("filePath"), &JsValue::from_str(file_path)).unwrap();

    let result = invoke("duplicate_effect_definition", obj.into()).await;
    from_js(result)
}

/// Create a new effect
pub async fn create_effect_definition(effect: &EffectListItem) -> Option<EffectListItem> {
    let args = build_args("effect", effect);
    let result = invoke("create_effect_definition", args).await;
    from_js(result)
}

/// Get list of effect files for "New Effect" file selection
pub async fn get_effect_files() -> Option<Vec<String>> {
    let result = invoke("get_effect_files", JsValue::NULL).await;
    from_js(result)
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
pub async fn upload_to_parsely(path: &str) -> Option<ParselyUploadResponse> {
    let result = invoke("upload_to_parsely", build_args("path", &path)).await;
    from_js(result)
}
