//! Service handle for communicating with the combat service
//!
//! Provides async methods for Tauri commands to interact with the background service.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tokio::sync::mpsc;

use baras_core::EncounterSummary;
use baras_core::context::{AppConfig, AppConfigExt, resolve};
use baras_core::encounter::EncounterState;

use super::{CombatData, LogFileInfo, ServiceCommand, SessionInfo};
use crate::state::SharedState;

/// Handle to communicate with the combat service and query state
#[derive(Clone)]
pub struct ServiceHandle {
    pub cmd_tx: mpsc::Sender<ServiceCommand>,
    pub shared: Arc<SharedState>,
}

impl ServiceHandle {
    // ─────────────────────────────────────────────────────────────────────────
    // Tailing Operations
    // ─────────────────────────────────────────────────────────────────────────

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

    /// Check if currently tailing a file
    pub async fn is_tailing(&self) -> bool {
        self.shared.session.read().await.is_some()
    }

    /// Check if directory watcher is active
    pub fn is_watching(&self) -> bool {
        self.shared.watching.load(Ordering::SeqCst)
    }

    pub async fn active_file(&self) -> Option<String> {
        self.shared
            .with_session(|session| {
                session
                    .active_file
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string())
            })
            .await
            .unwrap_or(Some("None".to_string()))
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Directory Operations
    // ─────────────────────────────────────────────────────────────────────────

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
                date: e.formatted_datetime(),
                is_empty: e.is_empty,
                file_size: e.file_size,
            })
            .collect()
    }

    /// Get total size of all log files in bytes
    pub async fn log_directory_size(&self) -> u64 {
        let index = self.shared.directory_index.read().await;
        index.total_size()
    }

    /// Get count of log files
    pub async fn log_file_count(&self) -> usize {
        let index = self.shared.directory_index.read().await;
        index.len()
    }

    /// Clean up log files based on provided settings. Returns (empty_deleted, old_deleted).
    pub async fn cleanup_logs(
        &self,
        delete_empty: bool,
        retention_days: Option<u32>,
    ) -> (u32, u32) {
        let mut index = self.shared.directory_index.write().await;
        index.cleanup(delete_empty, retention_days)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Configuration
    // ─────────────────────────────────────────────────────────────────────────

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
        if new_slots != old_slots
            && let Ok(mut registry) = self.shared.raid_registry.lock()
        {
            registry.set_max_slots(new_slots);
        }

        if old_dir != new_dir {
            self.cmd_tx
                .send(ServiceCommand::DirectoryChanged)
                .await
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Session Data
    // ─────────────────────────────────────────────────────────────────────────

    /// Get current session info
    pub async fn session_info(&self) -> Option<SessionInfo> {
        let session_guard = self.shared.session.read().await;
        let session = session_guard.as_ref()?;
        let session = session.read().await;
        let cache = session.session_cache.as_ref()?;

        // Extract session start time from active file name
        let session_start = session.active_file.as_ref().and_then(|path| {
            let filename = path.file_name()?.to_str()?;
            let (_, datetime) = baras_core::context::parse_log_filename(filename)?;
            Some(datetime.format("%b %d, %l:%M %p").to_string())
        });

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
            encounter_count: cache
                .encounters()
                .filter(|e| e.state != EncounterState::NotStarted)
                .map(|e| e.id + 1)
                .max()
                .unwrap_or(0) as usize,
            session_start,
        })
    }

    /// Get current combat data (unified for all overlays)
    pub async fn current_combat_data(&self) -> Option<CombatData> {
        super::calculate_combat_data(&self.shared).await
    }

    /// Get encounter history for the current log file
    pub async fn encounter_history(&self) -> Vec<EncounterSummary> {
        let session_guard = self.shared.session.read().await;
        let Some(session) = session_guard.as_ref() else {
            return Vec::new();
        };
        let session = session.read().await;
        let Some(cache) = session.session_cache.as_ref() else {
            return Vec::new();
        };

        cache.encounter_history.summaries().to_vec()
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

    // ─────────────────────────────────────────────────────────────────────────
    // Timer Operations
    // ─────────────────────────────────────────────────────────────────────────

    /// Reload timer/boss definitions from disk and update active session
    pub async fn reload_timer_definitions(&self) -> Result<(), String> {
        self.cmd_tx
            .send(ServiceCommand::ReloadTimerDefinitions)
            .await
            .map_err(|e| e.to_string())
    }

    /// Reload effect definitions from disk and update active session
    pub async fn reload_effect_definitions(&self) -> Result<(), String> {
        self.cmd_tx
            .send(ServiceCommand::ReloadEffectDefinitions)
            .await
            .map_err(|e| e.to_string())
    }

    // ─────────────────────────────────────────────────────────────────────────
    // File Browser Operations
    // ─────────────────────────────────────────────────────────────────────────

    /// Open a historical file (pauses live tailing)
    pub async fn open_historical_file(&self, path: PathBuf) -> Result<(), String> {
        self.cmd_tx
            .send(ServiceCommand::OpenHistoricalFile(path))
            .await
            .map_err(|e| e.to_string())
    }

    /// Resume live tailing (switch to newest file)
    pub async fn resume_live_tailing(&self) -> Result<(), String> {
        self.cmd_tx
            .send(ServiceCommand::ResumeLiveTailing)
            .await
            .map_err(|e| e.to_string())
    }

    /// Check if in live tailing mode
    pub fn is_live_tailing(&self) -> bool {
        self.shared.is_live_tailing.load(Ordering::SeqCst)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Overlay Status Flags (for skipping work in effects loop)
    // ─────────────────────────────────────────────────────────────────────────

    /// Update the overlay status flag for a specific overlay type
    pub fn set_overlay_active(&self, kind: &str, active: bool) {
        match kind {
            "raid" => self
                .shared
                .raid_overlay_active
                .store(active, Ordering::SeqCst),
            "boss_health" => self
                .shared
                .boss_health_overlay_active
                .store(active, Ordering::SeqCst),
            "timers" => self
                .shared
                .timer_overlay_active
                .store(active, Ordering::SeqCst),
            "effects" => self
                .shared
                .effects_overlay_active
                .store(active, Ordering::SeqCst),
            _ => {}
        }
    }
}
