pub mod overlay;
pub mod service;
pub mod utils;
pub mod bridge;
use overlay::{OverlayState, SharedOverlayState, PositionEvent, OverlayType, OverlayHandle, OverlayCommand};
use bridge::spawn_overlay_bridge;
use service::{CombatService, OverlayUpdate, ServiceHandle};
use tauri::Manager;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use baras_core::context::OverlayPositionConfig;

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

        for key in enabled_keys {
            let Some(overlay_type) = OverlayType::from_config_key(&key) else {
                continue;
            };

            // Check if already running
            let (is_running, position_tx) = {
                let state = match overlay_state.lock() {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                (state.is_running(overlay_type), state.position_tx().cloned())
            };

            if is_running {
                continue;
            }

            // Load position and spawn
            let position = config.overlay_settings.get_position(&key);
            let (tx, handle) = overlay::spawn_overlay(overlay_type, position, position_tx);

            // Update state
            {
                let mut state = match overlay_state.lock() {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                state.insert(overlay_type, OverlayHandle { tx: tx.clone(), handle });
            }

            // Send current metrics if tailing
            if service_handle.is_tailing().await {
                if let Some(metrics) = service_handle.current_metrics().await {
                    if !metrics.is_empty() {
                        let entries = overlay::create_entries_for_type(overlay_type, &metrics);
                        let _ = tx.send(OverlayCommand::UpdateEntries(entries)).await;
                    }
                }
            }

            eprintln!("Auto-showed {} overlay on startup", overlay_type.config_key());
        }
    });
}

/// Spawn a task to handle position events and persist them to config
fn spawn_position_persistence(
    position_rx: std::sync::mpsc::Receiver<PositionEvent>,
    service_handle: ServiceHandle,
) {
    std::thread::spawn(move || {
        while let Ok(event) = position_rx.recv() {
            // Use blocking runtime to call async methods
            let handle = service_handle.clone();
            let _ = std::thread::spawn(move || {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap()
                    .block_on(async {
                        let mut config = handle.config().await;
                        config.overlay_settings.set_position(
                            event.overlay_type.config_key(),
                            OverlayPositionConfig {
                                x: event.x,
                                y: event.y,
                                width: event.width,
                                height: event.height,
                                monitor_id: None,
                            },
                        );
                        let _ = handle.update_config(config).await;
                    });
            }).join();
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Create shared overlay state upfront so we can clone it for the bridge
    let overlay_state = Arc::new(Mutex::new(OverlayState::default()));

    // Create position event channel for persistence
    let (position_tx, position_rx) = std::sync::mpsc::channel::<PositionEvent>();

    // Set position_tx on overlay state
    {
        let mut state = overlay_state.lock().unwrap();
        state.set_position_tx(position_tx);
    }

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

                // Spawn position persistence handler
                spawn_position_persistence(position_rx, handle.clone());

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
