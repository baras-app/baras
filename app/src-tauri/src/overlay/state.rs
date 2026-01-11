//! Overlay state management
//!
//! Types for managing overlay runtime state, commands, and thread handles.

use std::collections::HashMap;
use std::thread::JoinHandle;
use tokio::sync::mpsc::Sender;

use baras_overlay::{OverlayConfigUpdate, OverlayData};

use super::types::{MetricType, OverlayType};

// ─────────────────────────────────────────────────────────────────────────────
// Commands and Events
// ─────────────────────────────────────────────────────────────────────────────

/// Commands sent to overlay threads
pub enum OverlayCommand {
    /// Toggle move/resize mode (global, all overlays)
    SetMoveMode(bool),
    /// Toggle rearrange mode (raid overlay only - click-to-swap frames)
    SetRearrangeMode(bool),
    /// Update overlay data (metrics or personal stats)
    UpdateData(OverlayData),
    /// Update overlay configuration
    UpdateConfig(OverlayConfigUpdate),
    /// Set overlay position (x, y in screen coordinates)
    SetPosition(i32, i32),
    /// Request current position via oneshot channel
    GetPosition(tokio::sync::oneshot::Sender<PositionEvent>),
    /// Shutdown the overlay
    Shutdown,
}

/// Position update event from overlay thread
#[derive(Debug, Clone)]
pub struct PositionEvent {
    pub kind: OverlayType,
    /// Absolute X position (screen coordinates)
    pub x: i32,
    /// Absolute Y position (screen coordinates)
    pub y: i32,
    pub width: u32,
    pub height: u32,
    /// Monitor ID where the overlay is currently located
    pub monitor_id: Option<String>,
    /// Monitor's top-left X coordinate (for relative position calculation)
    pub monitor_x: i32,
    /// Monitor's top-left Y coordinate (for relative position calculation)
    pub monitor_y: i32,
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Handle
// ─────────────────────────────────────────────────────────────────────────────

/// Handle to a running overlay instance
pub struct OverlayHandle {
    pub tx: Sender<OverlayCommand>,
    pub handle: JoinHandle<()>,
    pub kind: OverlayType,
    /// Optional receiver for registry actions from the raid overlay
    /// Uses RaidRegistryAction from baras_overlay crate
    pub registry_action_rx: Option<std::sync::mpsc::Receiver<baras_overlay::RaidRegistryAction>>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay State
// ─────────────────────────────────────────────────────────────────────────────

/// State managing all overlay threads
pub struct OverlayState {
    /// All running overlays, keyed by their kind
    pub overlays: HashMap<OverlayType, OverlayHandle>,
    /// Global move mode state
    pub move_mode: bool,
    /// Raid rearrange mode state (click-to-swap frames)
    pub rearrange_mode: bool,
    /// Whether overlays are currently visible (mirrors config)
    pub overlays_visible: bool,
}

impl Default for OverlayState {
    fn default() -> Self {
        Self {
            overlays: HashMap::new(),
            move_mode: false,
            rearrange_mode: false,
            overlays_visible: true,
        }
    }
}

impl std::fmt::Debug for OverlayState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OverlayState")
            .field("overlays", &self.overlays.keys().collect::<Vec<_>>())
            .field("move_mode", &self.move_mode)
            .finish()
    }
}

impl OverlayState {
    /// Check if a specific overlay is running
    pub fn is_running(&self, kind: OverlayType) -> bool {
        self.overlays.contains_key(&kind)
    }

    /// Check if a metric overlay type is running (convenience method)
    pub fn is_metric_running(&self, overlay_type: MetricType) -> bool {
        self.overlays
            .contains_key(&OverlayType::Metric(overlay_type))
    }

    /// Check if personal overlay is running
    pub fn is_personal_running(&self) -> bool {
        self.overlays.contains_key(&OverlayType::Personal)
    }

    /// Check if any overlay is running
    pub fn any_running(&self) -> bool {
        !self.overlays.is_empty()
    }

    /// Get the channel for a specific overlay
    pub fn get_tx(&self, kind: OverlayType) -> Option<&Sender<OverlayCommand>> {
        self.overlays.get(&kind).map(|h| &h.tx)
    }

    /// Get the channel for a metric overlay type (convenience)
    pub fn get_metric_tx(&self, overlay_type: MetricType) -> Option<&Sender<OverlayCommand>> {
        self.get_tx(OverlayType::Metric(overlay_type))
    }

    /// Get the channel for personal overlay (convenience)
    pub fn get_personal_tx(&self) -> Option<&Sender<OverlayCommand>> {
        self.get_tx(OverlayType::Personal)
    }

    /// Get the channel for raid overlay (convenience)
    pub fn get_raid_tx(&self) -> Option<&Sender<OverlayCommand>> {
        self.get_tx(OverlayType::Raid)
    }

    /// Check if raid overlay is running
    pub fn is_raid_running(&self) -> bool {
        self.overlays.contains_key(&OverlayType::Raid)
    }

    /// Get the channel for boss health overlay (convenience)
    pub fn get_boss_health_tx(&self) -> Option<&Sender<OverlayCommand>> {
        self.get_tx(OverlayType::BossHealth)
    }

    /// Get the channel for timer overlay (convenience)
    pub fn get_timers_tx(&self) -> Option<&Sender<OverlayCommand>> {
        self.get_tx(OverlayType::Timers)
    }

    /// Check if boss health overlay is running
    pub fn is_boss_health_running(&self) -> bool {
        self.overlays.contains_key(&OverlayType::BossHealth)
    }

    /// Get all channels
    pub fn all_txs(&self) -> Vec<&Sender<OverlayCommand>> {
        self.overlays.values().map(|h| &h.tx).collect()
    }

    /// Get all running metric overlay types
    pub fn running_metric_types(&self) -> Vec<MetricType> {
        self.overlays
            .keys()
            .filter_map(|k| match k {
                OverlayType::Metric(ot) => Some(*ot),
                _ => None,
            })
            .collect()
    }

    /// Get the channel for challenges overlay (convenience)
    pub fn get_challenges_tx(&self) -> Option<&Sender<OverlayCommand>> {
        self.get_tx(OverlayType::Challenges)
    }

    /// Check if challenges overlay is running
    pub fn is_challenges_running(&self) -> bool {
        self.overlays.contains_key(&OverlayType::Challenges)
    }

    /// Get the channel for personal buffs overlay (convenience)
    pub fn get_personal_buffs_tx(&self) -> Option<&Sender<OverlayCommand>> {
        self.get_tx(OverlayType::PersonalBuffs)
    }

    /// Get the channel for personal debuffs overlay (convenience)
    pub fn get_personal_debuffs_tx(&self) -> Option<&Sender<OverlayCommand>> {
        self.get_tx(OverlayType::PersonalDebuffs)
    }

    /// Get the channel for cooldowns overlay (convenience)
    pub fn get_cooldowns_tx(&self) -> Option<&Sender<OverlayCommand>> {
        self.get_tx(OverlayType::Cooldowns)
    }

    /// Get the channel for DOT tracker overlay (convenience)
    pub fn get_dot_tracker_tx(&self) -> Option<&Sender<OverlayCommand>> {
        self.get_tx(OverlayType::DotTracker)
    }

    /// Insert an overlay handle
    pub fn insert(&mut self, handle: OverlayHandle) {
        self.overlays.insert(handle.kind, handle);
    }

    /// Remove an overlay by kind
    pub fn remove(&mut self, kind: OverlayType) -> Option<OverlayHandle> {
        self.overlays.remove(&kind)
    }

    /// Drain all overlays
    pub fn drain(&mut self) -> Vec<OverlayHandle> {
        self.overlays.drain().map(|(_, h)| h).collect()
    }

    /// Get all overlay kinds and their channels
    pub fn all_overlays(&self) -> Vec<(OverlayType, &Sender<OverlayCommand>)> {
        self.overlays.iter().map(|(k, h)| (*k, &h.tx)).collect()
    }

    /// Get list of running overlay kinds
    pub fn running_overlays(&self) -> Vec<OverlayType> {
        self.overlays.keys().copied().collect()
    }

    /// Set move mode and broadcast to all overlays
    pub fn set_move_mode(&mut self, enabled: bool) {
        self.move_mode = enabled;
        // Note: Actual broadcast to overlays must be done by caller with async context
    }

    /// Set rearrange mode for raid overlay
    pub fn set_rearrange_mode(&mut self, enabled: bool) {
        self.rearrange_mode = enabled;
        // Note: Actual broadcast to overlay must be done by caller with async context
    }
}
