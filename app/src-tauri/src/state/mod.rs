//! Application state management
//!
//! This module contains all shared state types used across the Tauri application:
//! - `SharedState`: Core application state shared between service and commands
//! - `RaidSlotRegistry`: Persistent player-to-slot assignments for raid frames

mod raid_registry;

pub use raid_registry::{RaidSlotRegistry, RegisteredPlayer};

use std::sync::atomic::{AtomicBool, AtomicI64};
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;

use baras_core::context::{AppConfig, DirectoryIndex, ParsingSession};
use baras_core::query::QueryContext;

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
    /// Whether personal buffs overlay is currently running
    pub personal_buffs_overlay_active: AtomicBool,
    /// Whether personal debuffs overlay is currently running
    pub personal_debuffs_overlay_active: AtomicBool,
    /// Whether cooldowns overlay is currently running
    pub cooldowns_overlay_active: AtomicBool,
    /// Whether DOT tracker overlay is currently running
    pub dot_tracker_overlay_active: AtomicBool,
    /// Whether raid frame rearrange mode is active (bypasses rendering gates)
    pub rearrange_mode: AtomicBool,

    // ─── Conversation auto-hide state ───────────────────────────────────────
    /// Whether overlays are temporarily hidden due to conversation
    pub conversation_hiding_active: AtomicBool,
    /// Whether overlays were visible before conversation started (for restore)
    pub overlays_visible_before_conversation: AtomicBool,

    /// Shared query context for DataFusion queries (reuses SessionContext)
    pub query_context: QueryContext,
}

impl SharedState {
    pub fn new(config: AppConfig, directory_index: DirectoryIndex) -> Self {
        Self {
            config: RwLock::new(config),
            directory_index: RwLock::new(directory_index),
            session: RwLock::new(None),
            in_combat: AtomicBool::new(false),
            watching: AtomicBool::new(false),
            is_live_tailing: AtomicBool::new(true), // Start in live tailing mode
            raid_registry: Mutex::new(RaidSlotRegistry::new(8)), // Default 8 slots (2x4 grid)
            current_area_id: AtomicI64::new(0),
            // Overlay status flags - updated by OverlayManager
            raid_overlay_active: AtomicBool::new(false),
            boss_health_overlay_active: AtomicBool::new(false),
            timer_overlay_active: AtomicBool::new(false),
            personal_buffs_overlay_active: AtomicBool::new(false),
            personal_debuffs_overlay_active: AtomicBool::new(false),
            cooldowns_overlay_active: AtomicBool::new(false),
            dot_tracker_overlay_active: AtomicBool::new(false),
            rearrange_mode: AtomicBool::new(false),
            // Conversation auto-hide state
            conversation_hiding_active: AtomicBool::new(false),
            overlays_visible_before_conversation: AtomicBool::new(false),
            // Shared query context for DataFusion (reuses SessionContext across queries)
            query_context: QueryContext::new(),
        }
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
