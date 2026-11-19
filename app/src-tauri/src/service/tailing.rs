//! Log file tailing and the overlay data loops

use super::boss_health::build_boss_health_data;
use super::combat_data::calculate_combat_data;
use super::effect_data::{build_cooldowns_data, build_dot_tracker_data, build_effects_ab_data, process_effect_audio};
use super::operation_timer::AreaKind;
use super::raid_frames::{build_raid_frame_data, promote_provisional_slots};
use super::signal_handler::CombatSignalHandler;
use super::timer_data::build_timer_data_with_audio;
use super::{CombatService, MetricsTrigger, OverlayUpdate, ServiceCommand, SessionEvent, history};
use crate::audio::AudioEvent;
use baras_core::context::ParsingSession;
use baras_core::encounter::PhaseType;
use baras_core::game_data::{Difficulty, Discipline};
use baras_core::{DisplayTarget, Reader};
use baras_overlay::{CooldownData, DotTrackerData, EffectsABData};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tauri::{Emitter, Manager};
use tokio::sync::{RwLock, mpsc};
use tracing::{debug, error, info, warn};

// ─────────────────────────────────────────────────────────────────────────────
// Parse Worker IPC
// ─────────────────────────────────────────────────────────────────────────────

use baras_core::state::ParseWorkerOutput;

/// Fallback to streaming parse if subprocess fails.
/// Returns true if the session is in combat after parsing.
/// In historical mode (`is_live` false) an encounter left open at EOF is
/// finalized instead, and this always returns false.
async fn fallback_streaming_parse(
    reader: &Reader,
    session: &Arc<RwLock<ParsingSession>>,
    encounters_dir: PathBuf,
    is_live: bool,
) -> bool {
    let timer = std::time::Instant::now();
    let mut session_guard = session.write().await;
    let session_date = session_guard.game_session_date.unwrap_or_default();
    let result = reader.read_log_file_streaming(session_date, |event| {
        session_guard.process_event(event);
    });

    if let Ok((end_pos, event_count)) = result {
        session_guard.current_byte = Some(end_pos);

        // Enable live parquet writing so Data Explorer can query encounters
        // Start from encounter 0 since fallback doesn't write parquet files
        session_guard.enable_live_parquet(encounters_dir, 0);

        session_guard.finalize_session();
        session_guard.sync_timer_context();

        if !is_live {
            // Historical open: close an encounter left open at EOF so it
            // doesn't tick as live combat (see start_tailing handoff note)
            session_guard.finalize_open_encounter();
        }

        // Check if we're mid-combat
        let in_combat = if let Some(cache) = &session_guard.session_cache {
            if let Some(enc) = cache.current_encounter() {
                use baras_core::encounter::EncounterState;
                enc.state == EncounterState::InCombat
            } else {
                false
            }
        } else {
            false
        };

        info!(
            event_count,
            elapsed_ms = timer.elapsed().as_millis() as u64,
            in_combat,
            "Fallback streaming parse completed"
        );

        in_combat
    } else {
        false
    }
}

impl CombatService {
    pub(super) async fn start_tailing(&mut self, path: PathBuf) {
        info!(path = %path.display(), "Starting to tail log file");
        self.stop_tailing().await;

        // Clear old parquet data from previous session
        if let Err(e) = baras_core::storage::clear_data_dir() {
            warn!(error = %e, "Failed to clear data directory");
        }

        // Clear all overlay data when switching files
        let _ = self.overlay_tx.try_send(OverlayUpdate::ClearAllData);

        // Clear raid registry when switching files (new session = fresh state)
        self.shared
            .raid_registry
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
        // Entity ids do not survive a session change, so neither can the
        // ability-cast roster (a user frame clear deliberately keeps it).
        self.shared
            .ability_roster
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
        *self
            .shared
            .pvp_ocr_roster_started_at
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = None;

        // Create trigger channel for signal-driven metrics updates (tokio channel - no spawn_blocking needed)
        let (trigger_tx, mut trigger_rx) = mpsc::channel::<MetricsTrigger>(8);
        // Create channel for frontend session events (replaces polling).
        // Unbounded tokio channel: `send` is sync (usable from the signal handler)
        // and `recv().await` never parks a runtime worker thread.
        let (session_event_tx, mut session_event_rx) = mpsc::unbounded_channel::<SessionEvent>();

        let mut session = ParsingSession::new(path.clone(), self.definitions.clone());

        // Historical opens never render effect overlays — skip effect
        // evaluation entirely (a resume-live re-runs start_tailing fresh)
        if !self.shared.is_live_tailing.load(Ordering::SeqCst) {
            session.disable_effect_tracking();
        }

        // Load timer preferences into the session's timer manager (Live mode only)
        if let Some(prefs_path) = Self::timer_preferences_path()
            && let Some(timer_mgr) = session.timer_manager()
            && let Ok(mut mgr) = timer_mgr.lock()
            && let Err(e) = mgr.load_preferences(&prefs_path)
        {
            warn!(error = %e, "Failed to load timer preferences");
        }

        // Set up sync definition loader for AreaEntered events (fixes race condition)
        let area_index = self.area_index.clone();
        let user_encounters_dir = baras_core::paths::config_dir().map(|p| p.join("definitions").join("encounters"));
        let loader: baras_core::context::DefinitionLoader = Box::new(move |area_id: i64| {
            use baras_core::boss::load_bosses_with_custom;
            area_index
                .get(&area_id)
                .and_then(|entry| load_bosses_with_custom(&entry.file_path, user_encounters_dir.as_deref()).ok())
        });
        session.set_definition_loader(std::sync::Arc::new(loader));

        // Reset area tracking for new session
        self.loaded_area_id = 0;
        self.shared.current_area_id.store(0, Ordering::SeqCst);

        // Add signal handler that triggers metrics on combat state changes
        let handler = CombatSignalHandler::new(
            self.shared.clone(),
            trigger_tx.clone(),
            session_event_tx.clone(),
            self.overlay_tx.clone(),
            self.cmd_tx.clone(),
        );
        session.add_signal_handler(Box::new(handler));

        let session = Arc::new(RwLock::new(session));

        // Spawn task to emit session events to frontend (event-driven, not polled)
        // and persist pull history once each fight's summary has landed.
        //
        // Holds only a `Weak` to the session: the session owns the signal handler
        // that owns the sender, so a strong ref here would keep both alive forever
        // and this task would never see the channel close after stop_tailing.
        let app_handle = self.app_handle.clone();
        let event_session = Arc::downgrade(&session);
        tokio::spawn(async move {
            while let Some(event) = session_event_rx.recv().await {
                if matches!(event, SessionEvent::CombatEnded)
                    && let Some(session) = event_session.upgrade()
                {
                    history::persist_pull_history(&session, &app_handle).await;
                }
                // Emit event to frontend - they can fetch fresh data
                let _ = app_handle.emit("session-updated", format!("{:?}", event));
            }
        });

        // Update shared state
        *self.shared.session.write().await = Some(session.clone());

        // Notify frontend of active file change
        let _ = self
            .app_handle
            .emit("active-file-changed", path.to_string_lossy().to_string());

        // Notify frontend that a new session has started (reset UI state)
        let _ = self.app_handle.emit("new-session-started", ());

        // Create reader for live tailing (after subprocess parse)
        let reader = Reader::from(path.clone(), session.clone());

        // Parse historical file in subprocess to avoid memory fragmentation
        let timer = std::time::Instant::now();
        let session_id = path
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("unknown")
            .to_string();

        // Get encounters output directory
        let encounters_dir =
            baras_core::storage::encounters_dir(&session_id).unwrap_or_else(|_| PathBuf::from("/tmp/baras-encounters"));

        // Get boss definitions directory for phase detection
        let definitions_dir = self
            .app_handle
            .path()
            .resolve("definitions/encounters", tauri::path::BaseDirectory::Resource)
            .ok();

        // Spawn parse worker subprocess
        // Check multiple locations: bundled sidecar (with target triple), next to exe, fallback to PATH
        let worker_path = history::worker_binary_path();

        debug!(worker_path = ?worker_path, "Using parse worker");

        let mut cmd = std::process::Command::new(&worker_path);
        cmd.arg(&path).arg(&session_id).arg(&encounters_dir);

        // Pass definitions directory if available
        if let Some(ref def_dir) = definitions_dir {
            cmd.arg(def_dir);
            debug!(definitions_path = ?def_dir, "Using definitions directory");
        }

        // Pass log path so subprocess writes to same log file
        if let Some(log_path) = baras_core::paths::config_dir().map(|p| p.join("baras.log")) {
            cmd.env("BARAS_LOG_PATH", &log_path);
        }

        // Run subprocess on a blocking thread so it doesn't stall the tokio runtime
        let output = tokio::task::spawn_blocking(move || cmd.output()).await;
        // Flatten: JoinError<io::Result<Output>> -> treat JoinError as spawn failure
        let output = match output {
            Ok(inner) => inner,
            Err(e) => Err(std::io::Error::other(e)),
        };

        match output {
            Ok(output) if output.status.success() => {
                // Parse JSON result from subprocess
                let json_result = String::from_utf8(output.stdout)
                    .map_err(|e| format!("Invalid UTF-8: {}", e))
                    .and_then(|result| {
                        serde_json::from_str::<ParseWorkerOutput>(&result).map_err(|e| {
                            format!("JSON parse error: {} (input: {})", e, &result[..result.len().min(500)])
                        })
                    });

                match json_result {
                    Ok(parse_result) => {
                        let mut session_guard = session.write().await;

                        // CRITICAL: Restore byte/line positions for live-tailing to work
                        session_guard.current_byte = Some(parse_result.end_pos);
                        session_guard.current_line = Some(parse_result.line_count);

                        // Import session state from subprocess using shared IPC contract
                        let mut player_context = None;
                        if let Some(cache) = &mut session_guard.session_cache {
                            // Restore player, area, disciplines, and encounter history
                            let generation_count = cache.restore_from_worker_output(&parse_result);

                            debug!(
                                area_id = parse_result.area.area_id,
                                area_name = %parse_result.area.area_name,
                                difficulty_id = parse_result.area.difficulty_id,
                                generation = generation_count,
                                "Imported state from subprocess"
                            );

                            // Sync current_area_id from subprocess so timer definition reloads work
                            if parse_result.area.area_id != 0 {
                                self.shared
                                    .current_area_id
                                    .store(parse_result.area.area_id, Ordering::SeqCst);
                            }

                            // Capture player context before releasing cache borrow.
                            // discipline_id comes from player_disciplines (not cache.player,
                            // which doesn't have it after worker restore)
                            if cache.player_initialized {
                                let disc_id = cache
                                    .player_disciplines
                                    .get(&cache.player.id)
                                    .map(|p| p.discipline_id)
                                    .unwrap_or(0);
                                player_context = Some((cache.player.id, disc_id));
                            }

                            // Check if there's an incomplete encounter (parquet file exists but no summary)
                            // If so, we'll continue with that encounter ID instead of creating a new one
                            let incomplete_encounter_exists = {
                                let incomplete_parquet = encounters_dir.join(baras_core::storage::encounter_filename(
                                    parse_result.encounter_count as u32,
                                ));
                                incomplete_parquet.exists()
                            };

                            if incomplete_encounter_exists {
                                // There's an incomplete encounter written to parquet
                                // Continue with the same encounter ID but increment next_encounter_id
                                // so the next encounter after this one has the correct ID
                                cache.set_next_encounter_id(parse_result.encounter_count as u64 + 1);

                                // Don't call push_new_encounter() - we'll continue accumulating
                                // to the existing encounter (ID 0) and it will be written with the correct ID
                                // Update the current encounter's area context
                                let difficulty = Difficulty::from_difficulty_id(cache.current_area.difficulty_id);
                                let area_id = if cache.current_area.area_id != 0 {
                                    Some(cache.current_area.area_id)
                                } else {
                                    None
                                };
                                let area_name = if cache.current_area.area_name.is_empty() {
                                    None
                                } else {
                                    Some(cache.current_area.area_name.clone())
                                };
                                let area_entered_line = cache.current_area.entered_at_line;

                                if let Some(enc) = cache.current_encounter_mut() {
                                    enc.id = parse_result.encounter_count as u64;
                                    enc.set_difficulty(difficulty);
                                    enc.set_area(area_id, area_name, area_entered_line);
                                }
                            } else {
                                // All encounters were finalized
                                // Set next ID to continue from where subprocess left off
                                cache.set_next_encounter_id(parse_result.encounter_count as u64);

                                // Create fresh encounter with correct area context
                                cache.push_new_encounter();
                            }
                        }

                        // Sync player context to effect tracker and shared state so
                        // discipline-scoped effects and profile auto-switching work
                        // immediately (handler missed historical signals from subprocess)
                        if let Some((player_id, discipline_id)) = player_context {
                            if let Some(tracker) = session_guard.effect_tracker()
                                && let Ok(mut tracker) = tracker.lock()
                            {
                                tracker.set_player_context(player_id, discipline_id);
                            }
                            self.shared.last_player_id.store(player_id, Ordering::SeqCst);
                            if let Some(discipline) = Discipline::from_guid(discipline_id) {
                                let role = discipline.role();
                                self.shared.last_player_role.store(role.to_u8(), Ordering::SeqCst);
                                // Apply role-based profile now: the DisciplineChanged signal
                                // that normally drives auto-switching was consumed by the
                                // subprocess, so a role change while the app was closed would
                                // otherwise keep the wrong profile until the next area
                                // transition. Idempotent (skips if already active) and gated
                                // on is_live_tailing, so historical opens never switch.
                                let _ = self.cmd_tx.try_send(ServiceCommand::AutoSwitchProfile {
                                    role_name: format!("{role:?}"),
                                });
                            }
                        }

                        // Enable live parquet writing (continues from where subprocess left off)
                        session_guard.enable_live_parquet(encounters_dir.clone(), parse_result.encounter_count as u32);

                        // Load boss definitions for initial area (before releasing lock)
                        if parse_result.area.area_id != 0
                            && let Some(bosses) = self.load_area_definitions(parse_result.area.area_id)
                        {
                            // Send notes to overlay
                            self.send_notes_from_bosses(&bosses);
                            session_guard.load_boss_definitions(bosses, false);
                        }

                        session_guard.finalize_session();
                        session_guard.sync_timer_context();

                        // Historical opens: the worker rewinds end_pos so live
                        // tailing can replay an encounter left open at EOF
                        // (e.g. logged out mid-warzone). Live mode wants that
                        // replay; a historical file must not tick like live
                        // combat, so consume the tail here and close the
                        // encounter at its last event.
                        if !self.shared.is_live_tailing.load(Ordering::SeqCst) {
                            let session_date = session_guard.game_session_date.unwrap_or_default();
                            match reader.read_log_file_streaming_from(
                                session_date,
                                parse_result.end_pos,
                                parse_result.line_count,
                                |event| session_guard.process_event(event),
                            ) {
                                Ok((eof_pos, next_line, event_count)) => {
                                    session_guard.current_byte = Some(eof_pos);
                                    session_guard.current_line = Some(next_line);
                                    if event_count > 0 {
                                        info!(event_count, "Consumed handoff tail for historical open");
                                    }
                                }
                                Err(e) => {
                                    warn!(error = %e, "Failed to consume handoff tail");
                                }
                            }
                            session_guard.finalize_open_encounter();
                        }

                        // Record this log's pulls for the Historical tab
                        if let Some(history) = history::build_pull_history(&session_guard) {
                            history::save_in_background(history, self.app_handle.clone());
                        }

                        // Check if we're starting mid-encounter (live mode only)
                        // This enables overlays to display data immediately when app starts during combat
                        let mid_combat_startup = if !self.shared.is_live_tailing.load(Ordering::SeqCst) {
                            false
                        } else if let Some(cache) = &session_guard.session_cache {
                            if let Some(enc) = cache.current_encounter() {
                                use baras_core::encounter::EncounterState;
                                if enc.state == EncounterState::InCombat {
                                    info!(
                                        encounter_id = enc.id,
                                        "Detected mid-combat startup - will enable overlays"
                                    );
                                    true
                                } else {
                                    false
                                }
                            } else {
                                false
                            }
                        } else {
                            false
                        };

                        drop(session_guard);

                        info!(
                            event_count = parse_result.event_count,
                            encounter_count = parse_result.encounter_count,
                            elapsed_ms = parse_result.elapsed_ms,
                            "Subprocess parse completed"
                        );

                        // Requirement 5: Backfill the operation timer when resuming live
                        // mode mid-area so the timer immediately reflects how long the
                        // player has been in the instance.
                        //
                        // - Flashpoints / PvP: timer starts from area entry time.
                        // - Operations: timer starts from the first boss pull time
                        //   (using the earliest encounter start in parse_result.encounters).
                        //
                        // Only fires when in live tailing mode and the timer isn't already
                        // running (handles the app restart / reconnect case).
                        if self.shared.is_live_tailing.load(Ordering::SeqCst) {
                            let area_id = parse_result.area.area_id;
                            let area_kind = AreaKind::from_area_id(area_id);
                            let timer_already_running = self.shared.operation_timer.lock().unwrap().is_running();

                            if !timer_already_running {
                                const MAX_BACKFILL_SECS: u64 = 4 * 3600; // 4-hour cap

                                match area_kind {
                                    AreaKind::Flashpoint | AreaKind::PvP => {
                                        // Use area entry timestamp for backfill
                                        if let Some(entered_at) = parse_result.area.entered_at {
                                            let now = chrono::Local::now().naive_local();
                                            let elapsed = now.signed_duration_since(entered_at);
                                            let elapsed_secs = elapsed.num_seconds().max(0) as u64;
                                            let capped = elapsed_secs.min(MAX_BACKFILL_SECS);
                                            let mut timer = self.shared.operation_timer.lock().unwrap();
                                            if !timer.manually_stopped {
                                                info!(
                                                    area_kind = ?area_kind,
                                                    elapsed_secs = capped,
                                                    "Backfilling operation timer from area entry"
                                                );
                                                timer.start_with_offset(capped);
                                                drop(timer);
                                                self.emit_operation_timer_tick();
                                            }
                                        }
                                    }
                                    AreaKind::Operation => {
                                        // The clock of the instance being played (a
                                        // reset earlier in the log starts another): its
                                        // start line crossing, else its first boss pull
                                        let entered = parse_result
                                            .encounters
                                            .last()
                                            .and_then(|s| s.instance_entered.as_deref());
                                        let earliest_start = parse_result
                                            .timer_starts
                                            .iter()
                                            .filter(|t| {
                                                t.area_id == area_id && entered.is_some_and(|e| t.time.as_str() >= e)
                                            })
                                            .map(|t| &t.time)
                                            .chain(
                                                parse_result
                                                    .encounters
                                                    .iter()
                                                    .filter(|s| {
                                                        s.encounter_type == PhaseType::Raid
                                                            && s.instance_entered.as_deref() == entered
                                                    })
                                                    .filter_map(|s| s.start_time.as_ref()),
                                            )
                                            .filter_map(|t| {
                                                chrono::NaiveDateTime::parse_from_str(t, "%Y-%m-%dT%H:%M:%S").ok()
                                            })
                                            .min();

                                        if let Some(first_pull) = earliest_start {
                                            let now = chrono::Local::now().naive_local();
                                            let elapsed = now.signed_duration_since(first_pull);
                                            let elapsed_secs = elapsed.num_seconds().max(0) as u64;
                                            let capped = elapsed_secs.min(MAX_BACKFILL_SECS);
                                            let mut timer = self.shared.operation_timer.lock().unwrap();
                                            if !timer.manually_stopped {
                                                info!(
                                                    elapsed_secs = capped,
                                                    "Backfilling operation timer from the run start"
                                                );
                                                timer.start_with_offset(capped);
                                                drop(timer);
                                                self.emit_operation_timer_tick();
                                            }
                                        }
                                    }
                                    AreaKind::Other => {}
                                }
                            }
                        }

                        // If we detected mid-combat startup, set in_combat BEFORE emitting event
                        // This ensures frontend gets correct state when it fetches session info
                        if mid_combat_startup {
                            self.shared.in_combat.store(true, std::sync::atomic::Ordering::SeqCst);
                            let _ = trigger_tx.try_send(MetricsTrigger::CombatStarted);
                            let _ = session_event_tx.send(SessionEvent::CombatStarted);
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "Subprocess output parse failed");
                        let in_combat = fallback_streaming_parse(
                            &reader,
                            &session,
                            encounters_dir.clone(),
                            self.shared.is_live_tailing.load(Ordering::SeqCst),
                        )
                        .await;
                        if in_combat {
                            self.shared.in_combat.store(true, std::sync::atomic::Ordering::SeqCst);
                            let _ = trigger_tx.try_send(MetricsTrigger::CombatStarted);
                            let _ = session_event_tx.send(SessionEvent::CombatStarted);
                            info!("Detected mid-combat startup (fallback parse)");
                        }
                    }
                }
            }
            Ok(output) => {
                error!(
                    stderr = %String::from_utf8_lossy(&output.stderr),
                    "Subprocess failed"
                );
                // Fallback to streaming parse in main process
                let in_combat = fallback_streaming_parse(
                    &reader,
                    &session,
                    encounters_dir.clone(),
                    self.shared.is_live_tailing.load(Ordering::SeqCst),
                )
                .await;
                if in_combat {
                    self.shared.in_combat.store(true, std::sync::atomic::Ordering::SeqCst);
                    let _ = trigger_tx.try_send(MetricsTrigger::CombatStarted);
                    let _ = session_event_tx.send(SessionEvent::CombatStarted);
                    info!("Detected mid-combat startup (fallback parse)");
                }
            }
            Err(e) => {
                error!(error = %e, "Failed to spawn subprocess");
                // Fallback to streaming parse in main process
                let in_combat = fallback_streaming_parse(
                    &reader,
                    &session,
                    encounters_dir.clone(),
                    self.shared.is_live_tailing.load(Ordering::SeqCst),
                )
                .await;
                if in_combat {
                    self.shared.in_combat.store(true, std::sync::atomic::Ordering::SeqCst);
                    let _ = trigger_tx.try_send(MetricsTrigger::CombatStarted);
                    let _ = session_event_tx.send(SessionEvent::CombatStarted);
                    info!("Detected mid-combat startup (fallback parse)");
                }
            }
        }

        info!(elapsed_ms = timer.elapsed().as_millis() as u64, "Parse completed");

        // Re-parsed fights get their recorded Parsely links back, then the
        // frontend refreshes session info (in_combat is already set above)
        history::restore_parsely_links(&session, &self.shared).await;
        let _ = self.app_handle.emit("session-updated", "FileLoaded");

        // Check if this live session is actually not-live (game closed, no player,
        // or newest log file is blank). This catches the case where the app starts
        // tailing an old/empty file. We emit unconditionally — the router checks the setting.
        //
        // Clear game_starting first so the liveness check below is not blocked by it —
        // the parse completing is exactly the point where we have enough information to
        // make the call. If the session is still not-live (stale/empty) we re-apply the
        // block via NotLiveStateChanged { is_live: false }.
        self.shared.auto_hide.set_game_starting(false);
        if self.shared.is_session_not_live().await {
            let _ = self
                .overlay_tx
                .try_send(OverlayUpdate::NotLiveStateChanged { is_live: false });
        } else {
            // Session IS live (player detected, recent data) — tell the router so it
            // can clear not-live auto-hide.
            let _ = self
                .overlay_tx
                .try_send(OverlayUpdate::NotLiveStateChanged { is_live: true });
        }

        // Trigger initial metrics send after file processing
        let _ = trigger_tx.try_send(MetricsTrigger::InitialLoad);

        // Set alacrity/latency from config for duration calculations
        {
            let session_guard = session.read().await;
            // The parsed part of the log emitted no DisciplineChanged
            self.apply_alacrity_preset(&session_guard).await;
            let config = self.shared.config.read().await;
            session_guard.set_effect_alacrity(config.alacrity_percent);
            session_guard.set_effect_latency(config.latency_ms);
        }

        // Spawn the tail task to watch for new lines
        // The tail loop is "immortal" - it only exits via task abort or initialization failure
        let path_for_logging = path.clone();
        let tail_handle = tokio::spawn(async move {
            match reader.tail_log_file().await {
                Ok(()) => {
                    // This is unreachable in normal operation since the loop is immortal
                    // Only happens if task is aborted
                    debug!(path = %path_for_logging.display(), "Tail task ended");
                }
                Err(e) => {
                    // Initialization failed (couldn't open file, seek, etc.)
                    error!(
                        error = %e,
                        path = %path_for_logging.display(),
                        "Tail task failed to initialize"
                    );
                }
            }
        });

        // Spawn signal-driven metrics task
        let shared = self.shared.clone();
        let overlay_tx = self.overlay_tx.clone();
        let metrics_handle = tokio::spawn(async move {
            loop {
                // Check for triggers with timeout to allow task cancellation
                let trigger = tokio::time::timeout(std::time::Duration::from_millis(100), trigger_rx.recv()).await;

                let trigger = match trigger {
                    Ok(Some(t)) => t,
                    Ok(None) => break,  // Channel closed
                    Err(_) => continue, // Timeout - check again
                };

                // Calculate and send unified combat data
                if let Some(data) = calculate_combat_data(&shared).await
                    && !data.metrics.is_empty()
                {
                    let _ = overlay_tx.try_send(OverlayUpdate::DataUpdated(data));
                }

                // For CombatStarted, start polling during combat
                if matches!(trigger, MetricsTrigger::CombatStarted) {
                    // Poll during active combat
                    while shared.in_combat.load(Ordering::SeqCst) {
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

                        if let Some(data) = calculate_combat_data(&shared).await
                            && !data.metrics.is_empty()
                        {
                            let _ = overlay_tx.try_send(OverlayUpdate::DataUpdated(data));
                        }
                    }
                }
            }
        });

        // Spawn effects + boss health + audio sampling task (polls continuously)
        // Uses adaptive sleep: fast when active, slow (500ms) when idle
        let shared = self.shared.clone();
        let overlay_tx = self.overlay_tx.clone();
        let audio_tx = self.audio_tx.clone();
        let icon_cache = self.icon_cache.clone();
        // Capture the current time so we can suppress audio for alerts that originated
        // before we started tailing (i.e. recovered from a stale encounter).
        let tailing_started_at = chrono::Local::now().naive_local();
        let effects_handle = tokio::spawn(async move {
            // Track previous state to avoid redundant updates
            let mut last_raid_effect_count: usize = 0;
            let _last_effects_count: usize = 0;

            // Track previous state for new overlays to avoid redundant updates
            let mut last_effects_a_count: usize = 0;
            let mut last_effects_b_count: usize = 0;
            let mut last_effects_c_count: usize = 0;
            let mut last_cooldowns_count: usize = 0;
            let mut last_cooldowns_b_count: usize = 0;
            let mut last_dot_tracker_count: usize = 0;

            // Throttle stale-recovery checks to once per second
            let mut last_stale_check = tokio::time::Instant::now();

            loop {
                // Check which overlays are active to determine sleep interval
                let raid_active = shared.raid_overlay_active.load(Ordering::Relaxed);
                let boss_active = shared.boss_health_overlay_active.load(Ordering::Relaxed);
                let timer_active = shared.timer_overlay_active.load(Ordering::Relaxed);
                let effects_a_active = shared.effects_a_overlay_active.load(Ordering::Relaxed);
                let effects_b_active = shared.effects_b_overlay_active.load(Ordering::Relaxed);
                let effects_c_active = shared.effects_c_overlay_active.load(Ordering::Relaxed);
                let cooldowns_active = shared.cooldowns_overlay_active.load(Ordering::Relaxed);
                let cooldowns_b_active = shared.cooldowns_b_overlay_active.load(Ordering::Relaxed);
                let dot_tracker_active = shared.dot_tracker_overlay_active.load(Ordering::Relaxed);
                let in_combat = shared.in_combat.load(Ordering::Relaxed);
                let is_live = shared.is_live_tailing.load(Ordering::SeqCst);

                // Determine if any work needs to be done
                let any_overlay_active = raid_active
                    || boss_active
                    || timer_active
                    || effects_a_active
                    || effects_b_active
                    || effects_c_active
                    || cooldowns_active
                    || cooldowns_b_active
                    || dot_tracker_active;
                let needs_audio = is_live && (in_combat || raid_active);

                // Adaptive sleep: fast when active, slow when idle
                // 30ms matches tail polling for consistent ~60ms max latency
                let sleep_ms = if any_overlay_active || needs_audio { 30 } else { 500 };
                tokio::time::sleep(std::time::Duration::from_millis(sleep_ms)).await;

                // Periodic stale-recovery: if the session was flagged not-live (e.g. game
                // closed, newest file blank), check whether conditions have changed. This
                // lets overlays restore when the game relaunches or a new file gets content.
                if shared.auto_hide.is_not_live_active()
                    && last_stale_check.elapsed() >= std::time::Duration::from_secs(1)
                {
                    last_stale_check = tokio::time::Instant::now();
                    if !shared.is_session_not_live().await {
                        let _ = overlay_tx.try_send(OverlayUpdate::NotLiveStateChanged { is_live: true });
                    }
                }

                // Skip processing if nothing needs updating
                if !any_overlay_active && !needs_audio {
                    continue;
                }

                // Update whenever there are effects (or always in rearrange mode)
                if raid_active {
                    // Only a newly registered player can settle a provisional slot.
                    if shared.roster_changed.swap(false, Ordering::Relaxed) {
                        promote_provisional_slots(&shared).await;
                    }

                    let rearranging = shared.rearrange_mode.load(Ordering::Relaxed);
                    if let Some(data) = build_raid_frame_data(&shared, rearranging, icon_cache.as_ref()).await {
                        let effect_count: usize = data.frames.iter().map(|f| f.effects.len()).sum();
                        // Always send in rearrange mode, otherwise only when effects exist/changed
                        if (rearranging || effect_count > 0 || last_raid_effect_count > 0)
                            && overlay_tx.try_send(OverlayUpdate::EffectsUpdated(data)).is_err()
                        {
                            warn!("Overlay channel full, dropped raid effects update");
                        }
                        last_raid_effect_count = effect_count;
                    } else if rearranging {
                        // In rearrange mode, send empty data to keep overlay rendering
                        if overlay_tx
                            .try_send(OverlayUpdate::EffectsUpdated(baras_overlay::RaidFrameData {
                                frames: vec![],
                            }))
                            .is_err()
                        {
                            warn!("Overlay channel full, dropped raid effects clear");
                        }
                        last_raid_effect_count = 0;
                    } else {
                        last_raid_effect_count = 0;
                    }
                }
                // Effects A: only send if there are effects or effects just cleared
                if effects_a_active {
                    if let Some(data) =
                        build_effects_ab_data(&shared, icon_cache.as_ref(), DisplayTarget::EffectsA).await
                    {
                        let count = data.effects.len();
                        if (count > 0 || last_effects_a_count > 0)
                            && overlay_tx.try_send(OverlayUpdate::EffectsAUpdated(data)).is_err()
                        {
                            warn!("Overlay channel full, dropped effects A update");
                        }
                        last_effects_a_count = count;
                    } else if last_effects_a_count > 0 {
                        if overlay_tx
                            .try_send(OverlayUpdate::EffectsAUpdated(EffectsABData { effects: vec![] }))
                            .is_err()
                        {
                            warn!("Overlay channel full, dropped effects A clear");
                        }
                        last_effects_a_count = 0;
                    }
                }

                // Effects B: only send if there are effects or effects just cleared
                if effects_b_active {
                    if let Some(data) =
                        build_effects_ab_data(&shared, icon_cache.as_ref(), DisplayTarget::EffectsB).await
                    {
                        let count = data.effects.len();
                        if (count > 0 || last_effects_b_count > 0)
                            && overlay_tx.try_send(OverlayUpdate::EffectsBUpdated(data)).is_err()
                        {
                            warn!("Overlay channel full, dropped effects B update");
                        }
                        last_effects_b_count = count;
                    } else if last_effects_b_count > 0 {
                        if overlay_tx
                            .try_send(OverlayUpdate::EffectsBUpdated(EffectsABData { effects: vec![] }))
                            .is_err()
                        {
                            warn!("Overlay channel full, dropped effects B clear");
                        }
                        last_effects_b_count = 0;
                    }
                }

                // Effects C: only send if there are effects or effects just cleared
                if effects_c_active {
                    if let Some(data) =
                        build_effects_ab_data(&shared, icon_cache.as_ref(), DisplayTarget::EffectsC).await
                    {
                        let count = data.effects.len();
                        if (count > 0 || last_effects_c_count > 0)
                            && overlay_tx.try_send(OverlayUpdate::EffectsCUpdated(data)).is_err()
                        {
                            warn!("Overlay channel full, dropped effects C update");
                        }
                        last_effects_c_count = count;
                    } else if last_effects_c_count > 0 {
                        if overlay_tx
                            .try_send(OverlayUpdate::EffectsCUpdated(EffectsABData { effects: vec![] }))
                            .is_err()
                        {
                            warn!("Overlay channel full, dropped effects C clear");
                        }
                        last_effects_c_count = 0;
                    }
                }

                // Cooldowns: only send if there are cooldowns or cooldowns just cleared
                if cooldowns_active {
                    if let Some(data) =
                        build_cooldowns_data(&shared, icon_cache.as_ref(), DisplayTarget::Cooldowns).await
                    {
                        let count = data.entries.len();
                        if (count > 0 || last_cooldowns_count > 0)
                            && overlay_tx.try_send(OverlayUpdate::CooldownsUpdated(data)).is_err()
                        {
                            warn!("Overlay channel full, dropped cooldowns update");
                        }
                        last_cooldowns_count = count;
                    } else if last_cooldowns_count > 0 {
                        if overlay_tx
                            .try_send(OverlayUpdate::CooldownsUpdated(CooldownData { entries: vec![] }))
                            .is_err()
                        {
                            warn!("Overlay channel full, dropped cooldowns clear");
                        }
                        last_cooldowns_count = 0;
                    }
                }

                // Cooldowns B: only send if there are cooldowns or cooldowns just cleared
                if cooldowns_b_active {
                    if let Some(data) =
                        build_cooldowns_data(&shared, icon_cache.as_ref(), DisplayTarget::CooldownsB).await
                    {
                        let count = data.entries.len();
                        if (count > 0 || last_cooldowns_b_count > 0)
                            && overlay_tx.try_send(OverlayUpdate::CooldownsBUpdated(data)).is_err()
                        {
                            warn!("Overlay channel full, dropped cooldowns B update");
                        }
                        last_cooldowns_b_count = count;
                    } else if last_cooldowns_b_count > 0 {
                        if overlay_tx
                            .try_send(OverlayUpdate::CooldownsBUpdated(CooldownData { entries: vec![] }))
                            .is_err()
                        {
                            warn!("Overlay channel full, dropped cooldowns B clear");
                        }
                        last_cooldowns_b_count = 0;
                    }
                }

                // DOT tracker: only send if there are targets or targets just cleared
                if dot_tracker_active {
                    if let Some(data) = build_dot_tracker_data(&shared, icon_cache.as_ref()).await {
                        let count = data.targets.len();
                        if (count > 0 || last_dot_tracker_count > 0)
                            && overlay_tx.try_send(OverlayUpdate::DotTrackerUpdated(data)).is_err()
                        {
                            warn!("Overlay channel full, dropped DOT tracker update");
                        }
                        last_dot_tracker_count = count;
                    } else if last_dot_tracker_count > 0 {
                        if overlay_tx
                            .try_send(OverlayUpdate::DotTrackerUpdated(DotTrackerData { targets: vec![] }))
                            .is_err()
                        {
                            warn!("Overlay channel full, dropped DOT tracker clear");
                        }
                        last_dot_tracker_count = 0;
                    }
                }

                // Effect audio: process in live mode
                // Note: On-end text alerts are handled via tick() -> fired_alerts,
                // consumed in build_timer_data_with_audio() below.
                if shared.is_live_tailing.load(Ordering::SeqCst) {
                    let effect_audio = process_effect_audio(&shared).await;
                    for (name, seconds, voice_pack) in effect_audio.countdowns {
                        let _ = audio_tx.try_send(AudioEvent::Countdown {
                            timer_name: name,
                            seconds,
                            voice_pack,
                        });
                    }
                    for alert in effect_audio.alerts {
                        let _ = audio_tx.try_send(AudioEvent::Alert {
                            text: alert.name,
                            custom_sound: alert.file,
                        });
                    }
                }

                // Boss health: only poll when in combat
                if boss_active
                    && in_combat
                    && let Some(data) = build_boss_health_data(&shared, icon_cache.as_ref()).await
                    && overlay_tx.try_send(OverlayUpdate::BossHealthUpdated(data)).is_err()
                {
                    warn!("Overlay channel full, dropped boss health update");
                }

                // Timers + Audio: always poll when in live mode (alerts can fire at combat end)
                if shared.is_live_tailing.load(Ordering::SeqCst)
                    && let Some(bundle) = build_timer_data_with_audio(&shared, icon_cache.as_ref()).await
                {
                    // Send timer overlay data (only when in combat)
                    if in_combat && timer_active {
                        if overlay_tx
                            .try_send(OverlayUpdate::TimersAUpdated(bundle.timers_a))
                            .is_err()
                        {
                            warn!("Overlay channel full, dropped timers A update");
                        }
                        if overlay_tx
                            .try_send(OverlayUpdate::TimersBUpdated(bundle.timers_b))
                            .is_err()
                        {
                            warn!("Overlay channel full, dropped timers B update");
                        }
                    }

                    // Ability queue: always send (overlay gates on its own active flag)
                    if overlay_tx
                        .try_send(OverlayUpdate::AbilityQueueUpdated(bundle.ability_queue))
                        .is_err()
                    {
                        warn!("Overlay channel full, dropped ability queue update");
                    }

                    // Send countdown audio events (only when in combat)
                    if in_combat {
                        for (name, seconds, voice_pack) in bundle.countdowns {
                            let _ = audio_tx.try_send(AudioEvent::Countdown {
                                timer_name: name,
                                seconds,
                                voice_pack,
                            });
                        }
                    }

                    // Send text alerts to overlay (only those with alert_text_enabled and not stale)
                    let text_alerts: Vec<_> = bundle
                        .alerts
                        .iter()
                        .filter(|a| a.alert_text_enabled && a.timestamp >= tailing_started_at)
                        .cloned()
                        .collect();
                    if !text_alerts.is_empty() && overlay_tx.try_send(OverlayUpdate::AlertsFired(text_alerts)).is_err()
                    {
                        warn!("Overlay channel full, dropped timer alerts");
                    }

                    // Send alert audio events (only if audio_enabled for that alert)
                    // Skip alerts from before we started tailing (stale encounter recovery)
                    for alert in bundle.alerts {
                        if alert.audio_enabled && alert.timestamp >= tailing_started_at {
                            let _ = audio_tx.try_send(AudioEvent::Alert {
                                text: alert.text,
                                custom_sound: alert.audio_file,
                            });
                        }
                    }
                }
            }
        });

        self.tail_handle = Some(tail_handle);
        self.metrics_handle = Some(metrics_handle);
        self.effects_handle = Some(effects_handle);
    }
}
