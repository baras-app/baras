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
use baras_core::game_data::Discipline;
use baras_core::query::{
    AbilityBreakdown, BreakdownMode, CombatLogFilters, CombatLogFindMatch, CombatLogRow, DataTab,
    EffectChartData, EffectWindow, EncounterTimeline, EntityBreakdown, PlayerDeath,
    RaidOverviewRow, TimeRange, TimeSeriesPoint,
};
use tauri::{AppHandle, Emitter};

use super::{CombatData, LogFileInfo, ServiceCommand, SessionInfo};
use crate::state::SharedState;

/// Handle to communicate with the combat service and query state
#[derive(Clone)]
pub struct ServiceHandle {
    pub cmd_tx: mpsc::Sender<ServiceCommand>,
    pub shared: Arc<SharedState>,
    pub app_handle: AppHandle,
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

    /// Refresh file sizes in the directory index (fast stat-only, no re-parsing)
    pub async fn refresh_file_sizes(&self) {
        let mut index = self.shared.directory_index.write().await;
        index.refresh_file_sizes();
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

        let alacrity_changed = old_config.alacrity_percent != config.alacrity_percent;
        let latency_changed = old_config.latency_ms != config.latency_ms;
        let new_alacrity = config.alacrity_percent;
        let new_latency = config.latency_ms;

        *self.shared.config.write().await = config.clone();
        if let Err(e) = config.save() {
            tracing::error!(error = %e, "Failed to save configuration");
        }

        // Update raid registry max slots if grid size changed
        if new_slots != old_slots {
            self.shared.raid_registry.lock().unwrap_or_else(|p| p.into_inner()).set_max_slots(new_slots);
        }

        // Update effect tracker alacrity/latency if changed
        if alacrity_changed || latency_changed {
            if let Some(session) = self.shared.session.read().await.as_ref() {
                let session = session.read().await;
                if let Some(tracker) = session.effect_tracker() {
                    let mut tracker = tracker.lock().unwrap_or_else(|p| p.into_inner());
                    if alacrity_changed {
                        tracker.set_alacrity(new_alacrity);
                    }
                    if latency_changed {
                        tracker.set_latency(new_latency);
                    }
                }
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
        let start_datetime = session.active_file.as_ref().and_then(|path| {
            let filename = path.file_name()?.to_str()?;
            let (_, datetime) = baras_core::context::parse_log_filename(filename)?;
            Some(datetime)
        });
        let session_start = start_datetime.map(|dt| dt.format("%b %d, %l:%M %p").to_string());

        // For historical sessions, calculate end time and duration
        let is_live = self.shared.is_live_tailing.load(Ordering::SeqCst);
        let (session_end, duration_formatted) = if !is_live {
            // Try to get end time from last encounter (ISO 8601 format)
            let last_end_time = cache
                .encounter_history
                .summaries()
                .iter()
                .rev()
                .find_map(|s| s.end_time.as_ref())
                .and_then(|iso| chrono::NaiveDateTime::parse_from_str(iso, "%+").ok());

            // Fallback to file modification time if no encounters
            let end_naive = last_end_time.or_else(|| {
                session.active_file.as_ref().and_then(|path| {
                    path.metadata()
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .map(|st| chrono::DateTime::<chrono::Local>::from(st).naive_local())
                })
            });

            let session_end = end_naive.map(|dt| dt.format("%b %d, %l:%M %p").to_string());

            // Calculate duration if we have both start and end
            let duration_formatted = match (start_datetime, end_naive) {
                (Some(start), Some(end)) => {
                    let duration = end.signed_duration_since(start);
                    let total_mins = duration.num_minutes();
                    if total_mins < 60 {
                        Some(format!("{}m", total_mins.max(1)))
                    } else {
                        let hours = total_mins / 60;
                        let mins = total_mins % 60;
                        if mins == 0 {
                            Some(format!("{}h", hours))
                        } else {
                            Some(format!("{}h {}m", hours, mins))
                        }
                    }
                }
                _ => None,
            };

            (session_end, duration_formatted)
        } else {
            (None, None)
        };

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
            session_end,
            duration_formatted,
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

    /// Set the Parsely link for a specific encounter
    pub async fn set_encounter_parsely_link(&self, encounter_id: u64, link: String) -> bool {
        let session_guard = self.shared.session.read().await;
        let Some(session) = session_guard.as_ref() else {
            return false;
        };
        let mut session = session.write().await;
        let Some(cache) = session.session_cache.as_mut() else {
            return false;
        };

        cache.encounter_history.set_parsely_link(encounter_id, link)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Raid Registry Operations
    // ─────────────────────────────────────────────────────────────────────────

    /// Swap two slots in the raid registry
    pub async fn swap_raid_slots(&self, slot_a: u8, slot_b: u8) {
        self.shared.raid_registry.lock().unwrap_or_else(|p| p.into_inner()).swap_slots(slot_a, slot_b);
        self.refresh_raid_frames().await;
    }

    /// Remove a slot from the raid registry
    pub async fn remove_raid_slot(&self, slot: u8) {
        self.shared.raid_registry.lock().unwrap_or_else(|p| p.into_inner()).remove_slot(slot);
        self.refresh_raid_frames().await;
    }

    /// Clear all raid registry slots
    pub async fn clear_raid_registry(&self) {
        self.shared.raid_registry.lock().unwrap_or_else(|p| p.into_inner()).clear();
        self.refresh_raid_frames().await;
    }

    /// Trigger immediate raid frame refresh
    pub async fn refresh_raid_frames(&self) {
        let _ = self.cmd_tx.send(ServiceCommand::RefreshRaidFrames).await;
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
    // Query Operations
    // ─────────────────────────────────────────────────────────────────────────

    /// Query ability breakdown for a specific encounter and data tab.
    /// If encounter_idx is None, queries the live encounter buffer.
    pub async fn query_breakdown(
        &self,
        tab: DataTab,
        encounter_idx: Option<u32>,
        entity_name: Option<String>,
        time_range: Option<TimeRange>,
        entity_types: Option<Vec<String>>,
        breakdown_mode: Option<BreakdownMode>,
        duration_secs: Option<f32>,
    ) -> Result<Vec<AbilityBreakdown>, String> {
        let session_guard = self.shared.session.read().await;
        let session = session_guard.as_ref().ok_or("No active session")?;
        let session = session.read().await;

        if let Some(idx) = encounter_idx {
            // Query historical parquet
            let dir = session.encounters_dir().ok_or("No encounters directory")?;
            let path = dir.join(baras_core::storage::encounter_filename(idx));
            if !path.exists() {
                return Err(format!("Encounter file not found: {:?}", path));
            }
            self.shared.query_context.register_parquet(&path).await?;
        } else {
            // Query live buffer
            let writer = session
                .encounter_writer()
                .ok_or("No live encounter buffer")?;
            let batch = writer.to_record_batch().ok_or("Live buffer is empty")?;
            self.shared.query_context.register_batch(batch).await?;
        }

        let types_ref: Option<Vec<&str>> = entity_types
            .as_ref()
            .map(|v| v.iter().map(|s| s.as_str()).collect());
        self.shared
            .query_context
            .query()
            .await
            .query()
            .query_breakdown(
                tab,
                entity_name.as_deref(),
                time_range.as_ref(),
                types_ref.as_deref(),
                breakdown_mode.as_ref(),
                duration_secs,
            )
            .await
    }

    /// Query breakdown by entity for a specific encounter and data tab.
    pub async fn query_entity_breakdown(
        &self,
        tab: DataTab,
        encounter_idx: Option<u32>,
        time_range: Option<TimeRange>,
    ) -> Result<Vec<EntityBreakdown>, String> {
        let session_guard = self.shared.session.read().await;
        let session = session_guard.as_ref().ok_or("No active session")?;
        let session = session.read().await;

        if let Some(idx) = encounter_idx {
            let dir = session.encounters_dir().ok_or("No encounters directory")?;
            let path = dir.join(baras_core::storage::encounter_filename(idx));
            if !path.exists() {
                return Err(format!("Encounter file not found: {:?}", path));
            }
            self.shared.query_context.register_parquet(&path).await?;
        } else {
            let writer = session
                .encounter_writer()
                .ok_or("No live encounter buffer")?;
            let batch = writer.to_record_batch().ok_or("Live buffer is empty")?;
            self.shared.query_context.register_batch(batch).await?;
        }

        self.shared
            .query_context
            .query()
            .await
            .query()
            .breakdown_by_entity(tab, time_range.as_ref())
            .await
    }

    /// Query raid overview - aggregated stats per player.
    pub async fn query_raid_overview(
        &self,
        encounter_idx: Option<u32>,
        time_range: Option<TimeRange>,
        duration_secs: Option<f32>,
    ) -> Result<Vec<RaidOverviewRow>, String> {
        let session_guard = self.shared.session.read().await;
        let session = session_guard.as_ref().ok_or("No active session")?;
        let session = session.read().await;

        // Build discipline map - source depends on live vs historical query
        let mut player_discipline_map: std::collections::HashMap<
            String,
            (String, String, String, String),
        > = std::collections::HashMap::new();

        if let Some(idx) = encounter_idx {
            // Historical query: get discipline info from that encounter's summary
            // This captures the discipline each player had AT THAT TIME
            if let Some(cache) = session.session_cache.as_ref() {
                if let Some(summary) = cache
                    .encounter_history
                    .summaries()
                    .iter()
                    .find(|s| s.encounter_id == idx as u64)
                {
                    for pm in &summary.player_metrics {
                        if let (Some(class_icon), Some(role_icon)) = (&pm.class_icon, &pm.role_icon)
                        {
                            player_discipline_map.insert(
                                pm.name.clone(),
                                (
                                    pm.class_name.clone().unwrap_or_default(),
                                    pm.discipline_name.clone().unwrap_or_default(),
                                    class_icon.clone(),
                                    role_icon.clone(),
                                ),
                            );
                        }
                    }
                }
            }

            let dir = session.encounters_dir().ok_or("No encounters directory")?;
            let path = dir.join(baras_core::storage::encounter_filename(idx));
            if !path.exists() {
                return Err(format!("Encounter file not found: {:?}", path));
            }
            self.shared.query_context.register_parquet(&path).await?;
        } else {
            // Live query: use session-level registry (current disciplines)
            if let Some(cache) = session.session_cache.as_ref() {
                for p in cache.player_disciplines.values() {
                    if let Some(disc) = Discipline::from_guid(p.discipline_id) {
                        let name = resolve(p.name).to_string();
                        let class_icon = disc.class().icon_name().to_string();
                        let role_icon = disc.role().icon_name().to_string();
                        let discipline_name = disc.name().to_string();
                        let class_name = format!("{:?}", disc.class());
                        player_discipline_map
                            .insert(name, (class_name, discipline_name, class_icon, role_icon));
                    }
                }
            }

            let writer = session
                .encounter_writer()
                .ok_or("No live encounter buffer")?;
            let batch = writer.to_record_batch().ok_or("Live buffer is empty")?;
            self.shared.query_context.register_batch(batch).await?;
        }

        let mut results = self
            .shared
            .query_context
            .query()
            .await
            .query()
            .query_raid_overview(time_range.as_ref(), duration_secs)
            .await?;

        // Enrich results with discipline info
        for row in &mut results {
            if let Some((class_name, discipline_name, class_icon, role_icon)) =
                player_discipline_map.get(&row.name)
            {
                row.class_name = Some(class_name.clone());
                row.discipline_name = Some(discipline_name.clone());
                row.class_icon = Some(class_icon.clone());
                row.role_icon = Some(role_icon.clone());
            }
        }

        Ok(results)
    }

    /// Query DPS over time for a specific encounter.
    pub async fn query_dps_over_time(
        &self,
        encounter_idx: Option<u32>,
        bucket_ms: i64,
        source_name: Option<String>,
        time_range: Option<TimeRange>,
    ) -> Result<Vec<TimeSeriesPoint>, String> {
        let session_guard = self.shared.session.read().await;
        let session = session_guard.as_ref().ok_or("No active session")?;
        let session = session.read().await;

        if let Some(idx) = encounter_idx {
            let dir = session.encounters_dir().ok_or("No encounters directory")?;
            let path = dir.join(baras_core::storage::encounter_filename(idx));
            if !path.exists() {
                return Err(format!("Encounter file not found: {:?}", path));
            }
            self.shared.query_context.register_parquet(&path).await?;
        } else {
            let writer = session
                .encounter_writer()
                .ok_or("No live encounter buffer")?;
            let batch = writer.to_record_batch().ok_or("Live buffer is empty")?;
            self.shared.query_context.register_batch(batch).await?;
        }

        self.shared
            .query_context
            .query()
            .await
            .query()
            .dps_over_time(bucket_ms, source_name.as_deref(), time_range.as_ref())
            .await
    }

    /// Get list of available encounter parquet files.
    pub async fn list_encounter_files(&self) -> Result<Vec<u32>, String> {
        let session_guard = self.shared.session.read().await;
        let session = session_guard.as_ref().ok_or("No active session")?;
        let session = session.read().await;

        let dir = session.encounters_dir().ok_or("No encounters directory")?;

        let mut indices = Vec::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str()
                    && name.ends_with(".parquet")
                    && let Ok(idx) = name.trim_end_matches(".parquet").parse::<u32>()
                {
                    indices.push(idx);
                }
            }
        }
        indices.sort();
        Ok(indices)
    }

    /// Query encounter timeline with phase segments.
    pub async fn query_encounter_timeline(
        &self,
        encounter_idx: Option<u32>,
    ) -> Result<EncounterTimeline, String> {
        let session_guard = self.shared.session.read().await;
        let session = session_guard.as_ref().ok_or("No active session")?;
        let session = session.read().await;

        if let Some(idx) = encounter_idx {
            let dir = session.encounters_dir().ok_or("No encounters directory")?;
            let path = dir.join(baras_core::storage::encounter_filename(idx));
            if !path.exists() {
                return Err(format!("Encounter file not found: {:?}", path));
            }
            self.shared.query_context.register_parquet(&path).await?;
        } else {
            let writer = session
                .encounter_writer()
                .ok_or("No live encounter buffer")?;
            let batch = writer.to_record_batch().ok_or("Live buffer is empty")?;
            self.shared.query_context.register_batch(batch).await?;
        }

        self.shared
            .query_context
            .query()
            .await
            .query()
            .encounter_timeline()
            .await
    }

    /// Query HPS over time for a specific encounter.
    pub async fn query_hps_over_time(
        &self,
        encounter_idx: Option<u32>,
        bucket_ms: i64,
        source_name: Option<String>,
        time_range: Option<TimeRange>,
    ) -> Result<Vec<TimeSeriesPoint>, String> {
        let session_guard = self.shared.session.read().await;
        let session = session_guard.as_ref().ok_or("No active session")?;
        let session = session.read().await;

        if let Some(idx) = encounter_idx {
            let dir = session.encounters_dir().ok_or("No encounters directory")?;
            let path = dir.join(baras_core::storage::encounter_filename(idx));
            if !path.exists() {
                return Err(format!("Encounter file not found: {:?}", path));
            }
            self.shared.query_context.register_parquet(&path).await?;
        } else {
            let writer = session
                .encounter_writer()
                .ok_or("No live encounter buffer")?;
            let batch = writer.to_record_batch().ok_or("Live buffer is empty")?;
            self.shared.query_context.register_batch(batch).await?;
        }

        self.shared
            .query_context
            .query()
            .await
            .query()
            .hps_over_time(bucket_ms, source_name.as_deref(), time_range.as_ref())
            .await
    }

    /// Query DTPS over time for a specific encounter.
    pub async fn query_dtps_over_time(
        &self,
        encounter_idx: Option<u32>,
        bucket_ms: i64,
        target_name: Option<String>,
        time_range: Option<TimeRange>,
    ) -> Result<Vec<TimeSeriesPoint>, String> {
        let session_guard = self.shared.session.read().await;
        let session = session_guard.as_ref().ok_or("No active session")?;
        let session = session.read().await;

        if let Some(idx) = encounter_idx {
            let dir = session.encounters_dir().ok_or("No encounters directory")?;
            let path = dir.join(baras_core::storage::encounter_filename(idx));
            if !path.exists() {
                return Err(format!("Encounter file not found: {:?}", path));
            }
            self.shared.query_context.register_parquet(&path).await?;
        } else {
            let writer = session
                .encounter_writer()
                .ok_or("No live encounter buffer")?;
            let batch = writer.to_record_batch().ok_or("Live buffer is empty")?;
            self.shared.query_context.register_batch(batch).await?;
        }

        self.shared
            .query_context
            .query()
            .await
            .query()
            .dtps_over_time(bucket_ms, target_name.as_deref(), time_range.as_ref())
            .await
    }

    /// Query effect uptime statistics for the charts panel.
    pub async fn query_effect_uptime(
        &self,
        encounter_idx: Option<u32>,
        target_name: Option<String>,
        time_range: Option<TimeRange>,
        duration_secs: f32,
    ) -> Result<Vec<EffectChartData>, String> {
        let session_guard = self.shared.session.read().await;
        let session = session_guard.as_ref().ok_or("No active session")?;
        let session = session.read().await;

        if let Some(idx) = encounter_idx {
            let dir = session.encounters_dir().ok_or("No encounters directory")?;
            let path = dir.join(baras_core::storage::encounter_filename(idx));
            if !path.exists() {
                return Err(format!("Encounter file not found: {:?}", path));
            }
            self.shared.query_context.register_parquet(&path).await?;
        } else {
            let writer = session
                .encounter_writer()
                .ok_or("No live encounter buffer")?;
            let batch = writer.to_record_batch().ok_or("Live buffer is empty")?;
            self.shared.query_context.register_batch(batch).await?;
        }

        self.shared
            .query_context
            .query()
            .await
            .query()
            .query_effect_uptime(target_name.as_deref(), time_range.as_ref(), duration_secs)
            .await
    }

    /// Query individual time windows for a specific effect.
    pub async fn query_effect_windows(
        &self,
        encounter_idx: Option<u32>,
        effect_id: i64,
        target_name: Option<String>,
        time_range: Option<TimeRange>,
        duration_secs: f32,
    ) -> Result<Vec<EffectWindow>, String> {
        let session_guard = self.shared.session.read().await;
        let session = session_guard.as_ref().ok_or("No active session")?;
        let session = session.read().await;

        if let Some(idx) = encounter_idx {
            let dir = session.encounters_dir().ok_or("No encounters directory")?;
            let path = dir.join(baras_core::storage::encounter_filename(idx));
            if !path.exists() {
                return Err(format!("Encounter file not found: {:?}", path));
            }
            self.shared.query_context.register_parquet(&path).await?;
        } else {
            let writer = session
                .encounter_writer()
                .ok_or("No live encounter buffer")?;
            let batch = writer.to_record_batch().ok_or("Live buffer is empty")?;
            self.shared.query_context.register_batch(batch).await?;
        }

        self.shared
            .query_context
            .query()
            .await
            .query()
            .query_effect_windows(
                effect_id,
                target_name.as_deref(),
                time_range.as_ref(),
                duration_secs,
            )
            .await
    }

    /// Query combat log rows with pagination for virtual scrolling.
    pub async fn query_combat_log(
        &self,
        encounter_idx: Option<u32>,
        offset: u64,
        limit: u64,
        source_filter: Option<String>,
        target_filter: Option<String>,
        search_filter: Option<String>,
        time_range: Option<TimeRange>,
        event_filters: Option<CombatLogFilters>,
    ) -> Result<Vec<CombatLogRow>, String> {
        let session_guard = self.shared.session.read().await;
        let session = session_guard.as_ref().ok_or("No active session")?;
        let session = session.read().await;

        if let Some(idx) = encounter_idx {
            let dir = session.encounters_dir().ok_or("No encounters directory")?;
            let path = dir.join(baras_core::storage::encounter_filename(idx));
            if !path.exists() {
                return Err(format!("Encounter file not found: {:?}", path));
            }
            self.shared.query_context.register_parquet(&path).await?;
        } else {
            let writer = session
                .encounter_writer()
                .ok_or("No live encounter buffer")?;
            let batch = writer.to_record_batch().ok_or("Live buffer is empty")?;
            self.shared.query_context.register_batch(batch).await?;
        }

        self.shared
            .query_context
            .query()
            .await
            .query()
            .query_combat_log(
                offset,
                limit,
                source_filter.as_deref(),
                target_filter.as_deref(),
                search_filter.as_deref(),
                time_range.as_ref(),
                event_filters.as_ref(),
            )
            .await
    }

    /// Get total count of combat log rows for pagination.
    pub async fn query_combat_log_count(
        &self,
        encounter_idx: Option<u32>,
        source_filter: Option<String>,
        target_filter: Option<String>,
        search_filter: Option<String>,
        time_range: Option<TimeRange>,
        event_filters: Option<CombatLogFilters>,
    ) -> Result<u64, String> {
        let session_guard = self.shared.session.read().await;
        let session = session_guard.as_ref().ok_or("No active session")?;
        let session = session.read().await;

        if let Some(idx) = encounter_idx {
            let dir = session.encounters_dir().ok_or("No encounters directory")?;
            let path = dir.join(baras_core::storage::encounter_filename(idx));
            if !path.exists() {
                return Err(format!("Encounter file not found: {:?}", path));
            }
            self.shared.query_context.register_parquet(&path).await?;
        } else {
            let writer = session
                .encounter_writer()
                .ok_or("No live encounter buffer")?;
            let batch = writer.to_record_batch().ok_or("Live buffer is empty")?;
            self.shared.query_context.register_batch(batch).await?;
        }

        self.shared
            .query_context
            .query()
            .await
            .query()
            .query_combat_log_count(
                source_filter.as_deref(),
                target_filter.as_deref(),
                search_filter.as_deref(),
                time_range.as_ref(),
                event_filters.as_ref(),
            )
            .await
    }

    /// Find matching rows in combat log (returns position and row_idx).
    pub async fn query_combat_log_find(
        &self,
        encounter_idx: Option<u32>,
        find_text: String,
        source_filter: Option<String>,
        target_filter: Option<String>,
        time_range: Option<TimeRange>,
        event_filters: Option<CombatLogFilters>,
    ) -> Result<Vec<CombatLogFindMatch>, String> {
        let session_guard = self.shared.session.read().await;
        let session = session_guard.as_ref().ok_or("No active session")?;
        let session = session.read().await;

        if let Some(idx) = encounter_idx {
            let dir = session.encounters_dir().ok_or("No encounters directory")?;
            let path = dir.join(baras_core::storage::encounter_filename(idx));
            if !path.exists() {
                return Err(format!("Encounter file not found: {:?}", path));
            }
            self.shared.query_context.register_parquet(&path).await?;
        } else {
            let writer = session
                .encounter_writer()
                .ok_or("No live encounter buffer")?;
            let batch = writer.to_record_batch().ok_or("Live buffer is empty")?;
            self.shared.query_context.register_batch(batch).await?;
        }

        let result = self.shared
            .query_context
            .query()
            .await
            .query()
            .query_combat_log_find(
                &find_text,
                source_filter.as_deref(),
                target_filter.as_deref(),
                time_range.as_ref(),
                event_filters.as_ref(),
            )
            .await;

        if let Err(ref e) = result {
            tracing::error!("query_combat_log_find failed: {}", e);
        }
        result
    }

    /// Get distinct source names for combat log filter dropdown, grouped by entity type.
    pub async fn query_source_names(
        &self,
        encounter_idx: Option<u32>,
    ) -> Result<baras_types::GroupedEntityNames, String> {
        let session_guard = self.shared.session.read().await;
        let session = session_guard.as_ref().ok_or("No active session")?;
        let session = session.read().await;

        if let Some(idx) = encounter_idx {
            let dir = session.encounters_dir().ok_or("No encounters directory")?;
            let path = dir.join(baras_core::storage::encounter_filename(idx));
            if !path.exists() {
                return Err(format!("Encounter file not found: {:?}", path));
            }
            self.shared.query_context.register_parquet(&path).await?;
        } else {
            let writer = session
                .encounter_writer()
                .ok_or("No live encounter buffer")?;
            let batch = writer.to_record_batch().ok_or("Live buffer is empty")?;
            self.shared.query_context.register_batch(batch).await?;
        }

        self.shared
            .query_context
            .query()
            .await
            .query()
            .query_source_names()
            .await
    }

    /// Get distinct target names for combat log filter dropdown, grouped by entity type.
    pub async fn query_target_names(
        &self,
        encounter_idx: Option<u32>,
    ) -> Result<baras_types::GroupedEntityNames, String> {
        let session_guard = self.shared.session.read().await;
        let session = session_guard.as_ref().ok_or("No active session")?;
        let session = session.read().await;

        if let Some(idx) = encounter_idx {
            let dir = session.encounters_dir().ok_or("No encounters directory")?;
            let path = dir.join(baras_core::storage::encounter_filename(idx));
            if !path.exists() {
                return Err(format!("Encounter file not found: {:?}", path));
            }
            self.shared.query_context.register_parquet(&path).await?;
        } else {
            let writer = session
                .encounter_writer()
                .ok_or("No live encounter buffer")?;
            let batch = writer.to_record_batch().ok_or("Live buffer is empty")?;
            self.shared.query_context.register_batch(batch).await?;
        }

        self.shared
            .query_context
            .query()
            .await
            .query()
            .query_target_names()
            .await
    }

    /// Query player deaths in an encounter.
    pub async fn query_player_deaths(
        &self,
        encounter_idx: Option<u32>,
    ) -> Result<Vec<PlayerDeath>, String> {
        let session_guard = self.shared.session.read().await;
        let session = session_guard.as_ref().ok_or("No active session")?;
        let session = session.read().await;

        if let Some(idx) = encounter_idx {
            let dir = session.encounters_dir().ok_or("No encounters directory")?;
            let path = dir.join(baras_core::storage::encounter_filename(idx));
            if !path.exists() {
                return Err(format!("Encounter file not found: {:?}", path));
            }
            self.shared.query_context.register_parquet(&path).await?;
        } else {
            let writer = session
                .encounter_writer()
                .ok_or("No live encounter buffer")?;
            let batch = writer.to_record_batch().ok_or("Live buffer is empty")?;
            self.shared.query_context.register_batch(batch).await?;
        }

        self.shared
            .query_context
            .query()
            .await
            .query()
            .query_player_deaths()
            .await
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
            // Support both legacy "timers" and new "timers_a"/"timers_b" keys
            // All map to the same flag since the service loop checks one flag for both overlays
            "timers" | "timers_a" | "timers_b" => self
                .shared
                .timer_overlay_active
                .store(active, Ordering::SeqCst),
            "effects_a" => self
                .shared
                .effects_a_overlay_active
                .store(active, Ordering::SeqCst),
            "effects_b" => self
                .shared
                .effects_b_overlay_active
                .store(active, Ordering::SeqCst),
            "cooldowns" => self
                .shared
                .cooldowns_overlay_active
                .store(active, Ordering::SeqCst),
            "dot_tracker" => self
                .shared
                .dot_tracker_overlay_active
                .store(active, Ordering::SeqCst),
            _ => {}
        }
    }

    /// Set rearrange mode flag (for rendering loop to bypass gates)
    pub fn set_rearrange_mode(&self, enabled: bool) {
        self.shared.rearrange_mode.store(enabled, Ordering::SeqCst);
    }

    /// Emit overlay status changed event to notify frontend UI
    /// Called when visibility, move mode, or rearrange mode changes
    pub fn emit_overlay_status_changed(&self) {
        let _ = self.app_handle.emit("overlay-status-changed", ());
    }
}
