use crate::service::LogFileInfo;
use crate::service::SharedState;
use crate::service::PlayerMetrics;
use crate::service::ServiceCommand;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc};

use baras_core::context::{resolve, AppConfig};
use baras_core::EntityType;

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

    /// Get the current configuration
    pub async fn config(&self) -> AppConfig {
        self.shared.config.read().await.clone()
    }

    /// Update the configuration
    pub async fn update_config(&self, config: AppConfig) -> Result<(), String> {
        self.cmd_tx
            .send(ServiceCommand::UpdateConfig(config))
            .await
            .map_err(|e| e.to_string())
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

    pub async fn active_file(&self) -> Option<String> {
        self.shared.with_session(|session|
            { session.active_file.as_ref().map(|p| p.to_string_lossy().to_string())})
            .await
            .unwrap_or(Some("None".to_string()))
    }

    /// Get current encounter metrics
    pub async fn current_metrics(&self) -> Option<Vec<PlayerMetrics>> {
        let session_guard = self.shared.session.read().await;
        let session = session_guard.as_ref()?;
        let session = session.read().await;
        let cache = session.session_cache.as_ref()?;
        let encounter = cache.last_combat_encounter()?;

        let entity_metrics = encounter.calculate_entity_metrics()?;

        Some(
            entity_metrics
                .into_iter()
                .filter(|m| m.entity_type != EntityType::Npc)
                .map(|m| PlayerMetrics {
                    entity_id: m.entity_id,
                    name: resolve(m.name).to_string(),
                    dps: m.dps as i64,
                    total_damage: m.total_damage as u64,
                    hps: m.hps as i64,
                    total_healing: m.total_healing as u64,
                })
                .collect(),
        )
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
pub async fn get_tailing_status(handle: State<'_, ServiceHandle>) -> Result<bool, String> {
    Ok(handle.is_tailing().await)
}

#[tauri::command]
pub async fn get_current_metrics(
    handle: State<'_, ServiceHandle>,
) -> Result<Option<Vec<PlayerMetrics>>, String> {
    Ok(handle.current_metrics().await)
}

#[tauri::command]
pub async fn get_config(handle: State<'_, ServiceHandle>) -> Result<AppConfig, String> {
    Ok(handle.config().await)
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
