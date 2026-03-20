//! Global hotkey registration
//!
//! Registers global keyboard shortcuts for overlay visibility, move mode, and rearrange mode.
//! Supported on Windows, macOS, Linux X11, and Linux Wayland (via XDG GlobalShortcuts portal).

use crate::overlay::{OverlayCommand, OverlayManager, OverlayType, SharedOverlayState};
use crate::service::ServiceHandle;
use tauri::Emitter;
use tracing::{error, info, warn};

#[cfg(target_os = "linux")]
use futures_util::StreamExt;

/// Check if running on Wayland (Linux only)
#[cfg(target_os = "linux")]
fn is_wayland() -> bool {
    std::env::var("WAYLAND_DISPLAY").is_ok()
        || std::env::var("XDG_SESSION_TYPE")
            .map(|v| v == "wayland")
            .unwrap_or(false)
}

/// Convert Tauri key format to XDG shortcuts format
/// 
/// Per XDG shortcuts spec:
/// - Modifiers are uppercase: CTRL, ALT, SHIFT, LOGO
/// - Keys are lowercase: a, w, return
/// 
/// Examples:
/// - "CommandOrControl+Shift+H" -> "CTRL+SHIFT+h"
/// - "Alt+F1" -> "ALT+f1"
/// - "Ctrl+Shift+W" -> "CTRL+SHIFT+w"
#[cfg(target_os = "linux")]
fn convert_key_format(tauri_key: &str) -> String {
    tauri_key
        .split('+')
        .map(|part| {
            let part_lower = part.to_lowercase();
            // Map known modifiers to uppercase XDG format
            match part_lower.as_str() {
                "commandorcontrol" | "control" | "ctrl" => "CTRL",
                "command" | "super" | "meta" => "LOGO",  // LOGO is the XDG name for Super/Windows key
                "alt" => "ALT",
                "shift" => "SHIFT",
                // Everything else is a key - keep it lowercase
                key => return key.to_owned(),
            }.to_owned()
        })
        .collect::<Vec<_>>()
        .join("+")
}

/// Try to register global shortcuts via XDG portal (Wayland)
/// 
/// This function creates a portal session, binds shortcuts, and listens for activation signals.
/// It runs indefinitely until an error occurs or the app closes.
#[cfg(target_os = "linux")]
async fn try_portal_shortcuts(
    _app_handle: tauri::AppHandle,
    overlay_state: SharedOverlayState,
    service_handle: ServiceHandle,
) -> Result<(), ashpd::Error> {
    use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
    
    let config = service_handle.config().await;
    let hotkeys = &config.hotkeys;
    
    // Build list of shortcuts to register
    // Store converted keys so we can pass references
    let mut converted_keys = Vec::new();
    let mut shortcuts = Vec::new();
    
    if let Some(ref key) = hotkeys.toggle_visibility {
        let converted = convert_key_format(key);
        converted_keys.push(converted);
        shortcuts.push(
            NewShortcut::new("toggle-visibility", "Toggle overlay visibility")
                .preferred_trigger(converted_keys.last().unwrap().as_str())
        );
    }
    if let Some(ref key) = hotkeys.toggle_move_mode {
        let converted = convert_key_format(key);
        converted_keys.push(converted);
        shortcuts.push(
            NewShortcut::new("toggle-move-mode", "Toggle move mode")
                .preferred_trigger(converted_keys.last().unwrap().as_str())
        );
    }
    if let Some(ref key) = hotkeys.toggle_rearrange_mode {
        let converted = convert_key_format(key);
        converted_keys.push(converted);
        shortcuts.push(
            NewShortcut::new("toggle-rearrange", "Toggle rearrange mode")
                .preferred_trigger(converted_keys.last().unwrap().as_str())
        );
    }
    if let Some(ref key) = hotkeys.toggle_operation_timer {
        let converted = convert_key_format(key);
        converted_keys.push(converted);
        shortcuts.push(
            NewShortcut::new("toggle-operation-timer", "Toggle operation timer")
                .preferred_trigger(converted_keys.last().unwrap().as_str())
        );
    }
    if let Some(ref key) = hotkeys.toggle_live_mode {
        let converted = convert_key_format(key);
        converted_keys.push(converted);
        shortcuts.push(
            NewShortcut::new("toggle-live-mode", "Go live")
                .preferred_trigger(converted_keys.last().unwrap().as_str())
        );
    }
    
    if shortcuts.is_empty() {
        info!("No hotkeys configured, skipping portal registration");
        return Ok(());
    }
    
    // Create portal connection and session
    let portal = GlobalShortcuts::new().await?;
    let session = portal.create_session().await?;
    
    // Bind shortcuts (may show user permission dialog on first run)
    let _response = portal
        .bind_shortcuts(&session, &shortcuts, None)
        .await?
        .response()?;
    
    info!(
        "Registered {} global shortcuts via portal",
        shortcuts.len()
    );
    
    // Spawn a dedicated task to listen for activation signals
    // This is critical - the stream must be polled in a separate task
    tauri::async_runtime::spawn(async move {
        // Keep session alive for the lifetime of this task - dropping it closes the portal session
        let _session = session;
        match portal.receive_activated().await {
            Ok(mut activated) => {
                while let Some(activation) = activated.next().await {
                    let shortcut_id = activation.shortcut_id();
                    let state = overlay_state.clone();
                    let handle = service_handle.clone();
                    
                    match shortcut_id {
                        "toggle-visibility" => {
                            tauri::async_runtime::spawn(async move {
                                toggle_visibility_hotkey(state, handle).await;
                            });
                        }
                        "toggle-move-mode" => {
                            tauri::async_runtime::spawn(async move {
                                toggle_move_mode_hotkey(state, handle).await;
                            });
                        }
                        "toggle-rearrange" => {
                            tauri::async_runtime::spawn(async move {
                                toggle_rearrange_mode_hotkey(state, handle).await;
                            });
                        }
                        "toggle-operation-timer" => {
                            tauri::async_runtime::spawn(async move {
                                toggle_operation_timer_hotkey(handle).await;
                            });
                        }
                        "toggle-live-mode" => {
                            tauri::async_runtime::spawn(async move {
                                toggle_live_mode_hotkey(handle).await;
                            });
                        }
                        _ => {
                            warn!("Unknown shortcut ID: {}", shortcut_id);
                        }
                    }
                }
                
                warn!("Global shortcuts activation stream ended");
            }
            Err(e) => {
                error!("Failed to create global shortcuts activation stream: {}", e);
            }
        }
    });
    
    Ok(())
}

/// Register global hotkeys from config
pub fn spawn_register_hotkeys(
    app_handle: tauri::AppHandle,
    overlay_state: SharedOverlayState,
    service_handle: ServiceHandle,
) {
    // Try portal on Wayland, otherwise use tauri-plugin-global-shortcut
    #[cfg(target_os = "linux")]
    if is_wayland() {
        info!("Detected Wayland, attempting to register shortcuts via XDG portal");
        
        let app = app_handle.clone();
        let state = overlay_state.clone();
        let handle = service_handle.clone();
        
        tauri::async_runtime::spawn(async move {
            // Small delay to ensure everything is initialized
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            
            if let Err(e) = try_portal_shortcuts(app.clone(), state, handle).await {
                let error_msg = format!("{}", e);
                warn!(
                    "GlobalShortcuts portal not available: {}. \
                     Hotkeys will not work. Configure shortcuts in your compositor settings instead.",
                    error_msg
                );
                
                // Emit event to frontend so user sees a helpful message
                let _ = app.emit("hotkeys-unavailable", error_msg);
            }
        });
        return;
    }
    
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
                    error!(error = %e, hotkey = %key_str, "Failed to register visibility hotkey");
                } else {
                    info!(hotkey = %key_str, "Registered visibility hotkey");
                }
            } else {
                warn!(hotkey = %key_str, "Invalid visibility hotkey format");
            }
        }

        // Register toggle move mode hotkey
        if let Some(ref key_str) = hotkeys.toggle_move_mode {
            if let Ok(shortcut) = key_str.parse::<Shortcut>() {
                let state = overlay_state.clone();
                let handle = service_handle.clone();

                if let Err(e) =
                    global_shortcut.on_shortcut(shortcut, move |_app, _shortcut, event| {
                        if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                            let state = state.clone();
                            let handle = handle.clone();
                            tauri::async_runtime::spawn(async move {
                                toggle_move_mode_hotkey(state, handle).await;
                            });
                        }
                    })
                {
                    error!(error = %e, hotkey = %key_str, "Failed to register move mode hotkey");
                } else {
                    info!(hotkey = %key_str, "Registered move mode hotkey");
                }
            } else {
                warn!(hotkey = %key_str, "Invalid move mode hotkey format");
            }
        }

        // Register toggle rearrange mode hotkey
        if let Some(ref key_str) = hotkeys.toggle_rearrange_mode {
            if let Ok(shortcut) = key_str.parse::<Shortcut>() {
                let state = overlay_state.clone();
                let handle = service_handle.clone();

                if let Err(e) =
                    global_shortcut.on_shortcut(shortcut, move |_app, _shortcut, event| {
                        if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                            let state = state.clone();
                            let handle = handle.clone();
                            tauri::async_runtime::spawn(async move {
                                toggle_rearrange_mode_hotkey(state, handle).await;
                            });
                        }
                    })
                {
                    error!(error = %e, hotkey = %key_str, "Failed to register rearrange mode hotkey");
                } else {
                    info!(hotkey = %key_str, "Registered rearrange mode hotkey");
                }
            } else {
                warn!(hotkey = %key_str, "Invalid rearrange mode hotkey format");
            }
        }

        // Register toggle operation timer hotkey
        if let Some(ref key_str) = hotkeys.toggle_operation_timer {
            if let Ok(shortcut) = key_str.parse::<Shortcut>() {
                let handle = service_handle.clone();

                if let Err(e) =
                    global_shortcut.on_shortcut(shortcut, move |_app, _shortcut, event| {
                        if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                            let handle = handle.clone();
                            tauri::async_runtime::spawn(async move {
                                toggle_operation_timer_hotkey(handle).await;
                            });
                        }
                    })
                {
                    error!(error = %e, hotkey = %key_str, "Failed to register operation timer hotkey");
                } else {
                    info!(hotkey = %key_str, "Registered operation timer hotkey");
                }
            } else {
                warn!(hotkey = %key_str, "Invalid operation timer hotkey format");
            }
        }

        // Register go live hotkey
        if let Some(ref key_str) = hotkeys.toggle_live_mode {
            if let Ok(shortcut) = key_str.parse::<Shortcut>() {
                let handle = service_handle.clone();

                if let Err(e) =
                    global_shortcut.on_shortcut(shortcut, move |_app, _shortcut, event| {
                        if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                            let handle = handle.clone();
                            tauri::async_runtime::spawn(async move {
                                toggle_live_mode_hotkey(handle).await;
                            });
                        }
                    })
                {
                    error!(error = %e, hotkey = %key_str, "Failed to register go live hotkey");
                } else {
                    info!(hotkey = %key_str, "Registered go live hotkey");
                }
            } else {
                warn!(hotkey = %key_str, "Invalid go live hotkey format");
            }
        }
    });
}

/// Hotkey handler: Toggle overlay visibility
///
/// If auto-hide is active, show_all() records intent but does not spawn overlays.
/// A toast event is emitted so the frontend can inform the user.
async fn toggle_visibility_hotkey(
    overlay_state: SharedOverlayState,
    service_handle: ServiceHandle,
) {
    let any_running = overlay_state
        .lock()
        .map(|s| s.any_running())
        .unwrap_or(false);

    if any_running {
        let _ = OverlayManager::hide_all(&overlay_state, &service_handle).await;
    } else {
        // show_all() records overlays_visible=true in config. If auto-hide is
        // active the spawn is a no-op; emit toast so user knows why.
        let _ = OverlayManager::show_all(&overlay_state, &service_handle).await;
        if service_handle.shared.auto_hide.is_auto_hidden() {
            let _ = service_handle
                .app_handle
                .emit("overlays-auto-hidden-toast", ());
        }
    }
}

/// Hotkey handler: Toggle move mode
async fn toggle_move_mode_hotkey(overlay_state: SharedOverlayState, service: ServiceHandle) {
    let (txs, new_mode, was_rearranging) = {
        let mut state = match overlay_state.lock() {
            Ok(s) => s,
            Err(_) => return,
        };

        if state.running_overlays().is_empty() {
            return;
        }

        let new_mode = !state.move_mode;
        let was_rearranging = state.rearrange_mode;
        state.set_move_mode(new_mode);
        if new_mode {
            state.rearrange_mode = false;
        }
        let txs: Vec<_> = state.all_txs().into_iter().cloned().collect();
        (txs, new_mode, was_rearranging)
    };

    // Update shared state flag if rearrange was disabled
    if was_rearranging && new_mode {
        service.set_rearrange_mode(false);
    }

    for tx in txs {
        let _ = tx.send(OverlayCommand::SetMoveMode(new_mode)).await;
    }

    // Notify frontend to update UI buttons
    service.emit_overlay_status_changed();
}

/// Hotkey handler: Toggle operation timer start/stop
/// No-ops in historical mode - timer should only be driven by live gameplay
async fn toggle_operation_timer_hotkey(service: ServiceHandle) {
    if !service.is_live_tailing() {
        return;
    }

    let is_running = service.is_operation_timer_running();

    if is_running {
        if let Err(e) = service.stop_operation_timer().await {
            error!(error = %e, "Failed to stop operation timer via hotkey");
        }
    } else {
        if let Err(e) = service.start_operation_timer().await {
            error!(error = %e, "Failed to start operation timer via hotkey");
        }
    }
}

/// Hotkey handler: Resume live tailing if not already live
async fn toggle_live_mode_hotkey(service: ServiceHandle) {
    if !service.is_live_tailing() {
        info!("Go Live hotkey pressed, resuming live tailing");
        if let Err(e) = service.resume_live_tailing().await {
            error!(error = %e, "Failed to resume live tailing via hotkey");
        }
    }
}

/// Hotkey handler: Toggle rearrange mode (raid frames)
async fn toggle_rearrange_mode_hotkey(overlay_state: SharedOverlayState, service: ServiceHandle) {
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

    // Update shared state flag for rendering loop
    service.set_rearrange_mode(new_mode);

    if let Some(tx) = raid_tx {
        let _ = tx.send(OverlayCommand::SetRearrangeMode(new_mode)).await;
    }

    // Notify frontend to update UI buttons
    service.emit_overlay_status_changed();
}
