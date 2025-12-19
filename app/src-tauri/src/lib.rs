pub mod overlay;
pub mod service;
pub mod utils;

use overlay::{OverlayState, SharedOverlayState};
use service::{CombatService, OverlayUpdate};
use tauri::Manager;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Create shared overlay state upfront so we can clone it for the bridge
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
                app.handle().manage(handle);

                // Spawn the overlay update bridge
                spawn_overlay_bridge(overlay_rx, overlay_state.clone());

                Ok(())
            }
        })
        .manage(overlay_state)
        .invoke_handler(tauri::generate_handler![
            // Overlay commands
            overlay::show_overlay,
            overlay::hide_overlay,
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
            // Utilities
            utils::default_log_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Bridge between service overlay updates and the overlay thread
fn spawn_overlay_bridge(
    mut rx: mpsc::Receiver<OverlayUpdate>,
    overlay_state: SharedOverlayState,
) {
    tauri::async_runtime::spawn(async move {
        while let Some(update) = rx.recv().await {
            // Get the overlay channel if overlay is running
            let tx = {
                let state = overlay_state.lock().ok();
                state.and_then(|s| s.tx.clone())
            };

            if let Some(tx) = tx {
                match update {
                    OverlayUpdate::MetricsUpdated(metrics) => {
                        // Convert PlayerMetrics to MeterEntry for overlay
                        let entries: Vec<_> = metrics
                            .iter()
                            .map(|m| {
                                baras_overlay::MeterEntry::new(&m.name, m.dps, m.dps)
                            })
                            .collect();

                        let max_dps = entries.iter().map(|e| e.value).fold(0, i64::max);
                        let entries: Vec<_> = entries
                            .into_iter()
                            .map(|mut e| {
                                e.max_value = max_dps;
                                e
                            })
                            .collect();

                        let _ = tx.send(overlay::OverlayCommand::UpdateEntries(entries)).await;
                    }
                    OverlayUpdate::CombatStarted => {
                        // Could show overlay or clear entries
                    }
                    OverlayUpdate::CombatEnded => {
                        // Could hide overlay or freeze display
                    }
                }
            }
        }
    });
}
