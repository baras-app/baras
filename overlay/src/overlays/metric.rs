//! DPS/HPS Meter Overlay
//!
//! Displays a ranked list of players with their damage/healing output.

use baras_core::context::OverlayAppearanceConfig;
use tiny_skia::Color;

use super::{Overlay, OverlayConfigUpdate, OverlayData};
use crate::frame::OverlayFrame;
use crate::platform::{OverlayConfig, PlatformError};
use crate::utils::{color_from_rgba, format_number, truncate_name};
use crate::widgets::colors;
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
const MIN_BAR_HEIGHT_ABSOLUTE: f32 = 16.0; // Absolute minimum bar height (not scaled)
const MIN_BAR_SPACING_ABSOLUTE: f32 = 2.0; // Absolute minimum spacing (not scaled)
const BASE_FONT_SIZE: f32 = 14.0;

/// Maximum characters for player names before truncation
const MAX_NAME_CHARS: usize = 16;

/// A specialized DPS/HPS metric overlay
pub struct MetricOverlay {
    frame: OverlayFrame,
    entries: Vec<MetricEntry>,
    title: String,
    appearance: OverlayAppearanceConfig,
    show_empty_bars: bool,
    stack_from_bottom: bool,
    scaling_factor: f32,
}

impl MetricOverlay {
    /// Create a new metric overlay
    pub fn new(
        config: OverlayConfig,
        title: &str,
        appearance: OverlayAppearanceConfig,
        background_alpha: u8,
        show_empty_bars: bool,
        stack_from_bottom: bool,
        scaling_factor: f32,
    ) -> Result<Self, PlatformError> {
        let mut frame = OverlayFrame::new(config, BASE_WIDTH, BASE_HEIGHT)?;
        frame.set_background_alpha(background_alpha);
        frame.set_label(title);

        Ok(Self {
            frame,
            entries: Vec::new(),
            title: title.to_string(),
            appearance,
            show_empty_bars,
            stack_from_bottom,
            scaling_factor: scaling_factor.clamp(1.0, 2.0),
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

    /// Update show empty bars setting
    pub fn set_show_empty_bars(&mut self, show: bool) {
        self.show_empty_bars = show;
    }

    /// Update stack from bottom setting
    pub fn set_stack_from_bottom(&mut self, stack: bool) {
        self.stack_from_bottom = stack;
    }

    /// Update scaling factor (clamped to 1.0-2.0)
    pub fn set_scaling_factor(&mut self, factor: f32) {
        self.scaling_factor = factor.clamp(1.0, 2.0);
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
        let height = self.frame.height() as f32;

        // Get scaled layout values
        let padding = self.frame.scaled(BASE_PADDING);
        // Scale font size partially with bar scaling (40% of the bar scale increase)
        let font_scale = 1.0 + (self.scaling_factor - 1.0) * 0.4;
        let font_size = self.frame.scaled(BASE_FONT_SIZE * font_scale);
        let scaled_bar_height = BASE_BAR_HEIGHT * self.scaling_factor;
        let ideal_bar_height = self.frame.scaled(scaled_bar_height);
        let bar_spacing = self.frame.scaled(BASE_BAR_SPACING);
        // Use absolute minimum bar height (not scaled) to handle extreme aspect ratios
        let min_bar_height = MIN_BAR_HEIGHT_ABSOLUTE;

        // Get colors from config
        let font_color = color_from_rgba(self.appearance.font_color);
        let bar_color = color_from_rgba(self.appearance.bar_color);

        // Get display options
        let show_total = self.appearance.show_total;
        let show_per_second = self.appearance.show_per_second;

        // Filter and limit entries to max_entries
        let max_entries = self.appearance.max_entries as usize;
        let visible_entries: Vec<_> = self
            .entries
            .iter()
            .filter(|e| self.show_empty_bars || e.value != 0)
            .take(max_entries)
            .collect();
        let num_entries = visible_entries.len();

        // Calculate space reserved for header and footer (must match actual widget heights)
        // Header with separator: font_size + spacing + 2.0 + spacing + 4.0 * scale
        // Footer: 2.0 (separator offset) + spacing + font_size + buffer
        let scale = self.frame.scale_factor();
        let header_space = if self.appearance.show_header {
            font_size + bar_spacing + 2.0 + bar_spacing + 4.0 * scale
        } else {
            0.0
        };
        let footer_space = if self.appearance.show_footer {
            2.0 + bar_spacing + font_size + 6.0 * scale // separator + spacing + text + buffer
        } else {
            0.0
        };

        // Calculate available space for bars (reserve footer space first)
        let available_for_bars = height - padding * 2.0 - header_space - footer_space;

        // Calculate effective bar height and spacing - compress proportionally if needed
        let (bar_height, effective_spacing) = if num_entries > 0 {
            let n = num_entries as f32;
            let ideal_total = n * ideal_bar_height + (n - 1.0) * bar_spacing;

            if ideal_total > available_for_bars && ideal_total > 0.0 {
                // Compress both bars and spacing proportionally
                let compression_ratio = available_for_bars / ideal_total;
                let compressed_bar = (ideal_bar_height * compression_ratio).max(min_bar_height);
                let compressed_spacing =
                    (bar_spacing * compression_ratio).max(MIN_BAR_SPACING_ABSOLUTE);
                (compressed_bar, compressed_spacing)
            } else {
                (ideal_bar_height, bar_spacing)
            }
        } else {
            (ideal_bar_height, bar_spacing)
        };

        // Begin frame (clear, background, border)
        self.frame.begin_frame();

        let content_width = width - padding * 2.0;
        let bar_radius = 4.0 * self.frame.scale_factor();

        // Calculate total height of all bars
        let total_bars_height = if num_entries > 0 {
            num_entries as f32 * bar_height + (num_entries - 1).max(0) as f32 * effective_spacing
        } else {
            0.0
        };

        // Calculate bar start position based on stack direction
        let bars_start_y = if self.stack_from_bottom {
            // Stack from bottom: position bars at bottom of available space
            padding + header_space + available_for_bars - total_bars_height
        } else {
            // Stack from top: bars start after header
            padding + header_space
        };

        // Draw header just above the first bar
        let mut y = if self.appearance.show_header {
            let header_y = bars_start_y - header_space;
            Header::new(&self.title).with_color(font_color).render(
                &mut self.frame,
                padding,
                header_y,
                content_width,
                font_size,
                bar_spacing,
            );
            bars_start_y
        } else {
            bars_start_y
        };

        // Find max value for scaling (use actual rate values, not max_value field)
        let max_val = visible_entries
            .iter()
            .map(|e| e.value as f64)
            .fold(1.0, f64::max);

        // Draw entries using ProgressBar widget
        // Scale text font size proportionally if bars are compressed
        let base_text_size = font_size - 2.0 * self.frame.scale_factor();
        let compression_ratio = bar_height / ideal_bar_height;
        let text_font_size = if compression_ratio < 1.0 {
            // When bars are compressed, scale text proportionally (keep minimum readable)
            let compressed = base_text_size * compression_ratio;
            compressed.max(10.0) // Absolute minimum 10px
        } else {
            base_text_size
        };

        // Calculate footer sums
        let rate_sum: i64 = visible_entries.iter().map(|e| e.value).sum();
        let total_sum: i64 = visible_entries.iter().map(|e| e.total_value).sum();

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

            bar.render(
                &mut self.frame,
                padding,
                y,
                content_width,
                bar_height,
                text_font_size,
                bar_radius,
            );

            y += bar_height + effective_spacing;
        }

        // Draw footer using Footer widget
        if self.appearance.show_footer {
            let footer = if show_per_second && show_total {
                // Both enabled: show total sum in center, rate sum on right
                Footer::new(format_number(rate_sum))
                    .with_secondary(format_number(total_sum))
                    .with_color(font_color)
            } else if show_per_second {
                // Rate only: show rate sum on right
                Footer::new(format_number(rate_sum)).with_color(font_color)
            } else if show_total {
                // Total only: show total sum on right
                Footer::new(format_number(total_sum)).with_color(font_color)
            } else {
                // Neither: empty footer (just separator)
                Footer::new("").with_color(font_color)
            };

            footer.render(&mut self.frame, padding, y, content_width, font_size - 2.0);
        }

        // End frame (resize indicator, commit)
        self.frame.end_frame();
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Trait Implementation
// ─────────────────────────────────────────────────────────────────────────────

impl Overlay for MetricOverlay {
    fn update_data(&mut self, data: OverlayData) -> bool {
        if let OverlayData::Metrics(entries) = data {
            self.set_entries(entries);
            true // Metric overlays always render when updated
        } else {
            false
        }
    }

    fn update_config(&mut self, config: OverlayConfigUpdate) {
        if let OverlayConfigUpdate::Metric(appearance, alpha, show_empty, stack_bottom, scale) =
            config
        {
            self.set_appearance(appearance);
            self.set_background_alpha(alpha);
            self.set_show_empty_bars(show_empty);
            self.set_stack_from_bottom(stack_bottom);
            self.set_scaling_factor(scale);
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
