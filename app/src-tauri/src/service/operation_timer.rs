//! Operation timer state and its service tick

use super::{CombatService, OverlayUpdate};
use crate::state::MutexExt;
use tauri::Emitter;

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
    /// Operations are excluded: the operation timer must not be driven by area
    /// changes — it starts on first boss pull (or manual start) and stops only
    /// when the final boss is defeated (or manual stop).
    pub fn auto_resets_on_entry(self) -> bool {
        matches!(self, AreaKind::Flashpoint | AreaKind::PvP)
    }
}

/// Whole seconds from a game-log (local) timestamp to now.
pub(super) fn secs_since(at: chrono::NaiveDateTime) -> u64 {
    chrono::Local::now()
        .naive_local()
        .signed_duration_since(at)
        .num_seconds()
        .max(0) as u64
}

/// Stop the operation timer if the player stays outside the instance this long.
pub(super) const OP_EXIT_TIMEOUT_SECS: u64 = 600;

/// Persistent timer state that tracks an entire operation run.
/// Lives in the service layer, independent of overlay visibility.
#[derive(Debug, Default)]
pub struct OperationTimerState {
    /// When the timer was last started (None = stopped/not started)
    pub(super) started_at: Option<std::time::Instant>,
    /// Accumulated seconds from previous start/stop cycles
    pub(super) accumulated_secs: u64,
    /// Whether the user manually started (suppresses auto-start)
    pub(super) manually_started: bool,
    /// Whether the user manually stopped (suppresses auto-start until reset)
    pub(super) manually_stopped: bool,
    /// Current operation name (from AreaEntered signal)
    pub(super) operation_name: Option<String>,
    /// Identity of the operation instance this timer belongs to:
    /// (area_id, difficulty_id). A different tuple on AreaEntered means the
    /// player joined another instance and the timer auto-stops.
    pub(super) op_instance: Option<(i64, i64)>,
    /// Set when the player leaves the operation area while the timer runs:
    /// (wall-clock exit moment, elapsed seconds at exit). If the same instance
    /// isn't re-entered within the timeout, the timer stops retroactively at
    /// the exit snapshot.
    pub(super) exited_at: Option<(std::time::Instant, u64)>,
}

impl OperationTimerState {
    /// Get total elapsed seconds including current running segment
    pub fn elapsed_secs(&self) -> u64 {
        self.accumulated_secs + self.started_at.map(|s| s.elapsed().as_secs()).unwrap_or(0)
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
        self.exited_at = None;
    }

    /// Start the timer with a pre-seeded elapsed time (for live-resume backfill).
    /// Pre-accumulates `pre_secs` seconds so the timer immediately shows backfilled time.
    pub fn start_with_offset(&mut self, pre_secs: u64) {
        self.accumulated_secs = pre_secs;
        self.started_at = Some(std::time::Instant::now());
        self.exited_at = None;
    }

    /// Record that the player left the operation area while the timer was
    /// running. Snapshots the elapsed time so a later timeout can stop the
    /// timer retroactively at the moment of exit.
    pub fn mark_exited(&mut self) {
        if self.started_at.is_some() && self.exited_at.is_none() {
            self.exited_at = Some((std::time::Instant::now(), self.elapsed_secs()));
        }
    }

    /// Player re-entered the operation instance — cancel any pending timeout.
    pub fn clear_exited(&mut self) {
        self.exited_at = None;
    }

    /// Stop the timer if the player has been outside the operation area for
    /// longer than `timeout_secs`. The elapsed total rolls back to the exit
    /// snapshot so idle time outside the instance doesn't count toward the run.
    /// Returns true if the timer was stopped.
    pub fn check_exit_timeout(&mut self, timeout_secs: u64) -> bool {
        if let Some((exit_time, elapsed_at_exit)) = self.exited_at
            && self.started_at.is_some()
            && exit_time.elapsed().as_secs() >= timeout_secs
        {
            self.started_at = None;
            self.accumulated_secs = elapsed_at_exit;
            self.exited_at = None;
            // Not a manual stop — the next op's first boss pull may auto-start
            return true;
        }
        false
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

    /// Reset all timer state (keeps operation_name and op_instance — both are
    /// re-set on every operation AreaEntered)
    pub fn reset(&mut self) {
        self.started_at = None;
        self.accumulated_secs = 0;
        self.manually_started = false;
        self.manually_stopped = false;
        self.exited_at = None;
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

impl CombatService {
    /// Emit the current operation timer state to the frontend via Tauri event
    /// and to the overlay via the overlay update channel.
    pub(super) fn emit_operation_timer_tick(&self) {
        let timer = self.shared.operation_timer.locked();
        let data = timer.to_overlay_data();
        drop(timer);

        // Send to frontend (session box UI)
        let _ = self.app_handle.emit(
            "operation-timer-tick",
            serde_json::json!({
                "elapsed_secs": data.elapsed_secs,
                "is_running": data.is_running,
                "operation_name": data.operation_name,
            }),
        );

        // Send to overlay
        let _ = self.overlay_tx.try_send(OverlayUpdate::OperationTimerUpdated(data));
    }
}
