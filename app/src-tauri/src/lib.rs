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

use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

use router::spawn_overlay_router;
use overlay::{OverlayManager, OverlayState, SharedOverlayState};
use service::{CombatService, OverlayUpdate, ServiceHandle};
use audio::create_audio_channel;
use tauri::Manager;

/// Auto-show all enabled overlays on startup (if overlays_visible is true)
fn spawn_auto_show_overlays(
    overlay_state: SharedOverlayState,
    service_handle: ServiceHandle,
) {
    tauri::async_runtime::spawn(async move {
        // Small delay to let everything initialize
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let config = service_handle.config().await;

        // Only show overlays if global visibility is enabled
        if !config.overlay_settings.overlays_visible {
            eprintln!("Overlays hidden on startup (overlays_visible=false)");
            return;
        }

        // Use OverlayManager to show all enabled overlays
        match OverlayManager::show_all(&overlay_state, &service_handle).await {
            Ok(metric_types) => {
                eprintln!("Auto-showed {} metric overlays on startup", metric_types.len());
            }
            Err(e) => {
                eprintln!("Failed to auto-show overlays on startup: {}", e);
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Create shared overlay state
    let overlay_state = Arc::new(Mutex::new(OverlayState::default()));

    tauri::Builder::default()
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

                // Create and spawn the combat service (includes audio service)
                let (service, handle) = CombatService::new(app.handle().clone(), overlay_tx, audio_tx, audio_rx);
                tauri::async_runtime::spawn(service.run());

                // Store the service handle for commands
                app.handle().manage(handle.clone());

                // Spawn the overlay update router (needs service handle for registry updates)
                spawn_overlay_router(overlay_rx, overlay_state.clone(), handle.clone());

                // Auto-show enabled overlays on startup
                spawn_auto_show_overlays(overlay_state.clone(), handle.clone());

                // Register global hotkeys (not supported on Linux/Wayland)
                #[cfg(not(target_os = "linux"))]
                hotkeys::spawn_register_hotkeys(app.handle().clone(), overlay_state.clone(), handle);

                // Set up system tray
                if let Err(e) = tray::setup_tray(app.handle()) {
                    eprintln!("[TRAY] Failed to set up system tray: {}", e);
                }

                Ok(())
            }
        })
        .manage(overlay_state)
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
            // Profile commands
            commands::get_profile_names,
            commands::get_active_profile,
            commands::save_profile,
            commands::load_profile,
            commands::delete_profile,
            commands::rename_profile,
            // Encounter editor commands
            commands::get_encounter_timers,
            commands::update_encounter_timer,
            commands::create_encounter_timer,
            commands::delete_encounter_timer,
            commands::duplicate_encounter_timer,
            commands::get_encounter_bosses,
            commands::get_area_index,
            commands::get_timers_for_area,
            commands::get_bosses_for_area,
            commands::create_boss,
            commands::create_area,
            commands::get_phases_for_area,
            commands::create_phase,
            commands::update_phase,
            commands::delete_phase,
            commands::get_counters_for_area,
            commands::create_counter,
            commands::update_counter,
            commands::delete_counter,
            commands::get_challenges_for_area,
            commands::create_challenge,
            commands::update_challenge,
            commands::delete_challenge,
            commands::get_entities_for_area,
            commands::create_entity,
            commands::update_entity,
            commands::delete_entity,
            // Effect editor commands
            commands::get_effect_definitions,
            commands::update_effect_definition,
            commands::create_effect_definition,
            commands::delete_effect_definition,
            commands::duplicate_effect_definition,
            commands::get_effect_files,
            // Parsely upload
            commands::upload_to_parsely,
            // Audio
            commands::pick_audio_file,
            commands::list_bundled_sounds,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
