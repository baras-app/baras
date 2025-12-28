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
pub use handler::*;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use baras_core::directory_watcher;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{mpsc, RwLock};

use baras_core::context::{resolve, AppConfig, AppConfigExt, DirectoryIndex, ParsingSession};
use baras_core::encounter::EncounterState;
use baras_core::encounter::summary::classify_encounter;
use baras_core::directory_watcher::DirectoryWatcher;
use baras_core::game_data::{Discipline, Role};
use baras_core::{ActiveEffect, BossEncounterDefinition, DefinitionConfig, DefinitionSet, EffectCategory, EntityType, GameSignal, PlayerMetrics, Reader, SignalHandler};
use baras_overlay::{BossHealthData, ChallengeData, ChallengeEntry, PersonalStats, PlayerContribution, PlayerRole, RaidEffect, RaidFrame, RaidFrameData, TimerData, TimerEntry};


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
        Self { shared, trigger_tx, area_load_tx, session_event_tx, overlay_tx }
    }
}

impl SignalHandler for CombatSignalHandler {
    fn handle_signal(&mut self, signal: &GameSignal) {
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
            GameSignal::DisciplineChanged { entity_id, discipline_id, .. } => {
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
                    self.shared.current_area_id.store(*area_id, Ordering::SeqCst);
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
    pub fn new(app_handle: AppHandle, overlay_tx: mpsc::Sender<OverlayUpdate>) -> (Self, ServiceHandle) {
        let (cmd_tx, cmd_rx) = mpsc::channel(32);

        let config = AppConfig::load();
        let directory_index = DirectoryIndex::build_index(&PathBuf::from(&config.log_directory))
            .unwrap_or_default();

        // Load effect definitions from builtin and user directories
        let definitions = Self::load_effect_definitions(&app_handle);

        // Build area index for lazy loading (fast - only reads headers)
        let area_index = Arc::new(Self::build_area_index(&app_handle));

        let shared = Arc::new(SharedState::new(config, directory_index));

        let service = Self {
            app_handle,
            shared: shared.clone(),
            overlay_tx,
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
            .resolve("definitions/encounters", tauri::path::BaseDirectory::Resource)
            .ok();

        // Custom definitions: user's config directory
        let custom_dir = dirs::config_dir().map(|p| p.join("baras").join("encounters"));

        let mut index = baras_core::boss::AreaIndex::new();

        // Build index from bundled directory
        if let Some(ref path) = bundled_dir && path.exists() {
            match build_area_index(path) {
                Ok(area_index) => {
                    eprintln!("[AREAS] Indexed {} areas from bundled definitions", area_index.len());
                    index.extend(area_index);
                }
                Err(e) => eprintln!("[AREAS] Failed to index bundled: {}", e),
            }
        }

        // Build index from custom directory (can override bundled)
        if let Some(ref path) = custom_dir && path.exists() {
            match build_area_index(path) {
                Ok(area_index) => {
                    eprintln!("[AREAS] Indexed {} areas from custom definitions", area_index.len());
                    index.extend(area_index);
                }
                Err(e) => eprintln!("[AREAS] Failed to index custom: {}", e),
            }
        }

        eprintln!("[AREAS] Total: {} areas indexed for lazy loading", index.len());
        index
    }

    /// Load boss definitions for a specific area
    fn load_area_definitions(&self, area_id: i64) -> Option<Vec<BossEncounterDefinition>> {
        use baras_core::boss::load_bosses_from_file;

        let entry = self.area_index.get(&area_id)?;

        match load_bosses_from_file(&entry.file_path) {
            Ok(bosses) => {
                let timer_count: usize = bosses.iter().map(|b| b.timers.len()).sum();
                eprintln!("[AREAS] Loaded {} bosses, {} timers for '{}'",
                    bosses.len(), timer_count, entry.name);
                Some(bosses)
            }
            Err(e) => {
                eprintln!("[AREAS] Failed to load definitions for '{}': {}", entry.name, e);
                None
            }
        }
    }

    /// Load effect definitions from bundled and user config directories
    fn load_effect_definitions(app_handle: &AppHandle) -> DefinitionSet {
        // Bundled definitions: shipped with the app in resources
        let bundled_dir = app_handle
            .path()
            .resolve("definitions/effects", tauri::path::BaseDirectory::Resource)
            .ok();

        // Custom definitions: user's config directory
        let custom_dir = dirs::config_dir().map(|p| p.join("baras").join("effects"));

        // Load definitions from TOML files
        let mut set = DefinitionSet::new();

        // Load from bundled directory first (defaults)
        if let Some(ref path) = bundled_dir && path.exists() {
            Self::load_definitions_from_dir(&mut set, path, "bundled", false);
        }

        // Load from custom directory (user edits override bundled)
        if let Some(ref path) = custom_dir && path.exists() {
            Self::load_definitions_from_dir(&mut set, path, "custom", true);
        }

        set
    }

    /// Load effect definitions from a directory of TOML files
    fn load_definitions_from_dir(set: &mut DefinitionSet, dir: &std::path::Path, source: &str, overwrite: bool) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("[DEFINITIONS] Failed to read {}: {}", source, e);
                return;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "toml") {
                match std::fs::read_to_string(&path) {
                    Ok(contents) => {
                        // Parse TOML and extract effect definitions
                        if let Ok(config) = toml::from_str::<DefinitionConfig>(&contents) {
                            let duplicates = set.add_definitions(config.effects, overwrite);
                            if !duplicates.is_empty() && !overwrite {
                                eprintln!("[EFFECT WARNING] Duplicate effect IDs SKIPPED in {:?}: {:?}. \
                                    Check your {} definitions for conflicts.",
                                    path.file_name(), duplicates, source);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("[{}] Failed to read {:?}: {}", source, path.file_name(), e);
                    }
                }
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
                    let _ = self.app_handle.emit("session-updated", "TailingModeChanged");
                    self.start_tailing(path).await;
                }
                ServiceCommand::ResumeLiveTailing => {
                    // Resume live tailing and switch to newest file
                    self.shared.is_live_tailing.store(true, Ordering::SeqCst);
                    let _ = self.app_handle.emit("session-updated", "TailingModeChanged");
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
        eprintln!("[SERVICE] Reloading effect definitions...");

        // Reload from disk
        self.definitions = Self::load_effect_definitions(&self.app_handle);
        eprintln!("[SERVICE] Loaded {} effect definitions", self.definitions.effects.len());

        // Update the active session if one exists
        if let Some(session) = self.shared.session.read().await.as_ref() {
            let session = session.read().await;
            session.set_definitions(self.definitions.clone());
            eprintln!("[SERVICE] Updated active session with new effect definitions");
        }
    }

    /// Reload timer and boss definitions from disk and update the active session
    async fn reload_timer_definitions(&mut self) {
        eprintln!("[SERVICE] Reloading timer definitions...");

        // Rebuild area index from disk
        self.area_index = Arc::new(Self::build_area_index(&self.app_handle));
        eprintln!("[SERVICE] Rebuilt area index with {} areas", self.area_index.len());

        // Reload definitions for the currently loaded area (if any)
        let current_area = self.shared.current_area_id.load(Ordering::SeqCst);
        if current_area != 0
            && let Some(bosses) = self.load_area_definitions(current_area) {
                // Update the active session if one exists
                if let Some(session) = self.shared.session.read().await.as_ref() {
                    let mut session = session.write().await;
                    session.load_boss_definitions(bosses);
                    eprintln!("[SERVICE] Updated active session with new definitions");
            }
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
            let session_guard= self.shared.session.read().await;
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
        match directory_watcher::build_index(&dir) {
            Ok((index, newest)) => {
                {
                    let mut index_guard = self.shared.directory_index.write().await;
                    *index_guard = index;
                }

                // Auto-load newest file if available
                if let Some(ref newest_path) = newest {
                    self.start_tailing(newest_path.clone()).await;
                }
            }
            Err(e) => {
                eprintln!("[WATCHER] Failed to build index: {}", e);
            }
        }

        let mut watcher = match DirectoryWatcher::new(&dir) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("[WATCHER] Failed to start: {}", e);
                self.shared.watching.store(false, Ordering::SeqCst);
                return;
            }
        };

      // Clone the command sender so watcher can send back to service
      let cmd_tx = self.cmd_tx.clone();
      let shared = self.shared.clone();

      let handle = tokio::spawn(async move {
          while let Some(event) = watcher.next_event().await {
              if let Some(cmd) = directory::translate_event(event)
                  && cmd_tx.send(cmd).await.is_err() {
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
                }).await {
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
        let _ = self.app_handle.emit("active-file-changed", path.to_string_lossy().to_string());

        // Create reader
        let reader = Reader::from(path, session.clone());

        // First, read and process the entire existing file
        match reader.read_log_file().await {
            Ok((events, end_pos)) => {
                let mut session_guard = session.write().await;
                for event in events {
                    session_guard.process_event(event);
                }
                session_guard.current_byte = Some(end_pos);
                // Finalize session to add the last encounter to history
                session_guard.finalize_session();
                drop(session_guard);
                // Trigger initial metrics send after file processing
                let _ = trigger_tx.send(MetricsTrigger::InitialLoad);
            }
            Err(e) => {
                eprintln!("[TAIL] Failed to read log file: {}", e);
            }
        }

        // Enable live mode for effect/timer tracking (skip historical events)
        {
            let session_guard = session.read().await;
            session_guard.set_effect_live_mode(true);
            session_guard.set_timer_live_mode(true);
        }

        // Spawn the tail task to watch for new lines
        let tail_handle = tokio::spawn(async move {
            if let Err(e) = reader.tail_log_file().await {
                eprintln!("[TAIL] Error: {}", e);
            }
        });

        // Spawn signal-driven metrics task
        let shared = self.shared.clone();
        let overlay_tx = self.overlay_tx.clone();
        let metrics_handle = tokio::spawn(async move {
            loop {
                // Check for triggers (non-blocking with timeout to allow task cancellation)
                let trigger = tokio::task::spawn_blocking({
                    let trigger_rx_timeout = trigger_rx.recv_timeout(std::time::Duration::from_millis(100));
                    move || trigger_rx_timeout
                }).await;

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

        // Spawn effects + boss health sampling task (polls continuously)
        // Checks overlay status flags and combat state to skip unnecessary work
        let shared = self.shared.clone();
        let overlay_tx = self.overlay_tx.clone();
        let effects_handle = tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(150)).await;

                // Check which overlays are active to skip unnecessary work
                let raid_active = shared.raid_overlay_active.load(Ordering::Relaxed);
                let boss_active = shared.boss_health_overlay_active.load(Ordering::Relaxed);
                let timer_active = shared.timer_overlay_active.load(Ordering::Relaxed);
                let effects_active = shared.effects_overlay_active.load(Ordering::Relaxed);
                let in_combat = shared.in_combat.load(Ordering::Relaxed);

                // Skip entire iteration if no relevant overlays are running
                if !raid_active && !boss_active && !timer_active && !effects_active {
                    continue;
                }

                // Raid frames: always poll when active (HOTs can tick outside combat)
                if raid_active &&
                    let Some(data) = build_raid_frame_data(&shared).await {
                        let _ = overlay_tx.try_send(OverlayUpdate::EffectsUpdated(data));
                }

                // Effects countdown: always poll when active (effects can tick outside combat)
                if effects_active
                    && let Some(data) = build_effects_overlay_data(&shared).await {
                        let _ = overlay_tx.try_send(OverlayUpdate::EffectsOverlayUpdated(data));
                }

                // Boss health: only poll when in combat
                if boss_active && in_combat &&
                     let Some(data) = build_boss_health_data(&shared).await {
                        let _ = overlay_tx.try_send(OverlayUpdate::BossHealthUpdated(data));
                }

                // Timers: only poll when in combat and in live mode
                if timer_active && in_combat && shared.is_live_tailing.load(Ordering::SeqCst)
                    && let Some(data) = build_timer_data(&shared).await {
                        let _ = overlay_tx.try_send(OverlayUpdate::TimersUpdated(data));
                    }
            }
        });

        // Spawn area loader task for lazy loading timer/boss definitions
        // Now event-driven via channel instead of polling
        let shared = self.shared.clone();
        let area_index = self.area_index.clone();
        let is_live = self.shared.is_live_tailing.load(Ordering::SeqCst);
        let area_loader_handle = if is_live {
            Some(tokio::spawn(async move {
                let mut loaded_area_id: i64 = 0;

                // Wait for area IDs via channel (event-driven, no polling)
                loop {
                    let area_id = match tokio::task::spawn_blocking({
                        let rx_recv = area_load_rx.recv();
                        move || rx_recv
                    }).await {
                        Ok(Ok(id)) => id,
                        Ok(Err(_)) => break, // Channel closed
                        Err(_) => break,     // Task cancelled
                    };

                    // Skip if already loaded this area
                    if area_id == loaded_area_id {
                        continue;
                    }

                    // Load definitions for this area
                    if let Some(entry) = area_index.get(&area_id) {
                        use baras_core::boss::load_bosses_from_file;

                        match load_bosses_from_file(&entry.file_path) {
                            Ok(bosses) => {
                                let timer_count: usize = bosses.iter().map(|b| b.timers.len()).sum();
                                eprintln!("[AREAS] Loaded {} bosses, {} timers for '{}'",
                                    bosses.len(), timer_count, entry.name);

                                // Update the session with new definitions
                                if let Some(session_arc) = &*shared.session.read().await {
                                    let mut session = session_arc.write().await;
                                    session.load_boss_definitions(bosses);
                                }

                                loaded_area_id = area_id;
                            }
                            Err(e) => {
                                eprintln!("[AREAS] Failed to load definitions for '{}': {}", entry.name, e);
                            }
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
    let class_discipline = if !player_info.class_name.is_empty() && !player_info.discipline_name.is_empty() {
        Some(format!("{} / {}", player_info.class_name, player_info.discipline_name))
    } else if !player_info.class_name.is_empty() {
        Some(player_info.class_name.clone())
    } else {
        None
    };
    let player_entity_id = player_info.id;

    // Get encounter info
    let encounter = cache.last_combat_encounter()?;
    let encounter_count = cache.encounters().filter(|e| e.state != EncounterState::NotStarted ).map(|e| e.id + 1).max().unwrap_or(0) as usize;
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
        let boss_name = cache.active_boss_idx.and_then(|idx| {
            cache.boss_definitions.get(idx).map(|def| def.name.clone())
        });
        let duration_secs = cache.boss_state.combat_time_secs.max(1.0);

        let entries: Vec<ChallengeEntry> = encounter.challenge_tracker
            .snapshot()
            .into_iter()
            .map(|val| {
                // Build per-player breakdown, sorted by value descending
                let mut by_player: Vec<PlayerContribution> = val.by_player
                    .iter()
                    .filter_map(|(&entity_id, &value)| {
                        // Resolve player name from encounter
                        let name = encounter.players
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
                                Some(value as f32 / duration_secs)
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
                        Some(val.value as f32 / duration_secs)
                    } else {
                        None
                    },
                    by_player,
                }
            })
            .collect();

        Some(ChallengeData {
            entries,
            boss_name,
            duration_secs,
            phase_durations: encounter.challenge_tracker.phase_durations().clone(),
        })
    } else {
        None
    };

    // Get phase info from boss state
    let current_phase = cache.boss_state.current_phase.clone();
    let phase_time_secs = cache.boss_state.phase_started_at
        .map(|start| {
            let now = chrono::Utc::now().naive_utc();
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

    // Get effect tracker
    let effect_tracker = session.effect_tracker();
    let mut tracker = match effect_tracker.lock() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("[RAID-DATA] Failed to lock effect tracker: {}", e);
            return None;
        }
    };

    // Get local player ID for is_self flag
    let local_player_id = session.session_cache.as_ref()
        .map(|c| c.player.id)
        .unwrap_or(0);

    // Lock registry
    let mut registry = match shared.raid_registry.lock() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[RAID-DATA] Failed to lock registry: {}", e);
            return None;
        }
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
        // Skip effects not marked for raid frames
        if !effect.show_on_raid_frames {
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
            let effects = effects_by_target.remove(&player.entity_id).unwrap_or_default();

            // Map discipline to role (defaults to DPS if unknown)
            let role = player.discipline_id
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

/// Build timer data from active timers
async fn build_timer_data(shared: &Arc<SharedState>) -> Option<TimerData> {
    let session_guard = shared.session.read().await;
    let session = session_guard.as_ref()?;
    let session = session.read().await;

    // Get active timers from timer manager
    let timer_mgr = session.timer_manager();
    let timer_mgr = timer_mgr.lock().ok()?;

    // If not in combat, return empty data
    let in_combat = shared.in_combat.load(Ordering::SeqCst);
    if !in_combat {
        return Some(TimerData::default());
    }

    // Get current time for remaining calculations
    use chrono::Local;
    let now = Local::now().naive_local();

    // Convert active timers to TimerEntry format
    let entries: Vec<TimerEntry> = timer_mgr
        .active_timers()
        .iter()
        .filter_map(|timer| {
            let remaining = timer.remaining_secs(now);
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

    Some(TimerData { entries })
}

/// Build effects countdown overlay data from active effects
async fn build_effects_overlay_data(shared: &Arc<SharedState>) -> Option<baras_overlay::EffectsData> {
    use baras_overlay::EffectEntry;

    // Get lag offset from config first (before locking tracker)
    let lag_offset_ms = {
        let config = shared.config.read().await;
        config.overlay_settings.effect_lag_offset_ms
    };

    let session_guard = shared.session.read().await;
    let session = session_guard.as_ref()?;
    let session = session.read().await;

    // Get effect tracker
    let effect_tracker = session.effect_tracker();
    let tracker = effect_tracker.lock().ok()?;

    // Early out if no effects are ticking (all removed/expired = just fading)
    if !tracker.has_ticking_effects() {
        return None;
    }

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
                name: effect.name.clone(),
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
        let player = self.metrics.iter().find(|m| m.entity_id == self.player_entity_id)?;
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
