pub mod overlay;
pub mod service;
pub mod utils;
pub mod bridge;
use overlay::{OverlayState, SharedOverlayState, OverlayType, OverlayHandle, OverlayCommand, PersonalOverlayHandle};
use bridge::spawn_overlay_bridge;
use service::{CombatService, OverlayUpdate, ServiceHandle};
use tauri::Manager;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

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

        let enabled_keys = config.overlay_settings.enabled_types();

        for key in &enabled_keys {
            // Skip personal overlay - handled separately
            if key == "personal" {
                continue;
            }

            let Some(overlay_type) = OverlayType::from_config_key(key) else {
                continue;
            };

            // Check if already running
            {
                let state = match overlay_state.lock() {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                if state.is_running(overlay_type) {
                    continue;
                }
            }

            // Load position, appearance, and spawn
            let position = config.overlay_settings.get_position(key);
            let appearance = config.overlay_settings.get_appearance(key);
            let background_alpha = config.overlay_settings.background_alpha;
            let (tx, handle) = overlay::spawn_overlay(overlay_type, position, appearance, background_alpha);

            // Update state
            {
                let mut state = match overlay_state.lock() {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                state.insert(overlay_type, OverlayHandle { tx: tx.clone(), handle });
            }

            // Send current metrics if tailing
            if service_handle.is_tailing().await
                && let Some(metrics) = service_handle.current_metrics().await
                && !metrics.is_empty()
            {
                let entries = overlay::create_entries_for_type(overlay_type, &metrics);
                let _ = tx.send(OverlayCommand::UpdateEntries(entries)).await;
            }

            eprintln!("Auto-showed {} overlay on startup", overlay_type.config_key());
        }

        // Auto-show personal overlay if enabled
        if enabled_keys.iter().any(|k| k == "personal") {
            let already_running = {
                match overlay_state.lock() {
                    Ok(s) => s.is_personal_running(),
                    Err(_) => return,
                }
            };

            if !already_running {
                let position = config.overlay_settings.get_position("personal");
                let personal_config = config.overlay_settings.personal_overlay.clone();
                let background_alpha = config.overlay_settings.background_alpha;
                let (tx, handle) = overlay::spawn_personal_overlay(position, personal_config, background_alpha);

                if let Ok(mut state) = overlay_state.lock() {
                    state.set_personal(PersonalOverlayHandle { tx, handle });
                }

                eprintln!("Auto-showed personal overlay on startup");
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

                // Spawn the overlay update bridge
                spawn_overlay_bridge(overlay_rx, overlay_state.clone());

                // Auto-show enabled overlays on startup
                spawn_auto_show_overlays(overlay_state.clone(), handle);

                Ok(())
            }
        })
        .manage(overlay_state)
        .invoke_handler(tauri::generate_handler![
            // Overlay commands
            overlay::show_overlay,
            overlay::hide_overlay,
            overlay::hide_all_overlays,
            overlay::show_all_overlays,
            overlay::toggle_move_mode,
            overlay::get_overlay_status,
            overlay::show_personal_overlay,
            overlay::hide_personal_overlay,
            overlay::refresh_overlay_settings,
            // Service commands
            service::get_log_files,
            service::start_tailing,
            service::stop_tailing,
            service::refresh_log_index,
            service::get_tailing_status,
            service::get_current_metrics,
            service::get_config,
            service::update_config,
            service::get_active_file,
            service::get_session_info,
            // Utilities
            utils::default_log_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
