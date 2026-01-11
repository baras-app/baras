//! Tauri API client module
//!
//! Provides type-safe wrappers around Tauri invoke() calls, eliminating
//! boilerplate and centralizing all backend communication.

use serde::Serialize;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

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
    match serde_wasm_bindgen::from_value(value) {
        Ok(v) => Some(v),
        Err(e) => {
            web_sys::console::error_1(&format!("[API] Deserialization error: {:?}", e).into());
            None
        }
    }
}

/// Invoke a Tauri command that may return an error, catching the rejection
/// Returns Ok(JsValue) on success, Err(String) on failure
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
pub async fn update_config(config: &AppConfig) -> bool {
    let _result = invoke("update_config", build_args("config", config)).await;
    true
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
    js_sys::Reflect::set(
        &args,
        &JsValue::from_str("deleteEmpty"),
        &JsValue::from_bool(delete_empty),
    )
    .unwrap();
    if let Some(days) = retention_days {
        js_sys::Reflect::set(
            &args,
            &JsValue::from_str("retentionDays"),
            &JsValue::from_f64(days as f64),
        )
        .unwrap();
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
    let _result = invoke("open_historical_file", build_args("path", &path)).await;
    true
}

/// Resume live tailing mode
pub async fn resume_live_tailing() -> bool {
    let _result = invoke("resume_live_tailing", JsValue::NULL).await;
    true
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
    let _result = invoke("save_profile", build_args("name", &name)).await;
    true
}

/// Load a profile by name
pub async fn load_profile(name: &str) -> bool {
    let _result = invoke("load_profile", build_args("name", &name)).await;
    true
}

/// Delete a profile by name
pub async fn delete_profile(name: &str) -> bool {
    let _result = invoke("delete_profile", build_args("name", &name)).await;
    true
}

// ─────────────────────────────────────────────────────────────────────────────
// Dialog Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Open a directory picker dialog
pub async fn pick_directory(title: &str) -> Option<String> {
    let options = js_sys::Object::new();
    js_sys::Reflect::set(&options, &JsValue::from_str("directory"), &JsValue::TRUE).unwrap();
    js_sys::Reflect::set(
        &options,
        &JsValue::from_str("title"),
        &JsValue::from_str(title),
    )
    .unwrap();

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
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("bossId"),
        &JsValue::from_str(boss_id),
    )
    .unwrap();
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("filePath"),
        &JsValue::from_str(file_path),
    )
    .unwrap();
    let item_js = serde_wasm_bindgen::to_value(item).unwrap_or(JsValue::NULL);
    js_sys::Reflect::set(&obj, &JsValue::from_str("item"), &item_js).unwrap();

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
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("bossId"),
        &JsValue::from_str(boss_id),
    )
    .unwrap();
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("filePath"),
        &JsValue::from_str(file_path),
    )
    .unwrap();
    let item_js = serde_wasm_bindgen::to_value(item).unwrap_or(JsValue::NULL);
    js_sys::Reflect::set(&obj, &JsValue::from_str("item"), &item_js).unwrap();
    if let Some(orig) = original_id {
        js_sys::Reflect::set(
            &obj,
            &JsValue::from_str("originalId"),
            &JsValue::from_str(orig),
        )
        .unwrap();
    } else {
        js_sys::Reflect::set(&obj, &JsValue::from_str("originalId"), &JsValue::NULL).unwrap();
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
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("itemType"),
        &JsValue::from_str(item_type),
    )
    .unwrap();
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("itemId"),
        &JsValue::from_str(item_id),
    )
    .unwrap();
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("bossId"),
        &JsValue::from_str(boss_id),
    )
    .unwrap();
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("filePath"),
        &JsValue::from_str(file_path),
    )
    .unwrap();

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
) -> Option<BossTimerDefinition> {
    let obj = js_sys::Object::new();
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("timerId"),
        &JsValue::from_str(timer_id),
    )
    .unwrap();
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("bossId"),
        &JsValue::from_str(boss_id),
    )
    .unwrap();
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("filePath"),
        &JsValue::from_str(file_path),
    )
    .unwrap();

    let result = invoke("duplicate_encounter_timer", obj.into()).await;
    from_js(result)
}

/// Get area index for lazy-loading timer editor
pub async fn get_area_index() -> Option<Vec<AreaListItem>> {
    let result = invoke("get_area_index", JsValue::NULL).await;
    from_js(result)
}

use crate::types::{BossEditItem, NewAreaRequest};

/// Create a new boss in an area file
pub async fn create_boss(boss: &BossEditItem) -> Option<BossEditItem> {
    let args = build_args("boss", boss);
    let result = invoke("create_boss", args).await;
    from_js(result)
}

/// Create a new area file
pub async fn create_area(area: &NewAreaRequest) -> Option<String> {
    let args = build_args("area", area);
    let result = invoke("create_area", args).await;
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
    let _result = invoke("update_effect_definition", args).await;
    true
}

/// Delete an effect
/// Returns true on success. Tauri commands returning Result<(), String> serialize Ok(()) as null.
pub async fn delete_effect_definition(effect_id: &str, file_path: &str) -> bool {
    let obj = js_sys::Object::new();
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("effectId"),
        &JsValue::from_str(effect_id),
    )
    .unwrap();
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("filePath"),
        &JsValue::from_str(file_path),
    )
    .unwrap();

    let _result = invoke("delete_effect_definition", obj.into()).await;
    true
}

/// Duplicate an effect
pub async fn duplicate_effect_definition(
    effect_id: &str,
    file_path: &str,
) -> Option<EffectListItem> {
    let obj = js_sys::Object::new();
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("effectId"),
        &JsValue::from_str(effect_id),
    )
    .unwrap();
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("filePath"),
        &JsValue::from_str(file_path),
    )
    .unwrap();

    let result = invoke("duplicate_effect_definition", obj.into()).await;
    from_js(result)
}

/// Create a new effect
/// Returns Ok(created effect) on success, Err(message) on failure (e.g., validation error)
pub async fn create_effect_definition(effect: &EffectListItem) -> Result<EffectListItem, String> {
    let args = build_args("effect", effect);
    let result = try_invoke("create_effect_definition", args).await?;
    from_js(result).ok_or_else(|| "Failed to deserialize created effect".to_string())
}

/// Get icon preview as base64 data URL for an ability ID
pub async fn get_icon_preview(ability_id: u64) -> Option<String> {
    let result = invoke("get_icon_preview", build_args("abilityId", &ability_id)).await;
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

// ─────────────────────────────────────────────────────────────────────────────
// Audio File Picker
// ─────────────────────────────────────────────────────────────────────────────

/// Open a file picker for audio files, returns the selected path or None
pub async fn pick_audio_file() -> Option<String> {
    let result = invoke("pick_audio_file", JsValue::NULL).await;
    from_js(result).unwrap_or(None)
}

// ─────────────────────────────────────────────────────────────────────────────
// Updater Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Install available update (downloads, installs, restarts app)
pub async fn install_update() -> Result<(), String> {
    let result = invoke("install_update", JsValue::NULL).await;
    if let Some(err) = result.as_string() {
        Err(err)
    } else {
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Query Commands (Data Explorer)
// ─────────────────────────────────────────────────────────────────────────────

// Re-export query types from shared types crate
pub use baras_types::{
    AbilityBreakdown, BreakdownMode, CombatLogRow, DataTab, EffectChartData, EffectWindow,
    EncounterTimeline, EntityBreakdown, PhaseSegment, PlayerDeath, RaidOverviewRow, TimeRange,
    TimeSeriesPoint,
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
    js_sys::Reflect::set(&obj, &JsValue::from_str("tab"), &tab_js).unwrap();
    if let Some(idx) = encounter_idx {
        js_sys::Reflect::set(
            &obj,
            &JsValue::from_str("encounterIdx"),
            &JsValue::from_f64(idx as f64),
        )
        .unwrap();
    } else {
        js_sys::Reflect::set(&obj, &JsValue::from_str("encounterIdx"), &JsValue::NULL).unwrap();
    }
    if let Some(name) = entity_name {
        js_sys::Reflect::set(
            &obj,
            &JsValue::from_str("entityName"),
            &JsValue::from_str(name),
        )
        .unwrap();
    } else {
        js_sys::Reflect::set(&obj, &JsValue::from_str("entityName"), &JsValue::NULL).unwrap();
    }
    if let Some(tr) = time_range {
        let tr_js = serde_wasm_bindgen::to_value(tr).unwrap_or(JsValue::NULL);
        js_sys::Reflect::set(&obj, &JsValue::from_str("timeRange"), &tr_js).unwrap();
    } else {
        js_sys::Reflect::set(&obj, &JsValue::from_str("timeRange"), &JsValue::NULL).unwrap();
    }
    if let Some(types) = entity_types {
        let types_js = serde_wasm_bindgen::to_value(types).unwrap_or(JsValue::NULL);
        js_sys::Reflect::set(&obj, &JsValue::from_str("entityTypes"), &types_js).unwrap();
    } else {
        js_sys::Reflect::set(&obj, &JsValue::from_str("entityTypes"), &JsValue::NULL).unwrap();
    }
    if let Some(mode) = breakdown_mode {
        let mode_js = serde_wasm_bindgen::to_value(mode).unwrap_or(JsValue::NULL);
        js_sys::Reflect::set(&obj, &JsValue::from_str("breakdownMode"), &mode_js).unwrap();
    } else {
        js_sys::Reflect::set(&obj, &JsValue::from_str("breakdownMode"), &JsValue::NULL).unwrap();
    }
    if let Some(dur) = duration_secs {
        js_sys::Reflect::set(
            &obj,
            &JsValue::from_str("durationSecs"),
            &JsValue::from_f64(dur as f64),
        )
        .unwrap();
    } else {
        js_sys::Reflect::set(&obj, &JsValue::from_str("durationSecs"), &JsValue::NULL).unwrap();
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
    js_sys::Reflect::set(&obj, &JsValue::from_str("tab"), &tab_js).unwrap();
    if let Some(idx) = encounter_idx {
        js_sys::Reflect::set(
            &obj,
            &JsValue::from_str("encounterIdx"),
            &JsValue::from_f64(idx as f64),
        )
        .unwrap();
    } else {
        js_sys::Reflect::set(&obj, &JsValue::from_str("encounterIdx"), &JsValue::NULL).unwrap();
    }
    if let Some(tr) = time_range {
        let tr_js = serde_wasm_bindgen::to_value(tr).unwrap_or(JsValue::NULL);
        js_sys::Reflect::set(&obj, &JsValue::from_str("timeRange"), &tr_js).unwrap();
    } else {
        js_sys::Reflect::set(&obj, &JsValue::from_str("timeRange"), &JsValue::NULL).unwrap();
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
        js_sys::Reflect::set(
            &obj,
            &JsValue::from_str("encounterIdx"),
            &JsValue::from_f64(idx as f64),
        )
        .unwrap();
    } else {
        js_sys::Reflect::set(&obj, &JsValue::from_str("encounterIdx"), &JsValue::NULL).unwrap();
    }
    if let Some(tr) = time_range {
        let tr_js = serde_wasm_bindgen::to_value(tr).unwrap_or(JsValue::NULL);
        js_sys::Reflect::set(&obj, &JsValue::from_str("timeRange"), &tr_js).unwrap();
    } else {
        js_sys::Reflect::set(&obj, &JsValue::from_str("timeRange"), &JsValue::NULL).unwrap();
    }
    if let Some(dur) = duration_secs {
        js_sys::Reflect::set(
            &obj,
            &JsValue::from_str("durationSecs"),
            &JsValue::from_f64(dur as f64),
        )
        .unwrap();
    } else {
        js_sys::Reflect::set(&obj, &JsValue::from_str("durationSecs"), &JsValue::NULL).unwrap();
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
        js_sys::Reflect::set(
            &obj,
            &JsValue::from_str("encounterIdx"),
            &JsValue::from_f64(idx as f64),
        )
        .unwrap();
    } else {
        js_sys::Reflect::set(&obj, &JsValue::from_str("encounterIdx"), &JsValue::NULL).unwrap();
    }
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("bucketMs"),
        &JsValue::from_f64(bucket_ms as f64),
    )
    .unwrap();
    if let Some(name) = source_name {
        js_sys::Reflect::set(
            &obj,
            &JsValue::from_str("sourceName"),
            &JsValue::from_str(name),
        )
        .unwrap();
    } else {
        js_sys::Reflect::set(&obj, &JsValue::from_str("sourceName"), &JsValue::NULL).unwrap();
    }
    if let Some(tr) = time_range {
        let tr_js = serde_wasm_bindgen::to_value(tr).unwrap_or(JsValue::NULL);
        js_sys::Reflect::set(&obj, &JsValue::from_str("timeRange"), &tr_js).unwrap();
    } else {
        js_sys::Reflect::set(&obj, &JsValue::from_str("timeRange"), &JsValue::NULL).unwrap();
    }
    let result = invoke("query_dps_over_time", obj.into()).await;
    from_js(result)
}

/// Query encounter timeline with phase segments.
pub async fn query_encounter_timeline(encounter_idx: Option<u32>) -> Option<EncounterTimeline> {
    let obj = js_sys::Object::new();
    if let Some(idx) = encounter_idx {
        js_sys::Reflect::set(
            &obj,
            &JsValue::from_str("encounterIdx"),
            &JsValue::from_f64(idx as f64),
        )
        .unwrap();
    } else {
        js_sys::Reflect::set(&obj, &JsValue::from_str("encounterIdx"), &JsValue::NULL).unwrap();
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
        js_sys::Reflect::set(
            &obj,
            &JsValue::from_str("encounterIdx"),
            &JsValue::from_f64(idx as f64),
        )
        .unwrap();
    } else {
        js_sys::Reflect::set(&obj, &JsValue::from_str("encounterIdx"), &JsValue::NULL).unwrap();
    }
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("bucketMs"),
        &JsValue::from_f64(bucket_ms as f64),
    )
    .unwrap();
    if let Some(name) = source_name {
        js_sys::Reflect::set(
            &obj,
            &JsValue::from_str("sourceName"),
            &JsValue::from_str(name),
        )
        .unwrap();
    } else {
        js_sys::Reflect::set(&obj, &JsValue::from_str("sourceName"), &JsValue::NULL).unwrap();
    }
    if let Some(tr) = time_range {
        let tr_js = serde_wasm_bindgen::to_value(tr).unwrap_or(JsValue::NULL);
        js_sys::Reflect::set(&obj, &JsValue::from_str("timeRange"), &tr_js).unwrap();
    } else {
        js_sys::Reflect::set(&obj, &JsValue::from_str("timeRange"), &JsValue::NULL).unwrap();
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
        js_sys::Reflect::set(
            &obj,
            &JsValue::from_str("encounterIdx"),
            &JsValue::from_f64(idx as f64),
        )
        .unwrap();
    } else {
        js_sys::Reflect::set(&obj, &JsValue::from_str("encounterIdx"), &JsValue::NULL).unwrap();
    }
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("bucketMs"),
        &JsValue::from_f64(bucket_ms as f64),
    )
    .unwrap();
    if let Some(name) = target_name {
        js_sys::Reflect::set(
            &obj,
            &JsValue::from_str("targetName"),
            &JsValue::from_str(name),
        )
        .unwrap();
    } else {
        js_sys::Reflect::set(&obj, &JsValue::from_str("targetName"), &JsValue::NULL).unwrap();
    }
    if let Some(tr) = time_range {
        let tr_js = serde_wasm_bindgen::to_value(tr).unwrap_or(JsValue::NULL);
        js_sys::Reflect::set(&obj, &JsValue::from_str("timeRange"), &tr_js).unwrap();
    } else {
        js_sys::Reflect::set(&obj, &JsValue::from_str("timeRange"), &JsValue::NULL).unwrap();
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
        js_sys::Reflect::set(
            &obj,
            &JsValue::from_str("encounterIdx"),
            &JsValue::from_f64(idx as f64),
        )
        .unwrap();
    } else {
        js_sys::Reflect::set(&obj, &JsValue::from_str("encounterIdx"), &JsValue::NULL).unwrap();
    }
    if let Some(name) = target_name {
        js_sys::Reflect::set(
            &obj,
            &JsValue::from_str("targetName"),
            &JsValue::from_str(name),
        )
        .unwrap();
    } else {
        js_sys::Reflect::set(&obj, &JsValue::from_str("targetName"), &JsValue::NULL).unwrap();
    }
    if let Some(tr) = time_range {
        let tr_js = serde_wasm_bindgen::to_value(tr).unwrap_or(JsValue::NULL);
        js_sys::Reflect::set(&obj, &JsValue::from_str("timeRange"), &tr_js).unwrap();
    } else {
        js_sys::Reflect::set(&obj, &JsValue::from_str("timeRange"), &JsValue::NULL).unwrap();
    }
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("durationSecs"),
        &JsValue::from_f64(duration_secs as f64),
    )
    .unwrap();
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
        js_sys::Reflect::set(
            &obj,
            &JsValue::from_str("encounterIdx"),
            &JsValue::from_f64(idx as f64),
        )
        .unwrap();
    } else {
        js_sys::Reflect::set(&obj, &JsValue::from_str("encounterIdx"), &JsValue::NULL).unwrap();
    }
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("effectId"),
        &JsValue::from_f64(effect_id as f64),
    )
    .unwrap();
    if let Some(name) = target_name {
        js_sys::Reflect::set(
            &obj,
            &JsValue::from_str("targetName"),
            &JsValue::from_str(name),
        )
        .unwrap();
    } else {
        js_sys::Reflect::set(&obj, &JsValue::from_str("targetName"), &JsValue::NULL).unwrap();
    }
    if let Some(tr) = time_range {
        let tr_js = serde_wasm_bindgen::to_value(tr).unwrap_or(JsValue::NULL);
        js_sys::Reflect::set(&obj, &JsValue::from_str("timeRange"), &tr_js).unwrap();
    } else {
        js_sys::Reflect::set(&obj, &JsValue::from_str("timeRange"), &JsValue::NULL).unwrap();
    }
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("durationSecs"),
        &JsValue::from_f64(duration_secs as f64),
    )
    .unwrap();
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
) -> Option<Vec<CombatLogRow>> {
    let obj = js_sys::Object::new();
    if let Some(idx) = encounter_idx {
        js_sys::Reflect::set(
            &obj,
            &JsValue::from_str("encounterIdx"),
            &JsValue::from_f64(idx as f64),
        )
        .unwrap();
    } else {
        js_sys::Reflect::set(&obj, &JsValue::from_str("encounterIdx"), &JsValue::NULL).unwrap();
    }
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("offset"),
        &JsValue::from_f64(offset as f64),
    )
    .unwrap();
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("limit"),
        &JsValue::from_f64(limit as f64),
    )
    .unwrap();
    if let Some(s) = source_filter {
        js_sys::Reflect::set(
            &obj,
            &JsValue::from_str("sourceFilter"),
            &JsValue::from_str(s),
        )
        .unwrap();
    } else {
        js_sys::Reflect::set(&obj, &JsValue::from_str("sourceFilter"), &JsValue::NULL).unwrap();
    }
    if let Some(t) = target_filter {
        js_sys::Reflect::set(
            &obj,
            &JsValue::from_str("targetFilter"),
            &JsValue::from_str(t),
        )
        .unwrap();
    } else {
        js_sys::Reflect::set(&obj, &JsValue::from_str("targetFilter"), &JsValue::NULL).unwrap();
    }
    if let Some(s) = search_filter {
        js_sys::Reflect::set(
            &obj,
            &JsValue::from_str("searchFilter"),
            &JsValue::from_str(s),
        )
        .unwrap();
    } else {
        js_sys::Reflect::set(&obj, &JsValue::from_str("searchFilter"), &JsValue::NULL).unwrap();
    }
    if let Some(tr) = time_range {
        let tr_js = serde_wasm_bindgen::to_value(tr).unwrap_or(JsValue::NULL);
        js_sys::Reflect::set(&obj, &JsValue::from_str("timeRange"), &tr_js).unwrap();
    } else {
        js_sys::Reflect::set(&obj, &JsValue::from_str("timeRange"), &JsValue::NULL).unwrap();
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
) -> Option<u64> {
    let obj = js_sys::Object::new();
    if let Some(idx) = encounter_idx {
        js_sys::Reflect::set(
            &obj,
            &JsValue::from_str("encounterIdx"),
            &JsValue::from_f64(idx as f64),
        )
        .unwrap();
    } else {
        js_sys::Reflect::set(&obj, &JsValue::from_str("encounterIdx"), &JsValue::NULL).unwrap();
    }
    if let Some(s) = source_filter {
        js_sys::Reflect::set(
            &obj,
            &JsValue::from_str("sourceFilter"),
            &JsValue::from_str(s),
        )
        .unwrap();
    } else {
        js_sys::Reflect::set(&obj, &JsValue::from_str("sourceFilter"), &JsValue::NULL).unwrap();
    }
    if let Some(t) = target_filter {
        js_sys::Reflect::set(
            &obj,
            &JsValue::from_str("targetFilter"),
            &JsValue::from_str(t),
        )
        .unwrap();
    } else {
        js_sys::Reflect::set(&obj, &JsValue::from_str("targetFilter"), &JsValue::NULL).unwrap();
    }
    if let Some(s) = search_filter {
        js_sys::Reflect::set(
            &obj,
            &JsValue::from_str("searchFilter"),
            &JsValue::from_str(s),
        )
        .unwrap();
    } else {
        js_sys::Reflect::set(&obj, &JsValue::from_str("searchFilter"), &JsValue::NULL).unwrap();
    }
    if let Some(tr) = time_range {
        let tr_js = serde_wasm_bindgen::to_value(tr).unwrap_or(JsValue::NULL);
        js_sys::Reflect::set(&obj, &JsValue::from_str("timeRange"), &tr_js).unwrap();
    } else {
        js_sys::Reflect::set(&obj, &JsValue::from_str("timeRange"), &JsValue::NULL).unwrap();
    }
    let result = invoke("query_combat_log_count", obj.into()).await;
    from_js(result)
}

/// Get distinct source names for combat log filter dropdown.
pub async fn query_source_names(encounter_idx: Option<u32>) -> Option<Vec<String>> {
    let obj = js_sys::Object::new();
    if let Some(idx) = encounter_idx {
        js_sys::Reflect::set(
            &obj,
            &JsValue::from_str("encounterIdx"),
            &JsValue::from_f64(idx as f64),
        )
        .unwrap();
    } else {
        js_sys::Reflect::set(&obj, &JsValue::from_str("encounterIdx"), &JsValue::NULL).unwrap();
    }
    let result = invoke("query_source_names", obj.into()).await;
    from_js(result)
}

/// Get distinct target names for combat log filter dropdown.
pub async fn query_target_names(encounter_idx: Option<u32>) -> Option<Vec<String>> {
    let obj = js_sys::Object::new();
    if let Some(idx) = encounter_idx {
        js_sys::Reflect::set(
            &obj,
            &JsValue::from_str("encounterIdx"),
            &JsValue::from_f64(idx as f64),
        )
        .unwrap();
    } else {
        js_sys::Reflect::set(&obj, &JsValue::from_str("encounterIdx"), &JsValue::NULL).unwrap();
    }
    let result = invoke("query_target_names", obj.into()).await;
    from_js(result)
}

/// Query player deaths in an encounter.
pub async fn query_player_deaths(encounter_idx: Option<u32>) -> Option<Vec<PlayerDeath>> {
    let obj = js_sys::Object::new();
    if let Some(idx) = encounter_idx {
        js_sys::Reflect::set(
            &obj,
            &JsValue::from_str("encounterIdx"),
            &JsValue::from_f64(idx as f64),
        )
        .unwrap();
    } else {
        js_sys::Reflect::set(&obj, &JsValue::from_str("encounterIdx"), &JsValue::NULL).unwrap();
    }
    let result = invoke("query_player_deaths", obj.into()).await;
    from_js(result)
}
