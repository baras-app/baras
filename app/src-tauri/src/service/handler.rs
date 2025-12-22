use crate::overlay::MetricType;
use crate::service::CombatData;
use crate::service::LogFileInfo;
use crate::service::SharedState;
use crate::service::SessionInfo;
use crate::service::ServiceCommand;
use std::sync::atomic::Ordering;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

use baras_core::context::{resolve, AppConfig, OverlayAppearanceConfig};
use baras_core::PlayerMetrics;
use baras_core::encounter::EncounterState;

// ─────────────────────────────────────────────────────────────────────────────
// Service Handle (for Tauri commands)
// ─────────────────────────────────────────────────────────────────────────────

/// Handle to communicate with the combat service and query state
#[derive(Clone)]
pub struct ServiceHandle {
    pub cmd_tx: mpsc::Sender<ServiceCommand>,
    pub shared: Arc<SharedState>,
}

impl ServiceHandle {
    /// Send command to start tailing a log file
    pub async fn start_tailing(&self, path: PathBuf) -> Result<(), String> {
        self.cmd_tx
            .send(ServiceCommand::StartTailing(path))
            .await
            .map_err(|e| e.to_string())
    }

    /// Send command to stop tailing
    pub async fn stop_tailing(&self) -> Result<(), String> {
        self.cmd_tx
            .send(ServiceCommand::StopTailing)
            .await
            .map_err(|e| e.to_string())
    }

    /// Send command to refresh the directory index
    pub async fn refresh_index(&self) -> Result<(), String> {
        self.cmd_tx
            .send(ServiceCommand::RefreshIndex)
            .await
            .map_err(|e| e.to_string())
    }

    /// Send command to restart the directory watcher
    pub async fn restart_watcher(&self) -> Result<(), String> {
        self.cmd_tx
            .send(ServiceCommand::DirectoryChanged)
            .await
            .map_err(|e| e.to_string())
    }

    /// Get the current configuration
    pub async fn config(&self) -> AppConfig {
        self.shared.config.read().await.clone()
    }

    /// Update the configuration
    pub async fn update_config(&self, config: AppConfig) -> Result<(), String> {
        let old_config = self.shared.config.read().await.clone();
        let old_dir = old_config.log_directory.clone();
        let new_dir = config.log_directory.clone();

        // Check if grid size changed
        let old_slots = old_config.overlay_settings.raid_overlay.grid_columns
            * old_config.overlay_settings.raid_overlay.grid_rows;
        let new_slots = config.overlay_settings.raid_overlay.grid_columns
            * config.overlay_settings.raid_overlay.grid_rows;

        *self.shared.config.write().await = config.clone();
        config.save();

        // Update raid registry max slots if grid size changed
        if new_slots != old_slots {
            if let Ok(mut registry) = self.shared.raid_registry.lock() {
                registry.set_max_slots(new_slots);
            }
        }

        if old_dir != new_dir {
            self.cmd_tx
                .send(ServiceCommand::DirectoryChanged)
                .await
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Get log file entries for the UI
    pub async fn log_files(&self) -> Vec<LogFileInfo> {
        let index = self.shared.directory_index.read().await;
        index
            .entries()
            .into_iter()
            .map(|e| LogFileInfo {
                path: e.path.clone(),
                display_name: e.display_name(),
                character_name: e.character_name.clone(),
                date: e.date.to_string(),
                is_empty: e.is_empty,
            })
            .collect()
    }

    /// Check if currently tailing a file
    pub async fn is_tailing(&self) -> bool {
        self.shared.session.read().await.is_some()
    }

    /// Check if directory watcher is active
    pub fn is_watching(&self) -> bool {
        self.shared.watching.load(Ordering::SeqCst)
    }

    pub async fn active_file(&self) -> Option<String> {
        self.shared.with_session(|session|
            { session.active_file.as_ref().map(|p| p.to_string_lossy().to_string())})
            .await
            .unwrap_or(Some("None".to_string()))
    }

    /// Get current session info
    pub async fn session_info(&self) -> Option<SessionInfo> {
        let session_guard = self.shared.session.read().await;
        let session = session_guard.as_ref()?;
        let session = session.read().await;
        let cache = session.session_cache.as_ref()?;

        Some(SessionInfo {
            player_name: if cache.player_initialized {
                Some(resolve(cache.player.name).to_string())
            } else {
                None
            },
            player_class: if cache.player_initialized {
                Some(cache.player.class_name.clone())
            } else {
                None
            },
            player_discipline: if cache.player_initialized {
                Some(cache.player.discipline_name.clone())
            } else {
                None
            },
            area_name: if !cache.current_area.area_name.is_empty() {
                Some(cache.current_area.area_name.clone())
            } else {
                None
            },
            in_combat: self.shared.in_combat.load(Ordering::SeqCst),
            encounter_count: cache.encounters().filter(|e| e.state != EncounterState::NotStarted ).map(|e| e.id + 1).max().unwrap_or(0) as usize
        })
    }

    /// Get current combat data (unified for all overlays)
    pub async fn current_combat_data(&self) -> Option<CombatData> {
        super::calculate_combat_data(&self.shared).await
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Raid Registry Operations
    // ─────────────────────────────────────────────────────────────────────────

    /// Swap two slots in the raid registry
    pub async fn swap_raid_slots(&self, slot_a: u8, slot_b: u8) {
        if let Ok(mut registry) = self.shared.raid_registry.lock() {
            registry.swap_slots(slot_a, slot_b);
        }
    }

    /// Remove a slot from the raid registry
    pub async fn remove_raid_slot(&self, slot: u8) {
        if let Ok(mut registry) = self.shared.raid_registry.lock() {
            registry.remove_slot(slot);
        }
    }

    /// Clear all raid registry slots
    pub async fn clear_raid_registry(&self) {
        if let Ok(mut registry) = self.shared.raid_registry.lock() {
            registry.clear();
        }
    }
}



// ─────────────────────────────────────────────────────────────────────────────
// Tauri Commands
// ─────────────────────────────────────────────────────────────────────────────

use tauri::State;

#[tauri::command]
pub async fn get_log_files(handle: State<'_, ServiceHandle>) -> Result<Vec<LogFileInfo>, String> {
    Ok(handle.log_files().await)
}

#[tauri::command]
pub async fn start_tailing(path: PathBuf, handle: State<'_, ServiceHandle>) -> Result<(), String> {
    handle.start_tailing(path).await
}

#[tauri::command]
pub async fn stop_tailing(handle: State<'_, ServiceHandle>) -> Result<(), String> {
    handle.stop_tailing().await
}

#[tauri::command]
pub async fn refresh_log_index(handle: State<'_, ServiceHandle>) -> Result<(), String> {
    handle.refresh_index().await
}

#[tauri::command]
pub async fn restart_watcher(handle: State<'_, ServiceHandle>) -> Result<(), String> {
    handle.restart_watcher().await
}

#[tauri::command]
pub async fn get_tailing_status(handle: State<'_, ServiceHandle>) -> Result<bool, String> {
    Ok(handle.is_tailing().await)
}

#[tauri::command]
pub async fn get_watching_status(handle: State<'_, ServiceHandle>) -> Result<bool, String> {
    Ok(handle.is_watching())
}

#[tauri::command]
pub async fn get_current_metrics(
    handle: State<'_, ServiceHandle>,
) -> Result<Option<Vec<PlayerMetrics>>, String> {
    Ok(handle.current_combat_data().await.map(|d| d.metrics))
}

#[tauri::command]
pub async fn get_config(handle: State<'_, ServiceHandle>) -> Result<AppConfig, String> {
    let mut config = handle.config().await;

    // Populate default appearances for each overlay type using baras_core constants
    for metric_type in MetricType::all() {
        let key = metric_type.config_key();
        config.overlay_settings.default_appearances.insert(
            key.to_string(),
            OverlayAppearanceConfig::default_for_type(key),
        );
    }

    Ok(config)
}

#[tauri::command]
pub async fn get_active_file(handle: State<'_, ServiceHandle>) -> Result<Option<String>, String> {
    Ok(handle.active_file().await)
}

#[tauri::command]
pub async fn update_config(
    config: AppConfig,
    handle: State<'_, ServiceHandle>
) -> Result<(), String> {
    handle.update_config(config).await
}

#[tauri::command]
pub async fn get_session_info(
    handle: State<'_, ServiceHandle>
) -> Result<Option<SessionInfo>, String> {
    Ok(handle.session_info().await)
}

// ─────────────────────────────────────────────────────────────────────────────
// Profile Commands
// ─────────────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_profile_names(handle: State<'_, ServiceHandle>) -> Result<Vec<String>, String> {
    let config = handle.config().await;
    Ok(config.profile_names())
}

#[tauri::command]
pub async fn get_active_profile(handle: State<'_, ServiceHandle>) -> Result<Option<String>, String> {
    let config = handle.config().await;
    Ok(config.active_profile_name.clone())
}

/// Sync the enabled map with actual running overlay state
pub fn sync_enabled_with_running(
    config: &mut baras_core::context::AppConfig,
    overlay_state: &crate::overlay::OverlayState,
) {
    use crate::overlay::{OverlayType, MetricType};

    // Sync raid overlay state
    let raid_running = overlay_state.is_running(OverlayType::Raid);
    config.overlay_settings.set_enabled("raid", raid_running);

    // Sync personal overlay state
    let personal_running = overlay_state.is_running(OverlayType::Personal);
    config.overlay_settings.set_enabled("personal", personal_running);

    // Sync all metric overlay states
    for metric_type in MetricType::all() {
        let running = overlay_state.is_running(OverlayType::Metric(*metric_type));
        config.overlay_settings.set_enabled(metric_type.config_key(), running);
    }
}

#[tauri::command]
pub async fn save_profile(
    name: String,
    handle: State<'_, ServiceHandle>,
    overlay_state: State<'_, crate::overlay::SharedOverlayState>,
) -> Result<(), String> {
    let mut config = handle.config().await;

    // Sync enabled state with actual running overlays before saving
    if let Ok(state) = overlay_state.lock() {
        sync_enabled_with_running(&mut config, &state);
    }

    config.save_profile(name).map_err(|e| e.to_string())?;
    *handle.shared.config.write().await = config.clone();
    config.save();
    Ok(())
}

#[tauri::command]
pub async fn load_profile(name: String, handle: State<'_, ServiceHandle>) -> Result<(), String> {
    let mut config = handle.config().await;
    config.load_profile(&name).map_err(|e| e.to_string())?;
    *handle.shared.config.write().await = config.clone();
    config.save();
    Ok(())
}

#[tauri::command]
pub async fn delete_profile(name: String, handle: State<'_, ServiceHandle>) -> Result<(), String> {
    let mut config = handle.config().await;
    config.delete_profile(&name).map_err(|e| e.to_string())?;
    *handle.shared.config.write().await = config.clone();
    config.save();
    Ok(())
}

#[tauri::command]
pub async fn rename_profile(
    old_name: String,
    new_name: String,
    handle: State<'_, ServiceHandle>
) -> Result<(), String> {
    let mut config = handle.config().await;
    config.rename_profile(&old_name, new_name).map_err(|e| e.to_string())?;
    *handle.shared.config.write().await = config.clone();
    config.save();
    Ok(())
}
