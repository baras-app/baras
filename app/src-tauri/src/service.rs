//! Combat service - coordinates parsing, state management, and overlay updates
//!
//! Architecture:
//! - SharedState: Arc-wrapped state readable by Tauri commands
//! - ServiceHandle: For sending commands + accessing shared state
//! - CombatService: Background task that processes commands and updates shared state

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

use baras_core::context::{resolve, AppConfig, DirectoryIndex, ParsingSession};
use baras_core::{EntityType, GameSignal, Reader, SignalHandler};

// ─────────────────────────────────────────────────────────────────────────────
// Shared State
// ─────────────────────────────────────────────────────────────────────────────

/// State shared between the service and Tauri commands
pub struct SharedState {
    pub config: RwLock<AppConfig>,
    pub directory_index: RwLock<DirectoryIndex>,
    pub session: RwLock<Option<Arc<RwLock<ParsingSession>>>>,
    /// Whether we're currently in active combat (for metrics updates)
    pub in_combat: AtomicBool,
}

impl SharedState {
    fn new(config: AppConfig, directory_index: DirectoryIndex) -> Self {
        Self {
            config: RwLock::new(config),
            directory_index: RwLock::new(directory_index),
            session: RwLock::new(None),
            in_combat: AtomicBool::new(false),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Service Handle (for Tauri commands)
// ─────────────────────────────────────────────────────────────────────────────

/// Handle to communicate with the combat service and query state
#[derive(Clone)]
pub struct ServiceHandle {
    cmd_tx: mpsc::Sender<ServiceCommand>,
    shared: Arc<SharedState>,
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
    pub async fn set_config(&self, config: AppConfig) {
        *self.shared.config.write().await = config;
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
// Service Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Messages sent to the service from Tauri commands
pub enum ServiceCommand {
    StartTailing(PathBuf),
    StopTailing,
    RefreshIndex,
    Shutdown,
}

/// Updates sent to the overlay system
#[derive(Debug, Clone)]
pub enum OverlayUpdate {
    CombatStarted,
    CombatEnded,
    MetricsUpdated(Vec<PlayerMetrics>),
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
    shared: Arc<SharedState>,
    overlay_tx: mpsc::Sender<OverlayUpdate>,
    cmd_rx: mpsc::Receiver<ServiceCommand>,
    tail_handle: Option<tokio::task::JoinHandle<()>>,
    metrics_handle: Option<tokio::task::JoinHandle<()>>,
}

impl CombatService {
    /// Create a new combat service and return a handle to communicate with it
    pub fn new(overlay_tx: mpsc::Sender<OverlayUpdate>) -> (Self, ServiceHandle) {
        let (cmd_tx, cmd_rx) = mpsc::channel(32);

        let config = AppConfig::load();
        let directory_index = DirectoryIndex::build_index(&PathBuf::from(&config.log_directory))
            .unwrap_or_default();

        let shared = Arc::new(SharedState::new(config, directory_index));

        let service = Self {
            shared: shared.clone(),
            overlay_tx,
            cmd_rx,
            tail_handle: None,
            metrics_handle: None,
        };

        let handle = ServiceHandle { cmd_tx, shared };

        (service, handle)
    }

    /// Run the service event loop
    pub async fn run(mut self) {
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
            }
        }
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

                if let Some(metrics) = metrics {
                    if !metrics.is_empty() {
                        eprintln!("Sending {} metrics to overlay", metrics.len());
                        let _ = overlay_tx.try_send(OverlayUpdate::MetricsUpdated(metrics));
                    }
                }

                // For CombatStarted, start polling during combat
                if matches!(trigger, MetricsTrigger::CombatStarted) {
                    // Poll during active combat
                    while shared.in_combat.load(Ordering::SeqCst) {
                        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                        if let Some(metrics) = calculate_metrics(&shared).await {
                            if !metrics.is_empty() {
                                let _ = overlay_tx.try_send(OverlayUpdate::MetricsUpdated(metrics));
                            }
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
                    total_damage: m.total_damage as u64,
                    hps: m.hps as i64,
                    total_healing: m.total_healing as u64,
                }
            })
            .collect(),
    )
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
    pub total_damage: u64,
    pub hps: i64,
    pub total_healing: u64,
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
