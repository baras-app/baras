//! Overlay Tauri commands
//!
//! Commands for showing, hiding, and configuring overlays.

use serde::Serialize;
use tauri::State;

use crate::overlay::{MetricType, OverlayManager, OverlayType, SharedOverlayState};
use crate::service::ServiceHandle;

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
    pub effects_running: bool,
    pub effects_enabled: bool,
    pub challenges_running: bool,
    pub challenges_enabled: bool,
    pub alerts_running: bool,
    pub alerts_enabled: bool,
    pub overlays_visible: bool,
    pub move_mode: bool,
    pub rearrange_mode: bool,
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
pub async fn toggle_raid_rearrange(state: State<'_, SharedOverlayState>) -> Result<bool, String> {
    OverlayManager::toggle_rearrange(&state).await
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
        effects_running,
        challenges_running,
        alerts_running,
        move_mode,
        rearrange_mode,
    ) = {
        let s = state.lock().map_err(|e| e.to_string())?;
        (
            s.running_metric_types(),
            s.is_personal_running(),
            s.is_raid_running(),
            s.is_boss_health_running(),
            s.is_running(OverlayType::Timers),
            s.is_effects_running(),
            s.is_challenges_running(),
            s.is_running(OverlayType::Alerts),
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
    let timers_enabled = config.overlay_settings.is_enabled("timers");
    let effects_enabled = config.overlay_settings.is_enabled("effects");
    let challenges_enabled = config.overlay_settings.is_enabled("challenges");
    let alerts_enabled = config.overlay_settings.is_enabled("alerts");

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
        effects_running,
        effects_enabled,
        challenges_running,
        challenges_enabled,
        alerts_running,
        alerts_enabled,
        overlays_visible: config.overlay_settings.overlays_visible,
        move_mode,
        rearrange_mode,
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
    OverlayManager::refresh_settings(&state, &service).await
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
