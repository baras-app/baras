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

mod commands;
#[cfg(not(target_os = "linux"))]
mod hotkeys;
pub mod overlay;
mod router;
pub mod service;
pub mod state;

use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

use router::spawn_overlay_router;
use overlay::{OverlayManager, OverlayState, SharedOverlayState};
use service::{CombatService, OverlayUpdate, ServiceHandle};
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

                // Create and spawn the combat service
                let (service, handle) = CombatService::new(app.handle().clone(), overlay_tx);
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

                Ok(())
            }
        })
        .manage(overlay_state)
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
            // Profile commands
            commands::get_profile_names,
            commands::get_active_profile,
            commands::save_profile,
            commands::load_profile,
            commands::delete_profile,
            commands::rename_profile,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
