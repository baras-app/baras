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

mod metric;
mod personal;

pub use metric::{MetricEntry, MetricOverlay};
pub use personal::{PersonalOverlay, PersonalStats};

use crate::frame::OverlayFrame;
use baras_core::context::{OverlayAppearanceConfig, PersonalOverlayConfig};

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
    // Future variants:
    // Timer(TimerData),
    // BossHealth(BossHealthData),
    // RaidFrame(RaidFrameData),
}

/// Configuration updates that can be sent to overlays
#[derive(Debug, Clone)]
pub enum OverlayConfigUpdate {
    /// Appearance config for metric overlays (+ background alpha)
    Metric(OverlayAppearanceConfig, u8),
    /// Config for personal overlay (+ background alpha)
    Personal(PersonalOverlayConfig, u8),
    // Future variants as needed
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
    /// Implementations should check if the data variant matches their type
    /// and update accordingly. Mismatched variants are silently ignored.
    fn update_data(&mut self, data: OverlayData);

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
}
