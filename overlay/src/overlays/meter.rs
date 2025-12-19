//! DPS/HPS Meter Overlay
//!
//! Displays a ranked list of players with their damage/healing output.

use tiny_skia::Color;

use crate::manager::OverlayWindow;
use crate::platform::{OverlayConfig, PlatformError};
use crate::renderer::colors;

/// Entry in a DPS/HPS meter
#[derive(Debug, Clone)]
pub struct MeterEntry {
    pub name: String,
    pub value: i64,
    pub max_value: i64,
    pub color: Color,
}

impl MeterEntry {
    pub fn new(name: impl Into<String>, value: i64, max_value: i64) -> Self {
        Self {
            name: name.into(),
            value,
            max_value,
            color: colors::dps_bar_fill(),
        }
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

/// A specialized DPS/HPS meter overlay
pub struct MeterOverlay {
    window: OverlayWindow,
    entries: Vec<MeterEntry>,
    title: String,
}

impl MeterOverlay {
    /// Create a new meter overlay
    pub fn new(config: OverlayConfig, title: &str) -> Result<Self, PlatformError> {
        let window = OverlayWindow::new(config)?;

        Ok(Self {
            window,
            entries: Vec::new(),
            title: title.to_string(),
        })
    }

    /// Calculate scale factor based on current window size
    fn scale_factor(&self) -> f32 {
        let width = self.window.width() as f32;
        let height = self.window.height() as f32;

        // Use geometric mean of width and height ratios for balanced scaling
        let width_ratio = width / BASE_WIDTH;
        let height_ratio = height / BASE_HEIGHT;
        (width_ratio * height_ratio).sqrt()
    }

    /// Get scaled bar height
    fn bar_height(&self) -> f32 {
        BASE_BAR_HEIGHT * self.scale_factor()
    }

    /// Get scaled bar spacing
    fn bar_spacing(&self) -> f32 {
        BASE_BAR_SPACING * self.scale_factor()
    }

    /// Get scaled padding
    fn padding(&self) -> f32 {
        BASE_PADDING * self.scale_factor()
    }

    /// Get scaled font size
    fn font_size(&self) -> f32 {
        BASE_FONT_SIZE * self.scale_factor()
    }

    /// Update the meter entries
    pub fn set_entries(&mut self, entries: Vec<MeterEntry>) {
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

    /// Render the meter
    pub fn render(&mut self) {
        let width = self.window.width() as f32;
        let height = self.window.height() as f32;

        // Get scaled layout values
        let padding = self.padding();
        let font_size = self.font_size();
        let bar_height = self.bar_height();
        let bar_spacing = self.bar_spacing();
        let corner_radius = 8.0 * self.scale_factor();

        // Clear with transparent background
        self.window.clear(colors::transparent());

        // Draw background
        self.window
            .fill_rounded_rect(0.0, 0.0, width, height, corner_radius, colors::overlay_bg());

        // Draw border when unlocked (interactive mode)
        if self.window.is_interactive() {
            let border_color = Color::from_rgba8(128, 128, 128, 200);
            self.window.stroke_rounded_rect(
                1.0, 1.0,
                width - 2.0, height - 2.0,
                corner_radius - 1.0,
                2.0,
                border_color,
            );
        }

        // Draw title
        let title_y = padding + font_size;
        self.window.draw_text(
            &self.title,
            padding,
            title_y,
            font_size,
            colors::white(),
        );

        // Draw separator line
        let sep_y = title_y + bar_spacing + 2.0;
        self.window.fill_rect(
            padding,
            sep_y,
            width - padding * 2.0,
            1.0 * self.scale_factor(),
            colors::white(),
        );

        // Find max value for scaling
        let max_val = self.entries.iter().map(|e| e.max_value as f64).fold(1.0, f64::max);

        // Draw entries
        let bar_width = width - padding * 2.0;
        let mut y = sep_y + bar_spacing + 4.0 * self.scale_factor();
        let bar_radius = 4.0 * self.scale_factor();
        let text_font_size = font_size - 2.0 * self.scale_factor();

        for entry in &self.entries {
            let progress = (entry.value as f64 / max_val).clamp(0.0, 1.0) as f32;

            // Draw bar background
            self.window.fill_rounded_rect(
                padding, y, bar_width, bar_height, bar_radius, colors::dps_bar_bg()
            );

            // Draw bar fill
            let fill_width = bar_width * progress;
            if fill_width > 0.0 {
                self.window.fill_rounded_rect(
                    padding, y, fill_width, bar_height, bar_radius, entry.color
                );
            }

            // Draw name on the left
            let text_y = y + bar_height / 2.0 + text_font_size / 3.0;
            self.window.draw_text(
                &entry.name,
                padding + 4.0 * self.scale_factor(),
                text_y,
                text_font_size,
                colors::white(),
            );

            // Draw value on the right
            let value_text = format!("{:.1}", entry.value);
            let (text_width, _) = self.window.measure_text(&value_text, text_font_size);
            self.window.draw_text(
                &value_text,
                width - padding - text_width - 4.0 * self.scale_factor(),
                text_y,
                text_font_size,
                colors::white(),
            );

            y += bar_height + bar_spacing;
        }

        // Draw resize indicator in bottom-right corner when pointer is there
        if self.window.in_resize_corner() || self.window.is_resizing() {
            let indicator_size = 16.0;
            let corner_x = width - indicator_size - 4.0;
            let corner_y = height - indicator_size - 4.0;

            // Draw a small triangle/grip indicator
            let highlight = if self.window.is_resizing() {
                colors::white()
            } else {
                Color::from_rgba8(255, 255, 255, 180)
            };

            // Draw diagonal lines as resize grip
            for i in 0..3 {
                let offset = i as f32 * 5.0;
                self.window.fill_rect(
                    corner_x + offset,
                    corner_y + indicator_size - 2.0,
                    indicator_size - offset,
                    2.0,
                    highlight,
                );
                self.window.fill_rect(
                    corner_x + indicator_size - 2.0,
                    corner_y + offset,
                    2.0,
                    indicator_size - offset,
                    highlight,
                );
            }
        }

        self.window.commit();
    }

    /// Poll for events
    pub fn poll_events(&mut self) -> bool {
        self.window.poll_events()
    }

    /// Get mutable access to the underlying window
    pub fn window_mut(&mut self) -> &mut OverlayWindow {
        &mut self.window
    }
}
