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
    ActiveEffect, BossEncounterDefinition, DefinitionConfig, DefinitionSet,
    EffectCategory, EntityType, GameSignal, PlayerMetrics, Reader, SignalHandler,
    EFFECTS_DSL_VERSION,
};
use baras_overlay::{
    BossHealthData, ChallengeData, ChallengeEntry, Color, CooldownData, CooldownEntry, DotEntry,
    DotTarget, DotTrackerData, EffectABEntry, EffectsABData, PersonalStats, PlayerContribution,
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
async fn fallback_streaming_parse(reader: &Reader, session: &Arc<RwLock<ParsingSession>>) {
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
    /// Trigger immediate raid frame data refresh (after registry changes)
    RefreshRaidFrames,
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
    /// Alert text for alerts overlay
    AlertsFired(Vec<FiredAlert>),
    /// Effects A overlay data
    EffectsAUpdated(EffectsABData),
    /// Effects B overlay data
    EffectsBUpdated(EffectsABData),
    /// Ability cooldowns
    CooldownsUpdated(CooldownData),
    /// DOTs on enemy targets
    DotTrackerUpdated(DotTrackerData),
    /// Clear all overlay data (sent when switching files)
    ClearAllData,
    /// Local player entered conversation - temporarily hide overlays
    ConversationStarted,
    /// Local player exited conversation - restore overlays if we hid them
    ConversationEnded,
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
    trigger_tx: mpsc::Sender<MetricsTrigger>,
    /// Channel for area load requests (event-driven, not polled)
    area_load_tx: std::sync::mpsc::Sender<i64>,
    /// Channel for frontend session updates (event-driven, not polled)
    session_event_tx: std::sync::mpsc::Sender<SessionEvent>,
    /// Channel for overlay updates (to clear overlays on combat end)
    overlay_tx: mpsc::Sender<OverlayUpdate>,
    /// Local player entity ID (set on first DisciplineChanged)
    local_player_id: Option<i64>,
}

impl CombatSignalHandler {
    fn new(
        shared: Arc<SharedState>,
        trigger_tx: mpsc::Sender<MetricsTrigger>,
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
            local_player_id: None,
        }
    }
}

impl SignalHandler for CombatSignalHandler {
    fn handle_signal(
        &mut self,
        signal: &GameSignal,
        _encounter: Option<&baras_core::encounter::CombatEncounter>,
    ) {
        match signal {
            GameSignal::CombatStarted { .. } => {
                self.shared.in_combat.store(true, Ordering::SeqCst);
                let _ = self.trigger_tx.try_send(MetricsTrigger::CombatStarted);
                let _ = self.session_event_tx.send(SessionEvent::CombatStarted);
            }
            GameSignal::CombatEnded { .. } => {
                self.shared.in_combat.store(false, Ordering::SeqCst);
                let _ = self.trigger_tx.try_send(MetricsTrigger::CombatEnded);
                let _ = self.session_event_tx.send(SessionEvent::CombatEnded);
                // Clear boss health and timer overlays
                let _ = self.overlay_tx.try_send(OverlayUpdate::CombatEnded);
            }
            GameSignal::DisciplineChanged {
                entity_id,
                discipline_id,
                ..
            } => {
                // First DisciplineChanged is always the local player
                if self.local_player_id.is_none() {
                    self.local_player_id = Some(*entity_id);
                }
                // Update raid registry with discipline info for role icons
                if let Ok(mut registry) = self.shared.raid_registry.lock() {
                    registry.update_discipline(*entity_id, 0, *discipline_id);
                }
                // Notify frontend of player info change
                let _ = self.session_event_tx.send(SessionEvent::PlayerInitialized);
            }
            GameSignal::EffectApplied {
                effect_id,
                target_id,
                ..
            } => {
                // Check for conversation effect on local player
                if *effect_id == baras_core::game_data::effect_id::CONVERSATION
                    && self.local_player_id == Some(*target_id)
                {
                    let _ = self.overlay_tx.try_send(OverlayUpdate::ConversationStarted);
                }
            }
            GameSignal::EffectRemoved {
                effect_id,
                target_id,
                ..
            } => {
                // Check for conversation effect removed from local player
                if *effect_id == baras_core::game_data::effect_id::CONVERSATION
                    && self.local_player_id == Some(*target_id)
                {
                    let _ = self.overlay_tx.try_send(OverlayUpdate::ConversationEnded);
                }
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
    /// Icon cache for ability icons (shared with SharedState for overlay data building)
    icon_cache: Option<Arc<baras_overlay::icons::IconCache>>,
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

        // Initialize icon cache for ability icons
        let icon_cache = Self::init_icon_cache(&app_handle);

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
            icon_cache,
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
        let custom_dir =
            dirs::config_dir().map(|p| p.join("baras").join("definitions").join("encounters"));

        let mut index = baras_core::boss::AreaIndex::new();

        // Build index from bundled directory
        if let Some(ref path) = bundled_dir
            && path.exists()
            && let Ok(area_index) = build_area_index(path)
        {
            index.extend(area_index);
        }

        // Build index from custom directory (can override bundled)
        if let Some(ref path) = custom_dir
            && path.exists()
            && let Ok(area_index) = build_area_index(path)
        {
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

    /// Initialize the icon cache for ability icons
    fn init_icon_cache(app_handle: &AppHandle) -> Option<Arc<baras_overlay::icons::IconCache>> {
        use baras_overlay::icons::IconCache;

        eprintln!("[ICONS] Initializing icon cache...");

        // Try bundled resources first, fall back to dev path
        let icons_dir = app_handle
            .path()
            .resolve("icons", tauri::path::BaseDirectory::Resource)
            .ok()
            .filter(|p| p.exists())
            .unwrap_or_else(|| {
                // Dev fallback: relative to project root
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .join("icons")
            });

        eprintln!("[ICONS] Looking for icons at {:?}", icons_dir);

        let csv_path = icons_dir.join("icons.csv");
        let zip_path = icons_dir.join("icons.zip");

        if !csv_path.exists() || !zip_path.exists() {
            eprintln!(
                "[ICONS] Icon files not found at {:?} (csv={}, zip={})",
                icons_dir,
                csv_path.exists(),
                zip_path.exists()
            );
            return None;
        }

        match IconCache::new(&csv_path, &zip_path, 200) {
            Ok(cache) => {
                eprintln!("[ICONS] Loaded icon cache from {:?}", icons_dir);
                Some(Arc::new(cache))
            }
            Err(e) => {
                eprintln!("[ICONS] Failed to load icon cache: {}", e);
                None
            }
        }
    }

    /// Get the user effects config file path
    fn get_user_effects_path() -> Option<PathBuf> {
        dirs::config_dir().map(|p| p.join("baras").join("definitions").join("effects.toml"))
    }

    /// Clean up old user effects directory structure (pre-delta architecture)
    fn cleanup_old_effects_dir() {
        let Some(old_dir) = dirs::config_dir().map(|p| p.join("baras").join("definitions").join("effects")) else {
            return;
        };

        if old_dir.is_dir() {
            eprintln!("[EFFECTS] Removing old effects directory: {:?}", old_dir);
            if let Err(e) = std::fs::remove_dir_all(&old_dir) {
                eprintln!("[EFFECTS] Failed to remove old effects directory: {}", e);
            }
        }
    }

    /// Load effect definitions from bundled resources and user config file.
    ///
    /// Architecture (delta-based):
    /// 1. Load bundled definitions from app resources (base layer)
    /// 2. Load user overrides from single file: ~/.config/baras/definitions/effects.toml
    /// 3. User effects with matching IDs replace bundled effects entirely
    ///
    /// Version checking:
    /// - User file must have `version = N` matching EFFECTS_DSL_VERSION
    /// - Mismatched versions cause user file to be deleted (breaking DSL change)
    fn load_effect_definitions(app_handle: &AppHandle) -> DefinitionSet {
        // Clean up old directory structure on first run after update
        Self::cleanup_old_effects_dir();

        let mut set = DefinitionSet::new();

        // 1. Load bundled definitions from app resources
        if let Some(bundled_dir) = app_handle
            .path()
            .resolve("definitions/effects", tauri::path::BaseDirectory::Resource)
            .ok()
            .filter(|p| p.exists())
        {
            Self::load_bundled_definitions(&mut set, &bundled_dir);
        }

        // 2. Load user overrides from single config file
        if let Some(user_path) = Self::get_user_effects_path()
            && user_path.exists()
        {
            Self::load_user_effects(&mut set, &user_path);
        }

        set
    }

    /// Load bundled effect definitions from a directory
    fn load_bundled_definitions(set: &mut DefinitionSet, dir: &std::path::Path) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            eprintln!("[EFFECTS] Failed to read bundled dir: {:?}", dir);
            return;
        };

        let files: Vec<_> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.extension().is_some_and(|ext| ext == "toml")
                    && !p.file_name().is_some_and(|n| n == "custom.toml") // Skip template
            })
            .collect();

        eprintln!("[EFFECTS] Loading {} bundled files from {:?}", files.len(), dir);

        for path in files {
            if let Ok(contents) = std::fs::read_to_string(&path)
                && let Ok(config) = toml::from_str::<DefinitionConfig>(&contents)
            {
                let count = config.effects.len();
                set.add_definitions(config.effects, false);
                eprintln!("[EFFECTS]   {:?}: {} effects", path.file_name().unwrap_or_default(), count);
            }
        }
    }

    /// Load user effect overrides from single config file
    fn load_user_effects(set: &mut DefinitionSet, path: &std::path::Path) {
        let Ok(contents) = std::fs::read_to_string(path) else {
            eprintln!("[EFFECTS] Failed to read user effects file: {:?}", path);
            return;
        };

        let Ok(config) = toml::from_str::<DefinitionConfig>(&contents) else {
            eprintln!("[EFFECTS] Failed to parse user effects file: {:?}", path);
            // Delete invalid file
            let _ = std::fs::remove_file(path);
            return;
        };

        // Version check - delete file if version mismatch
        if config.version != EFFECTS_DSL_VERSION {
            eprintln!(
                "[EFFECTS] User effects version mismatch (file={}, expected={}), deleting: {:?}",
                config.version, EFFECTS_DSL_VERSION, path
            );
            let _ = std::fs::remove_file(path);
            return;
        }

        if !config.effects.is_empty() {
            eprintln!("[EFFECTS] Loading {} user overrides from {:?}", config.effects.len(), path);
            set.add_definitions(config.effects, true); // Overwrite bundled
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
                ServiceCommand::RefreshRaidFrames => {
                    // Immediately send updated raid frame data to overlay
                    // Pass true to bypass early-out gates (ensures clear is reflected)
                    let data = build_raid_frame_data(&self.shared, true)
                        .await
                        .unwrap_or_else(|| baras_overlay::RaidFrameData { frames: vec![] });
                    let _ = self
                        .overlay_tx
                        .try_send(OverlayUpdate::EffectsUpdated(data));
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

        // Resume live tailing mode (restart means we want to watch for new files)
        self.shared.is_live_tailing.store(true, Ordering::SeqCst);

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

        // Clear old parquet data from previous session
        if let Err(e) = baras_core::storage::clear_data_dir() {
            eprintln!("[TAILING] Failed to clear data directory: {}", e);
        }

        // Clear all overlay data when switching files
        let _ = self.overlay_tx.try_send(OverlayUpdate::ClearAllData);

        // Clear raid registry when switching files (new session = fresh state)
        if let Ok(mut registry) = self.shared.raid_registry.lock() {
            registry.clear();
        }

        // Create trigger channel for signal-driven metrics updates (tokio channel - no spawn_blocking needed)
        let (trigger_tx, mut trigger_rx) = mpsc::channel::<MetricsTrigger>(8);
        // Create channel for event-driven area loading (replaces polling)
        let (area_load_tx, area_load_rx) = std::sync::mpsc::channel::<i64>();
        // Create channel for frontend session events (replaces polling)
        let (session_event_tx, session_event_rx) = std::sync::mpsc::channel::<SessionEvent>();

        let mut session = ParsingSession::new(path.clone(), self.definitions.clone());

        // Load timer preferences into the session's timer manager (Live mode only)
        if let Some(prefs_path) = Self::timer_preferences_path() {
            if let Some(timer_mgr) = session.timer_manager() {
                if let Ok(mut mgr) = timer_mgr.lock()
                    && let Err(e) = mgr.load_preferences(&prefs_path)
                {
                    eprintln!("Warning: Failed to load timer preferences: {}", e);
                }
            }
        }

        // Set up sync definition loader for AreaEntered events (fixes race condition)
        let area_index = self.area_index.clone();
        let user_encounters_dir =
            dirs::config_dir().map(|p| p.join("baras").join("definitions").join("encounters"));
        let loader: baras_core::context::DefinitionLoader = Box::new(move |area_id: i64| {
            use baras_core::boss::load_bosses_with_custom;
            area_index.get(&area_id).and_then(|entry| {
                load_bosses_with_custom(&entry.file_path, user_encounters_dir.as_deref()).ok()
            })
        });
        session.set_definition_loader(std::sync::Arc::new(loader));

        // Reset area tracking for new session
        self.loaded_area_id = 0;
        self.shared.current_area_id.store(0, Ordering::SeqCst);

        // Add signal handler that triggers metrics on combat state changes
        let area_load_tx_clone = area_load_tx.clone();
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

        // Get boss definitions directory for phase detection
        let definitions_dir = self
            .app_handle
            .path()
            .resolve(
                "definitions/encounters",
                tauri::path::BaseDirectory::Resource,
            )
            .ok();

        // Spawn parse worker subprocess
        // Check multiple locations: bundled sidecar (with target triple), next to exe, fallback to PATH
        let worker_path = std::env::current_exe()
            .ok()
            .and_then(|exe| {
                let dir = exe.parent()?;
                // Try sidecar name with target triple first (Tauri bundle format), then plain name
                let candidates = [
                    dir.join(format!(
                        "baras-parse-worker-{}-unknown-linux-gnu",
                        std::env::consts::ARCH
                    )),
                    dir.join("baras-parse-worker"),
                ];
                candidates.into_iter().find(|p| p.exists())
            })
            .unwrap_or_else(|| PathBuf::from("baras-parse-worker"));

        eprintln!("[PARSE] Using worker: {:?}", worker_path);

        let mut cmd = std::process::Command::new(&worker_path);
        cmd.arg(&path).arg(&session_id).arg(&encounters_dir);

        // Pass definitions directory if available
        if let Some(ref def_dir) = definitions_dir {
            cmd.arg(def_dir);
            eprintln!("[PARSE] Using definitions: {:?}", def_dir);
        }

        let output = cmd.output();

        match output {
            Ok(output) if output.status.success() => {
                // Parse JSON result from subprocess
                let json_result = String::from_utf8(output.stdout)
                    .map_err(|e| format!("Invalid UTF-8: {}", e))
                    .and_then(|result| {
                        serde_json::from_str::<ParseWorkerOutput>(&result).map_err(|e| {
                            format!(
                                "JSON parse error: {} (input: {})",
                                e,
                                &result[..result.len().min(500)]
                            )
                        })
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
                            cache.player.name =
                                baras_core::context::intern(&parse_result.player.name);
                            cache.player.id = parse_result.player.entity_id;
                            cache.player.class_name = parse_result.player.class_name.clone();
                            cache.player.discipline_name =
                                parse_result.player.discipline_name.clone();
                            cache.player_initialized = true;

                            // Import area info
                            cache.current_area.area_name = parse_result.area.area_name.clone();
                            cache.current_area.area_id = parse_result.area.area_id;
                            cache.current_area.difficulty_name =
                                parse_result.area.difficulty_name.clone();
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

                        // Trigger boss definition loading for initial area (if known)
                        if parse_result.area.area_id != 0 {
                            let _ = area_load_tx_clone.send(parse_result.area.area_id);
                        }

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
        let _ = trigger_tx.try_send(MetricsTrigger::InitialLoad);

        // Enable live mode for effect/timer tracking (skip historical events)
        // Also set alacrity/latency from config for duration calculations
        {
            let session_guard = session.read().await;
            session_guard.set_effect_live_mode(true);
            session_guard.set_timer_live_mode(true);
            let config = self.shared.config.read().await;
            session_guard.set_effect_alacrity(config.alacrity_percent);
            session_guard.set_effect_latency(config.latency_ms);
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
                // Check for triggers with timeout to allow task cancellation
                let trigger =
                    tokio::time::timeout(std::time::Duration::from_millis(100), trigger_rx.recv())
                        .await;

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
        let icon_cache = self.icon_cache.clone();
        let effects_handle = tokio::spawn(async move {
            // Track previous state to avoid redundant updates
            let mut last_raid_effect_count: usize = 0;
            let mut last_effects_count: usize = 0;

            // Track previous state for new overlays to avoid redundant updates
            let mut last_effects_a_count: usize = 0;
            let mut last_effects_b_count: usize = 0;
            let mut last_cooldowns_count: usize = 0;
            let mut last_dot_tracker_count: usize = 0;

            loop {
                // Check which overlays are active to determine sleep interval
                let raid_active = shared.raid_overlay_active.load(Ordering::Relaxed);
                let boss_active = shared.boss_health_overlay_active.load(Ordering::Relaxed);
                let timer_active = shared.timer_overlay_active.load(Ordering::Relaxed);
                let effects_a_active =
                    shared.effects_a_overlay_active.load(Ordering::Relaxed);
                let effects_b_active = shared
                    .effects_b_overlay_active
                    .load(Ordering::Relaxed);
                let cooldowns_active = shared.cooldowns_overlay_active.load(Ordering::Relaxed);
                let dot_tracker_active = shared.dot_tracker_overlay_active.load(Ordering::Relaxed);
                let in_combat = shared.in_combat.load(Ordering::Relaxed);
                let is_live = shared.is_live_tailing.load(Ordering::SeqCst);

                // Determine if any work needs to be done
                let any_overlay_active = raid_active
                    || boss_active
                    || timer_active
                    || effects_a_active
                    || effects_b_active
                    || cooldowns_active
                    || dot_tracker_active;
                let needs_audio = is_live && (in_combat || raid_active);

                // Adaptive sleep: fast when active, slow when idle
                // 30ms matches tail polling for consistent ~60ms max latency
                let sleep_ms = if any_overlay_active || needs_audio {
                    30
                } else {
                    500
                };
                tokio::time::sleep(std::time::Duration::from_millis(sleep_ms)).await;

                // Skip processing if nothing needs updating
                if !any_overlay_active && !needs_audio {
                    continue;
                }

                // Raid frames: send whenever there are effects (or always in rearrange mode)
                if raid_active {
                    let rearranging = shared.rearrange_mode.load(Ordering::Relaxed);
                    if let Some(data) = build_raid_frame_data(&shared, rearranging).await {
                        let effect_count: usize = data.frames.iter().map(|f| f.effects.len()).sum();
                        // Always send in rearrange mode, otherwise only when effects exist/changed
                        if rearranging || effect_count > 0 || last_raid_effect_count > 0 {
                            let _ = overlay_tx.try_send(OverlayUpdate::EffectsUpdated(data));
                        }
                        last_raid_effect_count = effect_count;
                    } else if rearranging {
                        // In rearrange mode, send empty data to keep overlay rendering
                        let _ = overlay_tx.try_send(OverlayUpdate::EffectsUpdated(
                            baras_overlay::RaidFrameData { frames: vec![] },
                        ));
                        last_raid_effect_count = 0;
                    } else {
                        last_raid_effect_count = 0;
                    }
                }
                // Effects A: only send if there are effects or effects just cleared
                if effects_a_active {
                    if let Some(data) = build_effects_a_data(&shared, icon_cache.as_ref()).await {
                        let count = data.effects.len();
                        if count > 0 || last_effects_a_count > 0 {
                            let _ = overlay_tx.try_send(OverlayUpdate::EffectsAUpdated(data));
                        }
                        last_effects_a_count = count;
                    } else if last_effects_a_count > 0 {
                        let _ = overlay_tx.try_send(OverlayUpdate::EffectsAUpdated(
                            EffectsABData { effects: vec![] },
                        ));
                        last_effects_a_count = 0;
                    }
                }

                // Effects B: only send if there are effects or effects just cleared
                if effects_b_active {
                    if let Some(data) = build_effects_b_data(&shared, icon_cache.as_ref()).await {
                        let count = data.effects.len();
                        if count > 0 || last_effects_b_count > 0 {
                            let _ =
                                overlay_tx.try_send(OverlayUpdate::EffectsBUpdated(data));
                        }
                        last_effects_b_count = count;
                    } else if last_effects_b_count > 0 {
                        let _ = overlay_tx.try_send(OverlayUpdate::EffectsBUpdated(
                            EffectsABData { effects: vec![] },
                        ));
                        last_effects_b_count = 0;
                    }
                }

                // Cooldowns: only send if there are cooldowns or cooldowns just cleared
                if cooldowns_active {
                    if let Some(data) = build_cooldowns_data(&shared, icon_cache.as_ref()).await {
                        let count = data.entries.len();
                        if count > 0 || last_cooldowns_count > 0 {
                            let _ = overlay_tx.try_send(OverlayUpdate::CooldownsUpdated(data));
                        }
                        last_cooldowns_count = count;
                    } else if last_cooldowns_count > 0 {
                        let _ =
                            overlay_tx.try_send(OverlayUpdate::CooldownsUpdated(CooldownData {
                                entries: vec![],
                            }));
                        last_cooldowns_count = 0;
                    }
                }

                // DOT tracker: only send if there are targets or targets just cleared
                if dot_tracker_active {
                    if let Some(data) = build_dot_tracker_data(&shared, icon_cache.as_ref()).await {
                        let count = data.targets.len();
                        if count > 0 || last_dot_tracker_count > 0 {
                            let _ = overlay_tx.try_send(OverlayUpdate::DotTrackerUpdated(data));
                        }
                        last_dot_tracker_count = count;
                    } else if last_dot_tracker_count > 0 {
                        let _ =
                            overlay_tx.try_send(OverlayUpdate::DotTrackerUpdated(DotTrackerData {
                                targets: vec![],
                            }));
                        last_dot_tracker_count = 0;
                    }
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
                    // Send text alerts to overlay
                    if !effect_audio.text_alerts.is_empty() {
                        let _ = overlay_tx.try_send(OverlayUpdate::AlertsFired(effect_audio.text_alerts));
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

                        // Send alerts to overlay (before audio consumes them)
                        if !alerts.is_empty() {
                            let _ = overlay_tx.try_send(OverlayUpdate::AlertsFired(alerts.clone()));
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
        let user_encounters_dir =
            dirs::config_dir().map(|p| p.join("baras").join("definitions").join("encounters"));
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
    let (encounter_type, boss_info) = classify_encounter(encounter, &cache.current_area);

    // Generate encounter name - if there's a boss use that, otherwise use phase type
    let encounter_name = if let Some(boss) = boss_info {
        Some(boss.boss.to_string())
    } else {
        // Use phase type for trash/non-boss encounters
        Some(format!("{:?}", encounter_type))
    };

    // Get difficulty from area info, fallback to phase type name for non-instanced content
    let difficulty = if !cache.current_area.difficulty_name.is_empty() {
        Some(cache.current_area.difficulty_name.clone())
    } else {
        Some(format!("{:?}", encounter_type))
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
        let boss_name = encounter.active_boss_idx().and_then(|idx| {
            encounter
                .boss_definitions()
                .get(idx)
                .map(|def| def.name.clone())
        });
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
async fn build_raid_frame_data(
    shared: &Arc<SharedState>,
    rearranging: bool,
) -> Option<RaidFrameData> {
    let session_guard = shared.session.read().await;
    let session = session_guard.as_ref()?;
    let session = session.read().await;

    // Get effect tracker (Live mode only)
    let effect_tracker = session.effect_tracker()?;
    let Ok(mut tracker) = effect_tracker.lock() else {
        return None;
    };

    // Lock registry
    let Ok(mut registry) = shared.raid_registry.lock() else {
        return None;
    };

    // Early out: skip building data if no effects AND no registered players
    // We need to keep sending updates while effects exist OR players are registered
    // so that removals/clears are reflected in the overlay
    // Skip this check in rearrange mode to always show frames
    if !rearranging && !tracker.has_active_effects() && registry.is_empty() {
        return None;
    }

    // Get local player ID for is_self flag
    let local_player_id = session
        .session_cache
        .as_ref()
        .map(|c| c.player.id)
        .unwrap_or(0);

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
                .push(convert_to_raid_effect(effect));
        }
    }

    // Build frames from registry (stable slot order)
    let max_slots = registry.max_slots();
    let mut frames = Vec::with_capacity(max_slots as usize);

    for slot in 0..max_slots {
        if let Some(player) = registry.get_player(slot) {
            let mut effects = effects_by_target
                .remove(&player.entity_id)
                .unwrap_or_default();

            // Sort effects by effect_id for stable visual ordering
            effects.sort_by_key(|e| e.effect_id);

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

    // Also get alerts from effect tracker (effect start/end alerts)
    if let Some(effect_tracker) = session.effect_tracker() {
        if let Ok(mut tracker) = effect_tracker.lock() {
            alerts.extend(tracker.take_fired_alerts());
        }
    }

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

/// Result of processing effect audio
struct EffectAudioResult {
    /// Countdown announcements: (effect_name, seconds, voice_pack)
    countdowns: Vec<(String, u8, String)>,
    /// Alert sounds to play
    alerts: Vec<EffectAlert>,
    /// Text alerts fired on effect expiration
    text_alerts: Vec<FiredAlert>,
}

struct EffectAlert {
    name: String,
    file: Option<String>,
}

/// Process effect audio (countdowns and alerts)
async fn process_effect_audio(shared: &std::sync::Arc<SharedState>) -> EffectAudioResult {
    let mut countdowns = Vec::new();
    let mut alerts = Vec::new();
    let mut text_alerts = Vec::new();

    // Get session (same pattern as build_effects_overlay_data)
    let session_guard = shared.session.read().await;
    let Some(session_arc) = session_guard.as_ref() else {
        return EffectAudioResult { countdowns, alerts, text_alerts };
    };
    let session = session_arc.read().await;

    // Get effect tracker (Live mode only)
    let Some(effect_tracker) = session.effect_tracker() else {
        return EffectAudioResult { countdowns, alerts, text_alerts };
    };
    let Ok(mut tracker) = effect_tracker.lock() else {
        return EffectAudioResult { countdowns, alerts, text_alerts };
    };

    for effect in tracker.active_effects_mut() {
        // Skip effects without audio (but don't skip removed - they might need expiration audio)
        if !effect.audio_enabled {
            continue;
        }

        // Check for countdown (uses realtime internally, matches timer logic)
        // Only for non-removed effects
        if effect.removed_at.is_none()
            && let Some(seconds) = effect.check_countdown()
        {
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

        // Check for text alert on expiration
        if let Some(text) = effect.check_expiration_alert().map(|s| s.to_string()) {
            text_alerts.push(FiredAlert {
                id: effect.definition_id.clone(),
                name: effect.name.clone(),
                text,
                color: Some(effect.color),
                timestamp: chrono::Local::now().naive_local(),
                audio_enabled: false,
                audio_file: None,
            });
        }
    }

    EffectAudioResult { countdowns, alerts, text_alerts }
}

/// Convert an ActiveEffect (core) to RaidEffect (overlay)
///
/// Uses the pre-computed lag compensation from ActiveEffect.
/// The applied_instant is already backdated to game event time in ActiveEffect::new() and refresh(),
/// so we just add the duration to get the expiry.
fn convert_to_raid_effect(effect: &ActiveEffect) -> RaidEffect {
    // Determine if this is a buff based on category
    let is_buff = matches!(
        effect.category,
        EffectCategory::Buff | EffectCategory::Hot | EffectCategory::Shield
    );

    let mut raid_effect = RaidEffect::new(effect.game_effect_id, effect.name.clone())
        .with_charges(effect.stacks)
        .with_color_rgba(effect.color)
        .with_is_buff(is_buff);

    // applied_instant is already lag-compensated (backdated to game event time)
    // Just add duration to get the expiry instant
    if let Some(dur) = effect.duration {
        let expires_at = effect.applied_instant + dur;
        raid_effect = raid_effect.with_duration(dur).with_expiry(expires_at);
    }

    raid_effect
}

/// Calculate remaining time for an effect in seconds
/// Uses the pre-computed lag compensation in applied_instant
fn calculate_remaining_secs(effect: &ActiveEffect) -> Option<f32> {
    let remaining = effect.remaining_secs_realtime();
    if remaining <= 0.0 {
        None
    } else {
        Some(remaining)
    }
}

/// Build effects A overlay data from active effects
async fn build_effects_a_data(
    shared: &Arc<SharedState>,
    icon_cache: Option<&Arc<baras_overlay::icons::IconCache>>,
) -> Option<EffectsABData> {
    use std::sync::Arc as StdArc;

    let session_guard = shared.session.read().await;
    let session = session_guard.as_ref()?;
    let session = session.read().await;

    let effect_tracker = session.effect_tracker()?;
    let tracker = effect_tracker.lock().ok()?;

    if !tracker.has_active_effects() {
        return None;
    }

    let mut effects: Vec<_> = tracker.effects_a().collect();
    effects.sort_by_key(|e| e.applied_at);

    let entries: Vec<EffectABEntry> = effects
        .into_iter()
        .filter_map(|effect| {
            let total_secs = effect.duration?.as_secs_f32();
            let remaining_secs = calculate_remaining_secs(effect)?;

            // Load icon from cache
            let icon = icon_cache.and_then(|cache| {
                cache.get_icon(effect.icon_ability_id).map(|data| {
                    StdArc::new((data.width, data.height, data.rgba))
                })
            });

            Some(EffectABEntry {
                effect_id: effect.game_effect_id,
                icon_ability_id: effect.icon_ability_id,
                name: effect.name.clone(),
                remaining_secs,
                total_secs,
                color: effect.color,
                stacks: effect.stacks,
                source_name: resolve(effect.source_name).to_string(),
                target_name: resolve(effect.target_name).to_string(),
                icon,
                show_icon: effect.show_icon,
                display_source: effect.display_source,
                is_cleansable: false,
            })
        })
        .collect();

    Some(EffectsABData { effects: entries })
}

/// Build effects B overlay data from active effects
async fn build_effects_b_data(
    shared: &Arc<SharedState>,
    icon_cache: Option<&Arc<baras_overlay::icons::IconCache>>,
) -> Option<EffectsABData> {
    use std::sync::Arc as StdArc;

    let session_guard = shared.session.read().await;
    let session = session_guard.as_ref()?;
    let session = session.read().await;

    let effect_tracker = session.effect_tracker()?;
    let tracker = effect_tracker.lock().ok()?;

    if !tracker.has_active_effects() {
        return None;
    }

    let mut effects: Vec<_> = tracker.effects_b().collect();
    effects.sort_by_key(|e| e.applied_at);

    let entries: Vec<EffectABEntry> = effects
        .into_iter()
        .filter_map(|effect| {
            let total_secs = effect.duration?.as_secs_f32();
            let remaining_secs = calculate_remaining_secs(effect)?;

            // Load icon from cache
            let icon = icon_cache.and_then(|cache| {
                cache.get_icon(effect.icon_ability_id).map(|data| {
                    StdArc::new((data.width, data.height, data.rgba))
                })
            });

            Some(EffectABEntry {
                effect_id: effect.game_effect_id,
                icon_ability_id: effect.icon_ability_id,
                name: effect.name.clone(),
                remaining_secs,
                total_secs,
                color: effect.color,
                stacks: effect.stacks,
                is_cleansable: effect.category == EffectCategory::Cleansable,
                source_name: resolve(effect.source_name).to_string(),
                target_name: resolve(effect.target_name).to_string(),
                icon,
                show_icon: effect.show_icon,
                display_source: effect.display_source,
            })
        })
        .collect();

    Some(EffectsABData { effects: entries })
}

/// Build cooldowns overlay data from active effects
async fn build_cooldowns_data(
    shared: &Arc<SharedState>,
    icon_cache: Option<&Arc<baras_overlay::icons::IconCache>>,
) -> Option<CooldownData> {
    use std::sync::Arc as StdArc;

    let session_guard = shared.session.read().await;
    let session = session_guard.as_ref()?;
    let session = session.read().await;

    let effect_tracker = session.effect_tracker()?;
    let tracker = effect_tracker.lock().ok()?;

    if !tracker.has_active_effects() {
        return None;
    }

    let mut effects: Vec<_> = tracker.cooldown_effects().collect();

    // Sort by remaining time (shortest first)
    effects.sort_by(|a, b| {
        let a_remaining = calculate_remaining_secs(a).unwrap_or(f32::MAX);
        let b_remaining = calculate_remaining_secs(b).unwrap_or(f32::MAX);
        a_remaining
            .partial_cmp(&b_remaining)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let entries: Vec<CooldownEntry> = effects
        .into_iter()
        .filter_map(|effect| {
            // Duration includes ready_secs for tracker lifetime, subtract for display
            let tracker_total = effect.duration?.as_secs_f32();
            let total_secs = tracker_total - effect.cooldown_ready_secs;

            // Remaining time until tracker expires
            let tracker_remaining = calculate_remaining_secs(effect)?;

            // Display remaining = tracker remaining minus ready period (clamped to 0)
            // So display hits 0 when entering ready state, not when effect disappears
            let remaining_secs = (tracker_remaining - effect.cooldown_ready_secs).max(0.0);

            // In ready state when tracker remaining is within the ready period
            let is_in_ready_state =
                effect.cooldown_ready_secs > 0.0 && tracker_remaining <= effect.cooldown_ready_secs;

            // Load icon from cache
            let icon = icon_cache.and_then(|cache| {
                cache.get_icon(effect.icon_ability_id).map(|data| {
                    StdArc::new((data.width, data.height, data.rgba))
                })
            });

            Some(CooldownEntry {
                ability_id: effect.game_effect_id,
                icon_ability_id: effect.icon_ability_id,
                name: effect.name.clone(),
                remaining_secs,
                total_secs,
                color: effect.color,
                charges: effect.stacks,
                max_charges: effect.stacks, // Default to current stacks (no max info available)
                source_name: resolve(effect.source_name).to_string(),
                target_name: resolve(effect.target_name).to_string(),
                icon,
                show_icon: effect.show_icon,
                display_source: effect.display_source,
                is_in_ready_state,
            })
        })
        .collect();

    Some(CooldownData { entries })
}

/// Build DOT tracker overlay data from active effects
async fn build_dot_tracker_data(
    shared: &Arc<SharedState>,
    icon_cache: Option<&Arc<baras_overlay::icons::IconCache>>,
) -> Option<DotTrackerData> {
    use std::sync::Arc as StdArc;
    use std::time::Instant;

    let session_guard = shared.session.read().await;
    let session = session_guard.as_ref()?;
    let session = session.read().await;

    let effect_tracker = session.effect_tracker()?;
    let tracker = effect_tracker.lock().ok()?;

    if !tracker.has_active_effects() {
        return None;
    }

    // Get DOTs grouped by target
    let dots_by_target = tracker.dot_tracker_effects();
    if dots_by_target.is_empty() {
        return None;
    }

    let mut targets: Vec<DotTarget> = dots_by_target
        .into_iter()
        .filter_map(|(target_id, effects)| {
            let target_name = resolve(effects.first()?.target_name).to_string();

            let dots: Vec<DotEntry> = effects
                .into_iter()
                .filter_map(|effect| {
                    let total_secs = effect.duration?.as_secs_f32();
                    let remaining_secs = calculate_remaining_secs(effect)?;

                    // Load icon from cache
                    let icon = icon_cache.and_then(|cache| {
                        cache.get_icon(effect.icon_ability_id).map(|data| {
                            StdArc::new((data.width, data.height, data.rgba))
                        })
                    });

                    Some(DotEntry {
                        effect_id: effect.game_effect_id,
                        icon_ability_id: effect.icon_ability_id,
                        name: effect.name.clone(),
                        remaining_secs,
                        total_secs,
                        color: effect.color,
                        stacks: effect.stacks,
                        source_name: resolve(effect.source_name).to_string(),
                        target_name: resolve(effect.target_name).to_string(),
                        icon,
                        show_icon: effect.show_icon,
                    })
                })
                .collect();

            if dots.is_empty() {
                return None;
            }

            Some(DotTarget {
                entity_id: target_id,
                name: target_name,
                dots,
                last_updated: Instant::now(),
            })
        })
        .collect();

    // Sort targets by entity ID for stable ordering
    targets.sort_by_key(|t| t.entity_id);

    Some(DotTrackerData { targets })
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
