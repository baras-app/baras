//! System tray support for BARAS
//!
//! Provides a system tray icon with menu for quick access to common actions.

use std::sync::{Arc, Mutex};

use tauri::{
    AppHandle, Manager,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Runtime,
};

use crate::overlay::{OverlayManager, OverlayState};
use crate::service::ServiceHandle;

/// Set up the system tray icon and menu
pub fn setup_tray<R: Runtime>(app: &AppHandle<R>) -> Result<(), Box<dyn std::error::Error>> {
    // Create menu items
    let show_hide = MenuItem::with_id(app, "show_hide", "Show/Hide Window", true, None::<&str>)?;
    let toggle_overlays = MenuItem::with_id(app, "toggle_overlays", "Toggle Overlays", true, None::<&str>)?;
    let separator = MenuItem::with_id(app, "sep", "─────────────", false, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

    // Build menu
    let menu = Menu::with_items(app, &[&show_hide, &toggle_overlays, &separator, &quit])?;

    // Build tray icon
    let _tray = TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .tooltip("BARAS - Combat Log Parser")
        .on_menu_event(|app, event| {
            handle_menu_event(app, event.id.as_ref());
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                // Double-click or single click to show window
                if let Some(window) = tray.app_handle().get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(())
}

/// Handle tray menu events
fn handle_menu_event<R: Runtime>(app: &AppHandle<R>, id: &str) {
    match id {
        "show_hide" => {
            if let Some(window) = app.get_webview_window("main") {
                if window.is_visible().unwrap_or(false) {
                    let _ = window.hide();
                } else {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        }
        "toggle_overlays" => {
            // Get the overlay state and service handle to toggle visibility
            let overlay_state = app.state::<Arc<Mutex<OverlayState>>>();
            let service_handle = app.state::<ServiceHandle>();

            let state = overlay_state.inner().clone();
            let handle = service_handle.inner().clone();

            tauri::async_runtime::spawn(async move {
                toggle_all_overlays(state, handle).await;
            });
        }
        "quit" => {
            std::process::exit(0);
        }
        _ => {}
    }
}

/// Toggle visibility of all overlays
async fn toggle_all_overlays(
    overlay_state: Arc<Mutex<OverlayState>>,
    service_handle: ServiceHandle,
) {
    let config = service_handle.config().await;
    let currently_visible = config.overlay_settings.overlays_visible;

    // OverlayManager::hide_all and show_all already update the config visibility
    if currently_visible {
        if let Err(e) = OverlayManager::hide_all(&overlay_state, &service_handle).await {
            eprintln!("[TRAY] Failed to hide overlays: {}", e);
        }
    } else if let Err(e) = OverlayManager::show_all(&overlay_state, &service_handle).await {
            eprintln!("[TRAY] Failed to show overlays: {}", e);
    }
}
