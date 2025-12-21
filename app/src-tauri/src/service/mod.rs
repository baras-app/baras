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
use baras_core::encounter::EncounterState;
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
    /// Unified combat data for both metric and personal overlays
    DataUpdated(CombatData),
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
            self.shared.watching.store(false, Ordering::SeqCst);
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

                // Calculate and send unified combat data
                if let Some(data) = calculate_combat_data(&shared).await
                    && !data.metrics.is_empty()
                {
                    eprintln!("Sending {} metrics to overlay", data.metrics.len());
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

    // Calculate metrics for all players
    let entity_metrics = encounter.calculate_entity_metrics()?;
    let metrics: Vec<PlayerMetrics> = entity_metrics
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
                total_damage_effective: m.total_damage_effective as u64,
                damage_crit_pct: m.damage_crit_pct,
                hps: m.hps as i64,
                ehps: m.ehps as i64,
                total_healing: m.total_healing as u64,
                total_healing_effective: m.total_healing_effective as u64,
                heal_crit_pct: m.heal_crit_pct,
                effective_heal_pct: m.effective_heal_pct,
                tps: m.tps as i64,
                total_threat: m.total_threat as u64,
                dtps: m.dtps as i64,
                edtps: m.edtps as i64,
                total_damage_taken: m.total_damage_taken as u64,
                total_damage_taken_effective: m.total_damage_taken_effective as u64,
                abs: m.abs as i64,
                total_shielding: m.total_shielding as u64,
                apm: m.apm,
            }
        })
        .collect();

    Some(CombatData {
        metrics,
        player_entity_id,
        encounter_time_secs,
        encounter_count,
        class_discipline,
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
    // Damage dealing
    pub dps: i64,
    pub edps: i64,
    pub total_damage: u64,
    pub total_damage_effective: u64,
    pub damage_crit_pct: f32,
    // Healing
    pub hps: i64,
    pub ehps: i64,
    pub total_healing: u64,
    pub total_healing_effective: u64,
    pub heal_crit_pct: f32,
    pub effective_heal_pct: f32,
    // Threat
    pub tps: i64,
    pub total_threat: u64,
    // Damage taken
    pub dtps: i64,
    pub edtps: i64,
    pub total_damage_taken: u64,
    pub total_damage_taken_effective: u64,
    // Shielding (absorbs)
    pub abs: i64,
    pub total_shielding: u64,
    // Activity
    pub apm: f32,
}

/// Unified combat data for all overlays
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
}

impl CombatData {
    /// Convert to PersonalStats by finding the player's entry in metrics
    pub fn to_personal_stats(&self) -> Option<PersonalStats> {
        let player = self.metrics.iter().find(|m| m.entity_id == self.player_entity_id)?;
        Some(PersonalStats {
            encounter_time_secs: self.encounter_time_secs,
            encounter_count: self.encounter_count,
            class_discipline: self.class_discipline.clone(),
            apm: player.apm,
            dps: player.dps as i32,
            edps: player.edps as i32,
            total_damage: player.total_damage as i64,
            hps: player.hps as i32,
            ehps: player.ehps as i32,
            total_healing: player.total_healing as i64,
            dtps: player.dtps as i32,
            edtps: player.edtps as i32,
            tps: player.tps as i32,
            total_threat: player.total_threat as i64,
            damage_crit_pct: player.damage_crit_pct,
            heal_crit_pct: player.heal_crit_pct,
            effective_heal_pct: player.effective_heal_pct,
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
}
