//! Tauri commands for overlay management
//!
//! All Tauri-invokable commands for showing, hiding, and configuring overlays.

use baras_core::context::OverlayPositionConfig;
use baras_overlay::{OverlayConfigUpdate, OverlayData};
use serde::Serialize;
use tauri::State;

use super::metrics::create_entries_for_type;
use super::spawn::{create_metric_overlay, create_personal_overlay};
use super::state::OverlayCommand;
use super::types::{OverlayType, MetricType};
use super::SharedOverlayState;

// ─────────────────────────────────────────────────────────────────────────────
// Response Types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct OverlayStatusResponse {
    pub running: Vec<MetricType>,
    pub enabled: Vec<MetricType>,
    pub personal_running: bool,
    pub personal_enabled: bool,
    pub overlays_visible: bool,
    pub move_mode: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Show/Hide Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Enable an overlay (persists to config, only shows if overlays_visible is true)
#[tauri::command]
pub async fn show_overlay(
    kind: OverlayType,
    state: State<'_, SharedOverlayState>,
    service: State<'_, crate::service::ServiceHandle>,
) -> Result<bool, String> {
    // Get current config and update enabled state
    let mut config = service.config().await;
    config.overlay_settings.set_enabled(kind.config_key(), true);

    // Save config immediately
    service.update_config(config.clone()).await?;

    // Only spawn overlay if global visibility is enabled
    if !config.overlay_settings.overlays_visible {
        return Ok(true);
    }

    // Check if already running
    {
        let state = state.lock().map_err(|e| e.to_string())?;
        if state.is_running(kind) {
            return Ok(true);
        }
    }

    // Load position from config
    let position = config.overlay_settings.get_position(kind.config_key());
    let needs_monitor_id_save = position.monitor_id.is_none();

    // Create and spawn overlay based on kind (with per-category opacity)
    let overlay_handle = match kind {
        OverlayType::Metric(overlay_type) => {
            let appearance = config.overlay_settings.get_appearance(kind.config_key());
            create_metric_overlay(overlay_type, position, appearance, config.overlay_settings.metric_opacity)?
        }
        OverlayType::Personal => {
            let personal_config = config.overlay_settings.personal_overlay.clone();
            create_personal_overlay(position, personal_config, config.overlay_settings.personal_opacity)?
        }
    };
    let tx = overlay_handle.tx.clone();

    // Update state
    {
        let mut state = state.lock().map_err(|e| e.to_string())?;
        state.insert(overlay_handle);
    }

    // Sync move mode state - if app is in move mode, new overlay should be too
    let current_move_mode = {
        state.lock().map_err(|e| e.to_string())?.move_mode
    };
    if current_move_mode {
        let _ = tx.send(OverlayCommand::SetMoveMode(true)).await;
    }

    // Send current data if tailing
    if service.is_tailing().await
        && let Some(data) = service.current_combat_data().await
        && !data.metrics.is_empty()
    {
        match kind {
            OverlayType::Metric(overlay_type) => {
                let entries = create_entries_for_type(overlay_type, &data.metrics);
                let _ = tx.send(OverlayCommand::UpdateData(OverlayData::Metrics(entries))).await;
            }
            OverlayType::Personal => {
                if let Some(stats) = data.to_personal_stats() {
                    let _ = tx.send(OverlayCommand::UpdateData(OverlayData::Personal(stats))).await;
                }
            }
        }
    }

    // If monitor_id was None, query and save the position to persist the monitor
    // the compositor chose. This ensures next spawn goes to same monitor.
    if needs_monitor_id_save {
        // Give overlay a moment to be placed by compositor
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let (pos_tx, pos_rx) = tokio::sync::oneshot::channel();
        let _ = tx.send(OverlayCommand::GetPosition(pos_tx)).await;
        if let Ok(pos) = pos_rx.await {
            let relative_x = pos.x - pos.monitor_x;
            let relative_y = pos.y - pos.monitor_y;
            let mut config = service.config().await;
            config.overlay_settings.set_position(
                kind.config_key(),
                OverlayPositionConfig {
                    x: relative_x,
                    y: relative_y,
                    width: pos.width,
                    height: pos.height,
                    monitor_id: pos.monitor_id,
                },
            );
            let _ = service.update_config(config).await;
        }
    }

    Ok(true)
}

/// Disable an overlay (persists to config, hides if currently running)
#[tauri::command]
pub async fn hide_overlay(
    kind: OverlayType,
    state: State<'_, SharedOverlayState>,
    service: State<'_, crate::service::ServiceHandle>,
) -> Result<bool, String> {
    // Get current config and update enabled state
    let mut config = service.config().await;
    config.overlay_settings.set_enabled(kind.config_key(), false);

    // Save config immediately
    service.update_config(config).await?;

    // Shutdown overlay if running
    let overlay_handle = {
        let mut state = state.lock().map_err(|e| e.to_string())?;
        state.remove(kind)
    };

    if let Some(handle) = overlay_handle {
        let _ = handle.tx.send(OverlayCommand::Shutdown).await;
        let _ = handle.handle.join();
    }

    Ok(true)
}

// ─────────────────────────────────────────────────────────────────────────────
// Bulk Overlay Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Hide all running overlays and set overlays_visible=false
#[tauri::command]
pub async fn hide_all_overlays(
    state: State<'_, SharedOverlayState>,
    service: State<'_, crate::service::ServiceHandle>,
) -> Result<bool, String> {
    // Update and persist overlays_visible = false
    let mut config = service.config().await;
    config.overlay_settings.overlays_visible = false;
    service.update_config(config).await?;

    // Shutdown all running overlays (both metric and personal are in unified state)
    let handles = {
        let mut state = state.lock().map_err(|e| e.to_string())?;
        state.move_mode = false;
        state.drain()
    };

    for handle in handles {
        let _ = handle.tx.send(OverlayCommand::Shutdown).await;
        let _ = handle.handle.join();
    }

    Ok(true)
}

/// Show all enabled overlays and set overlays_visible=true
#[tauri::command]
pub async fn show_all_overlays(
    state: State<'_, SharedOverlayState>,
    service: State<'_, crate::service::ServiceHandle>,
) -> Result<Vec<MetricType>, String> {
    // Update and persist overlays_visible = true
    let mut config = service.config().await;
    config.overlay_settings.overlays_visible = true;
    service.update_config(config.clone()).await?;

    let enabled_keys = config.overlay_settings.enabled_types();
    let metric_opacity = config.overlay_settings.metric_opacity;
    let personal_opacity = config.overlay_settings.personal_opacity;

    // Get current combat data once for all overlays
    let combat_data = if service.is_tailing().await {
        service.current_combat_data().await
    } else {
        None
    };

    let mut shown_metric_types = Vec::new();
    // Track overlays that need their monitor_id saved: (config_key, tx)
    let mut needs_monitor_save: Vec<(String, tokio::sync::mpsc::Sender<OverlayCommand>)> = Vec::new();

    for key in &enabled_keys {
        if key == "personal" {
            // Handle personal overlay
            let kind = OverlayType::Personal;
            let already_running = {
                let state = state.lock().map_err(|e| e.to_string())?;
                state.is_running(kind)
            };

            if !already_running {
                let position = config.overlay_settings.get_position("personal");
                let needs_save = position.monitor_id.is_none();
                let personal_config = config.overlay_settings.personal_overlay.clone();
                let overlay_handle = create_personal_overlay(position, personal_config, personal_opacity)?;
                let tx = overlay_handle.tx.clone();

                {
                    let mut state = state.lock().map_err(|e| e.to_string())?;
                    state.insert(overlay_handle);
                }

                // Send initial personal stats if available
                if let Some(ref data) = combat_data
                    && let Some(stats) = data.to_personal_stats()
                {
                    let _ = tx.send(OverlayCommand::UpdateData(OverlayData::Personal(stats))).await;
                }

                if needs_save {
                    needs_monitor_save.push(("personal".to_string(), tx));
                }
            }
        } else if let Some(overlay_type) = MetricType::from_config_key(key) {
            // Handle metric overlay
            let kind = OverlayType::Metric(overlay_type);

            // Check if already running
            {
                let state = state.lock().map_err(|e| e.to_string())?;
                if state.is_running(kind) {
                    shown_metric_types.push(overlay_type);
                    continue;
                }
            }

            // Load position, appearance, and spawn
            let position = config.overlay_settings.get_position(key);
            let needs_save = position.monitor_id.is_none();
            let appearance = config.overlay_settings.get_appearance(key);
            let overlay_handle = create_metric_overlay(overlay_type, position, appearance, metric_opacity)?;
            let tx = overlay_handle.tx.clone();

            // Update state
            {
                let mut state = state.lock().map_err(|e| e.to_string())?;
                state.insert(overlay_handle);
            }

            // Send current metrics if available
            if let Some(ref data) = combat_data
                && !data.metrics.is_empty()
            {
                let entries = create_entries_for_type(overlay_type, &data.metrics);
                let _ = tx.send(OverlayCommand::UpdateData(OverlayData::Metrics(entries))).await;
            }

            if needs_save {
                needs_monitor_save.push((key.clone(), tx.clone()));
            }

            shown_metric_types.push(overlay_type);
        }
    }

    // Save monitor_id for overlays that didn't have one
    if !needs_monitor_save.is_empty() {
        // Give overlays a moment to be placed by compositor
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let mut config = service.config().await;
        for (key, tx) in needs_monitor_save {
            let (pos_tx, pos_rx) = tokio::sync::oneshot::channel();
            let _ = tx.send(OverlayCommand::GetPosition(pos_tx)).await;
            if let Ok(pos) = pos_rx.await {
                let relative_x = pos.x - pos.monitor_x;
                let relative_y = pos.y - pos.monitor_y;
                config.overlay_settings.set_position(
                    &key,
                    OverlayPositionConfig {
                        x: relative_x,
                        y: relative_y,
                        width: pos.width,
                        height: pos.height,
                        monitor_id: pos.monitor_id,
                    },
                );
            }
        }
        let _ = service.update_config(config).await;
    }

    Ok(shown_metric_types)
}

// ─────────────────────────────────────────────────────────────────────────────
// Move Mode and Status
// ─────────────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn toggle_move_mode(
    state: State<'_, SharedOverlayState>,
    service: State<'_, crate::service::ServiceHandle>,
) -> Result<bool, String> {
    let (txs, new_mode) = {
        let mut state = state.lock().map_err(|e| e.to_string())?;
        if !state.any_running() {
            return Err("No overlays running".to_string());
        }
        state.move_mode = !state.move_mode;
        let txs: Vec<_> = state.all_txs().into_iter().cloned().collect();
        (txs, state.move_mode)
    };

    // Send to all overlays (both metric and personal use the same command now)
    for tx in &txs {
        let _ = tx.send(OverlayCommand::SetMoveMode(new_mode)).await;
    }

    // When locking (move_mode = false), save all overlay positions
    if !new_mode {
        let mut positions = Vec::new();
        for tx in &txs {
            let (pos_tx, pos_rx) = tokio::sync::oneshot::channel();
            let _ = tx.send(OverlayCommand::GetPosition(pos_tx)).await;
            if let Ok(pos) = pos_rx.await {
                positions.push(pos);
            }
        }

        // Save positions to config (relative to monitor)
        let mut config = service.config().await;
        for pos in positions {
            // Convert absolute screen position to relative monitor position
            let relative_x = pos.x - pos.monitor_x;
            let relative_y = pos.y - pos.monitor_y;

            config.overlay_settings.set_position(
                pos.kind.config_key(),
                OverlayPositionConfig {
                    x: relative_x,
                    y: relative_y,
                    width: pos.width,
                    height: pos.height,
                    monitor_id: pos.monitor_id.clone(),
                },
            );
        }
        service.update_config(config).await.map_err(|e| e.to_string())?;
    }

    Ok(new_mode)
}

#[tauri::command]
pub async fn get_overlay_status(
    state: State<'_, SharedOverlayState>,
    service: State<'_, crate::service::ServiceHandle>,
) -> Result<OverlayStatusResponse, String> {
    let (running_metric_types, personal_running, move_mode) = {
        let state = state.lock().map_err(|e| e.to_string())?;
        (
            state.running_metric_types(),
            state.is_personal_running(),
            state.move_mode,
        )
    };

    // Get enabled types and visibility from config
    let config = service.config().await;
    let enabled: Vec<MetricType> = config
        .overlay_settings
        .enabled_types()
        .iter()
        .filter_map(|key| MetricType::from_config_key(key))
        .collect();

    let personal_enabled = config.overlay_settings.is_enabled("personal");

    Ok(OverlayStatusResponse {
        running: running_metric_types,
        enabled,
        personal_running,
        personal_enabled,
        overlays_visible: config.overlay_settings.overlays_visible,
        move_mode,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Settings Refresh
// ─────────────────────────────────────────────────────────────────────────────

/// Refresh overlay settings for all running overlays
#[tauri::command]
pub async fn refresh_overlay_settings(
    state: State<'_, SharedOverlayState>,
    service: State<'_, crate::service::ServiceHandle>,
) -> Result<bool, String> {
    let config = service.config().await;
    let metric_opacity = config.overlay_settings.metric_opacity;
    let personal_opacity = config.overlay_settings.personal_opacity;

    // Get all running overlays with their kinds
    let overlays: Vec<_> = {
        let state = state.lock().map_err(|e| e.to_string())?;
        state.all_overlays().into_iter().map(|(k, tx)| (k, tx.clone())).collect()
    };

    // Send updated config to each overlay based on its type (with per-category opacity)
    for (kind, tx) in overlays {
        let config_update = match kind {
            OverlayType::Metric(overlay_type) => {
                let appearance = config.overlay_settings.get_appearance(overlay_type.config_key());
                OverlayConfigUpdate::Metric(appearance, metric_opacity)
            }
            OverlayType::Personal => {
                let personal_config = config.overlay_settings.personal_overlay.clone();
                OverlayConfigUpdate::Personal(personal_config, personal_opacity)
            }
        };
        let _ = tx.send(OverlayCommand::UpdateConfig(config_update)).await;
    }

    Ok(true)
}
