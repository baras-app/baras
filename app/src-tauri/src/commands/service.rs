//! Service Tauri commands
//!
//! Commands for log files, tailing, configuration, session info, and profiles.

use std::path::PathBuf;
use tauri::{AppHandle, State};

use baras_core::EncounterSummary;
use baras_core::PlayerMetrics;
use baras_core::context::{AppConfig, AppConfigExt, OverlayAppearanceConfig};

use crate::overlay::{MetricType, OverlayCommand, OverlayManager, OverlayType, SharedOverlayState};
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
    delete_small: bool,
    retention_days: Option<u32>,
) -> Result<(u32, u32, u32), String> {
    Ok(handle.cleanup_logs(delete_empty, delete_small, retention_days).await)
}

#[tauri::command]
pub async fn refresh_file_sizes(handle: State<'_, ServiceHandle>) -> Result<(), String> {
    handle.refresh_file_sizes().await;
    Ok(())
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
/// Preview (play) a sound file so the user can hear it in the editor
#[tauri::command]
pub async fn preview_sound(
    filename: String,
    app: tauri::AppHandle,
    handle: State<'_, ServiceHandle>,
) -> Result<(), String> {
    use tauri::Manager;

    if filename.is_empty() {
        return Err("No sound file specified".into());
    }

    // Read volume from current audio settings
    let volume = handle.config().await.audio.volume;

    // Resolve sound file path: user dir takes priority over bundled
    let user_path = dirs::config_dir()
        .map(|p| p.join("baras").join("sounds").join(&filename))
        .unwrap_or_else(|| PathBuf::from(&filename));

    let bundled_path = app
        .path()
        .resolve(
            format!("definitions/sounds/{}", filename),
            tauri::path::BaseDirectory::Resource,
        )
        .ok()
        .filter(|p| p.exists())
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .nth(2)
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."))
                .join("core/definitions/sounds")
                .join(&filename)
        });

    let path = if user_path.exists() {
        user_path
    } else if bundled_path.exists() {
        bundled_path
    } else {
        return Err(format!("Sound file not found: {}", filename));
    };

    std::thread::spawn(move || {
        use rodio::{Decoder, OutputStream, Sink};
        use std::fs::File;
        use std::io::BufReader;

        let Ok((_stream, stream_handle)) = OutputStream::try_default() else {
            return;
        };
        let Ok(file) = File::open(&path) else { return };
        let Ok(source) = Decoder::new(BufReader::new(file)) else {
            return;
        };
        let Ok(sink) = Sink::try_new(&stream_handle) else {
            return;
        };

        sink.set_volume(volume as f32 / 100.0);
        sink.append(source);
        sink.sleep_until_end();
    });

    Ok(())
}

/// List available sound files from both bundled and user directories
#[tauri::command]
pub async fn list_sound_files(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    use std::collections::BTreeSet;
    use tauri::Manager;

    let mut files = BTreeSet::new();

    // Bundled sounds dir
    let bundled = app
        .path()
        .resolve("definitions/sounds", tauri::path::BaseDirectory::Resource)
        .ok()
        .filter(|p: &std::path::PathBuf| p.exists())
        .unwrap_or_else(|| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .nth(2)
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join("core/definitions/sounds")
        });

    // User sounds dir
    let user = dirs::config_dir()
        .map(|p| p.join("baras").join("sounds"))
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    for dir in [&bundled, &user] {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        if ext.eq_ignore_ascii_case("mp3") || ext.eq_ignore_ascii_case("wav") {
                            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                                files.insert(name.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(files.into_iter().collect())
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

#[tauri::command]
pub async fn pick_log_directory(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let folder = app
        .dialog()
        .file()
        .set_title("Select Combat Log Directory")
        .blocking_pick_folder();

    Ok(folder.map(|f| f.to_string()))
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

#[tauri::command]
pub async fn set_encounter_parsely_link(
    encounter_id: u64,
    link: String,
    handle: State<'_, ServiceHandle>,
) -> Result<bool, String> {
    Ok(handle.set_encounter_parsely_link(encounter_id, link).await)
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

    // Sync enabled state with actual running overlays before saving,
    // but only when overlays are actually visible and not auto-hidden.
    // When auto-hidden or globally hidden, overlays are temporarily shut down
    // so running state doesn't reflect the user's intent — the config's
    // enabled map is already authoritative from show()/hide() calls.
    if config.overlay_settings.overlays_visible && !handle.shared.auto_hide.is_auto_hidden() {
        if let Ok(state) = overlay_state.lock() {
            sync_enabled_with_running(&mut config, &state);
        }
        // Flush current overlay positions so the profile captures where
        // overlays actually are on screen, not stale saved positions.
        OverlayManager::flush_positions(&overlay_state, &mut config).await;
    }

    config.save_profile(name).map_err(|e| e.to_string())?;
    *handle.shared.config.write().await = config.clone();
    config.save().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn load_profile(
    name: String,
    handle: State<'_, ServiceHandle>,
    overlay_state: State<'_, SharedOverlayState>,
) -> Result<(), String> {
    let old_config = handle.config().await;
    let old_slots = old_config.overlay_settings.raid_overlay.total_slots();

    let mut config = old_config;
    config.load_profile(&name).map_err(|e| e.to_string())?;
    let new_slots = config.overlay_settings.raid_overlay.total_slots();

    *handle.shared.config.write().await = config.clone();
    config.save().map_err(|e| e.to_string())?;

    // Update raid registry max slots if grid size changed between profiles
    if new_slots != old_slots {
        handle
            .shared
            .raid_registry
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .set_max_slots(new_slots);
    }

    // Reset move mode on profile switch
    let txs: Vec<_> = {
        if let Ok(mut state) = overlay_state.lock() {
            state.move_mode = false;
            state.rearrange_mode = false;
            state.all_txs().into_iter().cloned().collect()
        } else {
            vec![]
        }
    };

    // Broadcast move mode reset to all overlays
    for tx in txs {
        let _ = tx.send(OverlayCommand::SetMoveMode(false)).await;
    }

    Ok(())
}

#[tauri::command]
pub async fn delete_profile(name: String, handle: State<'_, ServiceHandle>) -> Result<(), String> {
    let mut config = handle.config().await;
    config.delete_profile(&name).map_err(|e| e.to_string())?;
    *handle.shared.config.write().await = config.clone();
    config.save().map_err(|e| e.to_string())?;
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
    config.save().map_err(|e| e.to_string())?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Role Default Profile Commands
// ─────────────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_default_profiles_per_role(
    handle: State<'_, ServiceHandle>,
) -> Result<std::collections::HashMap<String, String>, String> {
    let config = handle.config().await;
    Ok(config.default_profile_per_role.clone())
}

#[tauri::command]
pub async fn set_default_profile_for_role(
    role: String,
    profile_name: Option<String>,
    handle: State<'_, ServiceHandle>,
) -> Result<(), String> {
    let mut config = handle.config().await;
    match profile_name {
        Some(name) => {
            // Validate the profile exists
            if !config.profiles.iter().any(|p| p.name == name) {
                return Err("Profile not found".to_string());
            }
            config.default_profile_per_role.insert(role, name);
        }
        None => {
            config.default_profile_per_role.remove(&role);
        }
    }
    *handle.shared.config.write().await = config.clone();
    config.save().map_err(|e| e.to_string())?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Changelog Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Response for changelog check - contains HTML content if changelog should be shown.
#[derive(serde::Serialize)]
pub struct ChangelogResponse {
    pub should_show: bool,
    pub html: Option<String>,
    pub version: String,
}

/// Embedded changelog content (located at CHANGELOG.md in repo root)
const CHANGELOG_MD: &str = include_str!("../../../../CHANGELOG.md");

/// Check if changelog should be shown and return rendered HTML.
/// Compares current app version with last viewed version in config.
/// Always returns HTML content so it can be viewed on demand.
#[tauri::command]
pub async fn get_changelog(
    app: AppHandle,
    handle: State<'_, ServiceHandle>,
) -> Result<ChangelogResponse, String> {
    let config = handle.config().await;
    let current_version = app.config().version.clone().unwrap_or_default();

    let should_show = config
        .last_viewed_changelog_version
        .as_ref()
        .map(|v| v != &current_version)
        .unwrap_or(true); // Show if never viewed

    Ok(ChangelogResponse {
        should_show,
        html: Some(render_changelog_html()),
        version: current_version,
    })
}

/// Mark the changelog as viewed for the current version.
#[tauri::command]
pub async fn mark_changelog_viewed(
    app: AppHandle,
    handle: State<'_, ServiceHandle>,
) -> Result<(), String> {
    let current_version = app.config().version.clone().unwrap_or_default();
    let mut config = handle.config().await;
    config.last_viewed_changelog_version = Some(current_version);
    *handle.shared.config.write().await = config.clone();
    config.save().map_err(|e| e.to_string())?;
    Ok(())
}

/// Render markdown changelog to HTML.
fn render_changelog_html() -> String {
    use pulldown_cmark::{Options, Parser, html};

    let options = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES;
    let parser = Parser::new_ext(CHANGELOG_MD, options);

    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);
    html_output
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Sync the enabled map with actual running overlay state.
/// Only call this when overlays are globally visible and not auto-hidden,
/// otherwise running state doesn't reflect user intent.
fn sync_enabled_with_running(config: &mut AppConfig, overlay_state: &crate::overlay::OverlayState) {
    // Sync all fixed overlay types
    let fixed_types = [
        OverlayType::Personal,
        OverlayType::Raid,
        OverlayType::BossHealth,
        OverlayType::TimersA,
        OverlayType::TimersB,
        OverlayType::Challenges,
        OverlayType::Alerts,
        OverlayType::EffectsA,
        OverlayType::EffectsB,
        OverlayType::Cooldowns,
        OverlayType::DotTracker,
        OverlayType::Notes,
        OverlayType::CombatTime,
    ];

    for overlay_type in &fixed_types {
        let running = overlay_state.is_running(*overlay_type);
        config
            .overlay_settings
            .set_enabled(overlay_type.config_key(), running);
    }

    // Sync all metric overlay states
    for metric_type in MetricType::all() {
        let running = overlay_state.is_running(OverlayType::Metric(*metric_type));
        config
            .overlay_settings
            .set_enabled(metric_type.config_key(), running);
    }
}
