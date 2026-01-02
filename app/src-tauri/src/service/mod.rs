//! Combat service - coordinates parsing, state management, and overlay updates
//!
//! Architecture:
//! - SharedState: Arc-wrapped state readable by Tauri commands (in crate::state)
//! - ServiceHandle: For sending commands + accessing shared state
//! - CombatService: Background task that processes commands and updates shared state
mod directory;
mod handler;

use crate::state::SharedState;
pub use crate::state::{RaidSlotRegistry, RegisteredPlayer};
use baras_core::directory_watcher;
pub use handler::*;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{RwLock, mpsc};

use baras_core::context::{AppConfig, AppConfigExt, DirectoryIndex, ParsingSession, resolve};
use baras_core::directory_watcher::DirectoryWatcher;
use baras_core::encounter::EncounterState;
use baras_core::encounter::summary::classify_encounter;
use baras_core::game_data::{Discipline, Role};
use baras_core::timers::FiredAlert;
use baras_core::{
    ActiveEffect, BossEncounterDefinition, DefinitionConfig, DefinitionSet, EffectCategory,
    EntityType, GameSignal, PlayerMetrics, Reader, SignalHandler,
};
use baras_overlay::{
    BossHealthData, ChallengeData, ChallengeEntry, Color, PersonalStats, PlayerContribution,
    PlayerRole, RaidEffect, RaidFrame, RaidFrameData, TimerData, TimerEntry,
};

use crate::audio::{AudioEvent, AudioSender, AudioService};

// ─────────────────────────────────────────────────────────────────────────────
// Parse Worker IPC
// ─────────────────────────────────────────────────────────────────────────────

use baras_core::encounter::summary::EncounterSummary;

/// Player info from parse worker subprocess.
#[derive(Debug, serde::Deserialize)]
struct WorkerPlayerInfo {
    name: String,
    class_name: String,
    discipline_name: String,
    entity_id: i64,
}

/// Area info from parse worker subprocess.
#[derive(Debug, serde::Deserialize)]
struct WorkerAreaInfo {
    area_name: String,
    area_id: i64,
    difficulty_name: String,
}

/// Output from the parse worker subprocess (matches parse-worker JSON output).
#[derive(Debug, serde::Deserialize)]
struct ParseWorkerOutput {
    end_pos: u64,
    event_count: usize,
    encounter_count: usize,
    encounters: Vec<EncounterSummary>,
    player: WorkerPlayerInfo,
    area: WorkerAreaInfo,
    elapsed_ms: u128,
}

/// Fallback to streaming parse if subprocess fails.
async fn fallback_streaming_parse(
    reader: &Reader,
    session: &Arc<RwLock<ParsingSession>>,
) {
    let timer = std::time::Instant::now();
    let mut session_guard = session.write().await;
    let session_date = session_guard.game_session_date.unwrap_or_default();
    let result = reader.read_log_file_streaming(session_date, |event| {
        session_guard.process_event(event);
    });

    if let Ok((end_pos, event_count)) = result {
        session_guard.current_byte = Some(end_pos);
        session_guard.finalize_session();
        session_guard.sync_timer_context();

        eprintln!(
            "[PARSE] Fallback streaming: {} events in {:.0}ms",
            event_count,
            timer.elapsed().as_millis()
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Service Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Messages sent to the service from Tauri commands
pub enum ServiceCommand {
    StartTailing(PathBuf),
    StopTailing,
    RefreshIndex,
    StartWatcher,
    Shutdown,
    FileDetected(PathBuf),
    FileRemoved(PathBuf),
    DirectoryChanged,
    /// Reload timer/boss definitions from disk and update active session
    ReloadTimerDefinitions,
    /// Reload effect definitions from disk and update active session
    ReloadEffectDefinitions,
    /// Open a historical file (pauses live tailing)
    OpenHistoricalFile(PathBuf),
    /// Resume live tailing (switch to newest file)
    ResumeLiveTailing,
}

/// Updates sent to the overlay system
#[derive(Debug, Clone)]
pub enum OverlayUpdate {
    CombatStarted,
    CombatEnded,
    /// Combat metrics for metric and personal overlays
    DataUpdated(CombatData),
    /// Effect data for raid frame overlay (HoTs, debuffs, etc.)
    EffectsUpdated(RaidFrameData),
    /// Boss health data for boss health overlay
    BossHealthUpdated(BossHealthData),
    /// Timer data for timer overlay
    TimersUpdated(TimerData),
    /// Effects countdown overlay data
    EffectsOverlayUpdated(baras_overlay::EffectsData),
    /// Clear all overlay data (sent when switching files)
    ClearAllData,
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

/// Signal handler that tracks combat state and triggers metrics updates
struct CombatSignalHandler {
    shared: Arc<SharedState>,
    trigger_tx: std::sync::mpsc::Sender<MetricsTrigger>,
    /// Channel for area load requests (event-driven, not polled)
    area_load_tx: std::sync::mpsc::Sender<i64>,
    /// Channel for frontend session updates (event-driven, not polled)
    session_event_tx: std::sync::mpsc::Sender<SessionEvent>,
    /// Channel for overlay updates (to clear overlays on combat end)
    overlay_tx: mpsc::Sender<OverlayUpdate>,
}

impl CombatSignalHandler {
    fn new(
        shared: Arc<SharedState>,
        trigger_tx: std::sync::mpsc::Sender<MetricsTrigger>,
        area_load_tx: std::sync::mpsc::Sender<i64>,
        session_event_tx: std::sync::mpsc::Sender<SessionEvent>,
        overlay_tx: mpsc::Sender<OverlayUpdate>,
    ) -> Self {
        Self {
            shared,
            trigger_tx,
            area_load_tx,
            session_event_tx,
            overlay_tx,
        }
    }
}

impl SignalHandler for CombatSignalHandler {
    fn handle_signal(&mut self, signal: &GameSignal, _encounter: Option<&baras_core::encounter::CombatEncounter>) {
        match signal {
            GameSignal::CombatStarted { .. } => {
                self.shared.in_combat.store(true, Ordering::SeqCst);
                let _ = self.trigger_tx.send(MetricsTrigger::CombatStarted);
                let _ = self.session_event_tx.send(SessionEvent::CombatStarted);
            }
            GameSignal::CombatEnded { .. } => {
                self.shared.in_combat.store(false, Ordering::SeqCst);
                let _ = self.trigger_tx.send(MetricsTrigger::CombatEnded);
                let _ = self.session_event_tx.send(SessionEvent::CombatEnded);
                // Clear boss health and timer overlays
                let _ = self.overlay_tx.try_send(OverlayUpdate::CombatEnded);
            }
            GameSignal::DisciplineChanged {
                entity_id,
                discipline_id,
                ..
            } => {
                // Update raid registry with discipline info for role icons
                if let Ok(mut registry) = self.shared.raid_registry.lock() {
                    registry.update_discipline(*entity_id, 0, *discipline_id);
                }
                // Notify frontend of player info change
                let _ = self.session_event_tx.send(SessionEvent::PlayerInitialized);
            }
            GameSignal::AreaEntered { area_id, .. } => {
                // Send area ID through channel for event-driven loading
                let current = self.shared.current_area_id.load(Ordering::SeqCst);
                if *area_id != current && *area_id != 0 {
                    self.shared
                        .current_area_id
                        .store(*area_id, Ordering::SeqCst);
                    let _ = self.area_load_tx.send(*area_id);
                    let _ = self.session_event_tx.send(SessionEvent::AreaChanged);
                }
            }
            _ => {}
        }
    }
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
    area_loader_handle: Option<tokio::task::JoinHandle<()>>,
    /// Effect definitions loaded at startup for overlay tracking
    definitions: DefinitionSet,
    /// Area index for lazy loading encounter definitions (area_id -> file path)
    area_index: Arc<baras_core::boss::AreaIndex>,
    /// Currently loaded area ID (0 = none)
    loaded_area_id: i64,
}

impl CombatService {
    /// Create a new combat service and return a handle to communicate with it
    pub fn new(
        app_handle: AppHandle,
        overlay_tx: mpsc::Sender<OverlayUpdate>,
        audio_tx: AudioSender,
        audio_rx: mpsc::Receiver<AudioEvent>,
    ) -> (Self, ServiceHandle) {
        let (cmd_tx, cmd_rx) = mpsc::channel(32);

        let config = AppConfig::load();
        let directory_index =
            DirectoryIndex::build_index(&PathBuf::from(&config.log_directory)).unwrap_or_default();

        // Load effect definitions from builtin and user directories
        let definitions = Self::load_effect_definitions(&app_handle);

        // Build area index for lazy loading (fast - only reads headers)
        let area_index = Arc::new(Self::build_area_index(&app_handle));

        let shared = Arc::new(SharedState::new(config, directory_index));

        // Spawn the audio service (shares audio settings with config)
        let user_sounds_dir = dirs::config_dir()
            .map(|p| p.join("baras").join("sounds"))
            .unwrap_or_else(|| PathBuf::from("."));
        // In release: bundled resources. In dev: fall back to source directory
        let bundled_sounds_dir = app_handle
            .path()
            .resolve("definitions/sounds", tauri::path::BaseDirectory::Resource)
            .ok()
            .filter(|p| p.exists())
            .unwrap_or_else(|| {
                // Dev fallback: relative to project root
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .join("core/definitions/sounds")
            });
        let audio_settings = Arc::new(tokio::sync::RwLock::new(
            shared.config.blocking_read().audio.clone(),
        ));
        let audio_service = AudioService::new(
            audio_rx,
            audio_settings,
            user_sounds_dir,
            bundled_sounds_dir,
        );
        tauri::async_runtime::spawn(audio_service.run());

        let service = Self {
            app_handle,
            shared: shared.clone(),
            overlay_tx,
            audio_tx,
            cmd_rx,
            cmd_tx: cmd_tx.clone(),
            tail_handle: None,
            directory_handle: None,
            metrics_handle: None,
            effects_handle: None,
            area_loader_handle: None,
            definitions,
            area_index,
            loaded_area_id: 0,
        };

        let handle = ServiceHandle { cmd_tx, shared };

        (service, handle)
    }

    /// Build area index from encounter definition files (lightweight - only reads headers)
    fn build_area_index(app_handle: &AppHandle) -> baras_core::boss::AreaIndex {
        use baras_core::boss::build_area_index;

        // Bundled definitions: shipped with the app in resources
        let bundled_dir = app_handle
            .path()
            .resolve(
                "definitions/encounters",
                tauri::path::BaseDirectory::Resource,
            )
            .ok();

        // Custom definitions: user's config directory
        let custom_dir = dirs::config_dir().map(|p| p.join("baras").join("encounters"));

        let mut index = baras_core::boss::AreaIndex::new();

        // Build index from bundled directory
        if let Some(ref path) = bundled_dir
            && path.exists()
            && let Ok(area_index) = build_area_index(path) {
                index.extend(area_index);
            }

        // Build index from custom directory (can override bundled)
        if let Some(ref path) = custom_dir
            && path.exists()
            && let Ok(area_index) = build_area_index(path) {
                index.extend(area_index);
            }

        index
    }

    /// Load boss definitions for a specific area, merging with custom overlays
    fn load_area_definitions(&self, area_id: i64) -> Option<Vec<BossEncounterDefinition>> {
        use baras_core::boss::load_bosses_with_custom;

        let entry = self.area_index.get(&area_id)?;

        // User custom directory for overlay files
        let user_dir = dirs::config_dir().map(|p| p.join("baras").join("encounters"));

        load_bosses_with_custom(&entry.file_path, user_dir.as_deref()).ok()
    }

    /// Get the path to the timer preferences file
    fn timer_preferences_path() -> Option<std::path::PathBuf> {
        dirs::config_dir().map(|p| p.join("baras").join("timer_preferences.toml"))
    }

    /// Load effect definitions from bundled and user config directories
    /// Loading order (later overrides earlier):
    /// 1. Bundled definitions (shipped with app)
    /// 2. User defaults (~/.config/baras/effects/defaults/)
    /// 3. User root files (~/.config/baras/effects/*.toml, custom.toml last)
    fn load_effect_definitions(app_handle: &AppHandle) -> DefinitionSet {
        // Bundled definitions: shipped with the app in resources
        let bundled_dir = app_handle
            .path()
            .resolve("definitions/effects", tauri::path::BaseDirectory::Resource)
            .ok();

        // User config directories
        let effects_dir = dirs::config_dir().map(|p| p.join("baras").join("effects"));
        let defaults_dir = effects_dir.as_ref().map(|p| p.join("defaults"));

        // Load definitions from TOML files
        let mut set = DefinitionSet::new();

        // 1. Load from bundled directory first (lowest priority)
        if let Some(ref path) = bundled_dir
            && path.exists()
        {
            Self::load_definitions_from_dir(&mut set, path, "bundled", false);
        }

        // 2. Load from user defaults directory (user-editable base definitions)
        if let Some(ref path) = defaults_dir
            && path.exists()
        {
            Self::load_definitions_from_dir(&mut set, path, "defaults", true);
        }

        // 3. Load from user root directory (highest priority, custom.toml loaded last)
        if let Some(ref path) = effects_dir
            && path.exists()
        {
            Self::load_definitions_from_dir(&mut set, path, "custom", true);
        }

        set
    }

    /// Load effect definitions from a directory of TOML files
    /// custom.toml is always loaded last so user overrides take precedence
    fn load_definitions_from_dir(
        set: &mut DefinitionSet,
        dir: &std::path::Path,
        _source: &str,
        overwrite: bool,
    ) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };

        // Collect and sort files, putting custom.toml last
        let mut files: Vec<_> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|ext| ext == "toml"))
            .collect();

        files.sort_by(|a, b| {
            let a_is_custom = a.file_name().is_some_and(|n| n == "custom.toml");
            let b_is_custom = b.file_name().is_some_and(|n| n == "custom.toml");
            match (a_is_custom, b_is_custom) {
                (true, false) => std::cmp::Ordering::Greater,
                (false, true) => std::cmp::Ordering::Less,
                _ => a.cmp(b),
            }
        });

        for path in files {
            if let Ok(contents) = std::fs::read_to_string(&path)
                && let Ok(config) = toml::from_str::<DefinitionConfig>(&contents) {
                    set.add_definitions(config.effects, overwrite);
                }
        }
    }

    /// Run the service event loop
    pub async fn run(mut self) {
        self.start_watcher().await;

        loop {
            let Some(cmd) = self.cmd_rx.recv().await else {
                break;
            };

            match cmd {
                ServiceCommand::StartTailing(path) => {
                    self.start_tailing(path).await;
                }
                ServiceCommand::StopTailing => {
                    self.stop_tailing().await;
                }
                ServiceCommand::RefreshIndex => {
                    self.refresh_index().await;
                }
                ServiceCommand::Shutdown => {
                    self.stop_tailing().await;
                    break;
                }
                ServiceCommand::StartWatcher => {
                    self.start_watcher().await;
                }
                ServiceCommand::FileDetected(path) => {
                    self.file_detected(path).await;
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
                ServiceCommand::OpenHistoricalFile(path) => {
                    // Pause live tailing and open the historical file
                    self.shared.is_live_tailing.store(false, Ordering::SeqCst);
                    let _ = self
                        .app_handle
                        .emit("session-updated", "TailingModeChanged");
                    self.start_tailing(path).await;
                }
                ServiceCommand::ResumeLiveTailing => {
                    // Resume live tailing and switch to newest file
                    self.shared.is_live_tailing.store(true, Ordering::SeqCst);
                    let _ = self
                        .app_handle
                        .emit("session-updated", "TailingModeChanged");
                    let newest = {
                        let index = self.shared.directory_index.read().await;
                        index.newest_file().map(|f| f.path.clone())
                    };
                    if let Some(path) = newest {
                        self.start_tailing(path).await;
                    }
                }
            }
        }
    }

    /// Reload effect definitions from disk and update the active session
    async fn reload_effect_definitions(&mut self) {
        self.definitions = Self::load_effect_definitions(&self.app_handle);

        if let Some(session) = self.shared.session.read().await.as_ref() {
            let session = session.read().await;
            session.set_definitions(self.definitions.clone());
        }
    }

    /// Reload timer and boss definitions from disk and update the active session
    async fn reload_timer_definitions(&mut self) {
        self.area_index = Arc::new(Self::build_area_index(&self.app_handle));

        let current_area = self.shared.current_area_id.load(Ordering::SeqCst);
        if current_area != 0
            && let Some(bosses) = self.load_area_definitions(current_area)
            && let Some(session) = self.shared.session.read().await.as_ref()
        {
            let mut session = session.write().await;
            session.load_boss_definitions(bosses);
        }
    }

    async fn on_directory_changed(&mut self) {
        // Stop existing watcher
        if let Some(handle) = self.directory_handle.take() {
            self.shared.watching.store(false, Ordering::SeqCst);
            handle.abort();
            let _ = handle.await;
        }

        // Stop any active tailing
        self.stop_tailing().await;

        // Start new watcher (reads directory from config)
        self.start_watcher().await;
    }
    async fn file_detected(&mut self, path: PathBuf) {
        // Always update the index
        {
            let mut index = self.shared.directory_index.write().await;
            index.add_file(&path);
        }

        // Notify frontend that file list changed
        let _ = self.app_handle.emit("log-files-changed", ());

        // Only auto-switch if in live tailing mode
        if !self.shared.is_live_tailing.load(Ordering::SeqCst) {
            return;
        }

        let should_switch = {
            let index = self.shared.directory_index.read().await;
            index.newest_file().map(|f| f.path == path).unwrap_or(false)
        };

        if should_switch {
            // Method calls stop_tailing at beginning so won't create duplicate tasks
            self.start_tailing(path).await;
        }
    }

    async fn file_removed(&mut self, path: PathBuf) {
        let was_active = {
            let session_guard = self.shared.session.read().await;
            if let Some(session) = session_guard.as_ref() {
                let s = session.read().await;
                s.active_file.as_ref().map(|p| p == &path).unwrap_or(false)
            } else {
                false
            }
        };
        // Update index
        {
            let mut index = self.shared.directory_index.write().await;
            index.remove_file(&path);
        }

        // Notify frontend that file list changed
        let _ = self.app_handle.emit("log-files-changed", ());
        // Check if we need to switch files

        if was_active {
            self.stop_tailing().await;
            // Optionally switch to next newest
            let next = {
                let index = self.shared.directory_index.read().await;
                index.newest_file().map(|f| f.path.clone())
            };
            if let Some(next_path) = next {
                self.start_tailing(next_path).await;
            }
        }
    }

    async fn start_watcher(&mut self) {
        // Only read from what is stored in config
        let dir = {
            let config = self.shared.config.read().await;
            PathBuf::from(&config.log_directory)
        };

        // Guard against invalid input
        if !dir.exists() || !dir.is_dir() {
            return;
        }

        // Build initial index
        if let Ok((index, newest)) = directory_watcher::build_index(&dir) {
            {
                let mut index_guard = self.shared.directory_index.write().await;
                *index_guard = index;
            }

            // Auto-load newest file if available
            if let Some(ref newest_path) = newest {
                self.start_tailing(newest_path.clone()).await;
            }
        }

        let Ok(mut watcher) = DirectoryWatcher::new(&dir) else {
            self.shared.watching.store(false, Ordering::SeqCst);
            return;
        };

        // Clone the command sender so watcher can send back to service
        let cmd_tx = self.cmd_tx.clone();
        let shared = self.shared.clone();

        let handle = tokio::spawn(async move {
            while let Some(event) = watcher.next_event().await {
                if let Some(cmd) = directory::translate_event(event)
                    && cmd_tx.send(cmd).await.is_err()
                {
                    break; // Service shut down
                }
            }
            // Watcher stopped
            shared.watching.store(false, Ordering::SeqCst);
        });

        self.directory_handle = Some(handle);
        self.shared.watching.store(true, Ordering::SeqCst);
        let _ = self.app_handle.emit("session-updated", "WatcherStarted");
    }

    async fn start_tailing(&mut self, path: PathBuf) {
        self.stop_tailing().await;

        // Clear all overlay data when switching files
        let _ = self.overlay_tx.try_send(OverlayUpdate::ClearAllData);

        // Clear raid registry when switching files (new session = fresh state)
        if let Ok(mut registry) = self.shared.raid_registry.lock() {
            registry.clear();
        }

        // Create trigger channel for signal-driven metrics updates
        let (trigger_tx, trigger_rx) = std::sync::mpsc::channel::<MetricsTrigger>();
        // Create channel for event-driven area loading (replaces polling)
        let (area_load_tx, area_load_rx) = std::sync::mpsc::channel::<i64>();
        // Create channel for frontend session events (replaces polling)
        let (session_event_tx, session_event_rx) = std::sync::mpsc::channel::<SessionEvent>();

        let mut session = ParsingSession::new(path.clone(), self.definitions.clone());

        // Load timer preferences into the session's timer manager (Live mode only)
        if let Some(prefs_path) = Self::timer_preferences_path() {
            if let Some(timer_mgr) = session.timer_manager() {
                if let Ok(mut mgr) = timer_mgr.lock()
                    && let Err(e) = mgr.load_preferences(&prefs_path) {
                        eprintln!("Warning: Failed to load timer preferences: {}", e);
                    }
            }
        }

        // Timer/boss definitions are now lazy-loaded when AreaEntered signal fires
        // Reset area tracking for new session
        self.loaded_area_id = 0;
        self.shared.current_area_id.store(0, Ordering::SeqCst);

        // Add signal handler that triggers metrics on combat state changes
        let handler = CombatSignalHandler::new(
            self.shared.clone(),
            trigger_tx.clone(),
            area_load_tx,
            session_event_tx,
            self.overlay_tx.clone(),
        );
        session.add_signal_handler(Box::new(handler));

        // Spawn task to emit session events to frontend (event-driven, not polled)
        let app_handle = self.app_handle.clone();
        tokio::spawn(async move {
            loop {
                let event = match tokio::task::spawn_blocking({
                    let rx = session_event_rx.recv();
                    move || rx
                })
                .await
                {
                    Ok(Ok(e)) => e,
                    Ok(Err(_)) => break, // Channel closed
                    Err(_) => break,     // Task cancelled
                };
                // Emit event to frontend - they can fetch fresh data
                let _ = app_handle.emit("session-updated", format!("{:?}", event));
            }
        });

        let session = Arc::new(RwLock::new(session));

        // Update shared state
        *self.shared.session.write().await = Some(session.clone());

        // Notify frontend of active file change
        let _ = self
            .app_handle
            .emit("active-file-changed", path.to_string_lossy().to_string());

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
        let encounters_dir = baras_core::storage::encounters_dir(&session_id)
            .unwrap_or_else(|_| PathBuf::from("/tmp/baras-encounters"));

        // Spawn parse worker subprocess
        // Check multiple locations: bundled sidecar (with target triple), next to exe, fallback to PATH
        let worker_path = std::env::current_exe()
            .ok()
            .and_then(|exe| {
                let dir = exe.parent()?;
                // Try sidecar name with target triple first (Tauri bundle format), then plain name
                let candidates = [
                    dir.join(format!("baras-parse-worker-{}-unknown-linux-gnu", std::env::consts::ARCH)),
                    dir.join("baras-parse-worker"),
                ];
                candidates.into_iter().find(|p| p.exists())
            })
            .unwrap_or_else(|| PathBuf::from("baras-parse-worker"));

        eprintln!("[PARSE] Using worker: {:?}", worker_path);

        let output = std::process::Command::new(&worker_path)
            .arg(&path)
            .arg(&session_id)
            .arg(&encounters_dir)
            .output();

        match output {
            Ok(output) if output.status.success() => {
                // Parse JSON result from subprocess
                let json_result = String::from_utf8(output.stdout)
                    .map_err(|e| format!("Invalid UTF-8: {}", e))
                    .and_then(|result| {
                        serde_json::from_str::<ParseWorkerOutput>(&result)
                            .map_err(|e| format!("JSON parse error: {} (input: {})", e, &result[..result.len().min(500)]))
                    });

                match json_result {
                    Ok(parse_result) => {
                        let mut session_guard = session.write().await;
                        session_guard.current_byte = Some(parse_result.end_pos);

                        // Import encounter summaries and session metadata from subprocess
                        if let Some(cache) = &mut session_guard.session_cache {
                            for summary in parse_result.encounters {
                                cache.encounter_history.add(summary);
                            }

                            // Import player info
                            cache.player.name = baras_core::context::intern(&parse_result.player.name);
                            cache.player.id = parse_result.player.entity_id;
                            cache.player.class_name = parse_result.player.class_name.clone();
                            cache.player.discipline_name = parse_result.player.discipline_name.clone();
                            cache.player_initialized = true;

                            // Import area info
                            cache.current_area.area_name = parse_result.area.area_name.clone();
                            cache.current_area.area_id = parse_result.area.area_id;
                            cache.current_area.difficulty_name = parse_result.area.difficulty_name.clone();
                        }

                        // Enable live parquet writing (continues from where subprocess left off)
                        session_guard.enable_live_parquet(
                            encounters_dir.clone(),
                            parse_result.encounter_count as u32,
                        );

                        session_guard.finalize_session();
                        session_guard.sync_timer_context();
                        drop(session_guard);

                        eprintln!(
                            "[PARSE] Subprocess parsed {} events ({} encounters) in {}ms",
                            parse_result.event_count,
                            parse_result.encounter_count,
                            parse_result.elapsed_ms
                        );

                        // Notify frontend to refresh session info
                        let _ = self.app_handle.emit("session-updated", "FileLoaded");
                    }
                    Err(e) => {
                        eprintln!("[PARSE] Subprocess output parse failed: {}", e);
                        fallback_streaming_parse(&reader, &session).await;
                    }
                }
            }
            Ok(output) => {
                eprintln!(
                    "[PARSE] Subprocess failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                // Fallback to streaming parse in main process
                fallback_streaming_parse(&reader, &session).await;
            }
            Err(e) => {
                eprintln!("[PARSE] Failed to spawn subprocess: {}", e);
                // Fallback to streaming parse in main process
                fallback_streaming_parse(&reader, &session).await;
            }
        }

        eprintln!("[PARSE] Total time: {:.0}ms", timer.elapsed().as_millis());

        // Trigger initial metrics send after file processing
        let _ = trigger_tx.send(MetricsTrigger::InitialLoad);

        // Enable live mode for effect/timer tracking (skip historical events)
        {
            let session_guard = session.read().await;
            session_guard.set_effect_live_mode(true);
            session_guard.set_timer_live_mode(true);
        }

        // Spawn the tail task to watch for new lines
        let tail_handle = tokio::spawn(async move {
            let _ = reader.tail_log_file().await;
        });

        // Spawn signal-driven metrics task
        let shared = self.shared.clone();
        let overlay_tx = self.overlay_tx.clone();
        let metrics_handle = tokio::spawn(async move {
            loop {
                // Check for triggers (non-blocking with timeout to allow task cancellation)
                let trigger = tokio::task::spawn_blocking({
                    let trigger_rx_timeout =
                        trigger_rx.recv_timeout(std::time::Duration::from_millis(100));
                    move || trigger_rx_timeout
                })
                .await;

                let trigger = match trigger {
                    Ok(Ok(t)) => t,
                    Ok(Err(std::sync::mpsc::RecvTimeoutError::Timeout)) => continue,
                    Ok(Err(std::sync::mpsc::RecvTimeoutError::Disconnected)) => break,
                    Err(_) => break, // JoinError
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
                        tokio::time::sleep(std::time::Duration::from_millis(250)).await;

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
        let effects_handle = tokio::spawn(async move {
            // Track previous state to avoid redundant updates
            let mut last_raid_effect_count: usize = 0;
            let mut last_effects_count: usize = 0;

            loop {
                // Check which overlays are active to determine sleep interval
                let raid_active = shared.raid_overlay_active.load(Ordering::Relaxed);
                let boss_active = shared.boss_health_overlay_active.load(Ordering::Relaxed);
                let timer_active = shared.timer_overlay_active.load(Ordering::Relaxed);
                let effects_active = shared.effects_overlay_active.load(Ordering::Relaxed);
                let in_combat = shared.in_combat.load(Ordering::Relaxed);
                let is_live = shared.is_live_tailing.load(Ordering::SeqCst);

                // Determine if any work needs to be done
                let any_overlay_active =
                    raid_active || boss_active || timer_active || effects_active;
                let needs_audio = is_live && (in_combat || raid_active || effects_active);

                // Adaptive sleep: fast when active, slow when idle
                // 100ms = 10 updates/sec for smooth countdowns (visual-change detection skips redundant renders)
                let sleep_ms = if any_overlay_active || needs_audio {
                    100
                } else {
                    500
                };
                tokio::time::sleep(std::time::Duration::from_millis(sleep_ms)).await;

                // Skip processing if nothing needs updating
                if !any_overlay_active && !needs_audio {
                    continue;
                }

                // Raid frames: only send if there are effects or effects just cleared
                if raid_active
                    && let Some(data) = build_raid_frame_data(&shared).await {
                        let effect_count: usize = data.frames.iter().map(|f| f.effects.len()).sum();
                        // Only send if effects exist, or if we need to clear (was non-zero, now zero)
                        if effect_count > 0 || last_raid_effect_count > 0 {
                            let _ = overlay_tx.try_send(OverlayUpdate::EffectsUpdated(data));
                        }
                        last_raid_effect_count = effect_count;
                    }

                // Effects countdown: only send if there are effects or effects just cleared
                if effects_active
                    && let Some(data) = build_effects_overlay_data(&shared).await {
                        let effect_count = data.entries.len();
                        // Only send if effects exist, or if we need to clear (was non-zero, now zero)
                        if effect_count > 0 || last_effects_count > 0 {
                            let _ = overlay_tx.try_send(OverlayUpdate::EffectsOverlayUpdated(data));
                        }
                        last_effects_count = effect_count;
                    }

                // Effect audio: process in live mode
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
                    && let Some(data) = build_boss_health_data(&shared).await
                {
                    let _ = overlay_tx.try_send(OverlayUpdate::BossHealthUpdated(data));
                }

                // Timers + Audio: always poll when in live mode (alerts can fire at combat end)
                if shared.is_live_tailing.load(Ordering::SeqCst) {
                    // Process timer audio and get timer data
                    if let Some((data, countdowns, alerts)) =
                        build_timer_data_with_audio(&shared).await
                    {
                        // Send timer overlay data (only when in combat)
                        if in_combat && timer_active {
                            let _ = overlay_tx.try_send(OverlayUpdate::TimersUpdated(data));
                        }

                        // Send countdown audio events (only when in combat)
                        if in_combat {
                            for (name, seconds, voice_pack) in countdowns {
                                let _ = audio_tx.try_send(AudioEvent::Countdown {
                                    timer_name: name,
                                    seconds,
                                    voice_pack,
                                });
                            }
                        }

                        // Send alert audio events (only if audio_enabled for that alert)
                        for alert in alerts {
                            if alert.audio_enabled {
                                let _ = audio_tx.try_send(AudioEvent::Alert {
                                    text: alert.text,
                                    custom_sound: alert.audio_file,
                                });
                            }
                        }
                    }
                }
            }
        });

        // Spawn area loader task for lazy loading timer/boss definitions
        // Now event-driven via channel instead of polling
        let shared = self.shared.clone();
        let area_index = self.area_index.clone();
        let is_live = self.shared.is_live_tailing.load(Ordering::SeqCst);
        let user_encounters_dir = dirs::config_dir().map(|p| p.join("baras").join("encounters"));
        let area_loader_handle = if is_live {
            Some(tokio::spawn(async move {
                let mut loaded_area_id: i64 = 0;

                // Wait for area IDs via channel (event-driven, no polling)
                loop {
                    let area_id = match tokio::task::spawn_blocking({
                        let rx_recv = area_load_rx.recv();
                        move || rx_recv
                    })
                    .await
                    {
                        Ok(Ok(id)) => id,
                        Ok(Err(_)) => break, // Channel closed
                        Err(_) => break,     // Task cancelled
                    };

                    // Skip if already loaded this area
                    if area_id == loaded_area_id {
                        continue;
                    }

                    // Load definitions for this area (with custom overlay merging)
                    if let Some(entry) = area_index.get(&area_id) {
                        use baras_core::boss::load_bosses_with_custom;

                        if let Ok(bosses) = load_bosses_with_custom(
                            &entry.file_path,
                            user_encounters_dir.as_deref(),
                        ) {
                            if let Some(session_arc) = &*shared.session.read().await {
                                let mut session = session_arc.write().await;
                                session.load_boss_definitions(bosses);
                            }
                            loaded_area_id = area_id;
                        }
                    }
                }
            }))
        } else {
            None
        };

        self.tail_handle = Some(tail_handle);
        self.metrics_handle = Some(metrics_handle);
        self.effects_handle = Some(effects_handle);
        self.area_loader_handle = area_loader_handle;
    }

    async fn stop_tailing(&mut self) {
        // Reset combat state
        self.shared.in_combat.store(false, Ordering::SeqCst);

        // Cancel area loader task
        // Note: Don't await - the task uses sync recv() which can't be interrupted
        if let Some(handle) = self.area_loader_handle.take() {
            handle.abort();
        }

        // Cancel effects task
        if let Some(handle) = self.effects_handle.take() {
            handle.abort();
            let _ = handle.await;
        }

        // Cancel metrics task
        if let Some(handle) = self.metrics_handle.take() {
            handle.abort();
            let _ = handle.await;
        }

        // Cancel tail task
        if let Some(handle) = self.tail_handle.take() {
            handle.abort();
            let _ = handle.await;
        }

        *self.shared.session.write().await = None;
    }

    async fn refresh_index(&mut self) {
        let log_dir = self.shared.config.read().await.log_directory.clone();
        if let Ok(index) = DirectoryIndex::build_index(&PathBuf::from(&log_dir)) {
            *self.shared.directory_index.write().await = index;
        }
    }
}

/// Calculate unified combat data for all overlays
async fn calculate_combat_data(shared: &Arc<SharedState>) -> Option<CombatData> {
    let session_guard = shared.session.read().await;
    let session = session_guard.as_ref()?;
    let session = session.read().await;
    let cache = session.session_cache.as_ref()?;

    // Get player info for class/discipline and entity ID
    let player_info = &cache.player;
    let class_discipline =
        if !player_info.class_name.is_empty() && !player_info.discipline_name.is_empty() {
            Some(format!(
                "{} / {}",
                player_info.class_name, player_info.discipline_name
            ))
        } else if !player_info.class_name.is_empty() {
            Some(player_info.class_name.clone())
        } else {
            None
        };
    let player_entity_id = player_info.id;

    // Get encounter info
    let encounter = cache.last_combat_encounter()?;
    let encounter_count = cache
        .encounters()
        .filter(|e| e.state != EncounterState::NotStarted)
        .map(|e| e.id + 1)
        .max()
        .unwrap_or(0) as usize;
    let encounter_time_secs = encounter.duration_seconds().unwrap_or(0) as u64;

    // Classify the encounter to get phase type and boss info
    let (phase_type, boss_info) = classify_encounter(encounter, &cache.current_area);

    // Generate encounter name - if there's a boss use that, otherwise use phase type
    let encounter_name = if let Some(boss) = boss_info {
        Some(boss.boss.to_string())
    } else {
        // Use phase type for trash/non-boss encounters
        Some(format!("{:?}", phase_type))
    };

    // Get difficulty from area info, fallback to phase type name for non-instanced content
    let difficulty = if !cache.current_area.difficulty_name.is_empty() {
        Some(cache.current_area.difficulty_name.clone())
    } else {
        Some(format!("{:?}", phase_type))
    };

    // Calculate metrics for all players
    let entity_metrics = encounter.calculate_entity_metrics()?;
    let metrics: Vec<PlayerMetrics> = entity_metrics
        .into_iter()
        .filter(|m| m.entity_type != EntityType::Npc)
        .map(|m| m.to_player_metrics())
        .collect();

    // Build challenge data from encounter's tracker (persists with encounter, not boss state)
    let challenges = if encounter.challenge_tracker.is_active() {
        let boss_name = encounter
            .active_boss_idx()
            .and_then(|idx| encounter.boss_definitions().get(idx).map(|def| def.name.clone()));
        let overall_duration = encounter.combat_time_secs.max(1.0);
        let current_time = chrono::Local::now().naive_local();

        let entries: Vec<ChallengeEntry> = encounter
            .challenge_tracker
            .snapshot_live(current_time)
            .into_iter()
            .map(|val| {
                // Use the challenge's own duration (phase-scoped or total)
                let challenge_duration = val.duration_secs.max(1.0);

                // Build per-player breakdown, sorted by value descending
                let mut by_player: Vec<PlayerContribution> = val
                    .by_player
                    .iter()
                    .filter_map(|(&entity_id, &value)| {
                        // Resolve player name from encounter
                        let name = encounter
                            .players
                            .get(&entity_id)
                            .map(|p| resolve(p.name).to_string())
                            .unwrap_or_else(|| format!("Player {}", entity_id));

                        let percent = if val.value > 0 {
                            (value as f32 / val.value as f32) * 100.0
                        } else {
                            0.0
                        };

                        Some(PlayerContribution {
                            entity_id,
                            name,
                            value,
                            percent,
                            per_second: if value > 0 {
                                Some(value as f32 / challenge_duration)
                            } else {
                                None
                            },
                        })
                    })
                    .collect();

                // Sort by value descending (top contributors first)
                by_player.sort_by(|a, b| b.value.cmp(&a.value));

                ChallengeEntry {
                    name: val.name,
                    value: val.value,
                    event_count: val.event_count,
                    per_second: if val.value > 0 {
                        Some(val.value as f32 / challenge_duration)
                    } else {
                        None
                    },
                    by_player,
                    duration_secs: challenge_duration,
                    // Display settings from challenge definition
                    enabled: val.enabled,
                    color: val.color.map(|c| Color::from_rgba8(c[0], c[1], c[2], c[3])),
                    columns: val.columns,
                }
            })
            .collect();

        Some(ChallengeData {
            entries,
            boss_name,
            duration_secs: overall_duration,
            phase_durations: encounter.challenge_tracker.phase_durations().clone(),
        })
    } else {
        None
    };

    // Get phase info from encounter's boss state
    // Look up the phase display name from the boss definition
    let current_phase = encounter.current_phase.as_ref().and_then(|phase_id| {
        encounter.active_boss_definition().and_then(|def| {
            def.phases
                .iter()
                .find(|p| &p.id == phase_id)
                .map(|p| p.name.clone())
        })
    });
    let phase_time_secs = encounter
        .phase_started_at
        .map(|start| {
            let now = chrono::Local::now().naive_local();
            (now - start).num_milliseconds() as f32 / 1000.0
        })
        .unwrap_or(0.0);

    Some(CombatData {
        metrics,
        player_entity_id,
        encounter_time_secs,
        encounter_count,
        class_discipline,
        encounter_name,
        difficulty,
        challenges,
        current_phase,
        phase_time_secs,
    })
}

/// Build raid frame data from the effect tracker and registry
///
/// Uses RaidSlotRegistry to maintain stable player positions.
/// Players are registered ONLY when the local player applies a NEW effect to them
/// (via the new_targets queue), not on every tick.
async fn build_raid_frame_data(shared: &Arc<SharedState>) -> Option<RaidFrameData> {
    let session_guard = shared.session.read().await;
    let session = session_guard.as_ref()?;
    let session = session.read().await;

    // Get lag offset from config
    let lag_offset_ms = {
        let config = shared.config.read().await;
        config.overlay_settings.effect_lag_offset_ms
    };

    // Get effect tracker (Live mode only)
    let effect_tracker = session.effect_tracker()?;
    let Ok(mut tracker) = effect_tracker.lock() else {
        return None;
    };

    // Get local player ID for is_self flag
    let local_player_id = session
        .session_cache
        .as_ref()
        .map(|c| c.player.id)
        .unwrap_or(0);

    // Lock registry
    let Ok(mut registry) = shared.raid_registry.lock() else {
        return None;
    };

    // Process new targets queue - these are entities that JUST received an effect from local player
    // The registry handles duplicate rejection via try_register
    for target in tracker.take_new_targets() {
        let name = resolve(target.name).to_string();
        registry.try_register(target.entity_id, name);
    }

    // Group effects by target for registered players only
    let mut effects_by_target: std::collections::HashMap<i64, Vec<RaidEffect>> =
        std::collections::HashMap::new();

    for effect in tracker.active_effects() {
        // Skip effects not marked for raid frames or already removed
        if !effect.show_on_raid_frames || effect.removed_at.is_some() {
            continue;
        }

        let target_id = effect.target_entity_id;

        // Only group effects for already-registered players
        if registry.is_registered(target_id) {
            effects_by_target
                .entry(target_id)
                .or_default()
                .push(convert_to_raid_effect(effect, lag_offset_ms));
        }
    }

    // Build frames from registry (stable slot order)
    let max_slots = registry.max_slots();
    let mut frames = Vec::with_capacity(max_slots as usize);

    for slot in 0..max_slots {
        if let Some(player) = registry.get_player(slot) {
            let effects = effects_by_target
                .remove(&player.entity_id)
                .unwrap_or_default();

            // Map discipline to role (defaults to DPS if unknown)
            let role = player
                .discipline_id
                .and_then(Discipline::from_guid)
                .map(|d| match d.role() {
                    Role::Tank => PlayerRole::Tank,
                    Role::Healer => PlayerRole::Healer,
                    Role::Dps => PlayerRole::Dps,
                })
                .unwrap_or(PlayerRole::Dps);

            frames.push(RaidFrame {
                slot,
                player_id: Some(player.entity_id),
                name: player.name.clone(),
                hp_percent: 1.0,
                role,
                effects,
                is_self: player.entity_id == local_player_id,
            });
        }
    }

    Some(RaidFrameData { frames })
}

/// Build boss health data from the current encounter
async fn build_boss_health_data(shared: &Arc<SharedState>) -> Option<BossHealthData> {
    let session_guard = shared.session.read().await;
    let session = session_guard.as_ref()?;
    let session = session.read().await;
    let cache = session.session_cache.as_ref()?;

    // If not in combat, send empty data to clear overlay (if auto_hide enabled)
    let in_combat = shared.in_combat.load(Ordering::SeqCst);
    if !in_combat {
        return Some(BossHealthData::default());
    }

    let entries = cache.get_boss_health();
    Some(BossHealthData { entries })
}

/// Build timer data with audio events (countdowns and alerts)
///
/// Returns (TimerData, countdowns_to_announce, fired_alerts)
/// Countdowns are (timer_name, seconds, voice_pack)
async fn build_timer_data_with_audio(
    shared: &Arc<SharedState>,
) -> Option<(TimerData, Vec<(String, u8, String)>, Vec<FiredAlert>)> {
    let session_guard = shared.session.read().await;
    let session = session_guard.as_ref()?;
    let session = session.read().await;

    // Get active timers from timer manager (Live mode only, mutable for countdown checking)
    let timer_mgr = session.timer_manager()?;
    let mut timer_mgr = timer_mgr.lock().ok()?;

    // Always take alerts (even after combat ends, timer expirations need to play)
    let mut alerts = timer_mgr.take_fired_alerts();

    // Check for audio offset alerts (early warning sounds before timer expires)
    let offset_alerts = timer_mgr.check_audio_offsets();
    alerts.extend(offset_alerts);

    // If not in combat, return only alerts (no countdown checks)
    let in_combat = shared.in_combat.load(Ordering::SeqCst);
    if !in_combat {
        return Some((TimerData::default(), Vec::new(), alerts));
    }

    // Check for countdowns to announce (uses realtime internally)
    let countdowns = timer_mgr.check_all_countdowns();

    // Convert active timers to TimerEntry format (using realtime for display consistency)
    let entries: Vec<TimerEntry> = timer_mgr
        .active_timers()
        .iter()
        .filter_map(|timer| {
            let remaining = timer.remaining_secs_realtime();
            if remaining <= 0.0 {
                return None;
            }
            Some(TimerEntry {
                name: timer.name.clone(),
                remaining_secs: remaining,
                total_secs: timer.duration.as_secs_f32(),
                color: timer.color,
            })
        })
        .collect();

    Some((TimerData { entries }, countdowns, alerts))
}

/// Build effects countdown overlay data from active effects
async fn build_effects_overlay_data(
    shared: &Arc<SharedState>,
) -> Option<baras_overlay::EffectsData> {
    use baras_overlay::EffectEntry;

    // Get lag offset from config first (before locking tracker)
    let lag_offset_ms = {
        let config = shared.config.read().await;
        config.overlay_settings.effect_lag_offset_ms
    };

    let session_guard = shared.session.read().await;
    let session = session_guard.as_ref()?;
    let session = session.read().await;

    // Get effect tracker (Live mode only)
    let effect_tracker = session.effect_tracker()?;
    let tracker = effect_tracker.lock().ok()?;

    // Filter to effects marked for effects overlay and convert to entries
    // Use system time (like raid frames) so countdown ticks smoothly outside combat
    let entries: Vec<EffectEntry> = tracker
        .active_effects()
        .filter(|e| e.show_on_effects_overlay && e.removed_at.is_none())
        .filter_map(|effect| {
            let duration = effect.duration?;
            let total = duration.as_secs_f32();

            // Calculate remaining using system time (same logic as convert_to_raid_effect)
            let time_since_processing = effect.applied_instant.elapsed();
            let system_time_at_processing = chrono::Local::now().naive_local()
                - chrono::Duration::milliseconds(time_since_processing.as_millis() as i64);
            let lag_ms = system_time_at_processing
                .signed_duration_since(effect.last_refreshed_at)
                .num_milliseconds()
                .max(0) as u64;
            let total_lag_ms = (lag_ms as i64 + lag_offset_ms as i64).max(0) as u64;
            let total_lag = std::time::Duration::from_millis(total_lag_ms);

            // Compensated expiry instant
            let compensated_expiry = effect.applied_instant + duration - total_lag.min(duration);
            let remaining = compensated_expiry.saturating_duration_since(std::time::Instant::now());
            let remaining_secs = remaining.as_secs_f32();

            if remaining_secs <= 0.0 {
                return None;
            }

            Some(EffectEntry {
                name: effect.display_text.clone(),
                remaining_secs,
                total_secs: total,
                color: effect.color,
                stacks: effect.stacks,
            })
        })
        .collect();

    // Always return data (even empty) so overlay clears when effects expire
    Some(baras_overlay::EffectsData { entries })
}

/// Result of processing effect audio
struct EffectAudioResult {
    /// Countdown announcements: (effect_name, seconds, voice_pack)
    countdowns: Vec<(String, u8, String)>,
    /// Alert sounds to play
    alerts: Vec<EffectAlert>,
}

struct EffectAlert {
    name: String,
    file: Option<String>,
}

/// Process effect audio (countdowns and alerts)
async fn process_effect_audio(shared: &std::sync::Arc<SharedState>) -> EffectAudioResult {
    let mut countdowns = Vec::new();
    let mut alerts = Vec::new();

    // Get session (same pattern as build_effects_overlay_data)
    let session_guard = shared.session.read().await;
    let Some(session_arc) = session_guard.as_ref() else {
        return EffectAudioResult { countdowns, alerts };
    };
    let session = session_arc.read().await;

    // Get effect tracker (Live mode only)
    let Some(effect_tracker) = session.effect_tracker() else {
        return EffectAudioResult { countdowns, alerts };
    };
    let Ok(mut tracker) = effect_tracker.lock() else {
        return EffectAudioResult { countdowns, alerts };
    };

    for effect in tracker.active_effects_mut() {
        // Skip effects without audio (but don't skip removed - they might need expiration audio)
        if !effect.audio_enabled {
            continue;
        }

        // Check for countdown (uses realtime internally, matches timer logic)
        // Only for non-removed effects
        if effect.removed_at.is_none()
            && let Some(seconds) = effect.check_countdown() {
                countdowns.push((
                    effect.display_text.clone(),
                    seconds,
                    effect.countdown_voice.clone(),
                ));
            }

        // Check for audio offset trigger (early warning sound, offset > 0)
        if effect.check_audio_offset() {
            alerts.push(EffectAlert {
                name: effect.display_text.clone(),
                file: effect.audio_file.clone(),
            });
        }

        // Check for expiration audio (offset == 0, fire when effect expires)
        if effect.check_expiration_audio() {
            alerts.push(EffectAlert {
                name: effect.display_text.clone(),
                file: effect.audio_file.clone(),
            });
        }
    }

    EffectAudioResult { countdowns, alerts }
}

/// Convert an ActiveEffect (core) to RaidEffect (overlay)
///
/// `lag_offset_ms` is a user-configurable offset that compensates for the delay
/// between game events occurring and log lines being written/processed.
/// Positive values make countdowns end earlier, negative values make them end later.
fn convert_to_raid_effect(effect: &ActiveEffect, lag_offset_ms: i32) -> RaidEffect {
    use chrono::Local;

    // Determine if this is a buff based on category
    let is_buff = matches!(
        effect.category,
        EffectCategory::Buff | EffectCategory::Hot | EffectCategory::Shield
    );

    let mut raid_effect = RaidEffect::new(effect.game_effect_id, effect.name.clone())
        .with_charges(effect.stacks)
        .with_color_rgba(effect.color)
        .with_is_buff(is_buff);

    // Calculate system expiry with lag compensation
    if let Some(dur) = effect.duration {
        // Calculate the lag that existed when we PROCESSED the event, not current lag.
        // system_time_at_processing = now - time_since_we_processed
        let time_since_processing = effect.applied_instant.elapsed();
        let system_time_at_processing = Local::now().naive_local()
            - chrono::Duration::milliseconds(time_since_processing.as_millis() as i64);

        // Lag = system time at processing - game timestamp at processing
        let lag_ms = system_time_at_processing
            .signed_duration_since(effect.last_refreshed_at)
            .num_milliseconds()
            .max(0) as u64;

        // Add user-configurable offset for render/processing overhead not captured in timestamps
        let total_lag_ms = (lag_ms as i64 + lag_offset_ms as i64).max(0) as u64;
        let total_lag = std::time::Duration::from_millis(total_lag_ms);

        // Compensate: subtract lag from the calculated expiry
        let compensated_expiry = effect.applied_instant + dur - total_lag.min(dur);

        raid_effect = raid_effect
            .with_duration(dur)
            .with_expiry(compensated_expiry);
    }

    raid_effect
}

// ─────────────────────────────────────────────────────────────────────────────
// DTOs for Tauri IPC
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct LogFileInfo {
    pub path: PathBuf,
    pub display_name: String,
    pub character_name: Option<String>,
    pub date: String,
    pub is_empty: bool,
    pub file_size: u64,
}

/// Unified combat data for metric overlays
#[derive(Debug, Clone)]
pub struct CombatData {
    /// Metrics for all players
    pub metrics: Vec<PlayerMetrics>,
    /// Entity ID of the primary player (for personal overlay)
    pub player_entity_id: i64,
    /// Duration of current encounter in seconds
    pub encounter_time_secs: u64,
    /// Number of encounters in the session
    pub encounter_count: usize,
    /// Player's class and discipline (e.g., "Sorcerer / Corruption")
    pub class_discipline: Option<String>,
    /// Current encounter display name (e.g., "Raid Trash 3" or "Dread Master Bestia Pull 1")
    pub encounter_name: Option<String>,
    /// Current area difficulty (e.g., "NiM 8") or phase type for non-instanced content
    pub difficulty: Option<String>,
    /// Challenge metrics for boss encounters (polled with other metrics)
    pub challenges: Option<ChallengeData>,
    /// Current boss phase (if in a defined encounter)
    pub current_phase: Option<String>,
    /// Time spent in the current phase (seconds)
    pub phase_time_secs: f32,
}

impl CombatData {
    /// Convert to PersonalStats by finding the player's entry in metrics
    pub fn to_personal_stats(&self) -> Option<PersonalStats> {
        let player = self
            .metrics
            .iter()
            .find(|m| m.entity_id == self.player_entity_id)?;
        Some(PersonalStats {
            encounter_name: self.encounter_name.clone(),
            difficulty: self.difficulty.clone(),
            encounter_time_secs: self.encounter_time_secs,
            encounter_count: self.encounter_count,
            class_discipline: self.class_discipline.clone(),
            apm: player.apm,
            dps: player.dps as i32,
            edps: player.edps as i32,
            bossdps: player.bossdps as i32,
            total_damage: player.total_damage,
            total_damage_boss: player.total_damage_boss,
            hps: player.hps as i32,
            ehps: player.ehps as i32,
            total_healing: player.total_healing,
            dtps: player.dtps as i32,
            edtps: player.edtps as i32,
            tps: player.tps as i32,
            total_threat: player.total_threat,
            damage_crit_pct: player.damage_crit_pct,
            heal_crit_pct: player.heal_crit_pct,
            effective_heal_pct: player.effective_heal_pct,
            current_phase: self.current_phase.clone(),
            phase_time_secs: self.phase_time_secs,
        })
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionInfo {
    pub player_name: Option<String>,
    pub player_class: Option<String>,
    pub player_discipline: Option<String>,
    pub area_name: Option<String>,
    pub in_combat: bool,
    pub encounter_count: usize,
    /// Session start time extracted from log filename (formatted as HH:MM)
    pub session_start: Option<String>,
}
