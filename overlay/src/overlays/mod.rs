//! Complete overlay implementations
//!
//! Each overlay type is a self-contained window that displays specific
//! combat information. Overlays use the OverlayFrame for common chrome
//! and the platform layer for window management.
//!
//! # Overlay Trait
//!
//! All overlays implement the `Overlay` trait, which provides a unified
//! interface for the application layer to interact with any overlay type.

mod boss_health;
mod challenges;
mod effects;
mod metric;
mod personal;
mod raid;
mod timers;

pub use boss_health::{BossHealthData, BossHealthOverlay};
pub use challenges::{ChallengeData, ChallengeEntry, ChallengeOverlay, PlayerContribution};
pub use effects::{EffectEntry, EffectsData, EffectsOverlay};
pub use metric::{MetricEntry, MetricOverlay};
pub use personal::{PersonalOverlay, PersonalStats};
pub use raid::{
    // Effect config bounds (for UI sliders, validation, etc.)
    EFFECT_OFFSET_DEFAULT,
    EFFECT_OFFSET_MAX,
    EFFECT_OFFSET_MIN,
    EFFECT_SIZE_DEFAULT,
    EFFECT_SIZE_MAX,
    EFFECT_SIZE_MIN,
    InteractionMode,
    PlayerRole,
    RaidEffect,
    RaidFrame,
    RaidFrameData,
    RaidGridLayout,
    RaidOverlay,
    RaidOverlayConfig,
    SwapState,
};
pub use timers::{TimerData, TimerEntry, TimerOverlay};

// ─────────────────────────────────────────────────────────────────────────────
// Registry Action (for raid overlay → service communication)
// ─────────────────────────────────────────────────────────────────────────────

/// Actions that the raid overlay wants to perform on the registry.
/// These are collected by the overlay and polled by the spawn loop.
#[derive(Debug, Clone)]
pub enum RaidRegistryAction {
    /// Swap two slots
    SwapSlots(u8, u8),
    /// Clear a specific slot
    ClearSlot(u8),
}

use crate::frame::OverlayFrame;
use baras_core::context::{
    BossHealthConfig, ChallengeOverlayConfig, OverlayAppearanceConfig, PersonalOverlayConfig,
    TimerOverlayConfig,
};

// ─────────────────────────────────────────────────────────────────────────────
// Data Types
// ─────────────────────────────────────────────────────────────────────────────

/// Data that can be sent to overlays for display updates
#[derive(Debug, Clone)]
pub enum OverlayData {
    /// Metric entries for DPS/HPS/TPS meters
    Metrics(Vec<MetricEntry>),
    /// Personal player statistics
    Personal(PersonalStats),
    /// Raid frame data
    Raid(RaidFrameData),
    /// Boss health bar data
    BossHealth(BossHealthData),
    /// Timer countdown bars
    Timers(TimerData),
    /// Effects countdown bars
    Effects(EffectsData),
    /// Challenge metrics during boss encounters
    Challenges(ChallengeData),
}

/// Configuration updates that can be sent to overlays
#[derive(Debug, Clone)]
pub enum OverlayConfigUpdate {
    /// Appearance config for metric overlays (+ background alpha)
    Metric(OverlayAppearanceConfig, u8),
    /// Config for personal overlay (+ background alpha)
    Personal(PersonalOverlayConfig, u8),
    /// Config for raid overlay (+ background alpha)
    Raid(RaidOverlayConfig, u8),
    /// Config for boss health overlay (+ background alpha)
    BossHealth(BossHealthConfig, u8),
    /// Config for timer overlay (+ background alpha)
    Timers(TimerOverlayConfig, u8),
    /// Config for effects overlay (+ background alpha)
    Effects(TimerOverlayConfig, u8),
    /// Config for challenge overlay (+ background alpha)
    Challenge(ChallengeOverlayConfig, u8),
}

/// Position information for an overlay
#[derive(Debug, Clone, Copy)]
pub struct OverlayPosition {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Trait
// ─────────────────────────────────────────────────────────────────────────────

/// Trait implemented by all overlay types
///
/// This provides a unified interface for the application layer to interact
/// with any overlay type without needing to know its specific implementation.
///
/// Note: Overlays do NOT need to implement Send because they are created
/// inside their dedicated thread via spawn_overlay_with_factory. Only the
/// factory closure (which captures config data) needs to be Send.
pub trait Overlay: 'static {
    /// Update the overlay with new data
    ///
    /// Returns `true` if the data changed meaningfully and a re-render is needed.
    /// Returns `false` if the data is unchanged (e.g., empty -> empty).
    ///
    /// Implementations should check if the data variant matches their type
    /// and update accordingly. Mismatched variants return `false`.
    fn update_data(&mut self, data: OverlayData) -> bool;

    /// Update the overlay configuration/appearance
    ///
    /// Implementations should check if the config variant matches their type
    /// and update accordingly. Mismatched variants are silently ignored.
    fn update_config(&mut self, config: OverlayConfigUpdate);

    /// Render the overlay content
    fn render(&mut self);

    /// Poll for window events (non-blocking)
    ///
    /// Returns `false` if the window should close (e.g., user closed it).
    fn poll_events(&mut self) -> bool;

    /// Get the underlying frame for position/size queries
    fn frame(&self) -> &OverlayFrame;

    /// Get mutable access to the underlying frame
    fn frame_mut(&mut self) -> &mut OverlayFrame;

    /// Check if position/size changed since last check (clears dirty flag)
    fn take_position_dirty(&mut self) -> bool {
        self.frame_mut().take_position_dirty()
    }

    /// Get the current position and size
    fn position(&self) -> OverlayPosition {
        let frame = self.frame();
        OverlayPosition {
            x: frame.x(),
            y: frame.y(),
            width: frame.width(),
            height: frame.height(),
        }
    }

    /// Set click-through mode (true = clicks pass through, false = interactive)
    fn set_click_through(&mut self, enabled: bool) {
        self.frame_mut().set_click_through(enabled);
    }

    /// Set move mode (global overlay repositioning mode)
    /// Default implementation just toggles click-through. Override for custom behavior.
    fn set_move_mode(&mut self, enabled: bool) {
        self.set_click_through(!enabled);
    }

    /// Check if the overlay is in interactive mode (not click-through)
    fn is_interactive(&self) -> bool {
        self.frame().is_interactive()
    }

    /// Check if currently in resize corner
    fn in_resize_corner(&self) -> bool {
        self.frame().in_resize_corner()
    }

    /// Check if currently resizing
    fn is_resizing(&self) -> bool {
        self.frame().is_resizing()
    }

    /// Set rearrange mode (raid overlay only - click-to-swap frames)
    /// Default implementation does nothing. Override in RaidOverlay.
    fn set_rearrange_mode(&mut self, _enabled: bool) {
        // Default: no-op for non-raid overlays
    }

    /// Take any pending registry actions (raid overlay only).
    /// Returns actions that need to be sent to the service for registry updates.
    /// Default implementation returns empty vec.
    fn take_pending_registry_actions(&mut self) -> Vec<RaidRegistryAction> {
        Vec::new()
    }

    /// Check if the overlay has internal state requiring a render.
    /// Returns `true` if the overlay has pending state changes (e.g., click handling)
    /// that require a render pass. The overlay's `render()` method clears this flag.
    /// Default implementation returns `false` (most overlays don't track this internally).
    fn needs_render(&self) -> bool {
        false
    }
}
