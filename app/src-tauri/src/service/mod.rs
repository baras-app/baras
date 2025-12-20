//! Combat service - coordinates parsing, state management, and overlay updates
//!
//! Architecture:
//! - SharedState: Arc-wrapped state readable by Tauri commands
//! - ServiceHandle: For sending commands + accessing shared state
//! - CombatService: Background task that processes commands and updates shared state
mod directory;
mod handler;
mod state;

use state::SharedState;
pub use handler::*;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use baras_core::directory_watcher;
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, RwLock};

use baras_core::context::{resolve, AppConfig, DirectoryIndex, ParsingSession};
use baras_core::{EntityType, GameSignal, Reader, SignalHandler};
use baras_core::directory_watcher::DirectoryWatcher;
use baras_overlay::PersonalStats;


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
}

/// Updates sent to the overlay system
#[derive(Debug, Clone)]
pub enum OverlayUpdate {
    CombatStarted,
    CombatEnded,
    MetricsUpdated(Vec<PlayerMetrics>),
    PersonalStatsUpdated(PersonalStats),
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

/// Signal handler that tracks combat state and triggers metrics updates
struct CombatSignalHandler {
    shared: Arc<SharedState>,
    trigger_tx: std::sync::mpsc::Sender<MetricsTrigger>,
}

impl CombatSignalHandler {
    fn new(shared: Arc<SharedState>, trigger_tx: std::sync::mpsc::Sender<MetricsTrigger>) -> Self {
        Self { shared, trigger_tx }
    }
}

impl SignalHandler for CombatSignalHandler {
    fn handle_signal(&mut self, signal: &GameSignal) {
        match signal {
            GameSignal::CombatStarted { .. } => {
                self.shared.in_combat.store(true, Ordering::SeqCst);
                let _ = self.trigger_tx.send(MetricsTrigger::CombatStarted);
            }
            GameSignal::CombatEnded { .. } => {
                self.shared.in_combat.store(false, Ordering::SeqCst);
                let _ = self.trigger_tx.send(MetricsTrigger::CombatEnded);
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
}

impl CombatService {
    /// Create a new combat service and return a handle to communicate with it
    pub fn new(app_handle: AppHandle, overlay_tx: mpsc::Sender<OverlayUpdate>) -> (Self, ServiceHandle) {
        let (cmd_tx, cmd_rx) = mpsc::channel(32);

        let config = AppConfig::load();
        let directory_index = DirectoryIndex::build_index(&PathBuf::from(&config.log_directory))
            .unwrap_or_default();

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
        };

        let handle = ServiceHandle { cmd_tx, shared };

        (service, handle)
    }

    /// Run the service event loop
    pub async fn run(mut self) {
        // Start watcher on startup if we have a valid directory
        {
            let config = self.shared.config.read().await;
            if !config.log_directory.is_empty() {
                eprintln!("Service starting: found directory {}, starting watcher", config.log_directory);
            }
        }
        self.start_watcher().await;

        while let Some(cmd) = self.cmd_rx.recv().await {
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
            }
        }
    }

    async fn on_directory_changed(&mut self) {
        eprintln!("on_directory_changed: stopping existing watcher and tailing");

        // Stop existing watcher
        if let Some(handle) = self.directory_handle.take() {
            handle.abort();
            let _ = handle.await;
        }

        // Stop any active tailing
        self.stop_tailing().await;

        // Start new watcher (reads directory from config)
        eprintln!("on_directory_changed: starting new watcher");
        self.start_watcher().await;
    }
    async fn file_detected(&mut self, path: PathBuf) {
        {
        let mut index = self.shared.directory_index.write().await;
        index.add_file(&path);
        }

                  let should_switch = {
                      let index = self.shared.directory_index.read().await;
                      index.newest_file().map(|f| f.path == path).unwrap_or(false)
                  };

                  if should_switch {
                    //method calls stop_tailing at beginning so wont create two tailing tasks
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

        eprintln!("start_watcher: checking directory {}", dir.display());

        // Guard against invalid input
        if !dir.exists() || !dir.is_dir() {
            eprintln!("start_watcher: directory {} does not exist or is not a directory", dir.display());
            return;
        }

        // Build initial index
        match directory_watcher::build_index(&dir) {
            Ok((index, newest)) => {
                let file_count = index.len();

                {
                    let mut index_guard = self.shared.directory_index.write().await;
                    *index_guard = index;
                }

                eprintln!("start_watcher: indexed {} log files", file_count);

                // Auto-load newest file if available
                if let Some(ref newest_path) = newest {
                    eprintln!("start_watcher: auto-loading newest file {}", newest_path.display());
                    self.start_tailing(newest_path.clone()).await;
                } else {
                    eprintln!("start_watcher: no log files found in directory");
                }
            }
            Err(e) => {
                eprintln!("start_watcher: failed to build index: {}", e);
            }
        }

        let mut watcher = match DirectoryWatcher::new(&dir) {
            Ok(w) => {
                eprintln!("start_watcher: directory watcher started successfully");
                w
            }
            Err(e) => {
                eprintln!("start_watcher: failed to start directory watcher: {}", e);
                return;
            }
        };

      // Clone the command sender so watcher can send back to service
      let cmd_tx = self.cmd_tx.clone();

      let handle = tokio::spawn(async move {
          while let Some(event) = watcher.next_event().await {
              if let Some(cmd) = directory::translate_event(event)
                  && cmd_tx.send(cmd).await.is_err() {
                      break; // Service shut down
                  }
          }
      });

      self.directory_handle = Some(handle);
    }

    async fn start_tailing(&mut self, path: PathBuf) {
        self.stop_tailing().await;

        // Create trigger channel for signal-driven metrics updates
        let (trigger_tx, trigger_rx) = std::sync::mpsc::channel::<MetricsTrigger>();

        let mut session = ParsingSession::new(path.clone());

        // Add signal handler that triggers metrics on combat state changes
        let handler = CombatSignalHandler::new(self.shared.clone(), trigger_tx.clone());
        session.add_signal_handler(Box::new(handler));

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
                let event_count = events.len();
                {
                    let mut session_guard = session.write().await;
                    for event in events {
                        session_guard.process_event(event);
                    }
                    session_guard.current_byte = Some(end_pos);
                }
                eprintln!("Processed {} events from file", event_count);

                // Trigger initial metrics send after file processing
                let _ = trigger_tx.send(MetricsTrigger::InitialLoad);
            }
            Err(e) => {
                eprintln!("Error reading log file: {}", e);
            }
        }

        // Spawn the tail task to watch for new lines
        let tail_handle = tokio::spawn(async move {
            if let Err(e) = reader.tail_log_file().await {
                eprintln!("Tail error: {}", e);
            }
        });

        // Spawn signal-driven metrics task
        let shared = self.shared.clone();
        let overlay_tx = self.overlay_tx.clone();
        let metrics_handle = tokio::spawn(async move {
            // Wrap sync receiver for async usage
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

                eprintln!("Metrics trigger: {:?}", trigger);

                // Calculate and send metrics
                let metrics = calculate_metrics(&shared).await;
                let personal = calculate_personal_stats(&shared).await;

                if let Some(metrics) = metrics
                    && !metrics.is_empty()
                {
                    eprintln!("Sending {} metrics to overlay", metrics.len());
                    let _ = overlay_tx.try_send(OverlayUpdate::MetricsUpdated(metrics));
                }

                if let Some(personal) = personal {
                    let _ = overlay_tx.try_send(OverlayUpdate::PersonalStatsUpdated(personal));
                }

                // For CombatStarted, start polling during combat
                if matches!(trigger, MetricsTrigger::CombatStarted) {
                    // Poll during active combat
                    while shared.in_combat.load(Ordering::SeqCst) {
                        tokio::time::sleep(std::time::Duration::from_millis(250)).await;

                        if let Some(metrics) = calculate_metrics(&shared).await
                            && !metrics.is_empty()
                        {
                            let _ = overlay_tx.try_send(OverlayUpdate::MetricsUpdated(metrics));
                        }

                        if let Some(personal) = calculate_personal_stats(&shared).await {
                            let _ = overlay_tx.try_send(OverlayUpdate::PersonalStatsUpdated(personal));
                        }
                    }
                }
            }
        });

        self.tail_handle = Some(tail_handle);
        self.metrics_handle = Some(metrics_handle);
    }

    async fn stop_tailing(&mut self) {
        // Reset combat state
        self.shared.in_combat.store(false, Ordering::SeqCst);

        // Cancel metrics task first
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

/// Calculate current metrics from the session
async fn calculate_metrics(shared: &Arc<SharedState>) -> Option<Vec<PlayerMetrics>> {
    let session_guard = shared.session.read().await;
    let session = session_guard.as_ref()?;
    let session = session.read().await;
    let cache = session.session_cache.as_ref()?;
    let encounter = cache.last_combat_encounter()?;

    let entity_metrics = encounter.calculate_entity_metrics()?;

    Some(
        entity_metrics
            .into_iter()
            .filter(|m| m.entity_type != EntityType::Npc)
            .map(|m| {
                let name = resolve(m.name).to_string();
                // Filter out control characters for safe display
                let safe_name: String = name.chars().filter(|c| !c.is_control()).collect();
                PlayerMetrics {
                    entity_id: m.entity_id,
                    name: safe_name,
                    dps: m.dps as i64,
                    edps: m.edps as i64,
                    total_damage: m.total_damage as u64,
                    hps: m.hps as i64,
                    ehps: m.ehps as i64,
                    total_healing: m.total_healing as u64,
                    tps: m.tps as i64,
                    total_threat: m.total_threat as u64,
                    dtps: m.dtps as i64,
                    edtps: m.edtps as i64,
                    abs: m.abs as i64,
                }
            })
            .collect(),
    )
}

/// Calculate personal stats for the primary player
async fn calculate_personal_stats(shared: &Arc<SharedState>) -> Option<PersonalStats> {
    let session_guard = shared.session.read().await;
    let session = session_guard.as_ref()?;
    let session = session.read().await;
    let cache = session.session_cache.as_ref()?;

    // Get player info for class/discipline from cache
    let player_info = &cache.player;
    let class_discipline = if !player_info.class_name.is_empty() && !player_info.discipline_name.is_empty() {
        Some(format!("{} / {}", player_info.class_name, player_info.discipline_name))
    } else if !player_info.class_name.is_empty() {
        Some(player_info.class_name.clone())
    } else {
        None
    };

    let encounter = cache.last_combat_encounter()?;
    let encounter_count = cache.encounter_count();
    let encounter_time_secs = encounter.duration_ms().unwrap_or(0) / 1000;

    // Get player entity ID (id field, must be non-zero)
    let player_entity_id = player_info.id;
    if player_entity_id == 0 {
        return None;
    }

    // Find the player's metrics in the entity metrics
    let entity_metrics = encounter.calculate_entity_metrics()?;
    let player_metrics = entity_metrics
        .iter()
        .find(|m| m.entity_id == player_entity_id)?;

    Some(PersonalStats {
        encounter_time_secs: encounter_time_secs as u64,
        encounter_count,
        class_discipline,
        apm: player_metrics.apm,
        dps: player_metrics.dps,
        edps: player_metrics.edps,
        total_damage: player_metrics.total_damage,
        hps: player_metrics.hps,
        ehps: player_metrics.ehps,
        total_healing: player_metrics.total_healing,
        dtps: player_metrics.dtps,
        edtps: player_metrics.edtps,
        tps: player_metrics.tps,
        total_threat: player_metrics.total_threat,
        damage_crit_pct: player_metrics.damage_crit_pct,
        heal_crit_pct: player_metrics.heal_crit_pct,
        effective_heal_pct: player_metrics.effective_heal_pct,
    })
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
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PlayerMetrics {
    pub entity_id: i64,
    pub name: String,
    pub dps: i64,
    pub edps: i64,
    pub total_damage: u64,
    pub hps: i64,
    pub ehps: i64,
    pub total_healing: u64,
    pub tps: i64,
    pub total_threat: u64,
    pub dtps: i64,
    pub edtps: i64,
    pub abs: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionInfo {
    pub player_name: Option<String>,
    pub player_class: Option<String>,
    pub player_discipline: Option<String>,
    pub area_name: Option<String>,
    pub in_combat: bool,
    pub encounter_count: usize,
}
