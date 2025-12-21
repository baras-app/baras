//! DPS/HPS Meter Overlay
//!
//! Displays a ranked list of players with their damage/healing output.

use baras_core::context::OverlayAppearanceConfig;
use tiny_skia::Color;

use super::{Overlay, OverlayConfigUpdate, OverlayData};
use crate::frame::OverlayFrame;
use crate::platform::{OverlayConfig, PlatformError};
use crate::renderer::colors;
use crate::utils::{color_from_rgba, format_number, truncate_name};
use crate::widgets::{Footer, Header, ProgressBar};

/// Entry in a DPS/HPS metric
#[derive(Debug, Clone)]
pub struct MetricEntry {
    pub name: String,
    /// Per-second rate (e.g., DPS, HPS)
    pub value: i64,
    /// Maximum value for progress bar scaling
    pub max_value: i64,
    /// Cumulative total (e.g., total damage dealt)
    pub total_value: i64,
    pub color: Color,
}

impl MetricEntry {
    pub fn new(name: impl Into<String>, value: i64, max_value: i64) -> Self {
        Self {
            name: name.into(),
            value,
            max_value,
            total_value: 0,
            color: colors::dps_bar_fill(),
        }
    }

    /// Set the cumulative total value
    pub fn with_total(mut self, total: i64) -> Self {
        self.total_value = total;
        self
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
}

/// Base dimensions for scaling calculations
const BASE_WIDTH: f32 = 280.0;
const BASE_HEIGHT: f32 = 200.0;

/// Base layout values (at BASE_WIDTH x BASE_HEIGHT)
const BASE_BAR_HEIGHT: f32 = 20.0;
const BASE_BAR_SPACING: f32 = 4.0;
const BASE_PADDING: f32 = 8.0;
const BASE_FONT_SIZE: f32 = 14.0;

/// Maximum characters for player names before truncation
const MAX_NAME_CHARS: usize = 16;

/// A specialized DPS/HPS metric overlay
pub struct MetricOverlay {
    frame: OverlayFrame,
    entries: Vec<MetricEntry>,
    title: String,
    appearance: OverlayAppearanceConfig,
}

impl MetricOverlay {
    /// Create a new metric overlay
    pub fn new(
        config: OverlayConfig,
        title: &str,
        appearance: OverlayAppearanceConfig,
        background_alpha: u8,
    ) -> Result<Self, PlatformError> {
        let mut frame = OverlayFrame::new(config, BASE_WIDTH, BASE_HEIGHT)?;
        frame.set_background_alpha(background_alpha);

        Ok(Self {
            frame,
            entries: Vec::new(),
            title: title.to_string(),
            appearance,
        })
    }

    /// Update appearance config
    pub fn set_appearance(&mut self, appearance: OverlayAppearanceConfig) {
        self.appearance = appearance;
    }

    /// Update background alpha
    pub fn set_background_alpha(&mut self, alpha: u8) {
        self.frame.set_background_alpha(alpha);
    }

    /// Get scaled bar height
    fn bar_height(&self) -> f32 {
        self.frame.scaled(BASE_BAR_HEIGHT)
    }

    /// Get scaled bar spacing
    fn bar_spacing(&self) -> f32 {
        self.frame.scaled(BASE_BAR_SPACING)
    }

    /// Get scaled padding
    fn padding(&self) -> f32 {
        self.frame.scaled(BASE_PADDING)
    }

    /// Get scaled font size
    fn font_size(&self) -> f32 {
        self.frame.scaled(BASE_FONT_SIZE)
    }

    /// Update the metric entries
    pub fn set_entries(&mut self, entries: Vec<MetricEntry>) {
        self.entries = entries;
    }

    /// Set the title
    pub fn set_title(&mut self, title: &str) {
        self.title = title.to_string();
    }

    /// Get current entry count
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Render the metric
    pub fn render(&mut self) {
        let width = self.frame.width() as f32;

        // Get scaled layout values
        let padding = self.padding();
        let font_size = self.font_size();
        let bar_height = self.bar_height();
        let bar_spacing = self.bar_spacing();

        // Get colors from config
        let font_color = color_from_rgba(self.appearance.font_color);
        let bar_color = color_from_rgba(self.appearance.bar_color);

        // Get display options
        let show_total = self.appearance.show_total;
        let show_per_second = self.appearance.show_per_second;

        // Begin frame (clear, background, border)
        self.frame.begin_frame();

        let content_width = width - padding * 2.0;
        let bar_radius = 4.0 * self.frame.scale_factor();
        let mut y = padding;

        // Draw header using Header widget
        if self.appearance.show_header {
            y = Header::new(&self.title)
                .with_color(font_color)
                .render(&mut self.frame, padding, y, content_width, font_size, bar_spacing);
        }

        // Limit entries to max_entries
        let max_entries = self.appearance.max_entries as usize;
        let visible_entries: Vec<_> = self.entries.iter().take(max_entries).collect();

        // Find max value for scaling (use actual rate values, not max_value field)
        let max_val = visible_entries
            .iter()
            .map(|e| e.value as f64)
            .fold(1.0, f64::max);

        // Draw entries using ProgressBar widget
        let text_font_size = font_size - 2.0 * self.frame.scale_factor();

        for entry in &visible_entries {
            // Determine fill color (use entry color if custom, otherwise config bar_color)
            let fill_color = if entry.color != colors::dps_bar_fill() {
                entry.color
            } else {
                bar_color
            };

            let display_name = truncate_name(&entry.name, MAX_NAME_CHARS);
            let progress = if max_val > 0.0 {
                (entry.value as f64 / max_val) as f32
            } else {
                0.0
            };

            let mut bar = ProgressBar::new(display_name, progress)
                .with_fill_color(fill_color)
                .with_bg_color(colors::dps_bar_bg())
                .with_text_color(font_color);

            // Add text based on show_total and show_per_second settings
            // Per-second is always rightmost when enabled, total goes center or right
            if show_per_second && show_total {
                // Both: total in center, rate on right
                bar = bar
                    .with_center_text(format_number(entry.total_value))
                    .with_right_text(format_number(entry.value));
            } else if show_per_second {
                // Rate only (default): rate on right
                bar = bar.with_right_text(format_number(entry.value));
            } else if show_total {
                // Total only: total on right
                bar = bar.with_right_text(format_number(entry.total_value));
            }
            // If neither, just show name (no values)

            bar.render(&mut self.frame, padding, y, content_width, bar_height, text_font_size, bar_radius);

            y += bar_height + bar_spacing;
        }

        // Draw footer using Footer widget
        if self.appearance.show_footer {
            // Calculate sums based on display mode
            let rate_sum: i64 = visible_entries.iter().map(|e| e.value).sum();
            let total_sum: i64 = visible_entries.iter().map(|e| e.total_value).sum();

            let footer = if show_per_second && show_total {
                // Both enabled: show total sum in center, rate sum on right
                Footer::new(format_number(rate_sum))
                    .with_secondary(format_number(total_sum))
                    .with_color(font_color)
            } else if show_per_second {
                // Rate only: show rate sum on right
                Footer::new(format_number(rate_sum))
                    .with_color(font_color)
            } else if show_total {
                // Total only: show total sum on right
                Footer::new(format_number(total_sum))
                    .with_color(font_color)
            } else {
                // Neither: empty footer (just separator)
                Footer::new("")
                    .with_color(font_color)
            };

            footer.render(&mut self.frame, padding, y, content_width, font_size - 2.0, bar_spacing);
        }

        // End frame (resize indicator, commit)
        self.frame.end_frame();
    }

    /// Poll for events
    pub fn poll_events(&mut self) -> bool {
        self.frame.poll_events()
    }

    /// Get mutable access to the underlying frame
    pub fn frame_mut(&mut self) -> &mut OverlayFrame {
        &mut self.frame
    }

    /// Get immutable access to the underlying frame
    pub fn frame(&self) -> &OverlayFrame {
        &self.frame
    }

    /// Get mutable access to the underlying window (convenience method)
    pub fn window_mut(&mut self) -> &mut crate::manager::OverlayWindow {
        self.frame.window_mut()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Trait Implementation
// ─────────────────────────────────────────────────────────────────────────────

impl Overlay for MetricOverlay {
    fn update_data(&mut self, data: OverlayData) {
        if let OverlayData::Metrics(entries) = data {
            self.set_entries(entries);
        }
    }

    fn update_config(&mut self, config: OverlayConfigUpdate) {
        if let OverlayConfigUpdate::Metric(appearance, alpha) = config {
            eprintln!("[METRIC-OVERLAY] update_config: bar_color={:?}, font_color={:?}, alpha={}",
                appearance.bar_color, appearance.font_color, alpha);
            self.set_appearance(appearance);
            self.set_background_alpha(alpha);
        } else {
            eprintln!("[METRIC-OVERLAY] update_config: received non-Metric config variant, ignoring");
        }
    }

    fn render(&mut self) {
        MetricOverlay::render(self);
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
