//! Service Tauri commands
//!
//! Commands for log files, tailing, configuration, session info, and profiles.

use std::path::PathBuf;
use tauri::State;

use baras_core::EncounterSummary;
use baras_core::PlayerMetrics;
use baras_core::context::{AppConfig, AppConfigExt, OverlayAppearanceConfig};

use crate::overlay::{MetricType, OverlayType, SharedOverlayState};
use crate::service::{LogFileInfo, ServiceHandle, SessionInfo};

// ─────────────────────────────────────────────────────────────────────────────
// Log File Commands
// ─────────────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_log_files(handle: State<'_, ServiceHandle>) -> Result<Vec<LogFileInfo>, String> {
    Ok(handle.log_files().await)
}

#[tauri::command]
pub async fn refresh_log_index(handle: State<'_, ServiceHandle>) -> Result<(), String> {
    handle.refresh_index().await
}

#[tauri::command]
pub async fn restart_watcher(handle: State<'_, ServiceHandle>) -> Result<(), String> {
    handle.restart_watcher().await
}

#[tauri::command]
pub async fn get_log_directory_size(handle: State<'_, ServiceHandle>) -> Result<u64, String> {
    Ok(handle.log_directory_size().await)
}

#[tauri::command]
pub async fn get_log_file_count(handle: State<'_, ServiceHandle>) -> Result<usize, String> {
    Ok(handle.log_file_count().await)
}

#[tauri::command]
pub async fn cleanup_logs(
    handle: State<'_, ServiceHandle>,
    delete_empty: bool,
    retention_days: Option<u32>,
) -> Result<(u32, u32), String> {
    Ok(handle.cleanup_logs(delete_empty, retention_days).await)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tailing Commands
// ─────────────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn start_tailing(path: PathBuf, handle: State<'_, ServiceHandle>) -> Result<(), String> {
    handle.start_tailing(path).await
}

#[tauri::command]
pub async fn stop_tailing(handle: State<'_, ServiceHandle>) -> Result<(), String> {
    handle.stop_tailing().await
}

#[tauri::command]
pub async fn get_tailing_status(handle: State<'_, ServiceHandle>) -> Result<bool, String> {
    Ok(handle.is_tailing().await)
}

#[tauri::command]
pub async fn get_watching_status(handle: State<'_, ServiceHandle>) -> Result<bool, String> {
    Ok(handle.is_watching())
}

#[tauri::command]
pub async fn get_active_file(handle: State<'_, ServiceHandle>) -> Result<Option<String>, String> {
    Ok(handle.active_file().await)
}

// ─────────────────────────────────────────────────────────────────────────────
// File Browser Commands
// ─────────────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn open_historical_file(
    path: PathBuf,
    handle: State<'_, ServiceHandle>,
) -> Result<(), String> {
    handle.open_historical_file(path).await
}

#[tauri::command]
pub async fn resume_live_tailing(handle: State<'_, ServiceHandle>) -> Result<(), String> {
    handle.resume_live_tailing().await
}

#[tauri::command]
pub fn is_live_tailing(handle: State<'_, ServiceHandle>) -> Result<bool, String> {
    Ok(handle.is_live_tailing())
}
#[tauri::command]
pub async fn pick_audio_file(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let file = app
        .dialog()
        .file()
        .add_filter("Audio Files", &["mp3", "wav"])
        .blocking_pick_file();

    Ok(file.map(|f| f.to_string()))
}

// ─────────────────────────────────────────────────────────────────────────────
// Config Commands
// ─────────────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_config(handle: State<'_, ServiceHandle>) -> Result<AppConfig, String> {
    let mut config = handle.config().await;

    // Populate default appearances for each overlay type
    for metric_type in MetricType::all() {
        let key = metric_type.config_key();
        config.overlay_settings.default_appearances.insert(
            key.to_string(),
            OverlayAppearanceConfig::default_for_type(key),
        );
    }

    Ok(config)
}

#[tauri::command]
pub async fn update_config(
    config: AppConfig,
    handle: State<'_, ServiceHandle>,
) -> Result<(), String> {
    handle.update_config(config).await
}

// ─────────────────────────────────────────────────────────────────────────────
// Session Commands
// ─────────────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_current_metrics(
    handle: State<'_, ServiceHandle>,
) -> Result<Option<Vec<PlayerMetrics>>, String> {
    Ok(handle.current_combat_data().await.map(|d| d.metrics))
}

#[tauri::command]
pub async fn get_session_info(
    handle: State<'_, ServiceHandle>,
) -> Result<Option<SessionInfo>, String> {
    Ok(handle.session_info().await)
}

#[tauri::command]
pub async fn get_encounter_history(
    handle: State<'_, ServiceHandle>,
) -> Result<Vec<EncounterSummary>, String> {
    Ok(handle.encounter_history().await)
}

// ─────────────────────────────────────────────────────────────────────────────
// Profile Commands
// ─────────────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_profile_names(handle: State<'_, ServiceHandle>) -> Result<Vec<String>, String> {
    let config = handle.config().await;
    Ok(config.profile_names())
}

#[tauri::command]
pub async fn get_active_profile(
    handle: State<'_, ServiceHandle>,
) -> Result<Option<String>, String> {
    let config = handle.config().await;
    Ok(config.active_profile_name.clone())
}

#[tauri::command]
pub async fn save_profile(
    name: String,
    handle: State<'_, ServiceHandle>,
    overlay_state: State<'_, SharedOverlayState>,
) -> Result<(), String> {
    let mut config = handle.config().await;

    // Sync enabled state with actual running overlays before saving
    if let Ok(state) = overlay_state.lock() {
        sync_enabled_with_running(&mut config, &state);
    }

    config.save_profile(name).map_err(|e| e.to_string())?;
    *handle.shared.config.write().await = config.clone();
    config.save();
    Ok(())
}

#[tauri::command]
pub async fn load_profile(name: String, handle: State<'_, ServiceHandle>) -> Result<(), String> {
    let mut config = handle.config().await;
    config.load_profile(&name).map_err(|e| e.to_string())?;
    *handle.shared.config.write().await = config.clone();
    config.save();
    Ok(())
}

#[tauri::command]
pub async fn delete_profile(name: String, handle: State<'_, ServiceHandle>) -> Result<(), String> {
    let mut config = handle.config().await;
    config.delete_profile(&name).map_err(|e| e.to_string())?;
    *handle.shared.config.write().await = config.clone();
    config.save();
    Ok(())
}

#[tauri::command]
pub async fn rename_profile(
    old_name: String,
    new_name: String,
    handle: State<'_, ServiceHandle>,
) -> Result<(), String> {
    let mut config = handle.config().await;
    config
        .rename_profile(&old_name, new_name)
        .map_err(|e| e.to_string())?;
    *handle.shared.config.write().await = config.clone();
    config.save();
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Sync the enabled map with actual running overlay state
fn sync_enabled_with_running(config: &mut AppConfig, overlay_state: &crate::overlay::OverlayState) {
    // Sync raid overlay state
    let raid_running = overlay_state.is_running(OverlayType::Raid);
    config.overlay_settings.set_enabled("raid", raid_running);

    // Sync personal overlay state
    let personal_running = overlay_state.is_running(OverlayType::Personal);
    config
        .overlay_settings
        .set_enabled("personal", personal_running);

    // Sync all metric overlay states
    for metric_type in MetricType::all() {
        let running = overlay_state.is_running(OverlayType::Metric(*metric_type));
        config
            .overlay_settings
            .set_enabled(metric_type.config_key(), running);
    }
}
