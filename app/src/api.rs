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
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper Functions
// ─────────────────────────────────────────────────────────────────────────────

/// Build a JsValue object with a single key-value pair
fn build_args<T: Serialize>(key: &str, value: &T) -> JsValue {
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
// Encounter History
// ─────────────────────────────────────────────────────────────────────────────

/// Get encounter history summaries
pub async fn get_encounter_history() -> JsValue {
    invoke("get_encounter_history", JsValue::NULL).await
}
