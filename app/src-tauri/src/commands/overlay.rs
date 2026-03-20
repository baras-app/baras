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
    pub running: Vec<MetricType>,
    pub enabled: Vec<MetricType>,
    pub personal_running: bool,
    pub personal_enabled: bool,
    pub raid_running: bool,
    pub raid_enabled: bool,
    pub boss_health_running: bool,
    pub boss_health_enabled: bool,
    pub timers_running: bool,
    pub timers_enabled: bool,
    pub timers_b_running: bool,
    pub timers_b_enabled: bool,
    pub challenges_running: bool,
    pub challenges_enabled: bool,
    pub alerts_running: bool,
    pub alerts_enabled: bool,
    pub effects_a_running: bool,
    pub effects_a_enabled: bool,
    pub effects_b_running: bool,
    pub effects_b_enabled: bool,
    pub cooldowns_running: bool,
    pub cooldowns_enabled: bool,
    pub dot_tracker_running: bool,
    pub dot_tracker_enabled: bool,
    pub notes_running: bool,
    pub notes_enabled: bool,
    pub combat_time_running: bool,
    pub combat_time_enabled: bool,
    pub operation_timer_running: bool,
    pub operation_timer_enabled: bool,
    pub overlays_visible: bool,
    pub move_mode: bool,
    pub rearrange_mode: bool,
    /// Whether overlays are currently suppressed by any auto-hide condition
    pub auto_hidden: bool,
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
        let should_hide =
            shared.auto_hide.is_session_not_live() || shared.is_session_not_live().await;

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

#[tauri::command]
pub async fn get_overlay_status(
    state: State<'_, SharedOverlayState>,
    service: State<'_, ServiceHandle>,
) -> Result<OverlayStatusResponse, String> {
    let (
        running_metric_types,
        personal_running,
        raid_running,
        boss_health_running,
        timers_running,
        timers_b_running,
        challenges_running,
        alerts_running,
        effects_a_running,
        effects_b_running,
        cooldowns_running,
        dot_tracker_running,
        notes_running,
        combat_time_running,
        operation_timer_running,
        move_mode,
        rearrange_mode,
    ) = {
        let s = state.lock().map_err(|e| e.to_string())?;
        (
            s.running_metric_types(),
            s.is_personal_running(),
            s.is_raid_running(),
            s.is_boss_health_running(),
            s.is_running(OverlayType::TimersA),
            s.is_running(OverlayType::TimersB),
            s.is_challenges_running(),
            s.is_running(OverlayType::Alerts),
            s.is_running(OverlayType::EffectsA),
            s.is_running(OverlayType::EffectsB),
            s.is_running(OverlayType::Cooldowns),
            s.is_running(OverlayType::DotTracker),
            s.is_running(OverlayType::Notes),
            s.is_combat_time_running(),
            s.is_operation_timer_running(),
            s.move_mode,
            s.rearrange_mode,
        )
    };

    let config = service.config().await;
    let enabled: Vec<MetricType> = config
        .overlay_settings
        .enabled_types()
        .iter()
        .filter_map(|key| MetricType::from_config_key(key))
        .collect();

    let personal_enabled = config.overlay_settings.is_enabled("personal");
    let raid_enabled = config.overlay_settings.is_enabled("raid");
    let boss_health_enabled = config.overlay_settings.is_enabled("boss_health");
    let timers_enabled = config.overlay_settings.is_enabled("timers_a");
    let timers_b_enabled = config.overlay_settings.is_enabled("timers_b");
    let challenges_enabled = config.overlay_settings.is_enabled("challenges");
    let alerts_enabled = config.overlay_settings.is_enabled("alerts");
    let effects_a_enabled = config.overlay_settings.is_enabled("effects_a");
    let effects_b_enabled = config.overlay_settings.is_enabled("effects_b");
    let cooldowns_enabled = config.overlay_settings.is_enabled("cooldowns");
    let dot_tracker_enabled = config.overlay_settings.is_enabled("dot_tracker");
    let notes_enabled = config.overlay_settings.is_enabled("notes");
    let combat_time_enabled = config.overlay_settings.is_enabled("combat_time");
    let operation_timer_enabled = config.overlay_settings.is_enabled("operation_timer");

    Ok(OverlayStatusResponse {
        running: running_metric_types,
        enabled,
        personal_running,
        personal_enabled,
        raid_running,
        raid_enabled,
        boss_health_running,
        boss_health_enabled,
        timers_running,
        timers_enabled,
        timers_b_running,
        timers_b_enabled,
        challenges_running,
        challenges_enabled,
        alerts_running,
        alerts_enabled,
        effects_a_running,
        effects_a_enabled,
        effects_b_running,
        effects_b_enabled,
        cooldowns_running,
        cooldowns_enabled,
        dot_tracker_running,
        dot_tracker_enabled,
        notes_running,
        notes_enabled,
        combat_time_running,
        combat_time_enabled,
        operation_timer_running,
        operation_timer_enabled,
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
        let s = overlay_state.lock().map_err(|e| e.to_string())?;
        s.all_overlays()
            .into_iter()
            .map(|(k, tx)| (k, tx.clone()))
            .collect()
    };

    for (kind, tx) in overlays {
        let config_update = OverlayManager::create_config_update(kind, &settings, european);
        let _ = tx.send(OverlayCommand::UpdateConfig(config_update)).await;
    }

    Ok(true)
}

// ─────────────────────────────────────────────────────────────────────────────
// Operation Timer Commands
// ─────────────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn start_operation_timer(
    service: State<'_, ServiceHandle>,
) -> Result<(), String> {
    service.start_operation_timer().await
}

#[tauri::command]
pub async fn stop_operation_timer(
    service: State<'_, ServiceHandle>,
) -> Result<(), String> {
    service.stop_operation_timer().await
}

#[tauri::command]
pub async fn reset_operation_timer(
    service: State<'_, ServiceHandle>,
) -> Result<(), String> {
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

/// Swap two slots in the raid frame registry
#[tauri::command]
pub async fn swap_raid_slots(
    slot_a: u8,
    slot_b: u8,
    service: State<'_, ServiceHandle>,
) -> Result<(), String> {
    service.swap_raid_slots(slot_a, slot_b).await;
    Ok(())
}

/// Remove a player from a specific slot
#[tauri::command]
pub async fn remove_raid_slot(slot: u8, service: State<'_, ServiceHandle>) -> Result<(), String> {
    service.remove_raid_slot(slot).await;
    Ok(())
}
