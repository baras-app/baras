//! Overlay Tauri commands
//!
//! Commands for showing, hiding, and configuring overlays.

use serde::Serialize;
use tauri::State;

use crate::overlay::{MetricType, OverlayCommand, OverlayManager, OverlayType, SharedOverlayState};
use crate::service::ServiceHandle;
use baras_core::context::OverlaySettings;

// ─────────────────────────────────────────────────────────────────────────────
// Response Types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct OverlayStatusResponse {
    /// Config keys of every enabled overlay
    pub enabled: Vec<String>,
    pub overlays_visible: bool,
    pub move_mode: bool,
    pub rearrange_mode: bool,
    /// Whether overlays are currently suppressed by any auto-hide condition
    pub auto_hidden: bool,
}

#[tauri::command]
pub async fn enable_screen_capture() -> Result<(), String> {
    baras_overlay::capture::enable_portal().await.map_err(|e| e.to_string())
}

// ─────────────────────────────────────────────────────────────────────────────
// Show/Hide Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Enable an overlay (persists to config, only shows if overlays_visible is true)
#[tauri::command]
pub async fn show_overlay(
    kind: OverlayType,
    state: State<'_, SharedOverlayState>,
    service: State<'_, ServiceHandle>,
) -> Result<bool, String> {
    OverlayManager::show(kind, &state, &service).await
}

/// Disable an overlay (persists to config, hides if currently running)
#[tauri::command]
pub async fn hide_overlay(
    kind: OverlayType,
    state: State<'_, SharedOverlayState>,
    service: State<'_, ServiceHandle>,
) -> Result<bool, String> {
    OverlayManager::hide(kind, &state, &service).await
}

/// Show all enabled overlays and set overlays_visible=true
#[tauri::command]
pub async fn show_all_overlays(
    state: State<'_, SharedOverlayState>,
    service: State<'_, ServiceHandle>,
) -> Result<Vec<MetricType>, String> {
    OverlayManager::show_all(&state, &service).await
}

/// Hide all running overlays and set overlays_visible=false
#[tauri::command]
pub async fn hide_all_overlays(
    state: State<'_, SharedOverlayState>,
    service: State<'_, ServiceHandle>,
) -> Result<bool, String> {
    OverlayManager::hide_all(&state, &service).await
}

/// Apply or remove the "not live" auto-hide based on current session state.
/// Called by the frontend when the user toggles the hide_when_not_live setting.
/// Ensures overlay display state is immediately synchronized with the new setting.
#[tauri::command]
pub async fn apply_not_live_auto_hide(
    state: State<'_, SharedOverlayState>,
    service: State<'_, ServiceHandle>,
) -> Result<bool, String> {
    let config = service.config().await;
    let shared = &service.shared;

    if config.overlay_settings.hide_when_not_live {
        // Setting was just enabled — check if the session is currently not live.
        // We check both the tracked condition flag (from prior NotLiveStateChanged
        // events that may have fired while the setting was off) AND the async
        // session check (for conditions like stale session that don't emit events).
        let should_hide = shared.auto_hide.is_session_not_live() || shared.is_session_not_live().await;

        if should_hide {
            let was_hidden = shared.auto_hide.is_auto_hidden();
            shared.auto_hide.set_not_live(true);

            // Only tear down windows if we're transitioning to hidden
            if !was_hidden {
                let _ = OverlayManager::temporary_hide_all(&state, &service).await;
            }
            service.emit_overlay_status_changed();
            return Ok(true);
        }
    } else {
        // Setting was just disabled — clear the not-live flag
        if shared.auto_hide.is_not_live_active() {
            shared.auto_hide.set_not_live(false);
            // temporary_show_all checks is_auto_hidden() internally,
            // so if conversation hiding is still active, overlays stay hidden
            let _ = OverlayManager::temporary_show_all(&state, &service).await;
            service.emit_overlay_status_changed();
            return Ok(true);
        }
    }

    Ok(false)
}

// ─────────────────────────────────────────────────────────────────────────────
// Move Mode and Status
// ─────────────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn toggle_move_mode(
    state: State<'_, SharedOverlayState>,
    service: State<'_, ServiceHandle>,
) -> Result<bool, String> {
    OverlayManager::toggle_move_mode(&state, &service).await
}

#[tauri::command]
pub async fn toggle_raid_rearrange(
    state: State<'_, SharedOverlayState>,
    service: State<'_, ServiceHandle>,
) -> Result<bool, String> {
    OverlayManager::toggle_rearrange(&state, &service).await
}

/// List the font family names available on this system (for the font picker).
#[tauri::command]
pub async fn list_system_fonts() -> Result<Vec<String>, String> {
    Ok(baras_overlay::available_font_families())
}

/// Set the global overlay font family: persists it and applies to all overlays.
#[tauri::command]
pub async fn set_overlay_font_family(
    family: String,
    state: State<'_, SharedOverlayState>,
    service: State<'_, ServiceHandle>,
) -> Result<(), String> {
    OverlayManager::set_font_family(&state, &service, family).await
}

/// Set the countdown overlays' redraw rate (5–30 FPS): persists and applies it.
#[tauri::command]
pub async fn set_overlay_fps(fps: u8, service: State<'_, ServiceHandle>) -> Result<(), String> {
    OverlayManager::set_fps(&service, fps).await
}

#[tauri::command]
pub async fn get_overlay_status(
    state: State<'_, SharedOverlayState>,
    service: State<'_, ServiceHandle>,
) -> Result<OverlayStatusResponse, String> {
    let (move_mode, rearrange_mode) = {
        let s = state.lock().map_err(|e| e.to_string())?;
        (s.move_mode, s.rearrange_mode)
    };
    let config = service.config().await;
    Ok(OverlayStatusResponse {
        enabled: config.overlay_settings.enabled_types(),
        overlays_visible: config.overlay_settings.overlays_visible,
        move_mode,
        rearrange_mode,
        auto_hidden: service.shared.auto_hide.is_auto_hidden(),
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Settings Refresh
// ─────────────────────────────────────────────────────────────────────────────

/// Refresh overlay settings for all running overlays
#[tauri::command]
pub async fn refresh_overlay_settings(
    state: State<'_, SharedOverlayState>,
    service: State<'_, ServiceHandle>,
) -> Result<bool, String> {
    OverlayManager::refresh_settings(&state, &service, true).await
}

/// Return one overlay to its default position (respawns it if running)
#[tauri::command]
pub async fn reset_overlay_position(
    key: String,
    state: State<'_, SharedOverlayState>,
    service: State<'_, ServiceHandle>,
) -> Result<(), String> {
    OverlayManager::reset_position(&key, &state, &service).await
}

/// Preview overlay settings without persisting to disk.
/// Used for live preview while user is editing settings.
#[tauri::command]
pub async fn preview_overlay_settings(
    settings: OverlaySettings,
    overlay_state: State<'_, SharedOverlayState>,
    service: State<'_, ServiceHandle>,
) -> Result<bool, String> {
    let european = service.config().await.european_number_format;
    let overlays: Vec<_> = {
        let mut s = overlay_state.lock().map_err(|e| e.to_string())?;
        s.preview = Some(settings.clone());
        s.all_overlays().into_iter().map(|(k, tx)| (k, tx.clone())).collect()
    };

    for (kind, tx) in overlays {
        let config_update = OverlayManager::create_config_update(kind, &settings, european);
        let _ = tx.send(OverlayCommand::UpdateConfig(config_update)).await;
        for cmd in OverlayManager::frame_commands(kind, &settings) {
            let _ = tx.send(cmd).await;
        }
    }

    Ok(true)
}

/// Show an unsaved timer/effect draft on its overlays for a few seconds.
/// Returns the labels of requested overlays that are not enabled.
#[tauri::command]
pub async fn preview_overlay_entry(
    request: baras_types::preview::OverlayPreviewRequest,
    overlay_state: State<'_, SharedOverlayState>,
    service: State<'_, ServiceHandle>,
    icon_cache: State<'_, Option<std::sync::Arc<baras_overlay::icons::IconCache>>>,
    audio_tx: State<'_, crate::audio::AudioSender>,
) -> Result<Vec<&'static str>, String> {
    crate::preview::start(
        request,
        service.shared.clone(),
        overlay_state.inner().clone(),
        icon_cache.inner().clone(),
        audio_tx.inner().clone(),
    )
    .await
}

// ─────────────────────────────────────────────────────────────────────────────
// Operation Timer Commands
// ─────────────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn start_operation_timer(service: State<'_, ServiceHandle>) -> Result<(), String> {
    service.start_operation_timer().await
}

#[tauri::command]
pub async fn stop_operation_timer(service: State<'_, ServiceHandle>) -> Result<(), String> {
    service.stop_operation_timer().await
}

#[tauri::command]
pub async fn reset_operation_timer(service: State<'_, ServiceHandle>) -> Result<(), String> {
    service.reset_operation_timer().await
}

// ─────────────────────────────────────────────────────────────────────────────
// Raid Registry Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Clear all players from the raid frame registry
#[tauri::command]
pub async fn clear_raid_registry(service: State<'_, ServiceHandle>) -> Result<(), String> {
    service.clear_raid_registry().await;
    Ok(())
}
