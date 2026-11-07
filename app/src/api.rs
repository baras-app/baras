//! Tauri API client module
//!
//! Provides type-safe wrappers around Tauri invoke() calls, eliminating
//! boilerplate and centralizing all backend communication.

use serde::Serialize;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

use crate::types::{AppConfig, MirrorAbility, OverlayStatus, OverlayType, SessionInfo};
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

/// Set an optional entity instance id. Log ids are ~15 digits, well inside f64's exact range,
/// same as the ability/effect ids sent elsewhere in this file.
fn js_set_entity_id(obj: &JsValue, entity_id: Option<i64>) {
    match entity_id {
        Some(id) => js_set(obj, "entityId", &JsValue::from_f64(id as f64)),
        None => js_set(obj, "entityId", &JsValue::NULL),
    }
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

/// Start or stop the loopback web overlay and persist the setting.
pub async fn set_web_overlay_enabled(enabled: bool) -> Result<(), String> {
    try_invoke("set_web_overlay_enabled", build_args("enabled", &enabled)).await?;
    Ok(())
}

/// Whether the app is running in a Wayland session
pub async fn is_wayland_session() -> bool {
    let result = invoke("is_wayland_session", JsValue::NULL).await;
    from_js(result).unwrap_or(false)
}

pub async fn enable_screen_capture() -> Result<(), String> {
    try_invoke("enable_screen_capture", JsValue::NULL).await?;
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

/// List font family names available on this system (for the font picker)
pub async fn list_system_fonts() -> Vec<String> {
    let result = invoke("list_system_fonts", JsValue::NULL).await;
    from_js(result).unwrap_or_default()
}

/// Set the global overlay font family (persists + applies to all overlays)
pub async fn set_overlay_font_family(family: String) -> Result<(), String> {
    try_invoke("set_overlay_font_family", build_args("family", &family)).await?;
    Ok(())
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

// ─────────────────────────────────────────────────────────────────────────────
// Operation Timer
// ─────────────────────────────────────────────────────────────────────────────

/// Start the operation timer manually
pub async fn start_operation_timer() {
    invoke("start_operation_timer", JsValue::NULL).await;
}

/// Stop the operation timer
pub async fn stop_operation_timer() {
    invoke("stop_operation_timer", JsValue::NULL).await;
}

/// Reset the operation timer
pub async fn reset_operation_timer() {
    invoke("reset_operation_timer", JsValue::NULL).await;
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

/// Apply or remove the "not live" auto-hide based on current session state.
/// Called when the user toggles the hide_when_not_live setting.
pub async fn apply_not_live_auto_hide() {
    invoke("apply_not_live_auto_hide", JsValue::NULL).await;
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

/// Return one overlay to its default position
pub async fn reset_overlay_position(key: &str) -> Result<JsValue, String> {
    try_invoke("reset_overlay_position", build_args("key", key)).await
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

/// Refresh file sizes in the directory index (fast stat-only)
pub async fn refresh_file_sizes() {
    let _ = invoke("refresh_file_sizes", JsValue::NULL).await;
}

// ─────────────────────────────────────────────────────────────────────────────
// File Browser Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Open a historical log file (pauses live tailing)
/// Queue a historical open. `Ok(false)` means that file is already being opened.
pub async fn open_historical_file(path: &str) -> Result<bool, String> {
    let result = try_invoke("open_historical_file", build_args("path", &path)).await?;
    Ok(result.as_bool().unwrap_or(true))
}

/// Open a historical log and focus `encounter_id` in the Data Explorer once loaded
/// (backend emits `select-encounter` after the parse completes).
pub async fn open_historical_encounter(path: &str, encounter_id: u64) -> Result<bool, String> {
    let args = js_sys::Object::new();
    js_set(&args, "path", &JsValue::from_str(path));
    js_set(&args, "selectEncounter", &JsValue::from_f64(encounter_id as f64));
    let result = try_invoke("open_historical_file", args.into()).await?;
    Ok(result.as_bool().unwrap_or(true))
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

/// Rename a profile
pub async fn rename_profile(old_name: &str, new_name: &str) -> Result<(), String> {
    let obj = js_sys::Object::new();
    js_set(&obj, "oldName", &JsValue::from_str(old_name));
    js_set(&obj, "newName", &JsValue::from_str(new_name));
    try_invoke("rename_profile", obj.into()).await?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Role Default Profile Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Get the default profile mapping per role (Tank, Healer, Dps)
pub async fn get_default_profiles_per_role() -> std::collections::HashMap<String, String> {
    let result = invoke("get_default_profiles_per_role", JsValue::NULL).await;
    from_js(result).unwrap_or_default()
}

/// Set or clear the default profile for a role
pub async fn set_default_profile_for_role(role: &str, profile_name: Option<&str>) -> Result<(), String> {
    let obj = js_sys::Object::new();
    js_set(&obj, "role", &JsValue::from_str(role));
    match profile_name {
        Some(name) => js_set(&obj, "profileName", &JsValue::from_str(name)),
        None => js_set(&obj, "profileName", &JsValue::NULL),
    }
    try_invoke("set_default_profile_for_role", obj.into()).await?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Dialog Helpers
// ─────────────────────────────────────────────────────────────────────────────

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
-> Option<Vec<crate::components::encounter_types::EncounterSummary>> {
    let result = invoke("get_encounter_history", JsValue::NULL).await;
    from_js(result)
}

/// Filter options and per-boss pull counts for the History tab
pub async fn get_pull_history_overview(
    filter: &baras_types::history::PullFilter,
) -> Option<baras_types::history::PullHistoryOverview> {
    from_js(invoke("get_pull_history_overview", build_args("filter", filter)).await)
}

/// Backfill pull history from every log in the configured directory.
/// Progress arrives on the `history-backfill` event.
pub async fn start_history_backfill(stale_only: bool) -> Result<(), String> {
    try_invoke("start_history_backfill", build_args("staleOnly", &stale_only)).await?;
    Ok(())
}

/// Stop a running backfill; a `Done { cancelled: true }` event follows
pub async fn cancel_history_backfill() -> Result<(), String> {
    try_invoke("cancel_history_backfill", JsValue::NULL).await?;
    Ok(())
}

/// History store health: logs, pulls, files needing a re-scan
pub async fn get_history_status() -> Option<baras_types::history::HistoryStatus> {
    from_js(invoke("get_history_status", JsValue::NULL).await)
}

/// Every log with its history and metadata joined, for the Library tab
pub async fn get_library() -> Option<Vec<baras_types::history::LibraryRow>> {
    from_js(invoke("get_library", JsValue::NULL).await)
}

// ─────────────────────────────────────────────────────────────────────────
// Characters
// ─────────────────────────────────────────────────────────────────────────

pub async fn get_character_roster() -> Option<baras_types::characters::CharacterRoster> {
    from_js(invoke("get_character_roster", JsValue::NULL).await)
}

/// Create or update a character; `previous_key` when renaming. Returns the stored key.
pub async fn save_character(
    record: &baras_types::characters::CharacterRecord,
    previous_key: Option<&str>,
) -> Result<String, String> {
    let obj = js_sys::Object::new();
    js_set(&obj, "record", &serde_wasm_bindgen::to_value(record).map_err(|e| e.to_string())?);
    js_set(&obj, "previousKey", &previous_key.map_or(JsValue::NULL, JsValue::from_str));
    let result = try_invoke("save_character", obj.into()).await?;
    from_js(result).ok_or_else(|| "Failed to parse response".to_string())
}

pub async fn delete_character(key: &str) -> Result<bool, String> {
    let result = try_invoke("delete_character", build_args("key", key)).await?;
    from_js(result).ok_or_else(|| "Failed to parse response".to_string())
}

pub async fn restore_character(key: &str) -> Result<bool, String> {
    let result = try_invoke("restore_character", build_args("key", key)).await?;
    from_js(result).ok_or_else(|| "Failed to parse response".to_string())
}

pub async fn get_lockout_reset_hour() -> Option<u8> {
    from_js(invoke("get_lockout_reset_hour", JsValue::NULL).await)
}

pub async fn set_lockout_reset_hour(hour: u8) -> Result<bool, String> {
    let result = try_invoke("set_lockout_reset_hour", build_args("hour", &hour)).await?;
    from_js(result).ok_or_else(|| "Failed to parse response".to_string())
}

pub async fn add_manual_lock(key: &str, lock: &baras_types::characters::ManualLock) -> Result<bool, String> {
    let obj = js_sys::Object::new();
    js_set(&obj, "key", &JsValue::from_str(key));
    js_set(&obj, "lock", &serde_wasm_bindgen::to_value(lock).map_err(|e| e.to_string())?);
    let result = try_invoke("add_manual_lock", obj.into()).await?;
    from_js(result).ok_or_else(|| "Failed to parse response".to_string())
}

pub async fn clear_manual_lock(key: &str, operation: &str, tier: &str) -> Result<bool, String> {
    let obj = js_sys::Object::new();
    js_set(&obj, "key", &JsValue::from_str(key));
    js_set(&obj, "operation", &JsValue::from_str(operation));
    js_set(&obj, "tier", &JsValue::from_str(tier));
    let result = try_invoke("clear_manual_lock", obj.into()).await?;
    from_js(result).ok_or_else(|| "Failed to parse response".to_string())
}

pub async fn set_log_favorite(filename: &str, favorite: bool) -> Result<bool, String> {
    let obj = js_sys::Object::new();
    js_set(&obj, "filename", &JsValue::from_str(filename));
    js_set(&obj, "favorite", &JsValue::from_bool(favorite));
    let result = try_invoke("set_log_favorite", obj.into()).await?;
    from_js(result).ok_or_else(|| "Failed to parse response".to_string())
}

/// The user's tags and one-line note on a log
pub async fn set_log_meta(filename: &str, tags: &[String], note: &str) -> Result<bool, String> {
    let obj = js_sys::Object::new();
    js_set(&obj, "filename", &JsValue::from_str(filename));
    js_set(&obj, "tags", &serde_wasm_bindgen::to_value(tags).map_err(|e| e.to_string())?);
    js_set(&obj, "note", &JsValue::from_str(note));
    let result = try_invoke("set_log_meta", obj.into()).await?;
    from_js(result).ok_or_else(|| "Failed to parse response".to_string())
}

/// Content kinds the user wants kept out of every delete
pub async fn get_retained_content() -> Option<Vec<baras_types::history::ContentTag>> {
    from_js(invoke("get_retained_content", JsValue::NULL).await)
}

pub async fn set_retained_content(retain: &[baras_types::history::ContentTag]) -> Result<bool, String> {
    let result = try_invoke("set_retained_content", build_args("retain", retain)).await?;
    from_js(result).ok_or_else(|| "Failed to parse response".to_string())
}

/// Dry run of the saved cleanup rules: what would be archived and deleted now
pub async fn preview_log_cleanup() -> Option<baras_types::history::CleanupPlan> {
    from_js(invoke("preview_log_cleanup", JsValue::NULL).await)
}

/// Archive and delete whatever the saved rules select right now
pub async fn run_log_cleanup() -> Result<baras_types::history::CleanupReport, String> {
    let result = try_invoke("run_log_cleanup", JsValue::NULL).await?;
    from_js(result).ok_or_else(|| "Failed to parse response".to_string())
}

/// Decompress an archived log back into the log directory; returns its path
pub async fn restore_log(filename: &str) -> Result<String, String> {
    let result = try_invoke("restore_log", build_args("filename", filename)).await?;
    from_js(result).ok_or_else(|| "Failed to parse response".to_string())
}

/// Delete logs (or archives) and every artifact they own; refused for the live log
pub async fn delete_logs(paths: &[String]) -> Result<baras_types::history::DeleteReport, String> {
    let result = try_invoke("delete_logs", build_args("paths", paths)).await?;
    from_js(result).ok_or_else(|| "Failed to parse response".to_string())
}

/// Compress logs into the archive; progress arrives on the `log-cleanup` event
pub async fn archive_logs(paths: &[String]) -> Result<baras_types::history::CleanupReport, String> {
    let result = try_invoke("archive_logs", build_args("paths", paths)).await?;
    from_js(result).ok_or_else(|| "Failed to parse response".to_string())
}

/// Record a whole-file Parsely upload against the log
pub async fn set_log_parsely_link(path: &str, link: &str, visibility: u8) -> Result<bool, String> {
    let obj = js_sys::Object::new();
    js_set(&obj, "path", &JsValue::from_str(path));
    js_set(&obj, "link", &JsValue::from_str(link));
    js_set(&obj, "visibility", &JsValue::from_f64(visibility as f64));
    let result = try_invoke("set_log_parsely_link", obj.into()).await?;
    from_js(result).ok_or_else(|| "Failed to parse response".to_string())
}

/// Every recorded pull of one boss under `filter`, newest first
pub async fn get_operation_runs(filter: &baras_types::history::PullFilter) -> Option<baras_types::history::RunList> {
    from_js(invoke("get_operation_runs", build_args("filter", filter)).await)
}

pub async fn get_operation_run(filename: &str, start_ms: i64) -> Option<baras_types::history::RunDetail> {
    let obj = js_sys::Object::new();
    js_set(&obj, "filename", &JsValue::from_str(filename));
    js_set(&obj, "startMs", &JsValue::from_f64(start_ms as f64));
    from_js(invoke("get_operation_run", obj.into()).await).flatten()
}

pub async fn set_run_meta(filename: &str, start_ms: i64, meta: &baras_types::history::RunMeta) -> Result<bool, String> {
    let obj = js_sys::Object::new();
    js_set(&obj, "filename", &JsValue::from_str(filename));
    js_set(&obj, "startMs", &JsValue::from_f64(start_ms as f64));
    js_set(&obj, "meta", &serde_wasm_bindgen::to_value(meta).unwrap_or(JsValue::NULL));
    let result = try_invoke("set_run_meta", obj.into()).await?;
    from_js(result).ok_or_else(|| "Failed to parse response".to_string())
}

pub async fn get_boss_pulls(
    operation: &str,
    boss: &str,
    filter: &baras_types::history::PullFilter,
) -> Option<Vec<baras_types::history::PullRow>> {
    let obj = js_sys::Object::new();
    js_set(&obj, "operation", &JsValue::from_str(operation));
    js_set(&obj, "boss", &JsValue::from_str(boss));
    js_set(&obj, "filter", &serde_wasm_bindgen::to_value(filter).unwrap_or(JsValue::NULL));
    from_js(invoke("get_boss_pulls", obj.into()).await)
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

/// Bulk-set roles (visibility) for all timers of a boss
pub async fn set_all_timer_roles(
    boss_id: &str,
    file_path: &str,
    roles: &[String],
) -> Result<(), String> {
    let obj = js_sys::Object::new();
    js_set(&obj, "bossId", &JsValue::from_str(boss_id));
    js_set(&obj, "filePath", &JsValue::from_str(file_path));
    let roles_js = serde_wasm_bindgen::to_value(roles).unwrap_or(JsValue::NULL);
    js_set(&obj, "roles", &roles_js);
    try_invoke("set_all_timer_roles", obj.into()).await.map(|_| ())
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

/// Update a boss definition's enabled state
pub async fn update_boss_enabled(
    boss_id: &str,
    file_path: &str,
    enabled: bool,
) -> Result<(), String> {
    let obj = js_sys::Object::new();
    js_set(&obj, "bossId", &JsValue::from_str(boss_id));
    js_set(&obj, "filePath", &JsValue::from_str(file_path));
    js_set(&obj, "enabled", &JsValue::from_bool(enabled));
    try_invoke("update_boss_enabled", obj.into()).await?;
    Ok(())
}

/// Update a boss definition's is_final_boss flag
pub async fn update_boss_is_final_boss(
    boss_id: &str,
    file_path: &str,
    is_final_boss: bool,
) -> Result<(), String> {
    let obj = js_sys::Object::new();
    js_set(&obj, "bossId", &JsValue::from_str(boss_id));
    js_set(&obj, "filePath", &JsValue::from_str(file_path));
    js_set(&obj, "isFinalBoss", &JsValue::from_bool(is_final_boss));
    try_invoke("update_boss_is_final_boss", obj.into()).await?;
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

/// Export user effect overrides as TOML string.
/// If `effect_id` is provided, exports only that single effect; otherwise exports all.
pub async fn export_effects_toml(effect_id: Option<&str>) -> Result<String, String> {
    let obj = js_sys::Object::new();
    match effect_id {
        Some(id) => js_set(&obj, "effectId", &JsValue::from_str(id)),
        None => js_set(&obj, "effectId", &JsValue::NULL),
    }
    let result = try_invoke("export_effects_toml", obj.into()).await?;
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

/// Republic ⇄ Imperial mirror ability pairs.
pub async fn get_mirror_abilities() -> Vec<MirrorAbility> {
    from_js(invoke("get_mirror_abilities", JsValue::NULL).await).unwrap_or_default()
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
    guild_log: bool,
    guild: Option<String>,
    no_guild: bool,
) -> Result<ParselyUploadResponse, String> {
    let obj = js_sys::Object::new();
    js_set(&obj, "path", &JsValue::from_str(path));
    js_set(&obj, "visibility", &JsValue::from_f64(visibility as f64));
    if let Some(note) = notes {
        js_set(&obj, "notes", &JsValue::from_str(&note));
    } else {
        js_set(&obj, "notes", &JsValue::NULL);
    }
    js_set(&obj, "guildLog", &JsValue::from_bool(guild_log));
    if let Some(g) = guild {
        js_set(&obj, "guild", &JsValue::from_str(&g));
    } else {
        js_set(&obj, "guild", &JsValue::NULL);
    }
    js_set(&obj, "noGuild", &JsValue::from_bool(no_guild));
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
    guild_log: bool,
    guild: Option<String>,
    no_guild: bool,
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
    js_set(&obj, "guildLog", &JsValue::from_bool(guild_log));
    if let Some(g) = guild {
        js_set(&obj, "guild", &JsValue::from_str(&g));
    } else {
        js_set(&obj, "guild", &JsValue::NULL);
    }
    js_set(&obj, "noGuild", &JsValue::from_bool(no_guild));
    let result = try_invoke("upload_encounter_to_parsely", obj.into()).await?;
    from_js(result).ok_or_else(|| "Failed to parse upload response".to_string())
}

/// Set the Parsely link for an encounter (persists in backend)
pub async fn set_encounter_parsely_link(encounter_id: u64, link: &str, visibility: u8) -> Result<bool, String> {
    let obj = js_sys::Object::new();
    js_set(&obj, "encounterId", &JsValue::from_f64(encounter_id as f64));
    js_set(&obj, "link", &JsValue::from_str(link));
    js_set(&obj, "visibility", &JsValue::from_f64(visibility as f64));
    let result = try_invoke("set_encounter_parsely_link", obj.into()).await?;
    from_js(result).ok_or_else(|| "Failed to parse response".to_string())
}

// ─────────────────────────────────────────────────────────────────────────────
// Audio File Picker
// ─────────────────────────────────────────────────────────────────────────────

/// List available sound files grouped by category for the picker UI.
pub async fn list_sound_files() -> Vec<crate::types::SoundCategory> {
    let result = invoke("list_sound_files", JsValue::NULL).await;
    from_js(result).unwrap_or_default()
}

/// Open a file picker for audio files, returns the selected path or None
pub async fn pick_audio_file() -> Option<String> {
    let result = invoke("pick_audio_file", JsValue::NULL).await;
    from_js(result).unwrap_or(None)
}

/// Preview (play) a sound file so the user can hear it in the editor
pub async fn preview_sound(filename: &str) {
    let _ = try_invoke("preview_sound", build_args("filename", filename)).await;
}

/// Speak a test phrase so the user can hear TTS at the configured volume
pub async fn preview_tts() {
    let _ = try_invoke("preview_tts", JsValue::NULL).await;
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
    AbilityBreakdown, AbilityUsageRow, BreakdownMode, CombatLogFilters, CombatLogFindMatch,
    CombatLogRow, CombatLogSortColumn, DamageTakenSummary, DataTab, DeathRecap,
    DeathRecapEventKind, EffectChartData, EffectWindow,
    EncounterTimeline, EntityBreakdown, GroupedEntityNames, HpPoint, NpcHealthRow, NpcSpawn,
    PhaseSegment, PlayerDeath, PvpFaction, RaidOverviewRow, RotationAnalysis,
    SortDirection, TimeRange, TimeSeriesPoint,
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
    entity_id: Option<i64>,
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
    js_set_entity_id(&obj, entity_id);
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

/// Query final health state of all NPCs in an encounter.
pub async fn query_npc_health(
    encounter_idx: Option<u32>,
    time_range: Option<&TimeRange>,
) -> Option<Vec<NpcHealthRow>> {
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
    let result = invoke("query_npc_health", obj.into()).await;
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

/// Query EHPS (effective healing) over time with specified bucket size.
pub async fn query_ehps_over_time(
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
    let result = invoke("query_ehps_over_time", obj.into()).await;
    from_js(result)
}

/// Query EHT (effective healing taken) over time with specified bucket size.
pub async fn query_eht_over_time(
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
    let result = invoke("query_eht_over_time", obj.into()).await;
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

/// Query HP% over time with specified bucket size.
pub async fn query_hp_over_time(
    encounter_idx: Option<u32>,
    bucket_ms: i64,
    target_name: Option<&str>,
    time_range: Option<&TimeRange>,
) -> Option<Vec<HpPoint>> {
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
    let result = invoke("query_hp_over_time", obj.into()).await;
    from_js(result)
}

/// Query effect uptime statistics for charts panel.
pub async fn query_effect_uptime(
    encounter_idx: Option<u32>,
    target_name: Option<&str>,
    time_range: Option<&TimeRange>,
    duration_secs: f32,
    source_filter: Option<&str>,
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
    if let Some(sf) = source_filter {
        js_set(&obj, "sourceFilter", &JsValue::from_str(sf));
    } else {
        js_set(&obj, "sourceFilter", &JsValue::NULL);
    }
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
    source_filter: Option<&str>,
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
    if let Some(sf) = source_filter {
        js_set(&obj, "sourceFilter", &JsValue::from_str(sf));
    } else {
        js_set(&obj, "sourceFilter", &JsValue::NULL);
    }
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
    sort_column: CombatLogSortColumn,
    sort_direction: SortDirection,
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
    let sc_js = serde_wasm_bindgen::to_value(&sort_column).unwrap_or(JsValue::NULL);
    js_set(&obj, "sortColumn", &sc_js);
    let sd_js = serde_wasm_bindgen::to_value(&sort_direction).unwrap_or(JsValue::NULL);
    js_set(&obj, "sortDirection", &sd_js);
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
    search_filter: Option<&str>,
    time_range: Option<&TimeRange>,
    event_filters: Option<&CombatLogFilters>,
    sort_column: CombatLogSortColumn,
    sort_direction: SortDirection,
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
    if let Some(sf) = search_filter {
        js_set(&obj, "searchFilter", &JsValue::from_str(sf));
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
    let sc_js = serde_wasm_bindgen::to_value(&sort_column).unwrap_or(JsValue::NULL);
    js_set(&obj, "sortColumn", &sc_js);
    let sd_js = serde_wasm_bindgen::to_value(&sort_direction).unwrap_or(JsValue::NULL);
    js_set(&obj, "sortDirection", &sd_js);
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

/// Query a death recap for one player death in an encounter.
pub async fn query_death_recap(
    encounter_idx: Option<u32>,
    player_name: &str,
    death_time_secs: f32,
) -> Option<DeathRecap> {
    let obj = js_sys::Object::new();
    if let Some(idx) = encounter_idx {
        js_set(&obj, "encounterIdx", &JsValue::from_f64(idx as f64));
    } else {
        js_set(&obj, "encounterIdx", &JsValue::NULL);
    }
    js_set(&obj, "playerName", &JsValue::from_str(player_name));
    js_set(&obj, "deathTimeSecs", &JsValue::from_f64(death_time_secs as f64));
    let result = invoke("query_death_recap", obj.into()).await;
    from_js(result)
}

/// Query damage taken summary (damage type breakdown + mitigation stats).
pub async fn query_damage_taken_summary(
    encounter_idx: Option<u32>,
    entity_name: &str,
    entity_id: Option<i64>,
    time_range: Option<&TimeRange>,
    entity_types: Option<&[&str]>,
) -> Option<DamageTakenSummary> {
    let obj = js_sys::Object::new();
    if let Some(idx) = encounter_idx {
        js_set(&obj, "encounterIdx", &JsValue::from_f64(idx as f64));
    } else {
        js_set(&obj, "encounterIdx", &JsValue::NULL);
    }
    js_set(&obj, "entityName", &JsValue::from_str(entity_name));
    js_set_entity_id(&obj, entity_id);
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
    let result = invoke("query_damage_taken_summary", obj.into()).await;
    from_js(result)
}

/// Query rotation analysis for a player in an encounter.
pub async fn query_rotation(
    encounter_idx: Option<u32>,
    source_name: &str,
    anchor_ability_id: i64,
    time_range: Option<&TimeRange>,
) -> Option<RotationAnalysis> {
    let obj = js_sys::Object::new();
    if let Some(idx) = encounter_idx {
        js_set(&obj, "encounterIdx", &JsValue::from_f64(idx as f64));
    } else {
        js_set(&obj, "encounterIdx", &JsValue::NULL);
    }
    js_set(&obj, "sourceName", &JsValue::from_str(source_name));
    js_set(
        &obj,
        "anchorAbilityId",
        &JsValue::from_f64(anchor_ability_id as f64),
    );
    if let Some(tr) = time_range {
        let tr_js = serde_wasm_bindgen::to_value(tr).unwrap_or(JsValue::NULL);
        js_set(&obj, "timeRange", &tr_js);
    } else {
        js_set(&obj, "timeRange", &JsValue::NULL);
    }
    let result = invoke("query_rotation", obj.into()).await;
    from_js(result)
}

/// Query ability usage statistics for a single player.
pub async fn query_ability_usage(
    source_name: &str,
    encounter_idx: Option<u32>,
    time_range: Option<&TimeRange>,
) -> Option<Vec<AbilityUsageRow>> {
    let obj = js_sys::Object::new();
    js_set(&obj, "sourceName", &JsValue::from_str(source_name));
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
    let result = invoke("query_ability_usage", obj.into()).await;
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
