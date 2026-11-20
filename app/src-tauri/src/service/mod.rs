//! Combat service - coordinates parsing, state management, and overlay updates
//!
//! Architecture:
//! - SharedState: Arc-wrapped state readable by Tauri commands (in crate::state)
//! - ServiceHandle: For sending commands + accessing shared state
//! - CombatService: Background task that processes commands and updates shared state
mod boss_health;
mod characters;
mod cleanup;
mod combat_data;
mod definitions;
mod directory;
mod effect_data;
mod handler;
mod history;
mod info;
mod operation_timer;
pub(crate) mod process_monitor;
mod raid_detection;
mod raid_frames;
mod signal_handler;
mod tailing;
mod timer_data;
mod watcher;

use crate::state::SharedState;
pub use crate::state::{RaidSlotRegistry, RegisteredPlayer};
pub use combat_data::CombatData;
use combat_data::calculate_combat_data;
pub use handler::*;
pub use info::{AreaVisitInfo, LogFileInfo, SessionInfo};
use operation_timer::OP_EXIT_TIMEOUT_SECS;
pub use operation_timer::{AreaKind, OperationTimerState};
pub(crate) use raid_frames::build_raid_frame_data;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc;

use baras_core::DefinitionSet;
pub use baras_core::IncomingDamageRow;
use baras_core::context::{AppConfig, AppConfigExt, DirectoryIndex};
use baras_core::timers::FiredAlert;
use baras_overlay::{BossHealthData, CooldownData, DotTrackerData, EffectsABData, NotesData, RaidFrameData, TimerData};

use crate::audio::{AudioEvent, AudioSender, AudioService};
use crate::state::MutexExt;
use tracing::{debug, info, warn};

// ─────────────────────────────────────────────────────────────────────────────
// Service Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Messages sent to the service from Tauri commands
pub enum ServiceCommand {
    RefreshIndex,
    FileDetected(PathBuf),
    /// File was modified - re-check character data for files missing it
    FileModified(PathBuf),
    FileRemoved(PathBuf),
    DirectoryChanged,
    /// Reload timer/boss definitions from disk and update active session
    ReloadTimerDefinitions,
    /// Reload effect definitions from disk and update active session
    ReloadEffectDefinitions,
    /// Open a historical file (pauses live tailing)
    /// Open a log for viewing; `select_encounter` asks the Data Explorer to
    /// focus that encounter id once the file has loaded.
    OpenHistoricalFile {
        path: PathBuf,
        select_encounter: Option<u64>,
    },
    /// Resume live tailing (switch to newest file)
    ResumeLiveTailing,
    /// Trigger immediate raid frame data refresh (after registry changes)
    RefreshRaidFrames,
    /// Send specific boss notes to the overlay
    SendNotesToOverlay(NotesData),
    /// Reload area definitions for a new area (triggers notes update)
    ReloadAreaDefinitions(i64),
    /// Start monitoring the game process continuously (triggered on first live event)
    StartProcessMonitor,
    /// Manually start the operation timer
    StartOperationTimer,
    /// Stop the operation timer (manual only)
    StopOperationTimer,
    /// Reset the operation timer (clears all state)
    ResetOperationTimer,
    /// Update the operation name context (from area entered signal)
    SetOperationTimerContext {
        operation_name: Option<String>,
    },
    /// Auto-switch profile based on role change (triggered by DisciplineChanged)
    AutoSwitchProfile {
        role_name: String,
    },
    /// The local player's discipline changed: use its saved alacrity, if any
    ApplyAlacrityPreset,
    /// Emit the current operation timer state to the frontend and overlay
    EmitOperationTimerTick,
}

/// Updates sent to the overlay system
#[derive(Debug, Clone)]
pub enum OverlayUpdate {
    CombatStarted,
    CombatEnded,
    DetectRaidNames,
    /// Combat metrics for metric and personal overlays
    DataUpdated(CombatData),
    /// Effect data for raid frame overlay (HoTs, debuffs, etc.)
    EffectsUpdated(RaidFrameData),
    /// Boss health data for boss health overlay
    BossHealthUpdated(BossHealthData),
    /// Timer A data for Timers A overlay
    TimersAUpdated(TimerData),
    /// Timer B data for Timers B overlay
    TimersBUpdated(TimerData),
    /// Alert text for alerts overlay
    AlertsFired(Vec<FiredAlert>),
    /// Effects A overlay data
    EffectsAUpdated(EffectsABData),
    /// Effects B overlay data
    EffectsBUpdated(EffectsABData),
    /// Effects C overlay data
    EffectsCUpdated(EffectsABData),
    /// Ability cooldowns
    CooldownsUpdated(CooldownData),
    /// Ability cooldowns (second tracker)
    CooldownsBUpdated(CooldownData),
    /// DOTs on enemy targets
    DotTrackerUpdated(DotTrackerData),
    /// Encounter notes (sent when entering an area with boss definitions)
    NotesUpdated(NotesData),
    /// Operation timer update (persistent across encounters)
    OperationTimerUpdated(baras_overlay::OperationTimerData),
    /// Ability queue snapshot (GCD + queued + active countdown entries)
    AbilityQueueUpdated(baras_overlay::AbilityQueueData),
    /// Local player activated an ability (ability cast overlay)
    AbilityCast {
        ability_id: u64,
        name: String,
    },
    /// Drop the ability cast list (player identity changed)
    AbilityCastCleared,
    /// Clear all overlay data (sent when switching files)
    ClearAllData,
    /// Left a PvP area: the last match's enemies are gone
    EnemyFramesCleared,
    /// Local player entered conversation - temporarily hide overlays
    ConversationStarted,
    /// Local player exited conversation - restore overlays if we hid them
    ConversationEnded,
    /// Session liveness changed (historical mode entered/exited, player logged out/in)
    NotLiveStateChanged {
        is_live: bool,
    },
}

// ─────────────────────────────────────────────────────────────────────────────
// Signal Handler
// ─────────────────────────────────────────────────────────────────────────────

/// Trigger for metrics calculation
#[derive(Debug, Clone, Copy)]
pub enum MetricsTrigger {
    CombatStarted,
    CombatEnded,
    InitialLoad,
}

/// Events to notify frontend of session state changes
#[derive(Debug, Clone, Copy)]
pub enum SessionEvent {
    CombatStarted,
    CombatEnded,
    AreaChanged,
    PlayerInitialized,
}

// ─────────────────────────────────────────────────────────────────────────────
// Combat Service
// ─────────────────────────────────────────────────────────────────────────────

/// Main combat service that runs in a background task
pub struct CombatService {
    app_handle: AppHandle,
    shared: Arc<SharedState>,
    overlay_tx: mpsc::Sender<OverlayUpdate>,
    audio_tx: AudioSender,
    cmd_rx: mpsc::Receiver<ServiceCommand>,
    cmd_tx: mpsc::Sender<ServiceCommand>,
    tail_handle: Option<tokio::task::JoinHandle<()>>,
    directory_handle: Option<tokio::task::JoinHandle<()>>,
    metrics_handle: Option<tokio::task::JoinHandle<()>>,
    effects_handle: Option<tokio::task::JoinHandle<()>>,
    /// Effect definitions loaded at startup for overlay tracking
    definitions: DefinitionSet,
    /// Area index for lazy loading encounter definitions (area_id -> file path)
    area_index: Arc<baras_core::boss::AreaIndex>,
    /// Currently loaded area ID (0 = none)
    loaded_area_id: i64,
    /// Icon cache for ability icons (shared with SharedState for overlay data building)
    icon_cache: Option<Arc<baras_overlay::icons::IconCache>>,
    /// Pending file to switch to when it gets content (deferred rotation for empty files)
    pending_file: Option<PathBuf>,
    /// Poll timer for checking if the pending file has content.
    /// Active only when `pending_file` is `Some`. Works around OS-level event
    /// coalescing (e.g. Windows may merge Create+Modify into a single Create).
    pending_file_interval: Option<tokio::time::Interval>,
    /// Handle for the game process monitor task
    process_monitor_handle: Option<tokio::task::JoinHandle<()>>,
}

impl CombatService {
    /// Create a new combat service and return a handle to communicate with it
    pub fn new(
        app_handle: AppHandle,
        overlay_tx: mpsc::Sender<OverlayUpdate>,
        audio_tx: AudioSender,
        audio_rx: mpsc::Receiver<AudioEvent>,
    ) -> (Self, ServiceHandle, Option<Arc<baras_overlay::icons::IconCache>>) {
        let (cmd_tx, cmd_rx) = mpsc::channel(32);

        let config = AppConfig::load();
        // Start with an empty index — start_watcher() will quick-load the newest file
        // then backfill the rest from a disk cache in the background.
        let directory_index = DirectoryIndex::new();

        // Load effect definitions from builtin and user directories
        let definitions = Self::load_effect_definitions(&app_handle);

        // Build area index for lazy loading (fast - only reads headers)
        let area_index = Arc::new(Self::build_area_index(&app_handle));

        let shared = Arc::new(SharedState::new(config, directory_index));

        // Spawn the audio service (shares audio settings with config)
        let user_sounds_dir = baras_core::paths::config_dir()
            .map(|p| p.join("sounds"))
            .unwrap_or_else(|| PathBuf::from("."));
        let bundled_definitions_dir = crate::audio::resolve_bundled_definitions_dir(&app_handle);
        let audio_service = AudioService::new(audio_rx, shared.clone(), user_sounds_dir, bundled_definitions_dir);
        tauri::async_runtime::spawn(audio_service.run());

        // Initialize icon cache for ability icons
        let icon_cache = Self::init_icon_cache(&app_handle);

        // Clone area_index before moving it into the service (needed for background indexer)
        let area_index_for_scanner = area_index.clone();

        let service = Self {
            app_handle: app_handle.clone(),
            shared: shared.clone(),
            overlay_tx,
            audio_tx,
            cmd_rx,
            cmd_tx: cmd_tx.clone(),
            tail_handle: None,
            directory_handle: None,
            metrics_handle: None,
            effects_handle: None,
            definitions,
            area_index,
            loaded_area_id: 0,
            icon_cache,
            pending_file: None,
            pending_file_interval: None,
            process_monitor_handle: None,
        };

        let handle = ServiceHandle {
            cmd_tx,
            shared: shared.clone(),
            app_handle: app_handle.clone(),
        };

        // Spawn background area indexer to populate file area cache
        Self::spawn_area_indexer(shared, area_index_for_scanner, app_handle);

        // Clone icon_cache for the router (router needs it to resolve alert icons)
        let icon_cache_for_router = service.icon_cache.clone();

        (service, handle, icon_cache_for_router)
    }

    /// Run the service event loop
    pub async fn run(mut self) {
        self.start_watcher().await;

        // Scan new and out-of-date logs into the history store (worker subprocess, off the loop)
        if self.shared.config.read().await.auto_scan_history {
            let shared = self.shared.clone();
            let app = self.app_handle.clone();
            tokio::spawn(async move {
                if let Err(e) = history::run_backfill(shared, app, true).await {
                    tracing::warn!(error = %e, "Startup history scan did not start");
                }
            });
        }

        // Operation timer tick interval - 1 second precision
        let mut op_timer_interval = tokio::time::interval(std::time::Duration::from_secs(1));
        op_timer_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                cmd = self.cmd_rx.recv() => {
                    let Some(cmd) = cmd else { break; };

                    match cmd {
                        ServiceCommand::RefreshIndex => {
                            self.refresh_index().await;
                        }
                        ServiceCommand::FileDetected(path) => {
                            self.file_detected(path).await;
                        }
                        ServiceCommand::FileModified(path) => {
                            self.file_modified(path).await;
                        }
                        ServiceCommand::FileRemoved(path) => {
                            self.file_removed(path).await;
                        }
                        ServiceCommand::DirectoryChanged => {
                            self.on_directory_changed().await;
                        }
                        ServiceCommand::ReloadTimerDefinitions => {
                            self.reload_timer_definitions().await;
                        }
                        ServiceCommand::ReloadEffectDefinitions => {
                            self.reload_effect_definitions().await;
                        }
                        ServiceCommand::OpenHistoricalFile { path, select_encounter } => {
                            // Pause live tailing and open the historical file
                            self.shared.is_live_tailing.store(false, Ordering::SeqCst);
                            let _ = self
                                .app_handle
                                .emit("session-updated", "TailingModeChanged");
                            let _ = self
                                .overlay_tx
                                .try_send(OverlayUpdate::NotLiveStateChanged { is_live: false });
                            // Requirement 4: zero out the operation timer when a historical
                            // file is opened — it represents a different session entirely.
                            {
                                let mut timer = self.shared.operation_timer.locked();
                                timer.reset();
                            }
                            self.emit_operation_timer_tick();
                            // Parse is awaited inline, so "FileLoaded" has fired by now
                            self.start_tailing(path.clone()).await;
                            self.shared.pending_opens.locked().remove(&path);
                            if let Some(id) = select_encounter {
                                let _ = self.app_handle.emit("select-encounter", id);
                            }
                        }
                        ServiceCommand::ResumeLiveTailing => {
                            // Resume live tailing and switch to newest file
                            self.shared.is_live_tailing.store(true, Ordering::SeqCst);
                            let _ = self
                                .app_handle
                                .emit("session-updated", "TailingModeChanged");
                            let _ = self
                                .overlay_tx
                                .try_send(OverlayUpdate::NotLiveStateChanged { is_live: true });
                            let newest = {
                                let index = self.shared.directory_index.read().await;
                                index.newest_file().map(|f| f.path.clone())
                            };
                            if let Some(path) = newest {
                                self.start_tailing(path).await;
                            }
                        }
                        ServiceCommand::RefreshRaidFrames => {
                            // Immediately send updated raid frame data to overlay
                            // Pass true to bypass early-out gates (ensures clear is reflected)
                            let data = build_raid_frame_data(&self.shared, true, self.icon_cache.as_ref())
                                .await
                                .unwrap_or_else(|| baras_overlay::RaidFrameData { frames: vec![] });
                            let _ = self
                                .overlay_tx
                                .try_send(OverlayUpdate::EffectsUpdated(data));
                        }
                        ServiceCommand::SendNotesToOverlay(notes_data) => {
                            // Send specific boss notes to the overlay
                            let _ = self.overlay_tx.try_send(OverlayUpdate::NotesUpdated(notes_data));
                        }
                        ServiceCommand::StartProcessMonitor => {
                            self.start_process_monitor();
                        }
                        ServiceCommand::ReloadAreaDefinitions(area_id) => {
                            // Reload definitions for the new area and update notes overlay
                            if area_id == 0 {
                                // Left raid area (fleet, etc.) - clear notes
                                let _ = self.overlay_tx.try_send(OverlayUpdate::NotesUpdated(NotesData::default()));
                            } else if let Some(bosses) = self.load_area_definitions(area_id) {
                                // Send notes from new area's boss definitions
                                self.send_notes_from_bosses(&bosses);
                                // Also load definitions into the session
                                let timer_start = self.load_area_timer_start(area_id);
                                let session_guard = self.shared.session.read().await;
                                if let Some(session) = session_guard.as_ref() {
                                    let mut session = session.write().await;
                                    session.load_boss_definitions(bosses, false);
                                    session.set_timer_start(area_id, timer_start);
                                }
                            } else {
                                // No definitions for this area - clear notes
                                let _ = self.overlay_tx.try_send(OverlayUpdate::NotesUpdated(NotesData::default()));
                            }
                        }
                        ServiceCommand::StartOperationTimer => {
                            // Only allow in live mode - not during historical replay
                            if self.shared.is_live_tailing.load(Ordering::SeqCst) {
                                let mut timer = self.shared.operation_timer.locked();
                                timer.manually_started = true;
                                timer.manually_stopped = false;
                                timer.start();
                                drop(timer);
                                self.emit_operation_timer_tick();
                            }
                        }
                        ServiceCommand::StopOperationTimer => {
                            // Only allow in live mode - not during historical replay
                            if self.shared.is_live_tailing.load(Ordering::SeqCst) {
                                let mut timer = self.shared.operation_timer.locked();
                                timer.stop();
                                drop(timer);
                                self.emit_operation_timer_tick();
                                // Also send final state to overlay
                                let data = self.shared.operation_timer.locked().to_overlay_data();
                                let _ = self.overlay_tx.try_send(OverlayUpdate::OperationTimerUpdated(data));
                            }
                        }
                        ServiceCommand::ResetOperationTimer => {
                            // Only allow in live mode - not during historical replay
                            if self.shared.is_live_tailing.load(Ordering::SeqCst) {
                                let mut timer = self.shared.operation_timer.locked();
                                timer.reset();
                                drop(timer);
                                self.emit_operation_timer_tick();
                                // Send cleared state to overlay
                                let data = self.shared.operation_timer.locked().to_overlay_data();
                                let _ = self.overlay_tx.try_send(OverlayUpdate::OperationTimerUpdated(data));
                            }
                        }
                        ServiceCommand::SetOperationTimerContext { operation_name } => {
                            let mut timer = self.shared.operation_timer.locked();
                            timer.operation_name = operation_name;
                        }
                        ServiceCommand::EmitOperationTimerTick => {
                            self.emit_operation_timer_tick();
                        }
                        ServiceCommand::ApplyAlacrityPreset => {
                            if let Some(session) = self.shared.session.read().await.as_ref() {
                                self.apply_alacrity_preset(&*session.read().await).await;
                            }
                        }
                        ServiceCommand::AutoSwitchProfile { role_name } => {
                            // Only auto-switch profiles during live tailing — historical
                            // file playback should never change the user's active profile.
                            if !self.shared.is_live_tailing.load(Ordering::SeqCst) {
                                debug!("Ignoring auto-switch profile for role {} (not live tailing)", role_name);
                                continue;
                            }
                            let config = self.shared.config.read().await;
                            if let Some(profile_name) = config.default_profile_per_role.get(&role_name) {
                                // Skip if this profile is already active
                                let already_active = config.active_profile_name.as_deref() == Some(profile_name);
                                // Skip if the profile no longer exists
                                let profile_exists = config.profiles.iter().any(|p| p.name == *profile_name);
                                let profile_name = profile_name.clone();
                                drop(config);

                                if !already_active && profile_exists {
                                    let overlay_state = self.app_handle.state::<crate::overlay::SharedOverlayState>();
                                    let service_handle = self.app_handle.state::<ServiceHandle>();
                                    match crate::commands::apply_profile(&profile_name, &service_handle, &overlay_state).await {
                                        Ok(()) => {
                                            info!("Auto-switched to profile '{}' for role {}", profile_name, role_name);
                                            let _ = self.app_handle.emit("profile-auto-switched", &profile_name);
                                        }
                                        Err(e) => warn!("Failed to auto-switch profile '{}': {e}", profile_name),
                                    }
                                }
                            }
                        }
                    }
                }
                // Fallback poll: check if the pending file has content.
                // Only fires when pending_file_interval is Some (i.e. a deferred
                // file switch is waiting). Works around OS-level event coalescing
                // where Create+Modify get merged into a single Create event.
                _ = async {
                    match self.pending_file_interval {
                        Some(ref mut interval) => interval.tick().await,
                        None => std::future::pending::<tokio::time::Instant>().await,
                    }
                } => {
                    self.check_pending_file().await;
                }
                // Operation timer tick: emit current time every second while running
                // Only emit in live mode - historical replays should not drive the timer
                _ = op_timer_interval.tick() => {
                    if self.shared.is_live_tailing.load(Ordering::SeqCst) {
                        let (is_running, timed_out) = self.shared.operation_timer.lock()
                            .map(|mut t| (t.is_running(), t.check_exit_timeout(OP_EXIT_TIMEOUT_SECS)))
                            .unwrap_or((false, false));
                        if is_running || timed_out {
                            self.emit_operation_timer_tick();
                        }
                    }
                }
            }
        }
    }
}
