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
use baras_core::history::{now_iso, start_ms_of};
use baras_core::query::{
    AbilityBreakdown, BreakdownMode, CombatLogFilters, CombatLogFindMatch, CombatLogRow, CombatLogSortColumn,
    DamageTakenSummary, DataTab, EffectChartData, EffectUptimeCell, EffectWindow, EncounterTimeline, EntityBreakdown,
    GroupMetric, GroupTimeSeries, HpPoint, NpcHealthRow, PlayerDeath, RaidDebuffReport, RaidOverviewRow, ReplayData,
    SortDirection, ThreatReport, TimeRange, TimeSeriesPoint,
};
use baras_types::history::{
    CleanupPlan, CleanupReport, ContentTag, HistoryStatus, LibraryRow, LogStatus, ParselyUpload, PullFilter,
    PullHistoryOverview, PullRow, RunDetail, RunList, RunMeta,
};
use tauri::{AppHandle, Emitter};

use super::{AreaVisitInfo, CombatData, LogFileInfo, ServiceCommand, SessionInfo, cleanup};
use crate::state::MutexExt;
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

    /// Check if directory watcher is active
    pub fn is_watching(&self) -> bool {
        self.shared.watching.load(Ordering::SeqCst)
    }

    pub async fn active_file(&self) -> Option<String> {
        self.shared
            .with_session(|session| session.active_file.as_ref().map(|p| p.to_string_lossy().to_string()))
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
        let log_dir = PathBuf::from(self.config().await.log_directory);
        let catalog = self.area_catalog();
        let index = self.shared.directory_index.read().await;
        let area_cache = self.shared.area_cache.read().await;
        let shared = self.shared.clone();
        tokio::task::block_in_place(|| {
            let pulls = cleanup::pull_index(&shared, &log_dir);
            let meta = shared.library_meta.locked();

            index
                .entries()
                .into_iter()
                .map(|e| {
                    let facts = pulls.facts_for(&e.filename);
                    let areas = baras_core::library::log_areas(area_cache.get(&e.path), facts, &catalog).map(|areas| {
                        areas
                            .iter()
                            .map(|a| AreaVisitInfo {
                                display: a.display_name(),
                                area_name: a.area_name.clone(),
                                difficulty: a.difficulty_name.clone(),
                                difficulty_tier: a.difficulty_tier(),
                            })
                            .collect()
                    });

                    LogFileInfo {
                        link: facts.map(|f| f.link()).unwrap_or_default(),
                        path: e.path.clone(),
                        display_name: e.display_name(),
                        character_name: e.character_name.clone(),
                        date: e.formatted_datetime(),
                        day_of_week: e.day_of_week(),
                        is_empty: e.is_empty,
                        file_size: e.file_size,
                        areas,
                        parsely: meta
                            .log(&e.filename)
                            .and_then(|m| m.parsely.as_ref().map(|p| p.link.clone())),
                    }
                })
                .collect()
        })
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

    /// Dry run of the saved cleanup rules. Nothing is touched.
    pub async fn preview_log_cleanup(&self) -> CleanupPlan {
        let live = cleanup::live_path(&self.shared).await;
        cleanup::plan(&self.shared, &self.app_handle, live.as_deref()).await
    }

    /// Archive and delete whatever the saved rules select right now.
    pub async fn run_log_cleanup(&self) -> CleanupReport {
        cleanup::run(&self.shared, &self.app_handle).await
    }

    pub async fn retained_content(&self) -> Vec<ContentTag> {
        self.shared.library_meta.locked().retain.clone()
    }

    pub async fn set_retained_content(&self, retain: Vec<ContentTag>) -> bool {
        let mut meta = self.shared.library_meta.locked();
        meta.retain = retain;
        if let Err(e) = meta.save() {
            tracing::warn!(error = %e, "Failed to save library.json");
            return false;
        }
        true
    }

    /// Every log in the directory with its history and metadata joined.
    pub async fn library(&self) -> Vec<LibraryRow> {
        let open = self.active_file_path().await;
        let tailing = self.shared.is_live_tailing.load(std::sync::atomic::Ordering::SeqCst);
        let log_dir = PathBuf::from(self.config().await.log_directory);
        let shared = self.shared.clone();
        let catalog = self.area_catalog();
        let directory = self.shared.directory_index.read().await;
        let areas = self.shared.area_cache.read().await;
        tokio::task::block_in_place(|| {
            let pulls = cleanup::pull_index(&shared, &log_dir);
            let meta = shared.library_meta.locked();
            let mut rows_out: Vec<LibraryRow> = directory
                .entries()
                .into_iter()
                .map(|e| {
                    let info = pulls.log_info(&e.filename);
                    let rows = info.map_or(&[][..], |(r, _)| r);
                    let is_open = open.as_deref() == Some(e.path.as_path());
                    let status = if is_open && tailing {
                        LogStatus::Live
                    } else {
                        match info {
                            Some((_, true)) => LogStatus::Stale,
                            Some((_, false)) => LogStatus::Indexed,
                            None => LogStatus::NoHistory,
                        }
                    };
                    let log_meta = meta.log(&e.filename);
                    let facts = pulls.facts_for(&e.filename);
                    let log_areas =
                        baras_core::library::log_areas(areas.get(&e.path), facts, &catalog).unwrap_or_default();
                    LibraryRow {
                        link: facts.map(|f| f.link()).unwrap_or_default(),
                        content: baras_core::library::content_tags(&log_areas, rows),
                        path: e.path.to_string_lossy().into_owned(),
                        filename: e.filename.clone(),
                        date: e.formatted_datetime(),
                        character: e.character_name.clone(),
                        server: rows.first().map(|r| r.server.clone()).unwrap_or_default(),
                        size: e.file_size,
                        original_size: None,
                        areas: log_areas.iter().map(|x| x.display_name()).collect(),
                        pulls: rows.len() as u32,
                        kills: rows.iter().filter(|r| r.success).count() as u32,
                        dummy_only: !rows.is_empty()
                            && rows
                                .iter()
                                .all(|r| !r.is_operation && r.operation == baras_core::history::DUMMY_GROUP),
                        status,
                        open: is_open,
                        favorite: log_meta.is_some_and(|m| m.favorite),
                        tags: log_meta.map(|m| m.tags.clone()).unwrap_or_default(),
                        run_tags: meta.run_tags_for_log(&e.filename),
                        note: log_meta.map(|m| m.note.clone()).unwrap_or_default(),
                        parsely: log_meta.and_then(|m| m.parsely.as_ref().map(|p| p.link.clone())),
                        parsely_fights: meta.pull_parsely_count(&e.filename),
                    }
                })
                .collect();
            // Archived logs are no longer in the directory; the areas moved with them.
            // A log both on disk and archived (interrupted archive, manual copy) lists once, as the on-disk file
            let on_disk: std::collections::HashSet<String> = rows_out.iter().map(|r| r.filename.clone()).collect();
            for a in baras_core::library::archived_logs()
                .into_iter()
                .filter(|a| !on_disk.contains(&a.filename))
            {
                let info = pulls.log_info(&a.filename);
                let rows = info.map_or(&[][..], |(r, _)| r);
                let status = if info.is_some_and(|(_, stale)| stale) {
                    LogStatus::ArchivedStale
                } else {
                    LogStatus::Archived
                };
                let log_meta = meta.log(&a.filename);
                let facts = pulls.facts_for(&a.filename);
                let log_areas = baras_core::library::log_areas(areas.get(&a.path), facts, &catalog).unwrap_or_default();
                rows_out.push(LibraryRow {
                    link: facts.map(|f| f.link()).unwrap_or_default(),
                    content: baras_core::library::content_tags(&log_areas, rows),
                    path: a.path.to_string_lossy().into_owned(),
                    date: baras_core::context::parse_log_filename(&a.filename)
                        .map(|(_, dt)| dt.format("%Y-%m-%d %-H:%M").to_string())
                        .unwrap_or_default(),
                    character: rows.first().map(|r| r.character.clone()),
                    server: rows.first().map(|r| r.server.clone()).unwrap_or_default(),
                    size: a.compressed_size,
                    original_size: a.original_size,
                    areas: log_areas.iter().map(|x| x.display_name()).collect(),
                    pulls: rows.len() as u32,
                    kills: rows.iter().filter(|r| r.success).count() as u32,
                    dummy_only: !rows.is_empty()
                        && rows
                            .iter()
                            .all(|r| !r.is_operation && r.operation == baras_core::history::DUMMY_GROUP),
                    status,
                    open: false,
                    favorite: log_meta.is_some_and(|m| m.favorite),
                    tags: log_meta.map(|m| m.tags.clone()).unwrap_or_default(),
                    run_tags: meta.run_tags_for_log(&a.filename),
                    note: log_meta.map(|m| m.note.clone()).unwrap_or_default(),
                    parsely: log_meta.and_then(|m| m.parsely.as_ref().map(|p| p.link.clone())),
                    parsely_fights: meta.pull_parsely_count(&a.filename),
                    filename: a.filename,
                });
            }
            // The open log first, then newest first; the filename carries the
            // timestamp in sortable form for raw and archived logs alike
            rows_out.sort_by(|a, b| b.open.cmp(&a.open).then_with(|| b.filename.cmp(&a.filename)));
            rows_out
        })
    }

    pub async fn set_log_favorite(&self, filename: String, favorite: bool) -> bool {
        let mut meta = self.shared.library_meta.locked();
        meta.set_favorite(&filename, favorite);
        if let Err(e) = meta.save() {
            tracing::warn!(error = %e, "Failed to save library.json");
            return false;
        }
        true
    }

    /// The user's tags and note on one log.
    pub fn set_log_meta(&self, filename: &str, tags: Vec<String>, note: &str) -> bool {
        let mut meta = self.shared.library_meta.locked();
        meta.set_log_meta(filename, tags, note);
        match meta.save() {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to save library.json");
                false
            }
        }
    }

    /// Hide or show one pull in Boss Statistics
    pub fn set_pull_hidden(&self, filename: &str, start_ms: i64, hidden: bool) -> bool {
        let mut meta = self.shared.library_meta.locked();
        meta.set_pull_hidden(filename, start_ms, hidden);
        match meta.save() {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to save library.json");
                false
            }
        }
    }

    /// Remove a tag from every run of one log (the grey run tags in Log Management)
    pub fn remove_run_tag_for_log(&self, filename: &str, tag: &str) -> bool {
        let mut meta = self.shared.library_meta.locked();
        meta.remove_run_tag_for_log(filename, tag);
        match meta.save() {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to save library.json");
                false
            }
        }
    }

    /// Decompress an archived log back into the log directory.
    pub async fn restore_log(&self, filename: String) -> Result<PathBuf, String> {
        let log_dir = PathBuf::from(self.config().await.log_directory);
        let mut directory = self.shared.directory_index.write().await;
        let mut areas = self.shared.area_cache.write().await;
        let restored = {
            let mut meta = self.shared.library_meta.locked();
            let mut lib = baras_core::library::LibraryMut {
                directory: &mut directory,
                areas: &mut areas,
                meta: &mut meta,
                live: None,
            };
            baras_core::library::restore(&mut lib, &log_dir, &filename).map_err(|e| e.to_string())?
        };
        cleanup::save_log_caches(&self.app_handle, &directory, &areas);
        Ok(restored)
    }

    /// Delete one log (or archive) with everything it owns: history, notes,
    /// tags, Parsely links, caches. A favorite is unmarked first since the
    /// user asked for exactly this file; the live log is refused.
    /// Delete logs (raw or archived) with everything they own, favorites
    /// included: a deliberate delete outranks the favorite guard.
    pub async fn delete_logs(&self, paths: Vec<PathBuf>) -> baras_types::history::DeleteReport {
        let live = self.active_file_path().await;
        let report = cleanup::with_library(&self.shared, live.as_deref(), |lib| {
            for name in paths.iter().filter_map(|p| p.file_name().and_then(|f| f.to_str())) {
                lib.meta.set_favorite(name.strip_suffix(".zst").unwrap_or(name), false);
            }
            let report = baras_core::library::delete(lib, &paths, false);
            if let Err(e) = lib.meta.save() {
                tracing::warn!(error = %e, "Failed to save library.json");
            }
            report
        })
        .await;
        let directory = self.shared.directory_index.read().await;
        let areas = self.shared.area_cache.read().await;
        cleanup::save_log_caches(&self.app_handle, &directory, &areas);
        report
    }

    /// Compress logs into the archive, with progress on the cleanup event.
    pub async fn archive_logs(&self, paths: Vec<PathBuf>) -> baras_types::history::CleanupReport {
        cleanup::apply(&self.shared, &self.app_handle, &paths, &[]).await
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
    pub async fn update_config(&self, mut config: AppConfig) -> Result<(), String> {
        config.sync_active_profile();
        let old_config = self.shared.config.read().await.clone();
        let old_dir = old_config.log_directory.clone();
        let new_dir = config.log_directory.clone();

        // Check if grid size changed
        let old_slots =
            old_config.overlay_settings.raid_overlay.grid_columns * old_config.overlay_settings.raid_overlay.grid_rows;
        let new_slots =
            config.overlay_settings.raid_overlay.grid_columns * config.overlay_settings.raid_overlay.grid_rows;

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
            self.shared.raid_registry.locked().set_max_slots(new_slots);
        }

        // Update effect tracker alacrity/latency if changed
        if (alacrity_changed || latency_changed)
            && let Some(session) = self.shared.session.read().await.as_ref()
        {
            let session = session.read().await;
            if let Some(tracker) = session.effect_tracker() {
                let mut tracker = tracker.locked();
                if alacrity_changed {
                    tracker.set_alacrity(new_alacrity);
                }
                if latency_changed {
                    tracker.set_latency(new_latency);
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
        let session_start_short = start_datetime.map(|dt| dt.format("%l:%M %p").to_string());

        // Calculate end time (historical only) and duration (always)
        let is_live = self.shared.is_live_tailing.load(Ordering::SeqCst);

        // For historical sessions, find the end time
        let end_naive = if !is_live {
            // Try to get end time from last encounter (ISO 8601 format)
            let last_end_time = cache
                .encounter_history
                .summaries()
                .iter()
                .rev()
                .find_map(|s| s.end_time.as_ref())
                .and_then(|iso| chrono::NaiveDateTime::parse_from_str(iso, "%+").ok());

            // Fallback to file modification time if no encounters
            last_end_time.or_else(|| {
                session.active_file.as_ref().and_then(|path| {
                    path.metadata()
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .map(|st| chrono::DateTime::<chrono::Local>::from(st).naive_local())
                })
            })
        } else {
            None
        };

        let session_end = end_naive.map(|dt| dt.format("%b %d, %l:%M %p").to_string());

        // Always compute duration: for live sessions use last log event time, for historical use end time
        let duration_formatted = start_datetime.and_then(|start| {
            let end = if is_live {
                // Use the last log line timestamp so duration freezes when session goes stale
                session
                    .last_event_time
                    .unwrap_or_else(|| chrono::Local::now().naive_local())
            } else {
                end_naive?
            };
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
        });

        // Check if this is a stale session. Only relevant for live-tailing mode —
        // historical sessions are inherently "not live".
        // A live session is stale if:
        //   1. The game process is not running, OR
        //   2. The newest log file in the directory is blank (SWTOR rotated to
        //      a new empty file, meaning the player logged out / restarted)
        let stale_session = if is_live {
            if !self.shared.game_running.load(Ordering::SeqCst) {
                true
            } else {
                let index = self.shared.directory_index.read().await;
                index.newest_file().map(|f| f.is_empty).unwrap_or(false)
            }
        } else {
            false
        };

        // Look up discipline icons from the player's discipline name
        let discipline_enum = if cache.player_initialized {
            Discipline::from_name(&cache.player.discipline_name)
        } else {
            None
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
            class_icon: discipline_enum.map(|d| d.icon_name().to_string()),
            role_icon: discipline_enum.map(|d| d.role().icon_name().to_string()),
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
            session_start_short,
            session_end,
            duration_formatted,
            stale_session,
            character_mismatch: cache.character_mismatch,
            missing_area: cache.missing_area,
            area_resume_pending: cache.missing_area && session.area_resume_pending(),
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

    /// Set the Parsely link for a specific encounter: on the live summary (so
    /// the Data Explorer shows it now) and in `library.json` keyed by the
    /// pull's stable start time (so a re-parse cannot lose it).
    pub async fn set_encounter_parsely_link(&self, encounter_id: u64, link: String, visibility: u8) -> bool {
        let session_guard = self.shared.session.read().await;
        let Some(session) = session_guard.as_ref() else {
            return false;
        };
        let mut session = session.write().await;
        let filename = session
            .active_file
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|f| f.to_str())
            .map(str::to_string);
        let Some(cache) = session.session_cache.as_mut() else {
            return false;
        };

        let start_ms = cache
            .encounter_history
            .summaries()
            .iter()
            .find(|s| s.encounter_id == encounter_id)
            .and_then(|s| s.start_time.as_deref())
            .and_then(start_ms_of);
        let updated = cache.encounter_history.set_parsely_link(encounter_id, link.clone());
        if updated && let (Some(filename), Some(start_ms)) = (filename, start_ms) {
            let upload = ParselyUpload {
                link,
                uploaded_at: now_iso(),
                source_len: None,
                visibility,
            };
            let mut meta = self.shared.library_meta.locked();
            meta.set_pull_parsely(&filename, start_ms, upload);
            if let Err(e) = meta.save() {
                tracing::warn!(error = %e, "Failed to save library.json");
            }
        }
        updated
    }

    /// Filter options and per-boss pull counts from the history index.
    pub async fn pull_history_overview(&self, filter: PullFilter) -> PullHistoryOverview {
        let catalog = self.area_catalog();
        let shared = self.shared.clone();
        self.with_pull_index(move |index| {
            let meta = shared.library_meta.locked();
            index.overview(&filter, &catalog, &meta)
        })
        .await
        .unwrap_or_default()
    }

    /// Every recorded pull of one boss under `filter`, newest first.
    pub async fn boss_pulls(&self, operation: String, boss: String, filter: PullFilter) -> Vec<PullRow> {
        let catalog = self.area_catalog();
        let shared = self.shared.clone();
        self.with_pull_index(move |index| {
            let meta = shared.library_meta.locked();
            let mut rows = index.pulls(&operation, &boss, &filter, &catalog, &meta);
            for row in &mut rows {
                row.parsely = meta.parsely_link_for(&row.filename, row.start_ms).map(str::to_string);
            }
            rows
        })
        .await
        .unwrap_or_default()
    }

    /// Operation runs under `filter`, newest first, with tags and notes.
    pub async fn operation_runs(&self, filter: PullFilter) -> RunList {
        let catalog = self.area_catalog();
        let shared = self.shared.clone();
        self.with_pull_index(move |index| {
            let meta = shared.library_meta.locked();
            let runs = index.runs(&catalog, &filter, &meta);
            RunList {
                runs,
                tags: meta.run_tags(),
            }
        })
        .await
        .unwrap_or_default()
    }

    /// Every recorded warzone match and arena round, newest first.
    pub async fn pvp_matches(&self) -> Vec<baras_types::history::PvpMatchRow> {
        self.with_pull_index(|index| index.pvp_matches())
            .await
            .unwrap_or_default()
    }

    /// Every recorded training dummy kill, newest first, rated on Parsely.
    pub async fn dummy_parses(&self) -> Vec<baras_types::history::DummyRow> {
        let mut rows = self
            .with_pull_index(|index| index.dummy_parses())
            .await
            .unwrap_or_default();
        {
            let meta = self.shared.library_meta.locked();
            for r in &mut rows {
                r.hidden = meta.is_pull_hidden(&r.filename, r.start_ms);
            }
        }
        self.rate_dummy_rows(&mut rows).await;
        rows
    }

    /// The run each pull of the open log belongs to.
    pub async fn session_run_links(&self) -> Vec<baras_types::history::RunLink> {
        let Some(filename) = self
            .shared
            .with_session(|s| {
                s.active_file
                    .as_ref()?
                    .file_name()
                    .map(|f| f.to_string_lossy().into_owned())
            })
            .await
            .flatten()
        else {
            return Vec::new();
        };
        let catalog = self.area_catalog();
        let shared = self.shared.clone();
        self.with_pull_index(move |index| index.run_links(&filename, &catalog, &shared.library_meta.locked()))
            .await
            .unwrap_or_default()
    }

    /// One run with its bosses, players and pulls.
    pub async fn operation_run(&self, filename: String, start_ms: i64) -> Option<RunDetail> {
        let catalog = self.area_catalog();
        let shared = self.shared.clone();
        self.with_pull_index(move |index| {
            let meta = shared.library_meta.locked();
            let (run, rows) = index.run(&filename, start_ms, &catalog, &meta)?;
            // A run can span logs (disconnect, character swap)
            let mut files: Vec<&str> = rows.iter().map(|r| r.filename.as_str()).collect();
            files.dedup();
            let records: Vec<_> = files
                .into_iter()
                .filter_map(|f| baras_core::history::history_file_for(f).ok())
                .filter_map(|p| baras_core::history::LogHistory::load(&p).ok())
                .flat_map(|l| l.history.pulls)
                .collect();
            Some(baras_core::history::run_detail(run, rows, &records, &catalog))
        })
        .await
        .flatten()
    }

    /// Tags and note for one run; an empty meta removes the entry.
    pub fn set_run_meta(&self, filename: &str, start_ms: i64, meta: RunMeta) -> bool {
        let mut store = self.shared.library_meta.locked();
        store.set_run(filename, start_ms, meta);
        match store.save() {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to save library.json");
                false
            }
        }
    }

    pub(super) fn area_catalog(&self) -> Arc<baras_core::history::AreaCatalog> {
        cleanup::area_catalog(&self.shared, &self.app_handle)
    }

    /// Backfill pull history for every log in the configured directory
    /// (background). `stale_only` skips logs whose history is already current.
    pub async fn start_history_backfill(&self, stale_only: bool) -> Result<(), String> {
        super::history::run_backfill(self.shared.clone(), self.app_handle.clone(), stale_only).await
    }

    /// Ask a running backfill to stop; it reports `Done { cancelled: true }`.
    pub fn cancel_history_backfill(&self) {
        if self
            .shared
            .history_backfill_running
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            self.shared
                .history_backfill_cancel
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }

    /// History store health: logs, pulls, and files needing a re-scan. The
    /// live log is always behind its history file and is not counted.
    pub async fn history_status(&self) -> HistoryStatus {
        let live = if self.shared.is_live_tailing.load(std::sync::atomic::Ordering::SeqCst) {
            self.active_file_path().await
        } else {
            None
        };
        let mut logs: Vec<String> = {
            let directory = self.shared.directory_index.read().await;
            directory
                .entries()
                .iter()
                .filter(|e| !e.is_empty && live.as_deref() != Some(e.path.as_path()))
                .map(|e| e.filename.clone())
                .collect()
        };
        logs.extend(baras_core::library::archived_logs().into_iter().map(|a| a.filename));
        self.with_pull_index(move |index| {
            let mut status = index.status(live.as_deref());
            // "Need a scan" counts only what a scan covers (log directory + archive):
            // history of a log opened from elsewhere stays out, even when stale
            status.stale = logs
                .iter()
                .filter(|f| index.log_info(f).is_some_and(|(_, stale)| stale))
                .count() as u32;
            status.unscanned = logs.iter().filter(|f| index.log_info(f).is_none()).count() as u32;
            status
        })
        .await
        .unwrap_or_default()
    }

    async fn active_file_path(&self) -> Option<PathBuf> {
        let guard = self.shared.session.read().await;
        let session = guard.as_ref()?;
        session.read().await.active_file.clone()
    }

    /// Record a whole-file Parsely upload against the log in `library.json`.
    pub async fn set_log_parsely_link(&self, path: PathBuf, link: String, visibility: u8) -> bool {
        let Some(filename) = path.file_name().and_then(|f| f.to_str()) else {
            return false;
        };
        let source_len = std::fs::metadata(&path).ok().map(|m| m.len());
        let upload = ParselyUpload {
            link,
            uploaded_at: now_iso(),
            source_len,
            visibility,
        };
        let mut meta = self.shared.library_meta.locked();
        meta.set_log_parsely(filename, upload);
        if let Err(e) = meta.save() {
            tracing::warn!(error = %e, "Failed to save library.json");
        }
        true
    }

    /// Refresh the index against disk (mtime-based, cheap when unchanged) and query it
    /// off the async runtime.
    pub(super) async fn with_pull_index<T: Send + 'static>(
        &self,
        query: impl FnOnce(&baras_core::history::PullIndex) -> T + Send + 'static,
    ) -> Option<T> {
        let log_dir = PathBuf::from(self.config().await.log_directory);
        let shared = self.shared.clone();
        tokio::task::spawn_blocking(move || {
            let index = cleanup::pull_index(&shared, &log_dir);
            query(&index)
        })
        .await
        .ok()
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Raid Registry Operations
    // ─────────────────────────────────────────────────────────────────────────

    /// Swap two slots in the raid registry
    pub async fn swap_raid_slots(&self, slot_a: u8, slot_b: u8) {
        self.shared.raid_registry.locked().swap_slots(slot_a, slot_b);
        self.refresh_raid_frames().await;
    }

    /// Remove a slot from the raid registry
    pub async fn remove_raid_slot(&self, slot: u8) {
        self.shared.raid_registry.locked().remove_slot(slot);
        self.refresh_raid_frames().await;
    }

    /// Current-area roster for OCR. Names identify; health only supports.
    pub async fn raid_detection_candidates(&self) -> Vec<baras_core::raid_detect::PlayerCandidate> {
        super::raid_detection::raid_detection_candidates(&self.shared).await
    }

    /// Write detection results into the raid registry.
    pub async fn apply_raid_detection(
        &self,
        assignments: Vec<baras_core::raid_detect::RowAssignment>,
        ambiguous_rows: &[u8],
    ) -> usize {
        let remaining = {
            let mut registry = self.shared.raid_registry.locked();
            // The screen is the authority: players the pass did not
            // re-confirm are evicted before the confirmed ones are placed.
            // An ambiguous row is the exception — it read something that
            // fits its occupant among others, and absence of a verdict is
            // not evidence of absence, so the occupant is the tiebreak.
            let mut confirmed: Vec<i64> = assignments.iter().map(|a| a.entity_id).collect();
            confirmed.extend(
                ambiguous_rows
                    .iter()
                    .filter_map(|&row| registry.get_player(row).map(|p| p.entity_id)),
            );
            registry.retain_players(&confirmed);
            registry.assign_slots(
                assignments
                    .into_iter()
                    .filter_map(|a| u8::try_from(a.row).ok().map(|row| (row, a.entity_id, a.name))),
            );
            registry.provisional_len()
        };
        self.refresh_raid_frames().await;
        remaining
    }

    pub async fn apply_provisional_raid_detection(&self, assignments: Vec<(u8, String)>) -> (usize, usize) {
        let counts = {
            let mut registry = self.shared.raid_registry.locked();
            registry.assign_provisional_slots(assignments);
            (registry.provisional_len(), registry.registered_len())
        };
        // New provisionals are the other half of the pairing: match them against
        // players already on the roster.
        if counts.0 > 0 {
            self.shared
                .roster_changed
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
        self.refresh_raid_frames().await;
        counts
    }

    /// Mark the slots whose reading fitted more than one player.
    pub async fn set_ambiguous_slots(&self, slots: Vec<u8>) {
        self.shared.raid_registry.locked().set_ambiguous_slots(slots);
        self.refresh_raid_frames().await;
    }

    /// Clear all raid registry slots
    pub async fn clear_raid_registry(&self) {
        self.shared.raid_registry.locked().clear();
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
    /// Queue a historical open. Returns `Ok(false)` when the same file is
    /// already queued or being parsed, so the click is a no-op.
    pub async fn open_historical_file(&self, path: PathBuf, select_encounter: Option<u64>) -> Result<bool, String> {
        if !self.shared.pending_opens.locked().insert(path.clone()) {
            return Ok(false);
        }
        self.cmd_tx
            .send(ServiceCommand::OpenHistoricalFile { path, select_encounter })
            .await
            .map(|()| true)
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
        entity_id: Option<i64>,
        time_range: Option<TimeRange>,
        entity_types: Option<Vec<String>>,
        breakdown_mode: Option<BreakdownMode>,
        duration_secs: Option<f32>,
    ) -> Result<Vec<AbilityBreakdown>, String> {
        self.register_encounter(encounter_idx).await?;

        let types_ref: Option<Vec<&str>> = entity_types.as_ref().map(|v| v.iter().map(|s| s.as_str()).collect());
        self.shared
            .query_context
            .query()
            .await
            .query()
            .query_breakdown(
                tab,
                entity_name.as_deref(),
                entity_id,
                time_range.as_ref(),
                types_ref.as_deref(),
                breakdown_mode.as_ref(),
                duration_secs,
            )
            .await
    }

    /// Query damage taken summary (damage type breakdown + mitigation stats).
    pub async fn query_damage_taken_summary(
        &self,
        encounter_idx: Option<u32>,
        entity_name: String,
        entity_id: Option<i64>,
        time_range: Option<TimeRange>,
        entity_types: Option<Vec<String>>,
    ) -> Result<DamageTakenSummary, String> {
        self.register_encounter(encounter_idx).await?;

        let types_ref: Option<Vec<&str>> = entity_types.as_ref().map(|v| v.iter().map(|s| s.as_str()).collect());
        self.shared
            .query_context
            .query()
            .await
            .query()
            .query_damage_taken_summary(&entity_name, entity_id, time_range.as_ref(), types_ref.as_deref())
            .await
    }

    /// Query breakdown by entity for a specific encounter and data tab.
    pub async fn query_entity_breakdown(
        &self,
        tab: DataTab,
        encounter_idx: Option<u32>,
        time_range: Option<TimeRange>,
    ) -> Result<Vec<EntityBreakdown>, String> {
        self.register_encounter(encounter_idx).await?;

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
        // name → (class, discipline name, class icon, role icon, discipline GUID)
        let mut player_discipline_map: std::collections::HashMap<
            String,
            (String, String, String, String, Option<i64>),
        > = std::collections::HashMap::new();

        // PvP encounters get friend/enemy faction classification
        let mut is_pvp = false;

        if let Some(idx) = encounter_idx {
            // Historical query: get discipline info from that encounter's summary
            // This captures the discipline each player had AT THAT TIME
            if let Some(cache) = session.session_cache.as_ref()
                && let Some(summary) = cache
                    .encounter_history
                    .summaries()
                    .iter()
                    .find(|s| s.encounter_id == idx as u64)
            {
                is_pvp = summary.encounter_type == baras_core::encounter::PhaseType::PvP;
                for pm in &summary.player_metrics {
                    if let (Some(class_icon), Some(role_icon)) = (&pm.class_icon, &pm.role_icon) {
                        player_discipline_map.insert(
                            pm.name.clone(),
                            (
                                pm.class_name.clone().unwrap_or_default(),
                                pm.discipline_name.clone().unwrap_or_default(),
                                class_icon.clone(),
                                role_icon.clone(),
                                pm.discipline.map(Discipline::guid),
                            ),
                        );
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
                is_pvp = cache
                    .last_combat_encounter()
                    .and_then(|e| e.area_id)
                    .is_some_and(baras_core::game_data::is_pvp_area);
                for p in cache.player_disciplines.values() {
                    if let Some(disc) = Discipline::from_guid(p.discipline_id) {
                        let name = resolve(p.name).to_string();
                        let class_icon = disc.icon_name().to_string();
                        let role_icon = disc.role().icon_name().to_string();
                        let discipline_name = disc.name().to_string();
                        let class_name = format!("{:?}", disc.class());
                        let guid = Some(p.discipline_id);
                        player_discipline_map.insert(name, (class_name, discipline_name, class_icon, role_icon, guid));
                    }
                }
            }

            let writer = session.encounter_writer().ok_or("No live encounter buffer")?;
            self.shared.query_context.register_live(writer).await?;
        }

        let mut results = self
            .shared
            .query_context
            .query()
            .await
            .query()
            .query_raid_overview(time_range.as_ref(), duration_secs, is_pvp)
            .await?;

        // Enrich results with discipline info
        for row in &mut results {
            if let Some((class_name, discipline_name, class_icon, role_icon, guid)) =
                player_discipline_map.get(&row.name)
            {
                row.class_name = Some(class_name.clone());
                row.discipline_name = Some(discipline_name.clone());
                row.discipline_id = *guid;
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
        self.register_encounter(encounter_idx).await?;

        self.shared
            .query_context
            .query()
            .await
            .query()
            .dps_over_time(bucket_ms, source_name.as_deref(), time_range.as_ref())
            .await
    }

    /// Every player's metric per bucket, for the Overview's group chart.
    pub async fn query_group_over_time(
        &self,
        encounter_idx: Option<u32>,
        bucket_ms: i64,
        metric: GroupMetric,
        time_range: Option<TimeRange>,
    ) -> Result<GroupTimeSeries, String> {
        self.register_encounter(encounter_idx).await?;
        self.shared
            .query_context
            .query()
            .await
            .query()
            .group_over_time(bucket_ms, metric, time_range.as_ref())
            .await
    }

    /// Every effect `source` applied, uptime per target (Effect Uptime tab).
    pub async fn query_effect_uptime_by_target(
        &self,
        encounter_idx: Option<u32>,
        source: String,
        time_range: Option<TimeRange>,
        duration_secs: f32,
    ) -> Result<Vec<EffectUptimeCell>, String> {
        self.register_encounter(encounter_idx).await?;
        self.shared
            .query_context
            .query()
            .await
            .query()
            .effect_uptime_by_target(&source, time_range.as_ref(), duration_secs)
            .await
    }

    /// Uptime of each standard raid debuff on every enemy (Raid Debuffs tab).
    pub async fn query_raid_debuffs(
        &self,
        encounter_idx: Option<u32>,
        time_range: Option<TimeRange>,
    ) -> Result<RaidDebuffReport, String> {
        self.register_encounter(encounter_idx).await?;
        self.shared
            .query_context
            .query()
            .await
            .query()
            .raid_debuffs(time_range.as_ref())
            .await
    }

    /// Movement, casts and effects of the whole fight (Replay tab), with each
    /// player's icons from the encounter's summary.
    pub async fn query_replay(&self, encounter_idx: Option<u32>) -> Result<ReplayData, String> {
        self.register_encounter(encounter_idx).await?;
        let mut data = self.shared.query_context.query().await.query().replay().await?;

        let session_guard = self.shared.session.read().await;
        let session = session_guard.as_ref().ok_or("No active session")?;
        let session = session.read().await;
        if let Some(idx) = encounter_idx
            && let Some(cache) = session.session_cache.as_ref()
            && let Some(summary) = cache
                .encounter_history
                .summaries()
                .iter()
                .find(|s| s.encounter_id == idx as u64)
        {
            for entity in &mut data.entities {
                if let Some(pm) = summary.player_metrics.iter().find(|pm| pm.name == entity.name) {
                    entity.discipline_id = pm.discipline.map(Discipline::guid);
                    entity.class_icon = pm.class_icon.clone();
                    entity.role_icon = pm.role_icon.clone();
                    entity.faction = pm.pvp_faction;
                }
            }
        }
        Ok(data)
    }

    /// Enemy and player targets over the whole fight, taunts and threat drops (Aggro tab).
    pub async fn query_threat_report(&self, encounter_idx: Option<u32>) -> Result<ThreatReport, String> {
        self.register_encounter(encounter_idx).await?;
        self.shared.query_context.query().await.query().threat_report().await
    }

    /// Point the query context at an encounter's parquet file, or at the live
    /// buffer when `encounter_idx` is None.
    pub(super) async fn register_encounter(&self, encounter_idx: Option<u32>) -> Result<(), String> {
        let session_guard = self.shared.session.read().await;
        let session = session_guard.as_ref().ok_or("No active session")?;
        let session = session.read().await;
        if let Some(idx) = encounter_idx {
            let dir = session.encounters_dir().ok_or("No encounters directory")?;
            let path = dir.join(baras_core::storage::encounter_filename(idx));
            if !path.exists() {
                return Err(format!("Encounter file not found: {:?}", path));
            }
            self.shared.query_context.register_parquet(&path).await
        } else {
            let writer = session.encounter_writer().ok_or("No live encounter buffer")?;
            self.shared.query_context.register_live(writer).await
        }
    }

    /// Query encounter timeline with phase segments.
    pub async fn query_encounter_timeline(&self, encounter_idx: Option<u32>) -> Result<EncounterTimeline, String> {
        self.register_encounter(encounter_idx).await?;

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
        self.register_encounter(encounter_idx).await?;

        self.shared
            .query_context
            .query()
            .await
            .query()
            .hps_over_time(bucket_ms, source_name.as_deref(), time_range.as_ref())
            .await
    }

    /// Query EHPS (effective healing) over time for a specific encounter.
    pub async fn query_ehps_over_time(
        &self,
        encounter_idx: Option<u32>,
        bucket_ms: i64,
        source_name: Option<String>,
        time_range: Option<TimeRange>,
    ) -> Result<Vec<TimeSeriesPoint>, String> {
        self.register_encounter(encounter_idx).await?;

        self.shared
            .query_context
            .query()
            .await
            .query()
            .ehps_over_time(bucket_ms, source_name.as_deref(), time_range.as_ref())
            .await
    }

    /// Query EHT (effective healing taken) over time for a specific encounter.
    pub async fn query_eht_over_time(
        &self,
        encounter_idx: Option<u32>,
        bucket_ms: i64,
        target_name: Option<String>,
        time_range: Option<TimeRange>,
    ) -> Result<Vec<TimeSeriesPoint>, String> {
        self.register_encounter(encounter_idx).await?;

        self.shared
            .query_context
            .query()
            .await
            .query()
            .eht_over_time(bucket_ms, target_name.as_deref(), time_range.as_ref())
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
        self.register_encounter(encounter_idx).await?;

        self.shared
            .query_context
            .query()
            .await
            .query()
            .dtps_over_time(bucket_ms, target_name.as_deref(), time_range.as_ref())
            .await
    }

    /// Query HP% over time for a specific encounter.
    pub async fn query_hp_over_time(
        &self,
        encounter_idx: Option<u32>,
        bucket_ms: i64,
        target_name: Option<String>,
        time_range: Option<TimeRange>,
    ) -> Result<Vec<HpPoint>, String> {
        self.register_encounter(encounter_idx).await?;

        self.shared
            .query_context
            .query()
            .await
            .query()
            .hp_over_time(bucket_ms, target_name.as_deref(), time_range.as_ref())
            .await
    }

    /// Query effect uptime statistics for the charts panel.
    pub async fn query_effect_uptime(
        &self,
        encounter_idx: Option<u32>,
        target_name: Option<String>,
        time_range: Option<TimeRange>,
        duration_secs: f32,
        source_filter: Option<String>,
    ) -> Result<Vec<EffectChartData>, String> {
        self.register_encounter(encounter_idx).await?;

        self.shared
            .query_context
            .query()
            .await
            .query()
            .query_effect_uptime(
                target_name.as_deref(),
                time_range.as_ref(),
                duration_secs,
                source_filter.as_deref(),
            )
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
        source_filter: Option<String>,
    ) -> Result<Vec<EffectWindow>, String> {
        self.register_encounter(encounter_idx).await?;

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
                source_filter.as_deref(),
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
        sort_column: CombatLogSortColumn,
        sort_direction: SortDirection,
    ) -> Result<Vec<CombatLogRow>, String> {
        self.register_encounter(encounter_idx).await?;

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
                sort_column,
                sort_direction,
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
        self.register_encounter(encounter_idx).await?;

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
        search_filter: Option<String>,
        time_range: Option<TimeRange>,
        event_filters: Option<CombatLogFilters>,
        sort_column: CombatLogSortColumn,
        sort_direction: SortDirection,
    ) -> Result<Vec<CombatLogFindMatch>, String> {
        self.register_encounter(encounter_idx).await?;

        let result = self
            .shared
            .query_context
            .query()
            .await
            .query()
            .query_combat_log_find(
                &find_text,
                source_filter.as_deref(),
                target_filter.as_deref(),
                search_filter.as_deref(),
                time_range.as_ref(),
                event_filters.as_ref(),
                sort_column,
                sort_direction,
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
        self.register_encounter(encounter_idx).await?;

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
        self.register_encounter(encounter_idx).await?;

        self.shared
            .query_context
            .query()
            .await
            .query()
            .query_target_names()
            .await
    }

    /// Query player deaths in an encounter.
    pub async fn query_player_deaths(&self, encounter_idx: Option<u32>) -> Result<Vec<PlayerDeath>, String> {
        let session_guard = self.shared.session.read().await;
        let session = session_guard.as_ref().ok_or("No active session")?;
        let session = session.read().await;

        // PvP encounters get friend/enemy death classification
        let mut is_pvp = false;

        if let Some(idx) = encounter_idx {
            if let Some(cache) = session.session_cache.as_ref() {
                is_pvp = cache
                    .encounter_history
                    .summaries()
                    .iter()
                    .find(|s| s.encounter_id == idx as u64)
                    .is_some_and(|s| s.encounter_type == baras_core::encounter::PhaseType::PvP);
            }
            let dir = session.encounters_dir().ok_or("No encounters directory")?;
            let path = dir.join(baras_core::storage::encounter_filename(idx));
            if !path.exists() {
                return Err(format!("Encounter file not found: {:?}", path));
            }
            self.shared.query_context.register_parquet(&path).await?;
        } else {
            if let Some(cache) = session.session_cache.as_ref() {
                is_pvp = cache
                    .last_combat_encounter()
                    .and_then(|e| e.area_id)
                    .is_some_and(baras_core::game_data::is_pvp_area);
            }
            let writer = session.encounter_writer().ok_or("No live encounter buffer")?;
            self.shared.query_context.register_live(writer).await?;
        }

        self.shared
            .query_context
            .query()
            .await
            .query()
            .query_player_deaths(is_pvp)
            .await
    }

    /// Query a death recap for one player death in an encounter.
    pub async fn query_death_recap(
        &self,
        encounter_idx: Option<u32>,
        player_name: String,
        death_time_secs: f32,
    ) -> Result<baras_types::DeathRecap, String> {
        self.register_encounter(encounter_idx).await?;

        self.shared
            .query_context
            .query()
            .await
            .query()
            .query_death_recap(&player_name, death_time_secs)
            .await
    }

    /// Query final health state of all NPCs in an encounter.
    pub async fn query_npc_health(
        &self,
        encounter_idx: Option<u32>,
        time_range: Option<TimeRange>,
    ) -> Result<Vec<NpcHealthRow>, String> {
        self.register_encounter(encounter_idx).await?;

        self.shared
            .query_context
            .query()
            .await
            .query()
            .query_npc_health(time_range.as_ref())
            .await
    }

    /// Query rotation analysis for a player in an encounter.
    pub async fn query_rotation(
        &self,
        encounter_idx: Option<u32>,
        source_name: String,
        anchor_ability_id: i64,
        time_range: Option<TimeRange>,
    ) -> Result<baras_core::query::RotationAnalysis, String> {
        self.register_encounter(encounter_idx).await?;

        self.shared
            .query_context
            .query()
            .await
            .query()
            .query_rotation(&source_name, anchor_ability_id, time_range.as_ref())
            .await
    }

    /// Query ability usage statistics for a single player.
    pub async fn query_ability_usage(
        &self,
        source_name: String,
        encounter_idx: Option<u32>,
        time_range: Option<TimeRange>,
    ) -> Result<Vec<baras_core::query::AbilityUsageRow>, String> {
        self.register_encounter(encounter_idx).await?;

        self.shared
            .query_context
            .query()
            .await
            .query()
            .query_ability_usage(&source_name, time_range.as_ref())
            .await
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Overlay Status Flags (for skipping work in effects loop)
    // ─────────────────────────────────────────────────────────────────────────

    /// Update the overlay status flag for a specific overlay type
    pub fn set_overlay_active(&self, kind: &str, active: bool) {
        match kind {
            "raid" => self.shared.raid_overlay_active.store(active, Ordering::SeqCst),
            "boss_health" => self.shared.boss_health_overlay_active.store(active, Ordering::SeqCst),
            // Support both legacy "timers" and new "timers_a"/"timers_b" keys
            // All map to the same flag since the service loop checks one flag for both overlays
            "timers" | "timers_a" | "timers_b" => self.shared.timer_overlay_active.store(active, Ordering::SeqCst),
            "effects_a" => self.shared.effects_a_overlay_active.store(active, Ordering::SeqCst),
            "effects_b" => self.shared.effects_b_overlay_active.store(active, Ordering::SeqCst),
            "effects_c" => self.shared.effects_c_overlay_active.store(active, Ordering::SeqCst),
            "cooldowns" => self.shared.cooldowns_overlay_active.store(active, Ordering::SeqCst),
            "cooldowns_b" => self.shared.cooldowns_b_overlay_active.store(active, Ordering::SeqCst),
            "dot_tracker" => self.shared.dot_tracker_overlay_active.store(active, Ordering::SeqCst),
            "ability_cast" => self.shared.ability_cast_overlay_active.store(active, Ordering::SeqCst),
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

    /// Get current notes data from loaded boss definitions
    /// Returns the first boss with notes, or None if no notes are available
    pub async fn get_current_notes(&self) -> Option<baras_overlay::NotesData> {
        let session_guard = self.shared.session.read().await;
        let session = session_guard.as_ref()?;
        let session = session.read().await;
        let cache = session.session_cache.as_ref()?;

        // Get boss definitions from cache via public accessor
        let definitions = cache.boss_definitions();

        // Find first boss with notes
        for boss in definitions.iter() {
            if let Some(notes) = &boss.notes
                && !notes.is_empty()
            {
                return Some(baras_overlay::NotesData {
                    text: notes.clone(),
                    boss_name: boss.name.clone(),
                });
            }
        }

        None
    }

    /// Get list of bosses with notes status for the current area
    pub async fn get_area_bosses_for_notes(&self) -> Result<Vec<crate::commands::BossNotesInfo>, String> {
        let session_guard = self.shared.session.read().await;
        let session = session_guard.as_ref().ok_or("No active session")?;
        let session = session.read().await;
        let cache = session.session_cache.as_ref().ok_or("No session cache")?;

        let definitions = cache.boss_definitions();

        let bosses: Vec<crate::commands::BossNotesInfo> = definitions
            .iter()
            .map(|boss| crate::commands::BossNotesInfo {
                id: boss.id.clone(),
                name: boss.name.clone(),
                has_notes: boss.notes.as_ref().is_some_and(|n| !n.is_empty()),
            })
            .collect();

        Ok(bosses)
    }

    /// Send notes for a specific boss to the overlay
    pub async fn select_boss_notes(&self, boss_id: &str) -> Result<(), String> {
        let session_guard = self.shared.session.read().await;
        let session = session_guard.as_ref().ok_or("No active session")?;
        let session = session.read().await;
        let cache = session.session_cache.as_ref().ok_or("No session cache")?;

        let definitions = cache.boss_definitions();

        // Find the boss by ID
        let boss = definitions
            .iter()
            .find(|b| b.id == boss_id)
            .ok_or_else(|| format!("Boss '{}' not found", boss_id))?;

        // Send notes to overlay (even if empty, to allow clearing)
        let notes_data = baras_overlay::NotesData {
            text: boss.notes.clone().unwrap_or_default(),
            boss_name: boss.name.clone(),
        };

        // Send via the overlay channel
        let _ = self
            .cmd_tx
            .send(super::ServiceCommand::SendNotesToOverlay(notes_data))
            .await;

        Ok(())
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Operation Timer
    // ─────────────────────────────────────────────────────────────────────────

    /// Start the operation timer manually
    pub async fn start_operation_timer(&self) -> Result<(), String> {
        self.cmd_tx
            .send(super::ServiceCommand::StartOperationTimer)
            .await
            .map_err(|e| e.to_string())
    }

    /// Stop the operation timer
    pub async fn stop_operation_timer(&self) -> Result<(), String> {
        self.cmd_tx
            .send(super::ServiceCommand::StopOperationTimer)
            .await
            .map_err(|e| e.to_string())
    }

    /// Reset the operation timer
    pub async fn reset_operation_timer(&self) -> Result<(), String> {
        self.cmd_tx
            .send(super::ServiceCommand::ResetOperationTimer)
            .await
            .map_err(|e| e.to_string())
    }

    /// Get the current operation timer state
    pub fn operation_timer_state(&self) -> super::OperationTimerState {
        // Return a snapshot copy - can't return the actual state due to mutex
        let timer = self.shared.operation_timer.locked();
        super::OperationTimerState {
            started_at: timer.started_at,
            accumulated_secs: timer.accumulated_secs,
            manually_started: timer.manually_started,
            manually_stopped: timer.manually_stopped,
            operation_name: timer.operation_name.clone(),
            op_instance: timer.op_instance,
            exited_at: timer.exited_at,
        }
    }

    /// Check if the operation timer is currently running
    pub fn is_operation_timer_running(&self) -> bool {
        self.shared
            .operation_timer
            .lock()
            .map(|t| t.is_running())
            .unwrap_or(false)
    }
}
