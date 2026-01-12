//! BARAS - Combat log parser for Star Wars: The Old Republic
//!
//! This is the Tauri application entry point. The architecture:
//!
//! - `commands/` - All Tauri commands (overlay, service, profiles)
//! - `state/` - Application state (SharedState, RaidSlotRegistry)
//! - `service/` - Combat service (background log processing)
//! - `overlay/` - Overlay management (OverlayManager, spawn, state)
//! - `router` - Routes service updates to overlay threads
//! - `hotkeys` - Global hotkey registration (Windows/macOS only)

mod audio;
mod commands;
#[cfg(not(target_os = "linux"))]
mod hotkeys;
pub mod overlay;
mod router;
pub mod service;
pub mod state;
mod tray;
#[cfg(desktop)]
mod updater;

use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

use audio::create_audio_channel;
use overlay::{OverlayManager, OverlayState, SharedOverlayState};
use router::spawn_overlay_router;
use service::{CombatService, OverlayUpdate, ServiceHandle};
use tauri::Manager;

/// Auto-show all enabled overlays on startup (if overlays_visible is true)
fn spawn_auto_show_overlays(overlay_state: SharedOverlayState, service_handle: ServiceHandle) {
    tauri::async_runtime::spawn(async move {
        // Small delay to let everything initialize
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let config = service_handle.config().await;

        // Only show overlays if global visibility is enabled
        if !config.overlay_settings.overlays_visible {
            return;
        }

        // Use OverlayManager to show all enabled overlays
        let _ = OverlayManager::show_all(&overlay_state, &service_handle).await;
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Create shared overlay state
    let overlay_state = Arc::new(Mutex::new(OverlayState::default()));

    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup({
            let overlay_state = overlay_state.clone();
            move |app| {
                // Create channel for overlay updates
                let (overlay_tx, overlay_rx) = mpsc::channel::<OverlayUpdate>(64);

                // Create channel for audio events
                let (audio_tx, audio_rx) = create_audio_channel();

                // Clear old parquet data from previous sessions
                if let Err(e) = baras_core::storage::clear_data_dir() {
                    eprintln!("[STARTUP] Failed to clear data directory: {}", e);
                }

                // Create and spawn the combat service (includes audio service)
                let (service, handle) =
                    CombatService::new(app.handle().clone(), overlay_tx, audio_tx, audio_rx);
                tauri::async_runtime::spawn(service.run());

                // Store the service handle for commands
                app.handle().manage(handle.clone());

                // Spawn the overlay update router (needs service handle for registry updates)
                spawn_overlay_router(
                    overlay_rx,
                    overlay_state.clone(),
                    handle.clone(),
                    handle.shared.clone(),
                );

                // Auto-show enabled overlays on startup
                spawn_auto_show_overlays(overlay_state.clone(), handle.clone());

                // Register global hotkeys (not supported on Linux/Wayland)
                #[cfg(not(target_os = "linux"))]
                hotkeys::spawn_register_hotkeys(
                    app.handle().clone(),
                    overlay_state.clone(),
                    handle,
                );

                // Set up system tray
                let _ = tray::setup_tray(app.handle());

                // Check for updates in background
                #[cfg(desktop)]
                updater::spawn_update_check(app.handle().clone());

                Ok(())
            }
        })
        .manage(overlay_state)
        .manage(updater::PendingUpdate::default())
        .on_window_event(|window, event| {
            // Minimize to tray on close instead of quitting (if enabled)
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // Check if minimize_to_tray is enabled
                let minimize_to_tray = window
                    .app_handle()
                    .try_state::<ServiceHandle>()
                    .map(|handle| {
                        tauri::async_runtime::block_on(async {
                            handle.config().await.minimize_to_tray
                        })
                    })
                    .unwrap_or(true);

                if minimize_to_tray {
                    // Hide the window instead of closing
                    let _ = window.hide();
                    // Prevent the default close behavior
                    api.prevent_close();
                }
                // If minimize_to_tray is false, allow normal close (app quits)
            }
        })
        .invoke_handler(tauri::generate_handler![
            // Overlay commands
            commands::show_overlay,
            commands::hide_overlay,
            commands::hide_all_overlays,
            commands::show_all_overlays,
            commands::toggle_move_mode,
            commands::toggle_raid_rearrange,
            commands::get_overlay_status,
            commands::refresh_overlay_settings,
            commands::clear_raid_registry,
            commands::swap_raid_slots,
            commands::remove_raid_slot,
            // Service commands
            commands::get_log_files,
            commands::start_tailing,
            commands::stop_tailing,
            commands::refresh_log_index,
            commands::restart_watcher,
            commands::get_log_directory_size,
            commands::get_log_file_count,
            commands::cleanup_logs,
            commands::get_tailing_status,
            commands::get_watching_status,
            commands::get_current_metrics,
            commands::get_config,
            commands::update_config,
            commands::get_active_file,
            commands::get_session_info,
            commands::get_encounter_history,
            // File browser commands
            commands::open_historical_file,
            commands::resume_live_tailing,
            commands::is_live_tailing,
            commands::pick_audio_file,
            // Profile commands
            commands::get_profile_names,
            commands::get_active_profile,
            commands::save_profile,
            commands::load_profile,
            commands::delete_profile,
            commands::rename_profile,
            // Encounter editor commands
            commands::get_area_index,
            commands::fetch_area_bosses,
            commands::create_area,
            commands::create_boss,
            commands::create_encounter_item,
            commands::update_encounter_item,
            commands::delete_encounter_item,
            // Effect editor commands
            commands::get_effect_definitions,
            commands::update_effect_definition,
            commands::create_effect_definition,
            commands::delete_effect_definition,
            commands::duplicate_effect_definition,
            commands::get_icon_preview,
            // Parsely upload
            commands::upload_to_parsely,
            // Query commands
            commands::query_breakdown,
            commands::query_entity_breakdown,
            commands::query_raid_overview,
            commands::query_dps_over_time,
            commands::query_hps_over_time,
            commands::query_dtps_over_time,
            commands::query_effect_uptime,
            commands::query_effect_windows,
            commands::query_combat_log,
            commands::query_combat_log_count,
            commands::query_source_names,
            commands::query_target_names,
            commands::query_player_deaths,
            commands::query_encounter_timeline,
            commands::list_encounter_files,
            // Updater
            #[cfg(desktop)]
            updater::check_update,
            #[cfg(desktop)]
            updater::install_update,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
