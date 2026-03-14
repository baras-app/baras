//! Operation Timer Overlay
//!
//! A persistent timer overlay that tracks elapsed time across an entire
//! operation run (raid instance). Unlike CombatTime which resets each
//! encounter, this timer persists across encounters, trash, and wipes.
//! Supports an optional title showing the operation name.

use super::{Overlay, OverlayConfigUpdate, OverlayData};
use crate::frame::OverlayFrame;
use crate::platform::{OverlayConfig, PlatformError};
use crate::utils::color_from_rgba;
use crate::widgets::Header;

// ─────────────────────────────────────────────────────────────────────────────
// Configuration & Data
// ─────────────────────────────────────────────────────────────────────────────

/// Runtime configuration for the operation timer overlay
#[derive(Debug, Clone)]
pub struct OperationTimerConfig {
    /// Whether to show the title (operation name or "Op Timer") and separator
    pub show_title: bool,
    /// Font scale multiplier (0.5 - 3.0) — applies to the time digits only
    pub font_scale: f32,
    /// Font color (RGBA)
    pub font_color: [u8; 4],
    /// When true, background shrinks to fit content
    pub dynamic_background: bool,
}

impl Default for OperationTimerConfig {
    fn default() -> Self {
        Self {
            show_title: true,
            font_scale: 1.0,
            font_color: [255, 255, 255, 255],
            dynamic_background: false,
        }
    }
}

/// Data sent from the service layer to the operation timer overlay
#[derive(Debug, Clone, Default)]
pub struct OperationTimerData {
    /// Total elapsed seconds for the operation run
    pub elapsed_secs: u64,
    /// Whether the timer is currently running
    pub is_running: bool,
    /// Name of the current operation (e.g., "Dxun"), if known
    pub operation_name: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Time Formatting
// ─────────────────────────────────────────────────────────────────────────────

/// Format elapsed seconds as adaptive duration string.
/// Under 1 hour: `M:SS` (e.g., "23:45")
/// 1 hour or more: `H:MM:SS` (e.g., "1:23:45")
fn format_duration_hms(total_secs: u64) -> String {
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;
    if hours > 0 {
        format!("{}:{:02}:{:02}", hours, minutes, seconds)
    } else {
        format!("{}:{:02}", minutes, seconds)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Layout Constants
// ─────────────────────────────────────────────────────────────────────────────

/// Base dimensions for scaling calculations
const BASE_WIDTH: f32 = 160.0;
const BASE_HEIGHT: f32 = 60.0;

/// Base layout values (at BASE_WIDTH x BASE_HEIGHT)
const BASE_FONT_SIZE: f32 = 16.0;
const BASE_PADDING: f32 = 6.0;
/// Header uses a smaller fraction of the base font so the instance name
/// reads as a compact label above the large timer digits, not a title.
const HEADER_FONT_RATIO: f32 = 0.60;
const BASE_HEADER_SPACING: f32 = 2.0;

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Return `text` truncated with "…" if it exceeds `max_width` at `font_size`.
/// Otherwise returns the original string unchanged.
fn truncate_to_fit(frame: &mut OverlayFrame, text: &str, font_size: f32, max_width: f32) -> String {
    let (w, _) = frame.measure_text(text, font_size);
    if w <= max_width {
        return text.to_string();
    }
    // Binary-search the char boundary where truncation keeps us within max_width
    let ellipsis = "…";
    let (ew, _) = frame.measure_text(ellipsis, font_size);
    let budget = max_width - ew;
    // Walk chars from the end, trimming one at a time
    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() {
        let candidate: String = chars.iter().collect();
        let (cw, _) = frame.measure_text(&candidate, font_size);
        if cw <= budget {
            return format!("{}{}", candidate, ellipsis);
        }
        chars.pop();
    }
    ellipsis.to_string()
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Implementation
// ─────────────────────────────────────────────────────────────────────────────

/// Persistent operation timer overlay
pub struct OperationTimerOverlay {
    frame: OverlayFrame,
    config: OperationTimerConfig,
    data: OperationTimerData,
}

impl OperationTimerOverlay {
    /// Create a new operation timer overlay
    pub fn new(
        window_config: OverlayConfig,
        config: OperationTimerConfig,
        background_alpha: u8,
    ) -> Result<Self, PlatformError> {
        let mut frame = OverlayFrame::new(window_config, BASE_WIDTH, BASE_HEIGHT)?;
        frame.set_background_alpha(background_alpha);
        frame.set_label("Op Timer");

        Ok(Self {
            frame,
            config,
            data: OperationTimerData::default(),
        })
    }

    /// Update the overlay configuration
    pub fn set_config(&mut self, config: OperationTimerConfig) {
        self.config = config;
    }

    /// Update the background opacity
    pub fn set_background_alpha(&mut self, alpha: u8) {
        self.frame.set_background_alpha(alpha);
    }

    /// Render the overlay
    pub fn render(&mut self) {
        let padding = self.frame.scaled(BASE_PADDING);
        let base_font_size = self.frame.scaled(BASE_FONT_SIZE);
        let time_font_size = base_font_size * self.config.font_scale;
        // Title is intentionally small — it's a compact label, not a headline.
        let header_font_size = base_font_size * HEADER_FONT_RATIO;
        let header_spacing = self.frame.scaled(BASE_HEADER_SPACING);
        let color = color_from_rgba(self.config.font_color);

        // Calculate content height for dynamic background
        let content_height =
            self.content_height(padding, time_font_size, header_font_size, header_spacing);
        if self.config.dynamic_background {
            self.frame.begin_frame_with_content_height(content_height);
        } else {
            self.frame.begin_frame();
        }

        let mut y = padding;

        // Title: operation/flashpoint name if available, otherwise "Op Timer".
        // Truncated with "…" if too wide to fit in the overlay.
        if self.config.show_title {
            let raw_title = self.data.operation_name.as_deref().unwrap_or("Op Timer");
            let content_width = self.content_width(padding);
            let title =
                truncate_to_fit(&mut self.frame, raw_title, header_font_size, content_width);
            y = Header::new(title)
                .with_color(color)
                .with_centered(true)
                .render(
                    &mut self.frame,
                    padding,
                    y,
                    content_width,
                    header_font_size,
                    header_spacing,
                );
        }

        // Formatted time value (bold, glowed for transparent bg readability)
        // Always show the timer value, even at 0:00 (indicates overlay is ready)
        let time_str = format_duration_hms(self.data.elapsed_secs);
        let (text_width, _) =
            self.frame
                .measure_text_styled(&time_str, time_font_size, true, false);
        let time_x = (self.frame.width() as f32 - text_width) / 2.0;
        let time_y = y + time_font_size;
        self.frame.draw_text_with_glow(
            &time_str,
            time_x,
            time_y,
            time_font_size,
            color,
            true,
            false,
        );

        self.frame.end_frame();
    }

    /// Calculate the available content width
    fn content_width(&self, padding: f32) -> f32 {
        self.frame.width() as f32 - padding * 2.0
    }

    /// Calculate the total content height for dynamic background sizing
    fn content_height(
        &self,
        padding: f32,
        time_font_size: f32,
        header_font_size: f32,
        header_spacing: f32,
    ) -> f32 {
        let mut h = padding;

        if self.config.show_title {
            let scale = self.frame.scale_factor();
            h += Header::new("").height(header_font_size, header_spacing, scale);
        }

        // Time value row (always shown)
        h += time_font_size;
        h += padding;
        h
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Trait Implementation
// ─────────────────────────────────────────────────────────────────────────────

impl Overlay for OperationTimerOverlay {
    fn update_data(&mut self, data: OverlayData) -> bool {
        if let OverlayData::OperationTimer(timer_data) = data {
            let changed = self.data.elapsed_secs != timer_data.elapsed_secs
                || self.data.is_running != timer_data.is_running
                || self.data.operation_name != timer_data.operation_name;
            self.data = timer_data;
            changed
        } else {
            false
        }
    }

    fn update_config(&mut self, config: OverlayConfigUpdate) {
        if let OverlayConfigUpdate::OperationTimer(ot_config, alpha) = config {
            self.set_config(ot_config);
            self.set_background_alpha(alpha);
        }
    }

    fn render(&mut self) {
        OperationTimerOverlay::render(self);
    }

    fn poll_events(&mut self) -> bool {
        self.frame.poll_events()
    }

    fn frame(&self) -> &OverlayFrame {
        &self.frame
    }

    fn frame_mut(&mut self) -> &mut OverlayFrame {
        &mut self.frame
    }
}
