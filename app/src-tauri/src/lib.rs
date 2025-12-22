pub mod overlay;
pub mod service;
pub mod utils;
pub mod bridge;
use overlay::{
    OverlayState, SharedOverlayState, MetricType, OverlayType, OverlayCommand,
    create_metric_overlay, create_personal_overlay, create_raid_overlay,
};
use baras_overlay::{RaidGridLayout, RaidOverlayConfig};
use baras_overlay::OverlayData;
use bridge::spawn_overlay_bridge;
use service::{CombatService, OverlayUpdate, ServiceHandle};
use tauri::Manager;
use tauri::tray::TrayIconBuilder;
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
            } else if key == "raid" {
                // Handle raid overlay
                let kind = OverlayType::Raid;
                let already_running = {
                    match overlay_state.lock() {
                        Ok(s) => s.is_running(kind),
                        Err(_) => continue,
                    }
                };

                if !already_running {
                    let position = config.overlay_settings.get_position("raid");
                    let raid_settings = &config.overlay_settings.raid_overlay;
                    let layout = RaidGridLayout::from_config(raid_settings);
                    let raid_config: RaidOverlayConfig = raid_settings.clone().into();
                    let raid_opacity = config.overlay_settings.raid_opacity;

                    match create_raid_overlay(position, layout, raid_config, raid_opacity) {
                        Ok(overlay_handle) => {
                            if let Ok(mut state) = overlay_state.lock() {
                                state.insert(overlay_handle);
                            }
                            eprintln!("Auto-showed raid overlay on startup");
                        }
                        Err(e) => eprintln!("Failed to auto-show raid overlay: {}", e),
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

/// Register global hotkeys from config (Windows/macOS only)
#[cfg(not(target_os = "linux"))]
fn spawn_register_hotkeys(
    app_handle: tauri::AppHandle,
    overlay_state: SharedOverlayState,
    service_handle: ServiceHandle,
) {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

    tauri::async_runtime::spawn(async move {
        // Small delay to ensure everything is initialized
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        let config = service_handle.config().await;
        let hotkeys = &config.hotkeys;

        let global_shortcut = app_handle.global_shortcut();

        // Register toggle visibility hotkey
        if let Some(ref key_str) = hotkeys.toggle_visibility {
            if let Ok(shortcut) = key_str.parse::<Shortcut>() {
                let state = overlay_state.clone();
                let handle = service_handle.clone();

                if let Err(e) = global_shortcut.on_shortcut(shortcut, move |_app, _shortcut, event| {
                    // Only toggle on key press, not release or repeat
                    if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        let state = state.clone();
                        let handle = handle.clone();
                        tauri::async_runtime::spawn(async move {
                            toggle_visibility_hotkey(state, handle).await;
                        });
                    }
                }) {
                    eprintln!("[HOTKEY] Failed to register visibility hotkey: {}", e);
                } else {
                    eprintln!("[HOTKEY] Registered visibility hotkey: {}", key_str);
                }
            } else {
                eprintln!("[HOTKEY] Invalid visibility hotkey format: {}", key_str);
            }
        }

        // Register toggle move mode hotkey
        if let Some(ref key_str) = hotkeys.toggle_move_mode {
            if let Ok(shortcut) = key_str.parse::<Shortcut>() {
                let state = overlay_state.clone();

                if let Err(e) = global_shortcut.on_shortcut(shortcut, move |_app, _shortcut, event| {
                    // Only toggle on key press, not release or repeat
                    if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        let state = state.clone();
                        tauri::async_runtime::spawn(async move {
                            toggle_move_mode_hotkey(state).await;
                        });
                    }
                }) {
                    eprintln!("[HOTKEY] Failed to register move mode hotkey: {}", e);
                } else {
                    eprintln!("[HOTKEY] Registered move mode hotkey: {}", key_str);
                }
            } else {
                eprintln!("[HOTKEY] Invalid move mode hotkey format: {}", key_str);
            }
        }

        // Register toggle rearrange mode hotkey
        if let Some(ref key_str) = hotkeys.toggle_rearrange_mode {
            if let Ok(shortcut) = key_str.parse::<Shortcut>() {
                let state = overlay_state.clone();

                if let Err(e) = global_shortcut.on_shortcut(shortcut, move |_app, _shortcut, event| {
                    // Only toggle on key press, not release or repeat
                    if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        let state = state.clone();
                        tauri::async_runtime::spawn(async move {
                            toggle_rearrange_mode_hotkey(state).await;
                        });
                    }
                }) {
                    eprintln!("[HOTKEY] Failed to register rearrange mode hotkey: {}", e);
                } else {
                    eprintln!("[HOTKEY] Registered rearrange mode hotkey: {}", key_str);
                }
            } else {
                eprintln!("[HOTKEY] Invalid rearrange mode hotkey format: {}", key_str);
            }
        }
    });
}

/// Hotkey handler: Toggle overlay visibility
#[cfg(not(target_os = "linux"))]
async fn toggle_visibility_hotkey(overlay_state: SharedOverlayState, service_handle: ServiceHandle) {
    use overlay::commands::{hide_all_overlays_impl, show_all_overlays_impl};

    let is_visible = {
        if let Ok(state) = overlay_state.lock() {
            state.overlays_visible
        } else {
            return;
        }
    };

    if is_visible {
        hide_all_overlays_impl(overlay_state, service_handle).await;
    } else {
        show_all_overlays_impl(overlay_state, service_handle).await;
    }
    eprintln!("[HOTKEY] Toggled visibility to: {}", !is_visible);
}

/// Hotkey handler: Toggle move mode
#[cfg(not(target_os = "linux"))]
async fn toggle_move_mode_hotkey(overlay_state: SharedOverlayState) {
    let (txs, new_mode) = {
        let mut state = match overlay_state.lock() {
            Ok(s) => s,
            Err(_) => return,
        };

        if !state.overlays_visible || state.running_overlays().is_empty() {
            return;
        }

        let new_mode = !state.move_mode;
        state.set_move_mode(new_mode);
        // If entering move mode, disable rearrange mode
        if new_mode {
            state.rearrange_mode = false;
        }
        let txs: Vec<_> = state.all_txs().into_iter().cloned().collect();
        (txs, new_mode)
    };

    // Broadcast to all overlays
    for tx in txs {
        let _ = tx.send(OverlayCommand::SetMoveMode(new_mode)).await;
    }
    eprintln!("[HOTKEY] Toggled move mode to: {}", new_mode);
}

/// Hotkey handler: Toggle rearrange mode (raid frames)
#[cfg(not(target_os = "linux"))]
async fn toggle_rearrange_mode_hotkey(overlay_state: SharedOverlayState) {
    let (raid_tx, new_mode) = {
        let mut state = match overlay_state.lock() {
            Ok(s) => s,
            Err(_) => return,
        };

        // Only toggle if raid overlay is running
        if !state.is_running(OverlayType::Raid) {
            return;
        }

        let new_mode = !state.rearrange_mode;
        state.set_rearrange_mode(new_mode);
        let tx = state.get_raid_tx().cloned();
        (tx, new_mode)
    };

    // Broadcast to raid overlay
    if let Some(tx) = raid_tx {
        let _ = tx.send(OverlayCommand::SetRearrangeMode(new_mode)).await;
    }
    eprintln!("[HOTKEY] Toggled rearrange mode to: {}", new_mode);
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

                // Spawn the overlay update bridge (needs service handle for registry updates)
                spawn_overlay_bridge(overlay_rx, overlay_state.clone(), handle.clone());

                // Auto-show enabled overlays on startup
                spawn_auto_show_overlays(overlay_state.clone(), handle.clone());

                // Register global hotkeys (not supported on Linux/Wayland)
                #[cfg(not(target_os = "linux"))]
                spawn_register_hotkeys(app.handle().clone(), overlay_state.clone(), handle);

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
            overlay::commands::toggle_raid_rearrange,
            overlay::commands::get_overlay_status,
            overlay::commands::refresh_overlay_settings,
            overlay::commands::clear_raid_registry,
            overlay::commands::swap_raid_slots,
            overlay::commands::remove_raid_slot,
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
            service::get_encounter_history,
            // Profile commands
            service::get_profile_names,
            service::get_active_profile,
            service::save_profile,
            service::load_profile,
            service::delete_profile,
            service::rename_profile,
            // Utilities
            utils::default_log_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
