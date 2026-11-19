//! Signal handler bridging parser signals to the service

use super::operation_timer::{AreaKind, secs_since};
use super::{MetricsTrigger, OverlayUpdate, ServiceCommand, SessionEvent, raid_detection};
use crate::state::MutexExt;
use crate::state::SharedState;
use baras_core::context::resolve;
use baras_core::game_data::{Discipline, Role};
use baras_core::{GameSignal, SignalHandler};
use baras_overlay::{BossHealthData, NotesData};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tokio::sync::mpsc;
use tracing::info;

/// Signal handler that tracks combat state and triggers metrics updates
pub(super) struct CombatSignalHandler {
    shared: Arc<SharedState>,
    trigger_tx: mpsc::Sender<MetricsTrigger>,
    /// Channel for frontend session updates (event-driven, not polled)
    session_event_tx: mpsc::UnboundedSender<SessionEvent>,
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
}

impl CombatSignalHandler {
    pub(super) fn new(
        shared: Arc<SharedState>,
        trigger_tx: mpsc::Sender<MetricsTrigger>,
        session_event_tx: mpsc::UnboundedSender<SessionEvent>,
        overlay_tx: mpsc::Sender<OverlayUpdate>,
        cmd_tx: mpsc::Sender<ServiceCommand>,
    ) -> Self {
        let last_id = shared.last_player_id.load(Ordering::SeqCst);
        let local_player_id = if last_id != 0 { Some(last_id) } else { None };
        let current_role = Role::from_u8(shared.last_player_role.load(Ordering::SeqCst));
        Self {
            shared,
            trigger_tx,
            session_event_tx,
            overlay_tx,
            cmd_tx,
            local_player_id,
            current_role,
            monitor_requested: false,
            current_boss_is_final: false,
        }
    }

    /// Kind of the area the player is currently in, derived from the shared
    /// area id rather than handler-local state: the shared id is also synced
    /// from the parse-worker restore, so it stays correct when this handler
    /// never saw the AreaEntered signal (app attached mid-instance).
    fn current_area_kind(&self) -> AreaKind {
        AreaKind::from_area_id(self.shared.current_area_id.load(Ordering::SeqCst))
    }

    /// Auto-start the operation timer from 0 if it isn't running and the user
    /// hasn't stopped it. Starts directly (not via command channel) to avoid the
    /// 1-second tick delay. Live mode only — historical replays must not drive it.
    ///
    /// reset() before start() zeroes time preserved from a previous op's
    /// final-boss auto-stop (area entry no longer resets operation timers).
    fn try_auto_start_operation_timer(&self) {
        if !self.shared.is_live_tailing.load(Ordering::SeqCst) {
            return;
        }
        let mut timer = self.shared.operation_timer.locked();
        if !timer.is_running() && !timer.manually_stopped {
            timer.reset();
            timer.start();
            drop(timer);
            let _ = self.cmd_tx.try_send(ServiceCommand::EmitOperationTimerTick);
        }
    }
}

impl SignalHandler for CombatSignalHandler {
    fn handle_signal(&mut self, signal: &GameSignal, _encounter: Option<&baras_core::encounter::CombatEncounter>) {
        // On first event processed, start monitoring the game process.
        // Always monitor so game_running state is accurate for stale detection.
        if !self.monitor_requested {
            self.monitor_requested = true;
            let _ = self.cmd_tx.try_send(ServiceCommand::StartProcessMonitor);
        }

        // Re-sync player identity from SharedState if unset. This handler is
        // constructed before the subprocess import stores the restored player
        // id/role, so a fresh-process restart mid-session would otherwise leave
        // it blind (breaking conversation auto-hide and role-change detection)
        // until the next DisciplineChanged signal at an area transition.
        if self.local_player_id.is_none() {
            let id = self.shared.last_player_id.load(Ordering::SeqCst);
            if id != 0 {
                self.local_player_id = Some(id);
                if self.current_role.is_none() {
                    self.current_role = Role::from_u8(self.shared.last_player_role.load(Ordering::SeqCst));
                }
            }
        }

        match signal {
            GameSignal::CombatStarted { .. } => {
                self.shared.in_combat.store(true, Ordering::SeqCst);
                let _ = self.trigger_tx.try_send(MetricsTrigger::CombatStarted);
                let _ = self.session_event_tx.send(SessionEvent::CombatStarted);
                // Always wipe a stale boss HP bar at the start of a new encounter.
                // With `clear_after_combat` disabled the bar persists post-combat, but
                // it must not linger into the next fight (e.g. trash after a boss).
                let _ = self
                    .overlay_tx
                    .try_send(OverlayUpdate::BossHealthUpdated(BossHealthData {
                        force_clear: true,
                        ..Default::default()
                    }));
                // If overlays were auto-hidden for not-live, restore them — combat means live
                if self.shared.auto_hide.is_not_live_active() {
                    let _ = self
                        .overlay_tx
                        .try_send(OverlayUpdate::NotLiveStateChanged { is_live: true });
                }
            }
            GameSignal::CombatEnded { timestamp, success, .. } => {
                self.shared.in_combat.store(false, Ordering::SeqCst);
                let _ = self.trigger_tx.try_send(MetricsTrigger::CombatEnded);
                let _ = self.session_event_tx.send(SessionEvent::CombatEnded);
                // Clear boss health and timer overlays
                let _ = self.overlay_tx.try_send(OverlayUpdate::CombatEnded);

                // Auto-stop operation timer when the final boss is KILLED (not wiped).
                // `success` comes from the signal and is set by determine_success() at the
                // emit site; `current_boss_is_final` was snapshotted at BossEncounterDetected
                // because `_encounter` here is the new empty encounter (push_new_encounter()
                // already ran).
                //
                // Only in live mode — historical replays should not drive the timer.
                if self.shared.is_live_tailing.load(Ordering::SeqCst)
                    && self.current_boss_is_final
                    && *success
                {
                    let area_kind = self.current_area_kind();
                    if matches!(area_kind, AreaKind::Operation | AreaKind::Flashpoint) {
                        let mut timer = self.shared.operation_timer.locked();
                        if timer.is_running() {
                            // Use the game-log timestamp to subtract the grace-period lag
                            // (CombatEnded is dispatched ~3s after the actual kill event).
                            timer.stop_auto_at(*timestamp);
                        }
                        drop(timer);
                        let _ = self.cmd_tx.try_send(ServiceCommand::EmitOperationTimerTick);
                    }
                }
                // Only reset the final-boss flag on a successful kill. On a wipe the
                // group will retry the same boss, so we must keep the flag set so the
                // next CombatEnded (the actual kill) can stop the timer.
                if *success {
                    self.current_boss_is_final = false;
                }
            }
            GameSignal::PlayerInitialized { entity_id, .. } => {
                // Emitted only for the local player, before the paired
                // DisciplineChanged. Adopting the ID here corrects the stale seed
                // restored from SharedState when a new log file belongs to a
                // different character than the previous session. The signal comes
                // from the discipline event matching the player identified by
                // AreaEntered or EnterCombat.
                if self.local_player_id != Some(*entity_id) {
                    self.local_player_id = Some(*entity_id);
                    self.shared.last_player_id.store(*entity_id, Ordering::SeqCst);
                    // Force role re-evaluation so the DisciplineChanged that
                    // follows fires AutoSwitchProfile for the new character
                    self.current_role = None;
                    // A different character's casts must not linger
                    let _ = self.overlay_tx.try_send(OverlayUpdate::AbilityCastCleared);
                }

                let _ = self.session_event_tx.send(SessionEvent::PlayerInitialized);

                // Player just initialized — if not-live auto-hide is active, the
                // session is now live. Re-evaluate so overlays restore without
                // waiting for combat.
                if self.shared.auto_hide.is_not_live_active() {
                    let _ = self
                        .overlay_tx
                        .try_send(OverlayUpdate::NotLiveStateChanged { is_live: true });
                }
            }
            GameSignal::DisciplineChanged {
                entity_id,
                class_id,
                discipline_id,
                ..
            } => {
                // Update raid registry with discipline info for role icons
                let mut registry = self
                    .shared
                    .raid_registry
                    .locked();
                registry.update_discipline(*entity_id, *class_id, *discipline_id);
                // This may be the log identity a provisional slot is waiting
                // for; the raid tick re-matches when the flag is set.
                self.shared.roster_changed.store(true, Ordering::Relaxed);
                if self.local_player_id != Some(*entity_id) {
                    let _ = self.session_event_tx.send(SessionEvent::PlayerInitialized);
                }
                // Auto-switch profile on role change (only for local player)
                if self.local_player_id == Some(*entity_id)
                    && let Some(discipline) = Discipline::from_guid(*discipline_id) {
                        let new_role = discipline.role();
                        let role_changed = self.current_role != Some(new_role);
                        self.current_role = Some(new_role);
                        self.shared.last_player_role.store(new_role.to_u8(), Ordering::SeqCst);
                        let _ = self.cmd_tx.try_send(ServiceCommand::ApplyAlacrityPreset);
                        if role_changed {
                            let role_name = format!("{:?}", new_role);
                            let _ = self.cmd_tx.try_send(ServiceCommand::AutoSwitchProfile { role_name });
                        }
                    }
            }
            GameSignal::EffectApplied {
                effect_id,
                target_id,
                ..
            }
                // Check for conversation effect on local player
                if *effect_id == baras_core::game_data::effect_id::CONVERSATION
                    && self.local_player_id == Some(*target_id)
                => {
                    let _ = self.overlay_tx.try_send(OverlayUpdate::ConversationStarted);
                }
            GameSignal::EffectRemoved {
                effect_id,
                target_id,
                ..
            }
                // Check for conversation effect removed from local player
                if *effect_id == baras_core::game_data::effect_id::CONVERSATION
                    && self.local_player_id == Some(*target_id)
                => {
                    let _ = self.overlay_tx.try_send(OverlayUpdate::ConversationEnded);
                }
            GameSignal::AreaEntered {
                area_id,
                difficulty_id,
                timestamp,
                ..
            } => {
                // Note: Boss definitions are loaded synchronously in process_event via definition_loader
                let current = self.shared.current_area_id.load(Ordering::SeqCst);
                let is_live = self.shared.is_live_tailing.load(Ordering::SeqCst);
                if raid_detection::should_reset_on_area_entry(current, *area_id, is_live) {
                    self.shared
                        .raid_registry
                        .locked()
                        .clear();
                    self.shared
                        .ability_roster
                        .locked()
                        .clear();
                    self.shared.roster_changed.store(false, Ordering::Relaxed);
                    *self
                        .shared
                        .pvp_ocr_roster_started_at
                        .locked() = Some(*timestamp);
                    // The overlay renders its own copy of the slots; without a
                    // refresh it keeps showing the previous group's assignments.
                    let _ = self.cmd_tx.try_send(ServiceCommand::RefreshRaidFrames);
                    info!(area_id, "Cleared OCR roster on entering a new PvP area");
                } else if is_live && *area_id != current {
                    *self
                        .shared
                        .pvp_ocr_roster_started_at
                        .locked() = None;
                }
                if *area_id != current {
                    // Metric rebuilds keep the last encounter's enemies until a
                    // new encounter replaces them; leaving the warzone is the end.
                    if is_live
                        && baras_core::game_data::is_pvp_area(current)
                        && !baras_core::game_data::is_pvp_area(*area_id)
                    {
                        let _ = self.overlay_tx.try_send(OverlayUpdate::EnemyFramesCleared);
                    }
                    self.shared
                        .current_area_id
                        .store(*area_id, Ordering::SeqCst);
                    let _ = self.session_event_tx.send(SessionEvent::AreaChanged);
                    // Trigger area definition reload (will send notes or clear them)
                    let _ = self.cmd_tx.try_send(ServiceCommand::ReloadAreaDefinitions(*area_id));

                    // ── Operation timer area-transition logic ────────────────────
                    // Only in live mode - historical replays should not drive the timer
                    if is_live {
                        // `current` is the area id before this transition; it is
                        // also synced from the parse-worker restore, so this stays
                        // correct when the entry event was consumed by the worker.
                        let prev_kind = AreaKind::from_area_id(current);
                        let new_kind = AreaKind::from_area_id(*area_id);

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

                        // Entering a timed area (op/FP/PvP): update the display name.
                        // For FP/PvP: also reset the timer for a fresh run.
                        // For Operations: do NOT reset — the timer only changes state on
                        // first boss pull, final boss kill, explicit user action, joining
                        // a different instance, or the exit timeout.
                        if matches!(
                            new_kind,
                            AreaKind::Operation | AreaKind::Flashpoint | AreaKind::PvP
                        ) {
                            let mut timer = self.shared.operation_timer.locked();
                            if matches!(new_kind, AreaKind::Operation) {
                                let instance = (*area_id, *difficulty_id);
                                // Joining a different instance (other operation and/or
                                // difficulty) ends the previous run.
                                if timer.is_running()
                                    && timer.op_instance.is_some_and(|i| i != instance)
                                {
                                    timer.stop_auto();
                                    let _ = self
                                        .cmd_tx
                                        .try_send(ServiceCommand::EmitOperationTimerTick);
                                }
                                timer.op_instance = Some(instance);
                                // Back inside the op area — cancel any pending exit timeout
                                timer.clear_exited();
                            }
                            timer.operation_name = area_display_name.clone();
                            if new_kind.auto_resets_on_entry() {
                                timer.reset();
                            }
                            drop(timer);
                        }
                        // When returning to open world / fleet, do NOT touch operation_name.
                        // The name stays on the overlay so the user can see which run the
                        // (now-stopped) timer belongs to.

                        // If leaving an operation area while the timer runs, start the
                        // exit-timeout countdown (checked in the 1s tick loop). Re-entering
                        // the same instance cancels it via clear_exited() above.
                        if matches!(prev_kind, AreaKind::Operation)
                            && !matches!(new_kind, AreaKind::Operation)
                        {
                            let mut timer = self.shared.operation_timer.locked();
                            timer.mark_exited();
                        }

                        // If leaving a FP/PvP area, auto-stop the timer.
                        if matches!(prev_kind, AreaKind::Flashpoint | AreaKind::PvP)
                            && !matches!(new_kind, AreaKind::Flashpoint | AreaKind::PvP)
                        {
                            let mut timer = self.shared.operation_timer.locked();
                            if timer.is_running() {
                                timer.stop_auto();
                            }
                            drop(timer);
                            let _ = self.cmd_tx.try_send(ServiceCommand::EmitOperationTimerTick);
                        }

                        // If entering a FP/PvP area: immediately start the timer
                        // (requirement 1: no boss pull needed).
                        if new_kind.auto_starts_on_entry() {
                            let mut timer = self.shared.operation_timer.locked();
                            if !timer.is_running() && !timer.manually_stopped {
                                timer.start();
                                drop(timer);
                                let _ = self.cmd_tx.try_send(ServiceCommand::EmitOperationTimerTick);
                            }
                        }
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
                if let Some(enc) = _encounter
                    && let Some(def) = enc.boss_definitions().get(*definition_idx)
                        && let Some(notes) = &def.notes
                            && !notes.is_empty() {
                                let notes_data = NotesData {
                                    text: notes.clone(),
                                    boss_name: boss_name.clone(),
                                };
                                let _ = self.overlay_tx.try_send(OverlayUpdate::NotesUpdated(notes_data));
                            }

                // First boss pull: fallback start for operations without a
                // region-based `timer_start` (or if that region was never logged).
                if matches!(self.current_area_kind(), AreaKind::Operation) {
                    self.try_auto_start_operation_timer();
                }
            }
            GameSignal::OperationTimerStart { entity_id, .. } => {
                // A player crossed the area's achievement start line.
                // A `[area.timer_start]` region is explicit opt-in, so no area-kind gate.
                info!(entity_id, "Operation timer start region crossed");
                self.try_auto_start_operation_timer();
            }
            GameSignal::InstanceReset { run_started, .. }
                // The group reset the operation: the running clock belongs to
                // the abandoned instance. Restart it from the new instance's
                // start line, or zero it until that (or the first boss pull).
                if self.shared.is_live_tailing.load(Ordering::SeqCst)
                    && matches!(self.current_area_kind(), AreaKind::Operation)
                => {
                    let mut timer = self.shared.operation_timer.locked();
                    if timer.is_running() && !timer.manually_started {
                        timer.reset();
                        if let Some(at) = run_started {
                            timer.start_with_offset(secs_since(*at));
                        }
                        drop(timer);
                        info!(?run_started, "Operation timer restarted for a reset instance");
                        let _ = self.cmd_tx.try_send(ServiceCommand::EmitOperationTimerTick);
                    }
                }
            GameSignal::AbilityActivated {
                ability_id,
                ability_name,
                source_id,
                ..
            } if self.shared.ability_cast_overlay_active.load(Ordering::SeqCst)
                && self.local_player_id == Some(*source_id) =>
            {
                let _ = self.overlay_tx.try_send(OverlayUpdate::AbilityCast {
                    ability_id: *ability_id as u64,
                    name: resolve(*ability_name).to_string(),
                });
            }
            _ => {}
        }
    }
}
