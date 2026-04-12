//! Application state management
//!
//! This module contains all shared state types used across the Tauri application:
//! - `SharedState`: Core application state shared between service and commands
//! - `RaidSlotRegistry`: Persistent player-to-slot assignments for raid frames

mod raid_registry;

pub use raid_registry::{RaidSlotRegistry, RegisteredPlayer};

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;

use baras_core::context::{AppConfig, DirectoryIndex, LogAreaCache, ParsingSession};
use baras_core::query::QueryContext;

// ─── Centralized Auto-Hide State ─────────────────────────────────────────────

/// Centralized auto-hide state — the sole authority on whether overlays should
/// be suppressed. All overlay spawn paths check `is_auto_hidden()` before
/// creating windows. Each auto-hide condition sets its own flag; overlays are
/// hidden when ANY flag is true.
pub struct AutoHideState {
    /// Hidden because the local player is in a conversation
    conversation_active: AtomicBool,
    /// Hidden because the session is not live (historical, stale, game closed)
    not_live_active: AtomicBool,
    /// Raw condition state: whether the session is currently not-live, regardless
    /// of whether the hide_when_not_live setting is enabled. Updated by every
    /// `NotLiveStateChanged` event. Used by `apply_not_live_auto_hide` to know
    /// the current condition when the user toggles the setting ON.
    session_not_live: AtomicBool,
    /// Game process was just detected launching. Blocks stale-recovery restore
    /// until a fresh log session with a new character registration is confirmed,
    /// preventing overlays from flashing on while the old log file is still present.
    /// Cleared by the parse worker once a live session is verified.
    game_starting: AtomicBool,
}

impl AutoHideState {
    pub fn new() -> Self {
        Self {
            conversation_active: AtomicBool::new(false),
            not_live_active: AtomicBool::new(false),
            session_not_live: AtomicBool::new(false),
            game_starting: AtomicBool::new(false),
        }
    }

    /// Returns true if ANY auto-hide condition is active.
    /// This is the single check all overlay spawn paths use.
    pub fn is_auto_hidden(&self) -> bool {
        self.conversation_active.load(Ordering::SeqCst)
            || self.not_live_active.load(Ordering::SeqCst)
    }

    /// Whether conversation auto-hide is currently active.
    pub fn is_conversation_active(&self) -> bool {
        self.conversation_active.load(Ordering::SeqCst)
    }

    /// Whether not-live auto-hide is currently active.
    pub fn is_not_live_active(&self) -> bool {
        self.not_live_active.load(Ordering::SeqCst)
    }

    /// Whether the session is currently in a not-live state (condition flag).
    /// This is independent of the hide_when_not_live setting — it tracks the
    /// raw condition from NotLiveStateChanged events.
    pub fn is_session_not_live(&self) -> bool {
        self.session_not_live.load(Ordering::SeqCst)
    }

    /// Update the raw session not-live condition state.
    /// Called on every NotLiveStateChanged event regardless of settings.
    pub fn set_session_not_live(&self, not_live: bool) {
        self.session_not_live.store(not_live, Ordering::SeqCst);
    }

    /// Set conversation auto-hide state.
    /// Returns the new `is_auto_hidden()` value after the change.
    pub fn set_conversation(&self, active: bool) -> bool {
        self.conversation_active.store(active, Ordering::SeqCst);
        self.is_auto_hidden()
    }

    /// Set not-live auto-hide state.
    /// Returns the new `is_auto_hidden()` value after the change.
    pub fn set_not_live(&self, active: bool) -> bool {
        self.not_live_active.store(active, Ordering::SeqCst);
        self.is_auto_hidden()
    }

    /// Whether the game was just detected launching (blocks premature restore).
    pub fn is_game_starting(&self) -> bool {
        self.game_starting.load(Ordering::SeqCst)
    }

    /// Set or clear the game-starting flag.
    pub fn set_game_starting(&self, starting: bool) {
        self.game_starting.store(starting, Ordering::SeqCst);
    }

    /// Clear all auto-hide state.
    pub fn clear_all(&self) {
        self.conversation_active.store(false, Ordering::SeqCst);
        self.not_live_active.store(false, Ordering::SeqCst);
        self.session_not_live.store(false, Ordering::SeqCst);
        self.game_starting.store(false, Ordering::SeqCst);
    }
}

// ─── Shared State ────────────────────────────────────────────────────────────

/// State shared between the combat service and Tauri commands.
///
/// This is the central state container that coordinates:
/// - Configuration (persisted to disk)
/// - Directory index (log files available)
/// - Current parsing session (if tailing)
/// - Combat state flags
/// - Raid frame slot assignments
pub struct SharedState {
    /// Application configuration (persisted to disk)
    pub config: RwLock<AppConfig>,
    /// Index of log files in the configured directory
    pub directory_index: RwLock<DirectoryIndex>,
    /// Current parsing session (when tailing a log file)
    pub session: RwLock<Option<Arc<RwLock<ParsingSession>>>>,
    /// Whether we're currently in active combat (for metrics updates)
    pub in_combat: AtomicBool,
    /// Whether the directory watcher is active
    pub watching: AtomicBool,
    /// Whether we're in live tailing mode (vs viewing historical file)
    pub is_live_tailing: AtomicBool,
    /// Raid frame slot assignments (persists player positions)
    pub raid_registry: Mutex<RaidSlotRegistry>,
    /// Current area ID for lazy loading timers (0 = unknown)
    pub current_area_id: AtomicI64,

    // ─── Overlay status flags (for skipping work when not needed) ───
    /// Whether raid overlay is currently running
    pub raid_overlay_active: AtomicBool,
    /// Whether boss health overlay is currently running
    pub boss_health_overlay_active: AtomicBool,
    /// Whether timer overlay is currently running
    pub timer_overlay_active: AtomicBool,
    /// Whether effects A overlay is currently running
    pub effects_a_overlay_active: AtomicBool,
    /// Whether effects B overlay is currently running
    pub effects_b_overlay_active: AtomicBool,
    /// Whether cooldowns overlay is currently running
    pub cooldowns_overlay_active: AtomicBool,
    /// Whether DOT tracker overlay is currently running
    pub dot_tracker_overlay_active: AtomicBool,
    /// Whether raid frame rearrange mode is active (bypasses rendering gates)
    pub rearrange_mode: AtomicBool,

    /// Whether the game process is currently running (updated by the process monitor)
    pub game_running: AtomicBool,

    // ─── Centralized auto-hide ───────────────────────────────────────────────
    /// Unified auto-hide state — the single source of truth for overlay suppression
    pub auto_hide: AutoHideState,

    /// Shared query context for DataFusion queries (reuses SessionContext)
    pub query_context: QueryContext,

    /// Cache of area indexes for log files (persisted to disk)
    pub area_cache: RwLock<LogAreaCache>,

    /// Operation timer state (persistent across encounters, lives in service layer)
    pub operation_timer: Mutex<crate::service::OperationTimerState>,
}

impl SharedState {
    pub fn new(config: AppConfig, directory_index: DirectoryIndex) -> Self {
        let raid_slots = config.overlay_settings.raid_overlay.total_slots();
        Self {
            config: RwLock::new(config),
            directory_index: RwLock::new(directory_index),
            session: RwLock::new(None),
            in_combat: AtomicBool::new(false),
            watching: AtomicBool::new(false),
            is_live_tailing: AtomicBool::new(true), // Start in live tailing mode
            raid_registry: Mutex::new(RaidSlotRegistry::new(raid_slots)),
            current_area_id: AtomicI64::new(0),
            // Overlay status flags - updated by OverlayManager
            raid_overlay_active: AtomicBool::new(false),
            boss_health_overlay_active: AtomicBool::new(false),
            timer_overlay_active: AtomicBool::new(false),
            effects_a_overlay_active: AtomicBool::new(false),
            effects_b_overlay_active: AtomicBool::new(false),
            cooldowns_overlay_active: AtomicBool::new(false),
            dot_tracker_overlay_active: AtomicBool::new(false),
            rearrange_mode: AtomicBool::new(false),
            // Game process state — assume running until the process monitor says otherwise
            game_running: AtomicBool::new(true),
            // Centralized auto-hide state
            auto_hide: AutoHideState::new(),
            // Shared query context for DataFusion (reuses SessionContext across queries)
            query_context: QueryContext::new(),
            // Area cache - loaded from disk later in service startup
            area_cache: RwLock::new(LogAreaCache::new()),
            // Operation timer (defaults to stopped/empty)
            operation_timer: Mutex::new(crate::service::OperationTimerState::default()),
        }
    }

    /// Check if the current session is "not live" — i.e. historical, game closed,
    /// newest log file is blank, or has no player.
    /// Returns `true` if overlays should be auto-hidden due to not-live conditions.
    pub async fn is_session_not_live(&self) -> bool {
        if !self.is_live_tailing.load(Ordering::SeqCst) {
            return true;
        }

        // Game process not running → not live
        if !self.game_running.load(Ordering::SeqCst) {
            return true;
        }

        // Game process just detected launching but no fresh session yet — keep hidden
        // until the parse worker confirms a new character has registered.
        if self.auto_hide.is_game_starting() {
            return true;
        }

        let session_guard = self.session.read().await;
        let Some(session) = session_guard.as_ref() else {
            return true;
        };

        let s = session.read().await;
        let has_player = s
            .session_cache
            .as_ref()
            .map(|c| c.player_initialized)
            .unwrap_or(false);

        if !has_player {
            return true;
        }

        // Only stale if the newest log file is blank (session rotation —
        // SWTOR created a new empty file meaning the player logged out/restarted)
        let index = self.directory_index.read().await;
        if index.newest_file().map(|f| f.is_empty).unwrap_or(false) {
            return true;
        }

        false
    }

    /// Execute a function with mutable access to the current session.
    /// Returns `None` if no session is active.
    pub async fn with_session<F, T>(&self, f: F) -> Option<T>
    where
        F: FnOnce(&mut ParsingSession) -> T,
    {
        let session_lock = self.session.read().await;
        if let Some(session_arc) = &*session_lock {
            let mut session = session_arc.write().await;
            Some(f(&mut session))
        } else {
            None
        }
    }
}
