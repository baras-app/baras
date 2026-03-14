//! Combat service - coordinates parsing, state management, and overlay updates
//!
//! Architecture:
//! - SharedState: Arc-wrapped state readable by Tauri commands (in crate::state)
//! - ServiceHandle: For sending commands + accessing shared state
//! - CombatService: Background task that processes commands and updates shared state
mod directory;
mod handler;
pub(crate) mod process_monitor;

use crate::state::SharedState;
pub use crate::state::{RaidSlotRegistry, RegisteredPlayer};
use baras_core::directory_watcher;
pub use handler::*;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{RwLock, mpsc};

use baras_core::context::{AppConfig, AppConfigExt, ChallengeColumns, DirectoryIndex, ParsingSession, resolve};
use baras_core::directory_watcher::DirectoryWatcher;
use baras_core::encounter::{EncounterState, PhaseType};
use baras_core::encounter::summary::classify_encounter;
use baras_core::game_data::{Discipline, Role};
use baras_core::timers::FiredAlert;
use baras_core::{
    ActiveEffect, BossEncounterDefinition, DefinitionConfig, DefinitionSet, DisplayTarget,
    EFFECTS_DSL_VERSION, EntityType, GameSignal, PlayerMetrics, Reader, SignalHandler,
};
use baras_overlay::{
    BossHealthData, ChallengeData, ChallengeEntry, Color, CooldownData, CooldownEntry, DotEntry,
    DotTarget, DotTrackerData, EffectABEntry, EffectsABData, NotesData, PersonalStats,
    PlayerContribution, PlayerRole, RaidEffect, RaidFrame, RaidFrameData, TimerData, TimerEntry,
};

use crate::audio::{AudioEvent, AudioSender, AudioService};
use tracing::{debug, error, info, warn};

// ─────────────────────────────────────────────────────────────────────────────
// Parse Worker IPC
// ─────────────────────────────────────────────────────────────────────────────

use baras_core::state::ParseWorkerOutput;

/// Fallback to streaming parse if subprocess fails.
/// Returns true if the session is in combat after parsing.
async fn fallback_streaming_parse(
    reader: &Reader,
    session: &Arc<RwLock<ParsingSession>>,
    encounters_dir: PathBuf,
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
    /// File was modified - re-check character data for files missing it
    FileModified(PathBuf),
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
    SetOperationTimerContext { operation_name: Option<String> },
    /// Auto-switch profile based on role change (triggered by DisciplineChanged)
    AutoSwitchProfile { role_name: String },
    /// Emit the current operation timer state to the frontend and overlay
    EmitOperationTimerTick,
}

// ─────────────────────────────────────────────────────────────────────────────
// Operation Timer State
// ─────────────────────────────────────────────────────────────────────────────

/// The type of area the player is currently in, for operation timer routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AreaKind {
    Operation,
    Flashpoint,
    PvP,
    Other,
}

impl AreaKind {
    /// Classify an area_id into an AreaKind.
    pub fn from_area_id(area_id: i64) -> Self {
        if baras_core::game_data::is_operation(area_id) {
            AreaKind::Operation
        } else if baras_core::game_data::is_flashpoint(area_id) {
            AreaKind::Flashpoint
        } else if baras_core::game_data::is_pvp_area(area_id) {
            AreaKind::PvP
        } else {
            AreaKind::Other
        }
    }

    /// Whether this area type should have the timer auto-start immediately on entry
    /// (as opposed to operations which wait for boss pull).
    pub fn auto_starts_on_entry(self) -> bool {
        matches!(self, AreaKind::Flashpoint | AreaKind::PvP)
    }

    /// Whether this area type should auto-reset the timer when entered.
    pub fn auto_resets_on_entry(self) -> bool {
        matches!(self, AreaKind::Operation | AreaKind::Flashpoint | AreaKind::PvP)
    }
}

/// Persistent timer state that tracks an entire operation run.
/// Lives in the service layer, independent of overlay visibility.
#[derive(Debug)]
pub struct OperationTimerState {
    /// When the timer was last started (None = stopped/not started)
    started_at: Option<std::time::Instant>,
    /// Accumulated seconds from previous start/stop cycles
    accumulated_secs: u64,
    /// Whether the user manually started (suppresses auto-start)
    manually_started: bool,
    /// Whether the user manually stopped (suppresses auto-start until reset)
    manually_stopped: bool,
    /// Current operation name (from AreaEntered signal)
    operation_name: Option<String>,
}

impl Default for OperationTimerState {
    fn default() -> Self {
        Self {
            started_at: None,
            accumulated_secs: 0,
            manually_started: false,
            manually_stopped: false,
            operation_name: None,
        }
    }
}

impl OperationTimerState {
    /// Get total elapsed seconds including current running segment
    pub fn elapsed_secs(&self) -> u64 {
        self.accumulated_secs
            + self
                .started_at
                .map(|s| s.elapsed().as_secs())
                .unwrap_or(0)
    }

    /// Whether the timer is currently running
    pub fn is_running(&self) -> bool {
        self.started_at.is_some()
    }

    /// Start the timer (no-op if already running)
    pub fn start(&mut self) {
        if self.started_at.is_none() {
            self.started_at = Some(std::time::Instant::now());
        }
    }

    /// Start the timer with a pre-seeded elapsed time (for live-resume backfill).
    /// Pre-accumulates `pre_secs` seconds so the timer immediately shows backfilled time.
    pub fn start_with_offset(&mut self, pre_secs: u64) {
        self.accumulated_secs = pre_secs;
        self.started_at = Some(std::time::Instant::now());
    }

    /// Stop the timer, accumulating elapsed time (marks as manually stopped).
    pub fn stop(&mut self) {
        if let Some(started) = self.started_at.take() {
            self.accumulated_secs += started.elapsed().as_secs();
        }
        self.manually_stopped = true;
    }

    /// Stop the timer without marking as manually stopped.
    /// Used for auto-stop on area exit.
    pub fn stop_auto(&mut self) {
        if let Some(started) = self.started_at.take() {
            self.accumulated_secs += started.elapsed().as_secs();
        }
        // Do NOT set manually_stopped — auto-starts should still work next instance
    }

    /// Stop the timer, correcting for the grace-period lag between when the boss
    /// actually died (game timestamp) and when the CombatEnded signal is dispatched
    /// (which can be 3+ seconds later due to the boss combat-exit grace window).
    ///
    /// `combat_ended_at`: the game-log timestamp on the CombatEnded signal.
    /// We measure how far in the past that timestamp is (relative to wall clock)
    /// and subtract that overshoot from the accumulated total.
    pub fn stop_auto_at(&mut self, combat_ended_at: chrono::NaiveDateTime) {
        if let Some(started) = self.started_at.take() {
            let raw_elapsed = started.elapsed().as_secs();
            // Compute how many seconds ago the game event actually occurred.
            // chrono::Local::now().naive_local() gives the current local time;
            // combat_ended_at is the game-log timestamp (local time).
            let overshoot_secs = chrono::Local::now()
                .naive_local()
                .signed_duration_since(combat_ended_at)
                .num_seconds()
                .max(0) as u64;
            // Subtract the overshoot (grace period + dispatch lag), clamped to zero.
            let corrected = raw_elapsed.saturating_sub(overshoot_secs);
            self.accumulated_secs += corrected;
        }
        // Do NOT set manually_stopped
    }

    /// Reset all timer state
    pub fn reset(&mut self) {
        self.started_at = None;
        self.accumulated_secs = 0;
        self.manually_started = false;
        self.manually_stopped = false;
    }

    /// Build overlay data from current state
    pub fn to_overlay_data(&self) -> baras_overlay::OperationTimerData {
        baras_overlay::OperationTimerData {
            elapsed_secs: self.elapsed_secs(),
            is_running: self.is_running(),
            operation_name: self.operation_name.clone(),
        }
    }
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
    /// Ability cooldowns
    CooldownsUpdated(CooldownData),
    /// DOTs on enemy targets
    DotTrackerUpdated(DotTrackerData),
    /// Encounter notes (sent when entering an area with boss definitions)
    NotesUpdated(NotesData),
    /// Operation timer update (persistent across encounters)
    OperationTimerUpdated(baras_overlay::OperationTimerData),
    /// Clear all overlay data (sent when switching files)
    ClearAllData,
    /// Local player entered conversation - temporarily hide overlays
    ConversationStarted,
    /// Local player exited conversation - restore overlays if we hid them
    ConversationEnded,
    /// Session liveness changed (historical mode entered/exited, player logged out/in)
    NotLiveStateChanged { is_live: bool },
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
    /// Channel for frontend session updates (event-driven, not polled)
    session_event_tx: std::sync::mpsc::Sender<SessionEvent>,
    /// Channel for overlay updates (to clear overlays on combat end)
    overlay_tx: mpsc::Sender<OverlayUpdate>,
    /// Channel for service commands (to reload area definitions on area change)
    cmd_tx: mpsc::Sender<ServiceCommand>,
    /// Local player entity ID (set on first DisciplineChanged)
    local_player_id: Option<i64>,
    /// Current role of the local player (for auto-switching profiles on role change)
    current_role: Option<Role>,
    /// Whether we've already requested the process monitor for this tailing session
    monitor_requested: bool,
    /// Whether the active boss definition has is_final_boss = true.
    /// Captured in BossEncounterDetected and consumed in CombatEnded.
    /// (The _encounter arg in CombatEnded is already the new empty encounter,
    ///  so we must snapshot this flag earlier.)
    current_boss_is_final: bool,
    /// The type of area the player is currently in.
    /// Used to detect FP/PvP → other transitions for auto-stop.
    current_area_kind: AreaKind,
}

impl CombatSignalHandler {
    fn new(
        shared: Arc<SharedState>,
        trigger_tx: mpsc::Sender<MetricsTrigger>,
        session_event_tx: std::sync::mpsc::Sender<SessionEvent>,
        overlay_tx: mpsc::Sender<OverlayUpdate>,
        cmd_tx: mpsc::Sender<ServiceCommand>,
    ) -> Self {
        Self {
            shared,
            trigger_tx,
            session_event_tx,
            overlay_tx,
            cmd_tx,
            local_player_id: None,
            current_role: None,
            monitor_requested: false,
            current_boss_is_final: false,
            current_area_kind: AreaKind::Other,
        }
    }
}

impl SignalHandler for CombatSignalHandler {
    fn handle_signal(
        &mut self,
        signal: &GameSignal,
        _encounter: Option<&baras_core::encounter::CombatEncounter>,
    ) {
        // On first event processed, start monitoring the game process.
        // Always monitor so game_running state is accurate for stale detection.
        if !self.monitor_requested {
            self.monitor_requested = true;
            let _ = self.cmd_tx.try_send(ServiceCommand::StartProcessMonitor);
        }

        match signal {
            GameSignal::CombatStarted { .. } => {
                self.shared.in_combat.store(true, Ordering::SeqCst);
                let _ = self.trigger_tx.try_send(MetricsTrigger::CombatStarted);
                let _ = self.session_event_tx.send(SessionEvent::CombatStarted);
                // If overlays were auto-hidden for not-live, restore them — combat means live
                if self.shared.auto_hide.is_not_live_active() {
                    let _ = self
                        .overlay_tx
                        .try_send(OverlayUpdate::NotLiveStateChanged { is_live: true });
                }
            }
            GameSignal::CombatEnded { timestamp, .. } => {
                self.shared.in_combat.store(false, Ordering::SeqCst);
                let _ = self.trigger_tx.try_send(MetricsTrigger::CombatEnded);
                let _ = self.session_event_tx.send(SessionEvent::CombatEnded);
                // Clear boss health and timer overlays
                let _ = self.overlay_tx.try_send(OverlayUpdate::CombatEnded);

                // Auto-stop operation timer when the final boss is killed.
                //
                // NOTE: `_encounter` at this point is the brand-new empty encounter
                // (push_new_encounter() already ran before signal dispatch), so we
                // cannot call enc.active_boss_definition() here — it would return None.
                // Instead we use `current_boss_is_final`, which was snapshotted in the
                // preceding BossEncounterDetected signal.
                //
                // We use `_encounter` (the new encounter's predecessor in history) only
                // to determine success — but since the encounter has already been moved to
                // history by the time we get here, we rely on the fact that if is_final_boss
                // is set and we were in an operation, combat ending == a successful clear.
                // (Wipes also fire CombatEnded, so we additionally check determine_success
                // on the encounter that was passed — but since _encounter is now the new
                // one, we check the area type alone for ops and trust that a wipe restarts
                // the fight without hitting is_final_boss again.)
                //
                // Only in live mode - historical replays should not drive the timer.
                if self.shared.is_live_tailing.load(Ordering::SeqCst) && self.current_boss_is_final {
                    let area_kind = self.current_area_kind;
                    if matches!(area_kind, AreaKind::Operation | AreaKind::Flashpoint) {
                        let mut timer = self.shared.operation_timer.lock().unwrap();
                        if timer.is_running() {
                            // Use the game-log timestamp to subtract the grace-period lag
                            // (CombatEnded is dispatched ~3s after the actual kill event).
                            timer.stop_auto_at(*timestamp);
                        }
                        drop(timer);
                        let _ = self.cmd_tx.try_send(ServiceCommand::EmitOperationTimerTick);
                    }
                }
                // Reset the flag — next boss may or may not be final
                self.current_boss_is_final = false;
            }
            GameSignal::DisciplineChanged {
                entity_id,
                class_id,
                discipline_id,
                ..
            } => {
                // First DisciplineChanged is always the local player
                if self.local_player_id.is_none() {
                    self.local_player_id = Some(*entity_id);
                }
                // Update raid registry with discipline info for role icons
                let mut registry = self.shared.raid_registry.lock().unwrap_or_else(|p| p.into_inner());
                registry.update_discipline(*entity_id, *class_id, *discipline_id);
                // Notify frontend of player info change
                let _ = self.session_event_tx.send(SessionEvent::PlayerInitialized);
                // Player just initialized — if not-live auto-hide is active, the session
                // is now live. Re-evaluate so overlays restore without waiting for combat.
                if self.shared.auto_hide.is_not_live_active() {
                    let _ = self
                        .overlay_tx
                        .try_send(OverlayUpdate::NotLiveStateChanged { is_live: true });
                }
                // Auto-switch profile on role change (only for local player)
                if self.local_player_id == Some(*entity_id) {
                    if let Some(discipline) = Discipline::from_guid(*discipline_id) {
                        let new_role = discipline.role();
                        let role_changed = self.current_role.map_or(true, |r| r != new_role);
                        self.current_role = Some(new_role);
                        if role_changed {
                            let role_name = format!("{:?}", new_role);
                            let _ = self.cmd_tx.try_send(ServiceCommand::AutoSwitchProfile { role_name });
                        }
                    }
                }
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
                // Note: Boss definitions are loaded synchronously in process_event via definition_loader
                let current = self.shared.current_area_id.load(Ordering::SeqCst);
                if *area_id != current {
                    self.shared
                        .current_area_id
                        .store(*area_id, Ordering::SeqCst);
                    let _ = self.session_event_tx.send(SessionEvent::AreaChanged);
                    // Trigger area definition reload (will send notes or clear them)
                    let _ = self.cmd_tx.try_send(ServiceCommand::ReloadAreaDefinitions(*area_id));

                    // ── Operation timer area-transition logic ────────────────────
                    // Only in live mode - historical replays should not drive the timer
                    if self.shared.is_live_tailing.load(Ordering::SeqCst) {
                        let prev_kind = self.current_area_kind;
                        let new_kind = AreaKind::from_area_id(*area_id);
                        self.current_area_kind = new_kind;

                        // Determine the display name for this area.
                        // For non-timed areas (fleet/open world) we don't update the name —
                        // the previous instance name is preserved on the overlay so the user
                        // can still see what run the timer belongs to after returning to fleet.
                        let area_display_name: Option<String> = match new_kind {
                            AreaKind::Operation => baras_core::game_data::get_operation_name(*area_id)
                                .map(|s| s.to_string()),
                            AreaKind::Flashpoint => baras_core::game_data::get_flashpoint_name(*area_id)
                                .map(|s| s.to_string()),
                            AreaKind::PvP => Some("PvP".to_string()),
                            AreaKind::Other => None, // unused below — name preserved
                        };

                        // If entering a timed area (op/FP/PvP): reset the timer for a fresh run.
                        // This handles requirement 3: auto-clear on new instance.
                        // Clearing manually_stopped here is intentional — a new area = new run.
                        if new_kind.auto_resets_on_entry() {
                            let mut timer = self.shared.operation_timer.lock().unwrap();
                            timer.reset();
                            timer.operation_name = area_display_name.clone();
                            drop(timer);
                        }
                        // When returning to open world / fleet, do NOT touch operation_name.
                        // The name stays on the overlay so the user can see which run the
                        // (now-stopped) timer belongs to.

                        // If leaving a FP/PvP area, auto-stop the timer.
                        if matches!(prev_kind, AreaKind::Flashpoint | AreaKind::PvP)
                            && !matches!(new_kind, AreaKind::Flashpoint | AreaKind::PvP)
                        {
                            let mut timer = self.shared.operation_timer.lock().unwrap();
                            if timer.is_running() {
                                timer.stop_auto();
                            }
                            drop(timer);
                            let _ = self.cmd_tx.try_send(ServiceCommand::EmitOperationTimerTick);
                        }

                        // If entering a FP/PvP area: immediately start the timer
                        // (requirement 1: no boss pull needed).
                        if new_kind.auto_starts_on_entry() {
                            let mut timer = self.shared.operation_timer.lock().unwrap();
                            if !timer.is_running() && !timer.manually_stopped {
                                timer.start();
                                drop(timer);
                                let _ = self.cmd_tx.try_send(ServiceCommand::EmitOperationTimerTick);
                            }
                        }
                    } else {
                        // Historical mode: still track area kind so backfill works on resume
                        self.current_area_kind = AreaKind::from_area_id(*area_id);
                    }

                    // Legacy context update for operations (kept for backward compatibility;
                    // the name is now set above in the timer block, but this handles the
                    // case where live tailing is false at signal time).
                    if baras_core::game_data::is_operation(*area_id)
                        && !self.shared.is_live_tailing.load(Ordering::SeqCst)
                    {
                        let op_name = baras_core::game_data::get_operation_name(*area_id)
                            .map(|s| s.to_string());
                        let _ = self.cmd_tx.try_send(
                            ServiceCommand::SetOperationTimerContext {
                                operation_name: op_name,
                            },
                        );
                    }
                }
            }
            GameSignal::BossEncounterDetected {
                definition_idx,
                boss_name,
                ..
            } => {
                // Snapshot is_final_boss from this boss definition.
                // We must do it here because by the time CombatEnded fires, push_new_encounter()
                // has already run and _encounter will be the new, empty encounter.
                self.current_boss_is_final = if let Some(enc) = _encounter {
                    enc.boss_definitions()
                        .get(*definition_idx)
                        .map(|d| d.is_final_boss)
                        .unwrap_or(false)
                } else {
                    false
                };

                // Send notes for this specific boss to the overlay
                if let Some(enc) = _encounter {
                    if let Some(def) = enc.boss_definitions().get(*definition_idx) {
                        if let Some(notes) = &def.notes {
                            if !notes.is_empty() {
                                let notes_data = NotesData {
                                    text: notes.clone(),
                                    boss_name: boss_name.clone(),
                                };
                                let _ = self.overlay_tx.try_send(OverlayUpdate::NotesUpdated(notes_data));
                            }
                        }
                    }
                }

                // Auto-start operation timer on first boss pull in an operation.
                // Start directly (not via command channel) to avoid 1-second tick delay.
                // Only in live mode - historical replays should not drive the timer.
                if self.shared.is_live_tailing.load(Ordering::SeqCst) {
                    if matches!(self.current_area_kind, AreaKind::Operation) {
                        let mut timer = self.shared.operation_timer.lock().unwrap();
                        if !timer.is_running() && !timer.manually_stopped {
                            timer.start();
                            drop(timer);
                            let _ = self.cmd_tx.try_send(ServiceCommand::EmitOperationTimerTick);
                        }
                    }
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
    ) -> (Self, ServiceHandle) {
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
                    .ancestors()
                    .nth(2)
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| PathBuf::from("."))
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

        let handle = ServiceHandle { cmd_tx, shared: shared.clone(), app_handle: app_handle.clone() };

        // Spawn background area indexer to populate file area cache
        Self::spawn_area_indexer(shared, area_index_for_scanner, app_handle);

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
        {
            match build_area_index(path) {
                Ok(area_index) => index.extend(area_index),
                Err(e) => warn!(path = %path.display(), error = %e, "Failed to build bundled area index"),
            }
        }

        // Build index from custom directory (can override bundled)
        if let Some(ref path) = custom_dir
            && path.exists()
        {
            match build_area_index(path) {
                Ok(area_index) => index.extend(area_index),
                Err(e) => warn!(path = %path.display(), error = %e, "Failed to build custom area index"),
            }
        }

        index
    }

    /// Spawn a background task to index areas in log files.
    /// This scans files that aren't already in the cache (except the newest file).
    /// Runs silently in background without blocking normal app operations.
    fn spawn_area_indexer(
        shared: Arc<SharedState>,
        area_index: Arc<baras_core::boss::AreaIndex>,
        app_handle: AppHandle,
    ) {
        use baras_core::context::{
            FileAreaIndex, LogAreaCache, default_cache_path, extract_areas_from_file,
        };
        use std::collections::HashSet;

        tauri::async_runtime::spawn(async move {
            // Small delay to let app finish initializing - don't block startup
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;

            // Load existing cache from disk
            let cache_path = match default_cache_path() {
                Some(p) => p,
                None => {
                    warn!("Could not determine area cache path");
                    return;
                }
            };

            let mut cache = LogAreaCache::load_from_disk(&cache_path);
            debug!(
                cached_files = cache.len(),
                "Loaded area cache from disk"
            );

            // Update shared state with loaded cache (brief write lock)
            // Don't prune here - stale entries are harmless and pruning is slow
            {
                let mut area_cache = shared.area_cache.write().await;
                *area_cache = cache.clone();
            }
            // Don't emit event here - let frontend use its initial load

            // Get known area IDs from boss definitions
            let known_area_ids: HashSet<i64> = area_index.keys().copied().collect();
            if known_area_ids.is_empty() {
                debug!("No area definitions found, skipping area indexing");
                return;
            }

            // Collect file paths to scan quickly, then release lock
            let files_to_scan: Vec<(PathBuf, std::time::SystemTime)> = {
                let index = shared.directory_index.read().await;
                let newest_path = index.newest_file().map(|f| f.path.clone());
                index
                    .entries()
                    .iter()
                    .filter_map(|e| {
                        // Skip empty files
                        if e.is_empty {
                            return None;
                        }
                        // Skip the newest file (it's being actively written)
                        if Some(&e.path) == newest_path.as_ref() {
                            return None;
                        }
                        // Get modification time
                        let modified = std::fs::metadata(&e.path)
                            .and_then(|m| m.modified())
                            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                        // Check if needs update
                        if cache.needs_update(&e.path, modified) {
                            Some((e.path.clone(), modified))
                        } else {
                            None
                        }
                    })
                    .collect()
            };
            // Lock released here

            if files_to_scan.is_empty() {
                debug!("All files already indexed, nothing to scan");
                return;
            }

            debug!(
                files_to_scan = files_to_scan.len(),
                "Starting background area indexing"
            );

            // Scan files without holding any locks
            let mut scanned = 0;
            for (path, modified) in files_to_scan {
                let modified_secs = modified
                    .duration_since(std::time::SystemTime::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);

                match extract_areas_from_file(&path, &known_area_ids) {
                    Ok(areas) => {
                        cache.insert(
                            path.clone(),
                            FileAreaIndex {
                                modified_secs,
                                areas,
                            },
                        );
                        scanned += 1;
                    }
                    Err(e) => {
                        debug!(path = %path.display(), error = %e, "Failed to extract areas from file");
                    }
                }

                // Yield periodically to avoid starving other tasks
                if scanned % 10 == 0 {
                    tokio::task::yield_now().await;
                }
            }

            // Prune entries for deleted files (lazy cleanup)
            let cache_size_before = cache.len();
            cache.prune_missing();
            let pruned = cache_size_before - cache.len();
            if pruned > 0 {
                debug!(pruned, "Pruned stale entries from area cache");
            }

            // Save updated cache and notify frontend if anything changed
            if scanned > 0 || pruned > 0 {
                // Brief write lock to update shared state
                {
                    let mut area_cache = shared.area_cache.write().await;
                    *area_cache = cache.clone();
                }

                if let Err(e) = cache.save_to_disk(&cache_path) {
                    warn!(error = %e, "Failed to save area cache to disk");
                } else {
                    info!(scanned, pruned, "Area indexing complete, cache saved");
                }

                // Notify frontend that area data is now available
                let _ = app_handle.emit("log-files-changed", ());
            }
        });
    }

    /// Load boss definitions for a specific area, merging with custom overlays
    fn load_area_definitions(&self, area_id: i64) -> Option<Vec<BossEncounterDefinition>> {
        use baras_core::boss::load_bosses_with_custom;

        let entry = self.area_index.get(&area_id)?;

        // User custom directory for overlay files
        let user_dir = dirs::config_dir().map(|p| p.join("baras").join("definitions").join("encounters"));

        match load_bosses_with_custom(&entry.file_path, user_dir.as_deref()) {
            Ok(bosses) => Some(bosses),
            Err(e) => {
                warn!(
                    area_id,
                    path = %entry.file_path.display(),
                    error = %e,
                    "Failed to load boss definitions"
                );
                None
            }
        }
    }

    /// Get the path to the timer preferences file
    fn timer_preferences_path() -> Option<std::path::PathBuf> {
        dirs::config_dir().map(|p| p.join("baras").join("timer_preferences.toml"))
    }

    /// Initialize the icon cache for ability icons
    fn init_icon_cache(app_handle: &AppHandle) -> Option<Arc<baras_overlay::icons::IconCache>> {
        use baras_overlay::icons::IconCache;

        debug!("Initializing icon cache");

        // Try bundled resources first, fall back to dev path
        let icons_dir = app_handle
            .path()
            .resolve("icons", tauri::path::BaseDirectory::Resource)
            .ok()
            .filter(|p| p.exists())
            .unwrap_or_else(|| {
                // Dev fallback: relative to project root
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .ancestors()
                    .nth(2)
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join("icons")
            });

        debug!(path = ?icons_dir, "Looking for icons");

        let csv_path = icons_dir.join("icons.csv");
        let zip_path = icons_dir.join("icons.zip");

        if !csv_path.exists() || !zip_path.exists() {
            debug!(
                path = ?icons_dir,
                csv_exists = csv_path.exists(),
                zip_exists = zip_path.exists(),
                "Icon files not found"
            );
            return None;
        }

        match IconCache::new(&csv_path, &zip_path, 200) {
            Ok(cache) => {
                info!(path = ?icons_dir, "Loaded icon cache");
                Some(Arc::new(cache))
            }
            Err(e) => {
                error!(error = %e, "Failed to load icon cache");
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
        let Some(old_dir) =
            dirs::config_dir().map(|p| p.join("baras").join("definitions").join("effects"))
        else {
            return;
        };

        if old_dir.is_dir() {
            debug!(path = ?old_dir, "Removing old effects directory");
            if let Err(e) = std::fs::remove_dir_all(&old_dir) {
                error!(error = %e, path = ?old_dir, "Failed to remove old effects directory");
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
            debug!(path = ?dir, "Failed to read bundled effects directory");
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

        debug!(count = files.len(), path = ?dir, "Loading bundled effect files");

        for path in files {
            if let Ok(contents) = std::fs::read_to_string(&path)
                && let Ok(config) = toml::from_str::<DefinitionConfig>(&contents)
            {
                let count = config.effects.len();
                set.add_definitions(config.effects, false);
                debug!(
                    file = ?path.file_name().unwrap_or_default(),
                    count,
                    "Loaded effect definitions"
                );
            }
        }
    }

    /// Load user effect overrides from single config file
    fn load_user_effects(set: &mut DefinitionSet, path: &std::path::Path) {
        let Ok(contents) = std::fs::read_to_string(path) else {
            error!(path = ?path, "Failed to read user effects file");
            return;
        };

        let Ok(config) = toml::from_str::<DefinitionConfig>(&contents) else {
            error!(path = ?path, "Failed to parse user effects file");
            // Delete invalid file
            let _ = std::fs::remove_file(path);
            return;
        };

        // Version check - delete file if version mismatch
        if config.version != EFFECTS_DSL_VERSION {
            warn!(
                file_version = config.version,
                expected_version = EFFECTS_DSL_VERSION,
                path = ?path,
                "User effects version mismatch, deleting file"
            );
            let _ = std::fs::remove_file(path);
            return;
        }

        if !config.effects.is_empty() {
            debug!(count = config.effects.len(), path = ?path, "Loading user effect overrides");
            set.add_definitions(config.effects, true); // Overwrite bundled
        }
    }

    /// Run the service event loop
    pub async fn run(mut self) {
        // Auto-cleanup log files on startup based on user settings
        {
            let config = self.shared.config.read().await;
            let delete_empty = config.auto_delete_empty_files;
            let delete_small = config.auto_delete_small_files;
            let retention = if config.auto_delete_old_files {
                Some(config.log_retention_days)
            } else {
                None
            };
            drop(config);

            if delete_empty || delete_small || retention.is_some() {
                let mut index = self.shared.directory_index.write().await;
                let (empty, small, old) = index.cleanup(delete_empty, delete_small, retention);
                if empty > 0 || small > 0 || old > 0 {
                    tracing::info!(empty_deleted = empty, small_deleted = small, old_deleted = old, "Startup log cleanup");
                }
            }
        }

        self.start_watcher().await;

        // Operation timer tick interval - 1 second precision
        let mut op_timer_interval = tokio::time::interval(std::time::Duration::from_secs(1));
        op_timer_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                cmd = self.cmd_rx.recv() => {
                    let Some(cmd) = cmd else { break; };

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
                        ServiceCommand::OpenHistoricalFile(path) => {
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
                                let mut timer = self.shared.operation_timer.lock().unwrap();
                                timer.reset();
                            }
                            self.emit_operation_timer_tick();
                            self.start_tailing(path).await;
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
                                let session_guard = self.shared.session.read().await;
                                if let Some(session) = session_guard.as_ref() {
                                    let mut session = session.write().await;
                                    session.load_boss_definitions(bosses, false);
                                }
                            } else {
                                // No definitions for this area - clear notes
                                let _ = self.overlay_tx.try_send(OverlayUpdate::NotesUpdated(NotesData::default()));
                            }
                        }
                        ServiceCommand::StartOperationTimer => {
                            // Only allow in live mode - not during historical replay
                            if self.shared.is_live_tailing.load(Ordering::SeqCst) {
                                let mut timer = self.shared.operation_timer.lock().unwrap();
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
                                let mut timer = self.shared.operation_timer.lock().unwrap();
                                timer.stop();
                                drop(timer);
                                self.emit_operation_timer_tick();
                                // Also send final state to overlay
                                let data = self.shared.operation_timer.lock().unwrap().to_overlay_data();
                                let _ = self.overlay_tx.try_send(OverlayUpdate::OperationTimerUpdated(data));
                            }
                        }
                        ServiceCommand::ResetOperationTimer => {
                            // Only allow in live mode - not during historical replay
                            if self.shared.is_live_tailing.load(Ordering::SeqCst) {
                                let mut timer = self.shared.operation_timer.lock().unwrap();
                                timer.reset();
                                drop(timer);
                                self.emit_operation_timer_tick();
                                // Send cleared state to overlay
                                let data = self.shared.operation_timer.lock().unwrap().to_overlay_data();
                                let _ = self.overlay_tx.try_send(OverlayUpdate::OperationTimerUpdated(data));
                            }
                        }
                        ServiceCommand::SetOperationTimerContext { operation_name } => {
                            let mut timer = self.shared.operation_timer.lock().unwrap();
                            timer.operation_name = operation_name;
                        }
                        ServiceCommand::EmitOperationTimerTick => {
                            self.emit_operation_timer_tick();
                        }
                        ServiceCommand::AutoSwitchProfile { role_name } => {
                            let config = self.shared.config.read().await;
                            if let Some(profile_name) = config.default_profile_per_role.get(&role_name) {
                                // Skip if this profile is already active
                                let already_active = config.active_profile_name.as_deref() == Some(profile_name);
                                // Skip if the profile no longer exists
                                let profile_exists = config.profiles.iter().any(|p| p.name == *profile_name);
                                let profile_name = profile_name.clone();
                                drop(config);

                                if !already_active && profile_exists {
                                    let mut config = self.shared.config.write().await;
                                    if config.load_profile(&profile_name).is_ok() {
                                        let saved = config.clone().save();
                                        drop(config);
                                        if let Err(e) = saved {
                                            warn!("Failed to save config after auto-switching profile: {e}");
                                        } else {
                                            info!("Auto-switched to profile '{}' for role {}", profile_name, role_name);
                                            let _ = self.app_handle.emit("profile-auto-switched", &profile_name);
                                        }
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
                        let is_running = self.shared.operation_timer.lock()
                            .map(|t| t.is_running())
                            .unwrap_or(false);
                        if is_running {
                            self.emit_operation_timer_tick();
                        }
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

    /// Reload timer and boss definitions from disk and update the active session.
    /// Invalidates the timer cache first to ensure definitions actually reload.
    async fn reload_timer_definitions(&mut self) {
        self.area_index = Arc::new(Self::build_area_index(&self.app_handle));

        let current_area = self.shared.current_area_id.load(Ordering::SeqCst);
        if current_area == 0 {
            return;
        }
        
        let Some(bosses) = self.load_area_definitions(current_area) else {
            return;
        };
        
        // Send notes from boss definitions to overlay (first boss with notes)
        self.send_notes_from_bosses(&bosses);
        
        let session_guard = self.shared.session.read().await;
        let Some(session) = session_guard.as_ref() else {
            return;
        };
        
        let mut session = session.write().await;
        // Invalidate timer cache to ensure definitions actually reload
        // (bypasses fingerprint optimization for user-triggered reloads)
        if let Some(timer_mgr) = session.timer_manager() {
            if let Ok(mut mgr) = timer_mgr.lock() {
                mgr.invalidate_definitions_cache();
            }
        }
        session.load_boss_definitions(bosses, true);
    }

    /// Send notes from boss definitions to the notes overlay (only in live mode)
    fn send_notes_from_bosses(&self, bosses: &[BossEncounterDefinition]) {
        // Only send notes in live tailing mode (not for historical files)
        if !self.shared.is_live_tailing.load(Ordering::SeqCst) {
            return;
        }
        
        // Find the first boss with notes (or aggregate all notes)
        // For now, send notes from first boss that has them
        for boss in bosses {
            if let Some(notes) = &boss.notes {
                if !notes.is_empty() {
                    let notes_data = NotesData {
                        text: notes.clone(),
                        boss_name: boss.name.clone(),
                    };
                    let _ = self.overlay_tx.try_send(OverlayUpdate::NotesUpdated(notes_data));
                    return;
                }
            }
        }
        
        // No notes found - send empty to clear the overlay
        let _ = self.overlay_tx.try_send(OverlayUpdate::NotesUpdated(NotesData::default()));
    }

    /// Emit the current operation timer state to the frontend via Tauri event
    /// and to the overlay via the overlay update channel.
    fn emit_operation_timer_tick(&self) {
        let timer = self.shared.operation_timer.lock().unwrap();
        let data = timer.to_overlay_data();
        drop(timer);

        // Send to frontend (session box UI)
        let _ = self.app_handle.emit("operation-timer-tick", serde_json::json!({
            "elapsed_secs": data.elapsed_secs,
            "is_running": data.is_running,
            "operation_name": data.operation_name,
        }));

        // Send to overlay
        let _ = self.overlay_tx.try_send(OverlayUpdate::OperationTimerUpdated(data));
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
        let _ = self
            .overlay_tx
            .try_send(OverlayUpdate::NotLiveStateChanged { is_live: true });

        // Start new watcher (reads directory from config)
        self.start_watcher().await;
    }
    async fn file_detected(&mut self, path: PathBuf) {
        // Always update the index
        {
            let mut index = self.shared.directory_index.write().await;
            index.add_file(&path);
        }

        // Trigger area indexer to scan any files that aren't indexed yet
        // (the previous "newest" file is now complete and should be indexed)
        Self::spawn_area_indexer(
            self.shared.clone(),
            self.area_index.clone(),
            self.app_handle.clone(),
        );

        // Notify frontend that file list changed
        let _ = self.app_handle.emit("log-files-changed", ());

        // Only auto-switch if in live tailing mode
        if !self.shared.is_live_tailing.load(Ordering::SeqCst) {
            return;
        }

        let (is_newest, is_empty) = {
            let index = self.shared.directory_index.read().await;
            match index.newest_file() {
                Some(f) if f.path == path => (true, f.is_empty),
                _ => (false, false),
            }
        };

        if !is_newest {
            return;
        }

        // Check if current session has player initialized
        let has_player = {
            let session_guard = self.shared.session.read().await;
            if let Some(session) = session_guard.as_ref() {
                let s = session.read().await;
                s.session_cache
                    .as_ref()
                    .map(|c| c.player_initialized)
                    .unwrap_or(false)
            } else {
                false
            }
        };

        // If new file is empty and we have existing player data, defer the switch
        // This preserves the current session for viewing/uploading until new data arrives
        if is_empty && has_player {
            info!(
                new_file = %path.display(),
                "Empty log file detected, deferring switch until content arrives"
            );
            self.pending_file = Some(path);
            // Start polling for content — the OS may not deliver a separate
            // Modify event after the Create (Windows coalesces them).
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
            interval.tick().await; // consume the immediate first tick
            self.pending_file_interval = Some(interval);
            // Emit event so frontend can show "session ended" indicator
            let _ = self.app_handle.emit("session-ended", ());
            let _ = self
                .overlay_tx
                .try_send(OverlayUpdate::NotLiveStateChanged { is_live: false });
            return;
        }

        info!(
            new_file = %path.display(),
            "Log file rotation detected, switching to new file"
        );
        self.pending_file = None;
        self.pending_file_interval = None;
        self.start_tailing(path).await;
    }

    /// Handle file modification - re-check character data for files that were missing it
    async fn file_modified(&mut self, path: PathBuf) {
        let updated = {
            let mut index = self.shared.directory_index.write().await;
            // Check if this specific file needs character re-extraction
            if index.is_missing_character(&path) {
                // Re-run character extraction since file may now have content
                index.refresh_missing_characters()
            } else {
                0
            }
        };

        if updated > 0 {
            debug!(updated, "Re-read character names from modified files");
            // Notify frontend that file list changed (display names may have updated)
            let _ = self.app_handle.emit("log-files-changed", ());
        }

        // If this is the pending file, delegate to the shared resolution logic
        if self.pending_file.as_ref() == Some(&path) {
            self.check_pending_file().await;
        }
    }

    /// Check if the pending file now has content and switch to it if so.
    /// Called both from `file_modified()` (on OS events) and from the poll
    /// timer (as a fallback when the OS coalesces Create+Modify events).
    async fn check_pending_file(&mut self) {
        let path = match self.pending_file.clone() {
            Some(p) => p,
            None => {
                // No pending file — disable the poll timer if it's running
                self.pending_file_interval = None;
                return;
            }
        };

        // Re-stat the file and try to extract character data
        {
            let mut index = self.shared.directory_index.write().await;
            if index.is_missing_character(&path) {
                index.refresh_missing_characters();
            }
        }

        let has_content = {
            let index = self.shared.directory_index.read().await;
            index
                .newest_file()
                .map(|f| f.path == path && !f.is_empty)
                .unwrap_or(false)
        };

        if has_content {
            info!(
                file = %path.display(),
                "Pending file now has content, switching"
            );
            self.pending_file = None;
            self.pending_file_interval = None;
            let _ = self
                .overlay_tx
                .try_send(OverlayUpdate::NotLiveStateChanged { is_live: true });
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
        if !dir.exists() {
            warn!(directory = %dir.display(), "Log directory does not exist");
            return;
        }
        if !dir.is_dir() {
            warn!(directory = %dir.display(), "Log directory path is not a directory");
            return;
        }

        // Phase 1: Quick-index only the newest file so the user can start working immediately
        match directory_watcher::build_index_newest(&dir) {
            Ok((quick_index, newest)) => {
                {
                    let mut index_guard = self.shared.directory_index.write().await;
                    *index_guard = quick_index;
                }
                let _ = self.app_handle.emit("log-files-changed", ());

                // Auto-load newest file if available
                if let Some(ref newest_path) = newest {
                    self.start_tailing(newest_path.clone()).await;
                }
            }
            Err(e) => {
                error!(directory = %dir.display(), error = %e, "Failed to quick-index newest file");
                let _ = self.app_handle.emit("directory-error", e);
            }
        }

        // Phase 2: Backfill the full index in the background using the disk cache.
        // This avoids re-opening thousands of files on every startup.
        let backfill_dir = dir.clone();
        let backfill_shared = self.shared.clone();
        let backfill_app_handle = self.app_handle.clone();
        tokio::task::spawn_blocking(move || {
            let cache_path = DirectoryIndex::default_cache_path()
                .unwrap_or_else(|| PathBuf::from("directory_cache.json"));
            match directory_watcher::build_index_cached(&backfill_dir, &cache_path) {
                Ok((full_index, _)) => {
                    let shared = backfill_shared;
                    let app_handle = backfill_app_handle;
                    // Use a blocking approach to write into the async RwLock since
                    // we're on a spawn_blocking thread
                    let rt = tokio::runtime::Handle::current();
                    rt.block_on(async {
                        let mut index_guard = shared.directory_index.write().await;
                        index_guard.merge_from(full_index);
                    });
                    let _ = app_handle.emit("log-files-changed", ());
                    info!("Background index backfill complete");
                }
                Err(e) => {
                    error!(error = %e, "Background index backfill failed");
                }
            }
        });

        let mut watcher = match DirectoryWatcher::new(&dir) {
            Ok(w) => w,
            Err(e) => {
                error!(directory = %dir.display(), error = %e, "Failed to create directory watcher");
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
        info!(path = %path.display(), "Starting to tail log file");
        self.stop_tailing().await;

        // Clear old parquet data from previous session
        if let Err(e) = baras_core::storage::clear_data_dir() {
            warn!(error = %e, "Failed to clear data directory");
        }

        // Clear all overlay data when switching files
        let _ = self.overlay_tx.try_send(OverlayUpdate::ClearAllData);

        // Clear raid registry when switching files (new session = fresh state)
        self.shared.raid_registry.lock().unwrap_or_else(|p| p.into_inner()).clear();

        // Create trigger channel for signal-driven metrics updates (tokio channel - no spawn_blocking needed)
        let (trigger_tx, mut trigger_rx) = mpsc::channel::<MetricsTrigger>(8);
        // Create channel for frontend session events (replaces polling)
        let (session_event_tx, session_event_rx) = std::sync::mpsc::channel::<SessionEvent>();

        let mut session = ParsingSession::new(path.clone(), self.definitions.clone());

        // Load timer preferences into the session's timer manager (Live mode only)
        if let Some(prefs_path) = Self::timer_preferences_path() {
            if let Some(timer_mgr) = session.timer_manager() {
                if let Ok(mut mgr) = timer_mgr.lock()
                    && let Err(e) = mgr.load_preferences(&prefs_path)
                {
                    warn!(error = %e, "Failed to load timer preferences");
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
        let handler = CombatSignalHandler::new(
            self.shared.clone(),
            trigger_tx.clone(),
            session_event_tx.clone(),
            self.overlay_tx.clone(),
            self.cmd_tx.clone(),
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
                    Err(_) => break,     // Task canceled
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

        debug!(worker_path = ?worker_path, "Using parse worker");

        let mut cmd = std::process::Command::new(&worker_path);
        cmd.arg(&path).arg(&session_id).arg(&encounters_dir);

        // Pass definitions directory if available
        if let Some(ref def_dir) = definitions_dir {
            cmd.arg(def_dir);
            debug!(definitions_path = ?def_dir, "Using definitions directory");
        }

        // Pass log path so subprocess writes to same log file
        if let Some(log_path) = dirs::config_dir().map(|p| p.join("baras").join("baras.log")) {
            cmd.env("BARAS_LOG_PATH", &log_path);
        }

        // Run subprocess on a blocking thread so it doesn't stall the tokio runtime
        let output = tokio::task::spawn_blocking(move || cmd.output()).await;
        // Flatten: JoinError<io::Result<Output>> -> treat JoinError as spawn failure
        let output = match output {
            Ok(inner) => inner,
            Err(e) => Err(std::io::Error::new(std::io::ErrorKind::Other, e)),
        };

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
                                self.shared.current_area_id.store(parse_result.area.area_id, Ordering::SeqCst);
                            }

                            // Capture player context before releasing cache borrow.
                            // discipline_id comes from player_disciplines (not cache.player,
                            // which doesn't have it after worker restore)
                            if cache.player_initialized {
                                let disc_id = cache.player_disciplines
                                    .get(&cache.player.id)
                                    .map(|p| p.discipline_id)
                                    .unwrap_or(0);
                                player_context = Some((cache.player.id, disc_id));
                            }

                            // Check if there's an incomplete encounter (parquet file exists but no summary)
                            // If so, we'll continue with that encounter ID instead of creating a new one
                            let incomplete_encounter_exists = {
                                let incomplete_parquet = encounters_dir.join(
                                    baras_core::storage::encounter_filename(parse_result.encounter_count as u32)
                                );
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
                                use baras_core::game_data::Difficulty;
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

                        // Sync player context to effect tracker so discipline-scoped
                        // effects work immediately (tracker missed historical signals)
                        if let Some((player_id, discipline_id)) = player_context {
                            if let Some(tracker) = session_guard.effect_tracker() {
                                if let Ok(mut tracker) = tracker.lock() {
                                    tracker.set_player_context(player_id, discipline_id);
                                }
                            }
                        }

                        // Enable live parquet writing (continues from where subprocess left off)
                        session_guard.enable_live_parquet(
                            encounters_dir.clone(),
                            parse_result.encounter_count as u32,
                        );

                        // Load boss definitions for initial area (before releasing lock)
                        if parse_result.area.area_id != 0 {
                            if let Some(bosses) = self.load_area_definitions(parse_result.area.area_id) {
                                // Send notes to overlay
                                self.send_notes_from_bosses(&bosses);
                                session_guard.load_boss_definitions(bosses, false);
                            }
                        }

                        session_guard.finalize_session();
                        session_guard.sync_timer_context();
                        
                        // Check if we're starting mid-encounter
                        // This enables overlays to display data immediately when app starts during combat
                        let mid_combat_startup = if let Some(cache) = &session_guard.session_cache {
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
                            let timer_already_running = self.shared.operation_timer
                                .lock().unwrap().is_running();

                            if !timer_already_running {
                                const MAX_BACKFILL_SECS: u64 = 4 * 3600; // 4-hour cap

                                match area_kind {
                                    AreaKind::Flashpoint | AreaKind::PvP => {
                                        // Use area entry timestamp for backfill
                                        if let Some(entered_at) = parse_result.area.entered_at {
                                            let now = chrono::Local::now().naive_local();
                                            let elapsed = now.signed_duration_since(entered_at);
                                            let elapsed_secs = elapsed.num_seconds()
                                                .max(0) as u64;
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
                                        // Use earliest boss pull time in this session
                                        let earliest_start = parse_result.encounters.iter()
                                            .filter(|s| s.encounter_type == PhaseType::Raid)
                                            .filter_map(|s| s.start_time.as_ref())
                                            .filter_map(|t| {
                                                chrono::NaiveDateTime::parse_from_str(
                                                    t, "%Y-%m-%dT%H:%M:%S"
                                                ).ok()
                                            })
                                            .min();

                                        if let Some(first_pull) = earliest_start {
                                            let now = chrono::Local::now().naive_local();
                                            let elapsed = now.signed_duration_since(first_pull);
                                            let elapsed_secs = elapsed.num_seconds()
                                                .max(0) as u64;
                                            let capped = elapsed_secs.min(MAX_BACKFILL_SECS);
                                            let mut timer = self.shared.operation_timer.lock().unwrap();
                                            if !timer.manually_stopped {
                                                info!(
                                                    elapsed_secs = capped,
                                                    "Backfilling operation timer from first boss pull"
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
                        
                        // Notify frontend to refresh session info (after in_combat is set)
                        let _ = self.app_handle.emit("session-updated", "FileLoaded");
                    }
                    Err(e) => {
                        error!(error = %e, "Subprocess output parse failed");
                        let in_combat = fallback_streaming_parse(&reader, &session, encounters_dir.clone()).await;
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
                let in_combat = fallback_streaming_parse(&reader, &session, encounters_dir.clone()).await;
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
                let in_combat = fallback_streaming_parse(&reader, &session, encounters_dir.clone()).await;
                if in_combat {
                    self.shared.in_combat.store(true, std::sync::atomic::Ordering::SeqCst);
                    let _ = trigger_tx.try_send(MetricsTrigger::CombatStarted);
                    let _ = session_event_tx.send(SessionEvent::CombatStarted);
                    info!("Detected mid-combat startup (fallback parse)");
                }
            }
        }

        info!(
            elapsed_ms = timer.elapsed().as_millis() as u64,
            "Parse completed"
        );

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
            let mut last_cooldowns_count: usize = 0;
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

                // Periodic stale-recovery: if the session was flagged not-live (e.g. game
                // closed, newest file blank), check whether conditions have changed. This
                // lets overlays restore when the game relaunches or a new file gets content.
                if shared.auto_hide.is_not_live_active()
                    && last_stale_check.elapsed() >= std::time::Duration::from_secs(1)
                {
                    last_stale_check = tokio::time::Instant::now();
                    if !shared.is_session_not_live().await {
                        let _ = overlay_tx
                            .try_send(OverlayUpdate::NotLiveStateChanged { is_live: true });
                    }
                }

                // Skip processing if nothing needs updating
                if !any_overlay_active && !needs_audio {
                    continue;
                }

                // Raid frames: send whenever there are effects (or always in rearrange mode)
                if raid_active {
                    let rearranging = shared.rearrange_mode.load(Ordering::Relaxed);
                    if let Some(data) = build_raid_frame_data(&shared, rearranging, icon_cache.as_ref()).await {
                        let effect_count: usize = data.frames.iter().map(|f| f.effects.len()).sum();
                        // Always send in rearrange mode, otherwise only when effects exist/changed
                        if rearranging || effect_count > 0 || last_raid_effect_count > 0 {
                            if overlay_tx.try_send(OverlayUpdate::EffectsUpdated(data)).is_err() {
                                warn!("Overlay channel full, dropped raid effects update");
                            }
                        }
                        last_raid_effect_count = effect_count;
                    } else if rearranging {
                        // In rearrange mode, send empty data to keep overlay rendering
                        if overlay_tx.try_send(OverlayUpdate::EffectsUpdated(
                            baras_overlay::RaidFrameData { frames: vec![] },
                        )).is_err() {
                            warn!("Overlay channel full, dropped raid effects clear");
                        }
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
                            if overlay_tx.try_send(OverlayUpdate::EffectsAUpdated(data)).is_err() {
                                warn!("Overlay channel full, dropped effects A update");
                            }
                        }
                        last_effects_a_count = count;
                    } else if last_effects_a_count > 0 {
                        if overlay_tx.try_send(OverlayUpdate::EffectsAUpdated(EffectsABData {
                            effects: vec![],
                        })).is_err() {
                            warn!("Overlay channel full, dropped effects A clear");
                        }
                        last_effects_a_count = 0;
                    }
                }

                // Effects B: only send if there are effects or effects just cleared
                if effects_b_active {
                    if let Some(data) = build_effects_b_data(&shared, icon_cache.as_ref()).await {
                        let count = data.effects.len();
                        if count > 0 || last_effects_b_count > 0 {
                            if overlay_tx.try_send(OverlayUpdate::EffectsBUpdated(data)).is_err() {
                                warn!("Overlay channel full, dropped effects B update");
                            }
                        }
                        last_effects_b_count = count;
                    } else if last_effects_b_count > 0 {
                        if overlay_tx.try_send(OverlayUpdate::EffectsBUpdated(EffectsABData {
                            effects: vec![],
                        })).is_err() {
                            warn!("Overlay channel full, dropped effects B clear");
                        }
                        last_effects_b_count = 0;
                    }
                }

                // Cooldowns: only send if there are cooldowns or cooldowns just cleared
                if cooldowns_active {
                    if let Some(data) = build_cooldowns_data(&shared, icon_cache.as_ref()).await {
                        let count = data.entries.len();
                        if count > 0 || last_cooldowns_count > 0 {
                            if overlay_tx.try_send(OverlayUpdate::CooldownsUpdated(data)).is_err() {
                                warn!("Overlay channel full, dropped cooldowns update");
                            }
                        }
                        last_cooldowns_count = count;
                    } else if last_cooldowns_count > 0 {
                        if overlay_tx.try_send(OverlayUpdate::CooldownsUpdated(CooldownData {
                            entries: vec![],
                        })).is_err() {
                            warn!("Overlay channel full, dropped cooldowns clear");
                        }
                        last_cooldowns_count = 0;
                    }
                }

                // DOT tracker: only send if there are targets or targets just cleared
                if dot_tracker_active {
                    if let Some(data) = build_dot_tracker_data(&shared, icon_cache.as_ref()).await {
                        let count = data.targets.len();
                        if count > 0 || last_dot_tracker_count > 0 {
                            if overlay_tx.try_send(OverlayUpdate::DotTrackerUpdated(data)).is_err() {
                                warn!("Overlay channel full, dropped DOT tracker update");
                            }
                        }
                        last_dot_tracker_count = count;
                    } else if last_dot_tracker_count > 0 {
                        if overlay_tx.try_send(OverlayUpdate::DotTrackerUpdated(DotTrackerData {
                            targets: vec![],
                        })).is_err() {
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
                    && let Some(data) = build_boss_health_data(&shared).await
                {
                    if overlay_tx.try_send(OverlayUpdate::BossHealthUpdated(data)).is_err() {
                        warn!("Overlay channel full, dropped boss health update");
                    }
                }

                // Timers + Audio: always poll when in live mode (alerts can fire at combat end)
                if shared.is_live_tailing.load(Ordering::SeqCst) {
                    // Process timer audio and get timer data (returns (TimersA data, TimersB data, countdowns, alerts))
                    if let Some((timers_a, timers_b, countdowns, alerts)) =
                        build_timer_data_with_audio(&shared, icon_cache.as_ref()).await
                    {
                        // Send timer overlay data (only when in combat)
                        if in_combat && timer_active {
                            if overlay_tx.try_send(OverlayUpdate::TimersAUpdated(timers_a)).is_err() {
                                warn!("Overlay channel full, dropped timers A update");
                            }
                            if overlay_tx.try_send(OverlayUpdate::TimersBUpdated(timers_b)).is_err() {
                                warn!("Overlay channel full, dropped timers B update");
                            }
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

                        // Send text alerts to overlay (only those with alert_text_enabled and not stale)
                        let text_alerts: Vec<_> = alerts.iter().filter(|a| a.alert_text_enabled && a.timestamp >= tailing_started_at).cloned().collect();
                        if !text_alerts.is_empty() {
                            if overlay_tx.try_send(OverlayUpdate::AlertsFired(text_alerts)).is_err() {
                                warn!("Overlay channel full, dropped timer alerts");
                            }
                        }

                        // Send alert audio events (only if audio_enabled for that alert)
                        // Skip alerts from before we started tailing (stale encounter recovery)
                        for alert in alerts {
                            if alert.audio_enabled && alert.timestamp >= tailing_started_at {
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

        self.tail_handle = Some(tail_handle);
        self.metrics_handle = Some(metrics_handle);
        self.effects_handle = Some(effects_handle);
    }

    async fn stop_tailing(&mut self) {
        if self.tail_handle.is_some() {
            debug!("Stopping active tail task");
        }

        // Reset combat state
        self.shared.in_combat.store(false, Ordering::SeqCst);

        // Stop process monitor
        self.stop_process_monitor();

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

    /// Start monitoring the game process continuously.
    ///
    /// Updates `shared.game_running` on every poll and emits `NotLiveStateChanged`
    /// events on transitions so the overlay auto-hide system can react.
    /// Runs for the lifetime of the tailing session (stopped by `stop_tailing`).
    /// Only one monitor runs at a time; calling this while one is active is a no-op.
    fn start_process_monitor(&mut self) {
        if self.process_monitor_handle.is_some() {
            return;
        }

        let overlay_tx = self.overlay_tx.clone();
        let shared = self.shared.clone();
        let handle = tokio::spawn(async move {
            let mut was_running = shared.game_running.load(Ordering::SeqCst);

            loop {
                match process_monitor::is_game_running().await {
                    Some(is_running) => {
                        shared.game_running.store(is_running, Ordering::SeqCst);

                        if was_running && !is_running {
                            // Game just closed
                            info!("Game process no longer detected");
                            let _ = overlay_tx
                                .try_send(OverlayUpdate::NotLiveStateChanged { is_live: false });
                        } else if !was_running && is_running {
                            // Game just launched — set the starting flag so stale-recovery
                            // stays blocked until a fresh session confirms a new character.
                            info!("Game process detected; waiting for fresh log session before restoring overlays");
                            shared.auto_hide.set_game_starting(true);
                        }

                        was_running = is_running;
                    }
                    None => {
                        // Process check failed — safe default: assume running,
                        // keep current state, and try again next poll
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(2500)).await;
            }
        });

        self.process_monitor_handle = Some(handle);
    }

    /// Stop the game process monitor if running.
    fn stop_process_monitor(&mut self) {
        if let Some(handle) = self.process_monitor_handle.take() {
            handle.abort();
        }
    }

    async fn refresh_index(&mut self) {
        let log_dir = self.shared.config.read().await.log_directory.clone();
        let cache_path = DirectoryIndex::default_cache_path()
            .unwrap_or_else(|| PathBuf::from("directory_cache.json"));
        if let Ok(index) = DirectoryIndex::build_index_cached(&PathBuf::from(&log_dir), &cache_path) {
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

    // Try live encounter first, fall back to historical summary for initial hydration
    if let Some(encounter) = cache.last_combat_encounter() {
        // Live encounter path - full data including challenges and phase info
        let encounter_count = cache
            .encounters()
            .filter(|e| e.state != EncounterState::NotStarted)
            .map(|e| e.id + 1)
            .max()
            .unwrap_or(0) as usize;
        let encounter_time_secs = encounter
            .duration_seconds(session.interpolated_game_time())
            .unwrap_or(0) as u64;

        // Classify the encounter to get phase type and boss info
        // Use encounter's stored area/difficulty info (falls back to cache if not set)
        let area_id_for_classification = encounter.area_id.unwrap_or(cache.current_area.area_id);
        let difficulty_id_for_classification = encounter.difficulty_id.unwrap_or(cache.current_area.difficulty_id);
        let (encounter_type, boss_info) = classify_encounter(encounter, area_id_for_classification, difficulty_id_for_classification);

        // Generate encounter name with pull count
        // Priority: definition name > hardcoded boss name > phase type
        // 
        // Important: PostCombat state alone doesn't mean the encounter is finalized.
        // During the grace period, the encounter is PostCombat but hasn't been added
        // to history yet (that happens when push_new_encounter() is called after
        // the grace period expires). We detect this by checking last_combat_exit_time:
        // - Some(_) = grace period active, encounter not yet in history
        // - None = grace period expired, encounter has been finalized to history
        let is_finalized = matches!(encounter.state, EncounterState::PostCombat { .. })
            && cache.last_combat_exit_time.is_none();

        let encounter_name = if is_finalized {
            // Encounter finalized and in history - use the display_name from history
            cache
                .encounter_history
                .summaries()
                .last()
                .map(|s| s.display_name.clone())
        } else if let Some(def) = encounter.active_boss_definition() {
            // Definition is active - use definition name with pull count
            let pull_count = cache.encounter_history.peek_pull_count(&def.name);
            Some(format!("{} - {}", def.name, pull_count))
        } else if let Some(boss) = boss_info {
            // Hardcoded boss detected (no definition) - use boss name with pull count
            let pull_count = cache.encounter_history.peek_pull_count(boss.boss);
            Some(format!("{} - {}", boss.boss, pull_count))
        } else {
            // Trash encounter - use phase type with trash count
            let trash_count = cache.encounter_history.peek_trash_count();
            let label = match encounter_type {
                PhaseType::Raid => "Raid Trash",
                PhaseType::Flashpoint => "Flashpoint Trash",
                PhaseType::DummyParse => "Dummy Parse",
                PhaseType::PvP => "PvP Match",
                PhaseType::OpenWorld => "Open World",
            };
            Some(format!("{} {}", label, trash_count))
        };

        // Get difficulty from area info (blank for non-instanced content)
        let difficulty = if !cache.current_area.difficulty_name.is_empty() {
            Some(cache.current_area.difficulty_name.clone())
        } else {
            None
        };

        // Calculate metrics for all players (use session-level discipline registry)
        let entity_metrics = encounter.calculate_entity_metrics(&cache.player_disciplines, session.interpolated_game_time())?;
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
            // Use exit time when in PostCombat (grace window) so duration doesn't keep ticking.
            // Fall back to last event time (game time) instead of system clock to avoid
            // clock skew between SWTOR's timestamps and the system clock.
            let current_time = encounter
                .exit_combat_time
                .or(session.last_event_time)
                .unwrap_or_else(|| chrono::Local::now().naive_local());

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
            .zip(session.interpolated_game_time().or(session.last_event_time))
            .map(|(start, now)| (now - start).num_milliseconds() as f32 / 1000.0)
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
    } else if let Some(summary) = cache.encounter_history.summaries().last() {
        // Fallback to historical summary for initial hydration when no live encounter exists
        let encounter_count = cache.encounter_history.summaries().len();
        let encounter_time_secs = summary.duration_seconds.max(0) as u64;
        let encounter_name = Some(summary.display_name.clone());
        let difficulty = summary.difficulty.clone();
        let metrics = summary.player_metrics.clone();

        // Build ChallengeData from historical summary if challenges exist
        let challenges = if !summary.challenges.is_empty() {
            let entries: Vec<ChallengeEntry> = summary
                .challenges
                .iter()
                .map(|cs| {
                    let by_player: Vec<PlayerContribution> = cs
                        .by_player
                        .iter()
                        .map(|p| PlayerContribution {
                            entity_id: p.entity_id,
                            name: p.name.clone(),
                            value: p.value,
                            percent: p.percent,
                            per_second: p.per_second,
                        })
                        .collect();
                    ChallengeEntry {
                        name: cs.name.clone(),
                        value: cs.total_value,
                        event_count: cs.event_count,
                        per_second: cs.per_second,
                        by_player,
                        duration_secs: cs.duration_secs,
                        enabled: true,
                        color: cs.color.map(|c| Color::from_rgba8(c[0], c[1], c[2], c[3])),
                        columns: match cs.columns.as_str() {
                            "TotalPercent" => ChallengeColumns::TotalPercent,
                            "TotalPerSecond" => ChallengeColumns::TotalPerSecond,
                            "PerSecondPercent" => ChallengeColumns::PerSecondPercent,
                            "TotalOnly" => ChallengeColumns::TotalOnly,
                            "PerSecondOnly" => ChallengeColumns::PerSecondOnly,
                            "PercentOnly" => ChallengeColumns::PercentOnly,
                            _ => ChallengeColumns::default(),
                        },
                    }
                })
                .collect();
            Some(ChallengeData {
                entries,
                boss_name: summary.boss_name.clone(),
                duration_secs: summary.duration_seconds.max(1) as f32,
                phase_durations: Default::default(),
            })
        } else {
            None
        };

        Some(CombatData {
            metrics,
            player_entity_id,
            encounter_time_secs,
            encounter_count,
            class_discipline,
            encounter_name,
            difficulty,
            challenges,
            current_phase: None,
            phase_time_secs: 0.0,
        })
    } else {
        None
    }
}

/// Build raid frame data from the effect tracker and registry
///
/// Uses RaidSlotRegistry to maintain stable player positions.
/// Players are registered ONLY when the local player applies a NEW effect to them
/// (via the new_targets queue), not on every tick.
async fn build_raid_frame_data(
    shared: &Arc<SharedState>,
    rearranging: bool,
    icon_cache: Option<&Arc<baras_overlay::icons::IconCache>>,
) -> Option<RaidFrameData> {
    let session_guard = shared.session.read().await;
    let session = session_guard.as_ref()?;
    let session = session.read().await;

    // Get effect tracker (Live mode only)
    let effect_tracker = session.effect_tracker()?;
    let mut tracker = effect_tracker.lock().unwrap_or_else(|poisoned| {
        tracing::warn!("Effect tracker mutex was poisoned, recovering");
        poisoned.into_inner()
    });

    // Lock registry (recover from poison to keep raid frames working)
    let mut registry = shared.raid_registry.lock().unwrap_or_else(|poisoned| {
        tracing::warn!("Raid registry mutex was poisoned, recovering");
        poisoned.into_inner()
    });

    // Early out: skip building data if no effects AND no registered players
    // We need to keep sending updates while effects exist OR players are registered
    // so that removals/clears are reflected in the overlay
    // Skip this check in rearrange mode to always show frames
    if !rearranging && !tracker.has_active_effects() && registry.is_empty() {
        return None;
    }

    // Compute interpolated game time once for all effects
    let interp_time = tracker.interpolated_game_time();

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
        // Skip effects not destined for raid frames or already removed
        if effect.display_target != DisplayTarget::RaidFrames || effect.removed_at.is_some() || effect.timer_expired {
            continue;
        }

        let target_id = effect.target_entity_id;

        // Only group effects for already-registered players
        if registry.is_registered(target_id) {
            effects_by_target
                .entry(target_id)
                .or_default()
                .push(convert_to_raid_effect(effect, icon_cache, interp_time));
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

            // Map discipline to role and class icon (defaults to DPS if unknown)
            let discipline = player
                .discipline_id
                .and_then(Discipline::from_guid);

            let role = discipline
                .map(|d| match d.role() {
                    Role::Tank => PlayerRole::Tank,
                    Role::Healer => PlayerRole::Healer,
                    Role::Dps => PlayerRole::Dps,
                })
                .unwrap_or(PlayerRole::Dps);

            let class_icon = discipline
                .map(|d| d.icon_name().to_string());

            frames.push(RaidFrame {
                slot,
                player_id: Some(player.entity_id),
                name: player.name.clone(),
                hp_percent: 1.0,
                role,
                class_icon,
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
/// Returns (TimersA data, TimersB data, countdowns_to_announce, fired_alerts)
/// Timers are routed to A or B based on their display_target field.
/// Countdowns are (timer_name, seconds, voice_pack)
async fn build_timer_data_with_audio(
    shared: &Arc<SharedState>,
    icon_cache: Option<&Arc<baras_overlay::icons::IconCache>>,
) -> Option<(TimerData, TimerData, Vec<(String, u8, String)>, Vec<FiredAlert>)> {
    use baras_core::timers::TimerDisplayTarget;

    let session_guard = shared.session.read().await;
    let session = session_guard.as_ref()?;
    let session = session.read().await;

    // Get active timers from timer manager (Live mode only, mutable for countdown checking)
    let timer_mgr = session.timer_manager()?;
    let mut timer_mgr = timer_mgr.lock().unwrap_or_else(|p| p.into_inner());

    // Always take alerts (even after combat ends, timer expirations need to play)
    let mut alerts = timer_mgr.take_fired_alerts();

    // Check for audio offset alerts (early warning sounds before timer expires)
    let offset_alerts = timer_mgr.check_audio_offsets();
    alerts.extend(offset_alerts);

    // Also get alerts from effect tracker (effect start/end alerts)
    if let Some(effect_tracker) = session.effect_tracker() {
        let mut tracker = effect_tracker.lock().unwrap_or_else(|p| p.into_inner());
        alerts.extend(tracker.take_fired_alerts());
    }

    // If not in combat, return only alerts (no countdown checks)
    let in_combat = shared.in_combat.load(Ordering::SeqCst);
    if !in_combat {
        return Some((
            TimerData::default(),
            TimerData::default(),
            Vec::new(),
            alerts,
        ));
    }

    // Check for countdowns to announce (uses realtime internally)
    let countdowns = timer_mgr.check_all_countdowns();

    // Compute interpolated game time once for all timers
    let interp_time = timer_mgr.interpolated_game_time();

    // Convert active timers to TimerEntry format, routing to A or B based on display_target
    let mut entries_a = Vec::new();
    let mut entries_b = Vec::new();

    for timer in timer_mgr.active_timers() {
        let remaining = interp_time
            .map(|t| timer.remaining_secs(t))
            .unwrap_or(0.0);
        if remaining <= 0.0 {
            continue;
        }

        // Skip timers hidden by role filtering
        if timer.role_hidden {
            continue;
        }

        // Skip timers hidden by show_at_secs threshold
        if !timer.is_visible(remaining) {
            continue;
        }

        // Load icon from cache if ability ID is set
        let icon = timer.icon_ability_id.and_then(|ability_id| {
            icon_cache.and_then(|cache| {
                cache
                    .get_icon(ability_id)
                    .map(|data| std::sync::Arc::new((data.width, data.height, data.rgba)))
            })
        });

        let entry = TimerEntry {
            name: timer.name.clone(),
            remaining_secs: remaining,
            total_secs: if timer.show_at_secs > 0.0 {
                timer.show_at_secs
            } else {
                timer.duration.as_secs_f32()
            },
            color: timer.color,
            icon_ability_id: timer.icon_ability_id,
            icon,
        };
        match timer.display_target {
            TimerDisplayTarget::TimersA => entries_a.push(entry),
            TimerDisplayTarget::TimersB => entries_b.push(entry),
            TimerDisplayTarget::None => {} // Don't show on any timer overlay
        }
    }

    Some((
        TimerData { entries: entries_a },
        TimerData { entries: entries_b },
        countdowns,
        alerts,
    ))
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
        return EffectAudioResult {
            countdowns,
            alerts,
        };
    };
    let session = session_arc.read().await;

    // Get effect tracker (Live mode only)
    let Some(effect_tracker) = session.effect_tracker() else {
        return EffectAudioResult {
            countdowns,
            alerts,
        };
    };
    let mut tracker = effect_tracker.lock().unwrap_or_else(|p| p.into_inner());

    // Note: On-end text alerts are handled by tick() -> fired_alerts -> take_fired_alerts(),
    // consumed in build_timer_data_with_audio(). We don't poll check_expiration_alert() here
    // because the window check races with signal-based refreshes, causing
    // false "effect ended" alerts when an effect is refreshed just before its timer expires.

    // Compute interpolated game time once for all effects
    let interp_time = tracker.interpolated_game_time();

    for effect in tracker.active_effects_mut() {
        // Skip audio checks for effects without audio enabled
        if !effect.audio_enabled {
            continue;
        }

        // Compute remaining times from interpolated game time
        let remaining_total = interp_time
            .and_then(|t| effect.remaining_secs(t))
            .unwrap_or(0.0);
        let remaining_base = effect.remaining_base_secs(remaining_total);

        // Check for countdown (uses interpolated game time)
        // Only for non-removed effects
        if effect.removed_at.is_none()
            && let Some(seconds) = effect.check_countdown(remaining_base)
        {
            countdowns.push((
                effect.display_text.clone(),
                seconds,
                effect.countdown_voice.clone(),
            ));
        }

        // Check for audio offset trigger (early warning sound, offset > 0)
        if effect.check_audio_offset(remaining_base) {
            alerts.push(EffectAlert {
                name: effect.display_text.clone(),
                file: effect.audio_file.clone(),
            });
        }

        // Check for expiration audio (offset == 0, fire when effect expires)
        if effect.check_expiration_audio(remaining_base) {
            alerts.push(EffectAlert {
                name: effect.display_text.clone(),
                file: effect.audio_file.clone(),
            });
        }
    }

    EffectAudioResult {
        countdowns,
        alerts,
    }
}

/// Convert an ActiveEffect (core) to RaidEffect (overlay)
///
/// Uses interpolated game time to compute remaining seconds, then derives
/// a monotonic `Instant` expiry for the overlay's own countdown rendering.
fn convert_to_raid_effect(
    effect: &ActiveEffect,
    icon_cache: Option<&Arc<baras_overlay::icons::IconCache>>,
    interp_time: Option<chrono::NaiveDateTime>,
) -> RaidEffect {
    // Effects on raid frames are typically HoTs/shields (is_buff defaults to true in RaidEffect::new())
    let mut raid_effect = RaidEffect::new(effect.game_effect_id, effect.name.clone())
        .with_charges(effect.stacks)
        .with_color_rgba(effect.color);

    // Compute expiry Instant from game-time remaining
    if let Some(dur) = effect.duration {
        let remaining_secs = interp_time
            .and_then(|t| effect.remaining_secs(t))
            .unwrap_or(0.0);
        if remaining_secs > 0.0 {
            let remaining_dur = std::time::Duration::from_secs_f32(remaining_secs);
            let expires_at = std::time::Instant::now() + remaining_dur;
            raid_effect = raid_effect.with_duration(dur).with_expiry(expires_at);
        }
    }

    // Load icon if cache available
    if let Some(cache) = icon_cache {
        if let Some(data) = cache.get_icon(effect.icon_ability_id) {
            raid_effect =
                raid_effect.with_icon(std::sync::Arc::new((data.width, data.height, data.rgba)));
        }
    }

    raid_effect
}

/// Calculate remaining time for an effect in seconds
/// Uses interpolated game time for accurate values without clock skew
fn calculate_remaining_secs(
    effect: &ActiveEffect,
    interp_time: Option<chrono::NaiveDateTime>,
) -> Option<f32> {
    let remaining = interp_time
        .and_then(|t| effect.remaining_secs(t))
        .unwrap_or(0.0);
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
    let tracker = effect_tracker.lock().unwrap_or_else(|p| p.into_inner());

    if !tracker.has_active_effects() {
        return None;
    }

    let interp_time = tracker.interpolated_game_time();

    let mut effects: Vec<_> = tracker.effects_a().collect();
    effects.sort_by_key(|e| e.applied_at);

    let entries: Vec<EffectABEntry> = effects
        .into_iter()
        .filter_map(|effect| {
            let remaining_total = interp_time
                .and_then(|t| effect.remaining_secs(t))
                .unwrap_or(0.0);
            // Skip effects hidden by show_at_secs threshold
            if !effect.is_visible(remaining_total) {
                return None;
            }
            let total_secs = effect.duration?.as_secs_f32();
            let remaining_secs = calculate_remaining_secs(effect, interp_time)?;

            // Load icon from cache
            let icon = icon_cache.and_then(|cache| {
                cache
                    .get_icon(effect.icon_ability_id)
                    .map(|data| StdArc::new((data.width, data.height, data.rgba)))
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
    let tracker = effect_tracker.lock().unwrap_or_else(|p| p.into_inner());

    if !tracker.has_active_effects() {
        return None;
    }

    let interp_time = tracker.interpolated_game_time();

    let mut effects: Vec<_> = tracker.effects_b().collect();
    effects.sort_by_key(|e| e.applied_at);

    let entries: Vec<EffectABEntry> = effects
        .into_iter()
        .filter_map(|effect| {
            let remaining_total = interp_time
                .and_then(|t| effect.remaining_secs(t))
                .unwrap_or(0.0);
            // Skip effects hidden by show_at_secs threshold
            if !effect.is_visible(remaining_total) {
                return None;
            }
            let total_secs = effect.duration?.as_secs_f32();
            let remaining_secs = calculate_remaining_secs(effect, interp_time)?;

            // Load icon from cache
            let icon = icon_cache.and_then(|cache| {
                cache
                    .get_icon(effect.icon_ability_id)
                    .map(|data| StdArc::new((data.width, data.height, data.rgba)))
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
    let tracker = effect_tracker.lock().unwrap_or_else(|p| p.into_inner());

    if !tracker.has_active_effects() {
        return None;
    }

    let interp_time = tracker.interpolated_game_time();

    let mut effects: Vec<_> = tracker.cooldown_effects().collect();

    // Sort by remaining time (shortest first)
    effects.sort_by(|a, b| {
        let a_remaining = calculate_remaining_secs(a, interp_time).unwrap_or(f32::MAX);
        let b_remaining = calculate_remaining_secs(b, interp_time).unwrap_or(f32::MAX);
        a_remaining
            .partial_cmp(&b_remaining)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let entries: Vec<CooldownEntry> = effects
        .into_iter()
        .filter_map(|effect| {
            let remaining_total = interp_time
                .and_then(|t| effect.remaining_secs(t))
                .unwrap_or(0.0);
            // Skip effects hidden by show_at_secs threshold
            if !effect.is_visible(remaining_total) {
                return None;
            }
            // Duration includes ready_secs for tracker lifetime, subtract for display
            let tracker_total = effect.duration?.as_secs_f32();
            let total_secs = tracker_total - effect.cooldown_ready_secs;

            // Remaining time until tracker expires
            let tracker_remaining = calculate_remaining_secs(effect, interp_time)?;

            // Display remaining = tracker remaining minus ready period (clamped to 0)
            // So display hits 0 when entering ready state, not when effect disappears
            let remaining_secs = (tracker_remaining - effect.cooldown_ready_secs).max(0.0);

            // In ready state when tracker remaining is within the ready period
            let is_in_ready_state =
                effect.cooldown_ready_secs > 0.0 && tracker_remaining <= effect.cooldown_ready_secs;

            // Load icon from cache
            let icon = icon_cache.and_then(|cache| {
                cache
                    .get_icon(effect.icon_ability_id)
                    .map(|data| StdArc::new((data.width, data.height, data.rgba)))
            });

            Some(CooldownEntry {
                ability_id: effect.game_effect_id,
                icon_ability_id: effect.icon_ability_id,
                name: effect.display_text.clone(),
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
    let tracker = effect_tracker.lock().unwrap_or_else(|p| p.into_inner());

    if !tracker.has_active_effects() {
        return None;
    }

    let interp_time = tracker.interpolated_game_time();

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
                    let remaining_total = interp_time
                        .and_then(|t| effect.remaining_secs(t))
                        .unwrap_or(0.0);
                    // Skip effects hidden by show_at_secs threshold
                    if !effect.is_visible(remaining_total) {
                        return None;
                    }
                    let total_secs = effect.duration?.as_secs_f32();
                    let remaining_secs = calculate_remaining_secs(effect, interp_time)?;

                    // Load icon from cache
                    let icon = icon_cache.and_then(|cache| {
                        cache
                            .get_icon(effect.icon_ability_id)
                            .map(|data| StdArc::new((data.width, data.height, data.rgba)))
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

/// Area visit info for display in file browser
#[derive(Debug, Clone, serde::Serialize)]
pub struct AreaVisitInfo {
    /// Display string: "AreaName Difficulty" (e.g., "Dxun NiM 8")
    pub display: String,
    /// Raw area name
    pub area_name: String,
    /// Difficulty string (may be empty)
    pub difficulty: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct LogFileInfo {
    pub path: PathBuf,
    pub display_name: String,
    pub character_name: Option<String>,
    pub date: String,
    /// Day of week (e.g., "Sunday")
    pub day_of_week: String,
    pub is_empty: bool,
    pub file_size: u64,
    /// Areas/operations visited in this file (None if not yet indexed)
    pub areas: Option<Vec<AreaVisitInfo>>,
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
            total_healing_effective: player.total_healing_effective,
            dtps: player.dtps as i32,
            edtps: player.edtps as i32,
            total_damage_taken: player.total_damage_taken,
            total_damage_taken_effective: player.total_damage_taken_effective,
            tps: player.tps as i32,
            total_threat: player.total_threat,
            damage_crit_pct: player.damage_crit_pct,
            heal_crit_pct: player.heal_crit_pct,
            effective_heal_pct: player.effective_heal_pct,
            defense_pct: player.defense_pct,
            shield_pct: player.shield_pct,
            total_shield_absorbed: player.total_shield_absorbed,
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
    /// Discipline icon filename (e.g., "medicine.png") for display
    pub class_icon: Option<String>,
    /// Role icon filename (e.g., "icon_heal.png") for display
    pub role_icon: Option<String>,
    pub area_name: Option<String>,
    pub in_combat: bool,
    pub encounter_count: usize,
    /// Session start time extracted from log filename (formatted as "Jan 18, 3:45 PM")
    pub session_start: Option<String>,
    /// Short start time for inline display (e.g., "3:45 PM")
    pub session_start_short: Option<String>,
    /// Session end time for historical sessions (formatted as "Jan 18, 3:45 PM")
    pub session_end: Option<String>,
    /// Duration formatted as short form (e.g., "47m" or "1h 23m") — always computed
    pub duration_formatted: Option<String>,
    /// True if the log file's last event is older than 30 minutes (no active session)
    pub stale_session: bool,
    /// True if this log file contains events from multiple characters (corrupted)
    pub character_mismatch: bool,
    /// True if the log file started without an AreaEntered event
    pub missing_area: bool,
}
