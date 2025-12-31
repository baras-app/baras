//! Global hotkey registration (Windows/macOS only)
//!
//! Registers global keyboard shortcuts for overlay visibility, move mode, and rearrange mode.
//! Not supported on Linux due to Wayland security model restrictions.

#![cfg(not(target_os = "linux"))]

use crate::overlay::{OverlayCommand, OverlayManager, OverlayType, SharedOverlayState};
use crate::service::ServiceHandle;

/// Register global hotkeys from config
pub fn spawn_register_hotkeys(
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

                if let Err(e) =
                    global_shortcut.on_shortcut(shortcut, move |_app, _shortcut, event| {
                        if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                            let state = state.clone();
                            let handle = handle.clone();
                            tauri::async_runtime::spawn(async move {
                                toggle_visibility_hotkey(state, handle).await;
                            });
                        }
                    })
                {
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

                if let Err(e) =
                    global_shortcut.on_shortcut(shortcut, move |_app, _shortcut, event| {
                        if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                            let state = state.clone();
                            tauri::async_runtime::spawn(async move {
                                toggle_move_mode_hotkey(state).await;
                            });
                        }
                    })
                {
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

                if let Err(e) =
                    global_shortcut.on_shortcut(shortcut, move |_app, _shortcut, event| {
                        if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                            let state = state.clone();
                            tauri::async_runtime::spawn(async move {
                                toggle_rearrange_mode_hotkey(state).await;
                            });
                        }
                    })
                {
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
async fn toggle_visibility_hotkey(
    overlay_state: SharedOverlayState,
    service_handle: ServiceHandle,
) {
    let is_visible = {
        if let Ok(state) = overlay_state.lock() {
            state.overlays_visible
        } else {
            return;
        }
    };

    if is_visible {
        let _ = OverlayManager::hide_all(&overlay_state, &service_handle).await;
    } else {
        let _ = OverlayManager::show_all(&overlay_state, &service_handle).await;
    }
}

/// Hotkey handler: Toggle move mode
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
        if new_mode {
            state.rearrange_mode = false;
        }
        let txs: Vec<_> = state.all_txs().into_iter().cloned().collect();
        (txs, new_mode)
    };

    for tx in txs {
        let _ = tx.send(OverlayCommand::SetMoveMode(new_mode)).await;
    }
}

/// Hotkey handler: Toggle rearrange mode (raid frames)
async fn toggle_rearrange_mode_hotkey(overlay_state: SharedOverlayState) {
    let (raid_tx, new_mode) = {
        let mut state = match overlay_state.lock() {
            Ok(s) => s,
            Err(_) => return,
        };

        if !state.is_running(OverlayType::Raid) {
            return;
        }

        let new_mode = !state.rearrange_mode;
        state.set_rearrange_mode(new_mode);
        let tx = state.get_raid_tx().cloned();
        (tx, new_mode)
    };

    if let Some(tx) = raid_tx {
        let _ = tx.send(OverlayCommand::SetRearrangeMode(new_mode)).await;
    }
}
