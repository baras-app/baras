pub mod overlay;
pub mod service;
pub mod utils;
pub mod bridge;
use overlay::{
    OverlayState, SharedOverlayState, MetricType, OverlayType, OverlayCommand,
    create_metric_overlay, create_personal_overlay,
};
use baras_overlay::OverlayData;
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
        let metric_opacity = config.overlay_settings.metric_opacity;
        let personal_opacity = config.overlay_settings.personal_opacity;

        // Get current combat data once for all overlays
        let combat_data = if service_handle.is_tailing().await {
            service_handle.current_combat_data().await
        } else {
            None
        };

        for key in &enabled_keys {
            if key == "personal" {
                // Handle personal overlay
                let kind = OverlayType::Personal;
                let already_running = {
                    match overlay_state.lock() {
                        Ok(s) => s.is_running(kind),
                        Err(_) => continue,
                    }
                };

                if !already_running {
                    let position = config.overlay_settings.get_position("personal");
                    let personal_config = config.overlay_settings.personal_overlay.clone();

                    match create_personal_overlay(position, personal_config, personal_opacity) {
                        Ok(overlay_handle) => {
                            let tx = overlay_handle.tx.clone();

                            if let Ok(mut state) = overlay_state.lock() {
                                state.insert(overlay_handle);
                            }

                            // Send initial personal stats if available
                            if let Some(ref data) = combat_data
                                && let Some(stats) = data.to_personal_stats()
                            {
                                let _ = tx.send(OverlayCommand::UpdateData(
                                    OverlayData::Personal(stats)
                                )).await;
                            }

                            eprintln!("Auto-showed personal overlay on startup");
                        }
                        Err(e) => eprintln!("Failed to auto-show personal overlay: {}", e),
                    }
                }
            } else if let Some(overlay_type) = MetricType::from_config_key(key) {
                // Handle metric overlay
                let kind = OverlayType::Metric(overlay_type);

                // Check if already running
                {
                    let state = match overlay_state.lock() {
                        Ok(s) => s,
                        Err(_) => continue,
                    };
                    if state.is_running(kind) {
                        continue;
                    }
                }

                // Load position, appearance, and spawn
                let position = config.overlay_settings.get_position(key);
                let appearance = overlay::get_appearance_for_type(&config.overlay_settings, overlay_type);

                match create_metric_overlay(overlay_type, position, appearance, metric_opacity) {
                    Ok(overlay_handle) => {
                        let tx = overlay_handle.tx.clone();

                        // Update state
                        {
                            let mut state = match overlay_state.lock() {
                                Ok(s) => s,
                                Err(_) => continue,
                            };
                            state.insert(overlay_handle);
                        }

                        // Send current metrics if available
                        if let Some(ref data) = combat_data
                            && !data.metrics.is_empty()
                        {
                            let entries = overlay::create_entries_for_type(overlay_type, &data.metrics);
                            let _ = tx.send(OverlayCommand::UpdateData(
                                OverlayData::Metrics(entries)
                            )).await;
                        }

                        eprintln!("Auto-showed {} overlay on startup", overlay_type.config_key());
                    }
                    Err(e) => eprintln!("Failed to auto-show {} overlay: {}", key, e),
                }
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
            overlay::commands::show_overlay,
            overlay::commands::hide_overlay,
            overlay::commands::hide_all_overlays,
            overlay::commands::show_all_overlays,
            overlay::commands::toggle_move_mode,
            overlay::commands::get_overlay_status,
            overlay::commands::refresh_overlay_settings,
            // Service commands
            service::get_log_files,
            service::start_tailing,
            service::stop_tailing,
            service::refresh_log_index,
            service::restart_watcher,
            service::get_tailing_status,
            service::get_watching_status,
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
