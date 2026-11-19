//! Effects Overlay
//!
//! Displays countdown bars for tracked effects (buffs, debuffs, HoTs, etc.)
//! Similar to the timer overlay but sourced from effect tracking.

use baras_core::context::TimerOverlayConfig;

use super::{Overlay, OverlayConfigUpdate, OverlayData};
use crate::frame::OverlayFrame;
use crate::platform::{OverlayConfig, PlatformError};
use crate::utils::color_from_rgba;
use crate::widgets::{colors, ProgressBar};

/// A single effect entry for display
#[derive(Debug, Clone)]
pub struct EffectEntry {
    /// Effect display name
    pub name: String,
    /// Remaining time in seconds
    pub remaining_secs: f32,
    /// Total duration in seconds (for progress calculation)
    pub total_secs: f32,
    /// Bar color (RGBA)
    pub color: [u8; 4],
    /// Stack count (0 = don't show stacks)
    pub stacks: u8,
}

impl EffectEntry {
    /// Progress as 0.0 (expired) to 1.0 (full)
    pub fn progress(&self) -> f32 {
        if self.total_secs <= 0.0 {
            return 0.0;
        }
        (self.remaining_secs / self.total_secs).clamp(0.0, 1.0)
    }

    /// Format remaining time as MM:SS or S.s
    pub fn format_time(&self, european: bool) -> String {
        baras_types::formatting::format_countdown(self.remaining_secs, "", "0:00", european)
    }

    /// Format display text (name + optional stacks)
    pub fn display_name(&self) -> String {
        if self.stacks > 1 {
            format!("{} ({})", self.name, self.stacks)
        } else {
            self.name.clone()
        }
    }
}

/// Data sent from service to effects overlay
#[derive(Debug, Clone, Default)]
pub struct EffectsData {
    /// Current active effects to display
    pub entries: Vec<EffectEntry>,
}

/// Base dimensions for scaling calculations
const BASE_WIDTH: f32 = 220.0;
const BASE_HEIGHT: f32 = 150.0;

/// Base layout values (at BASE_WIDTH x BASE_HEIGHT)
const BASE_BAR_HEIGHT: f32 = 18.0;
const BASE_ENTRY_SPACING: f32 = 4.0;
const BASE_PADDING: f32 = 6.0;
const BASE_FONT_SIZE: f32 = 11.0;

/// Effects countdown overlay
pub struct EffectsOverlay {
    frame: OverlayFrame,
    config: TimerOverlayConfig, // Reuse timer config for now
    data: EffectsData,
    european_number_format: bool,
}

impl EffectsOverlay {
    /// Create a new effects overlay
    pub fn new(
        window_config: OverlayConfig,
        config: TimerOverlayConfig,
    ) -> Result<Self, PlatformError> {
        let mut frame = OverlayFrame::new(window_config, BASE_WIDTH, BASE_HEIGHT)?;
        frame.set_label("Effects Countdown");

        Ok(Self {
            frame,
            config,
            data: EffectsData::default(),
            european_number_format: false,
        })
    }

    /// Update the config
    pub fn set_config(&mut self, config: TimerOverlayConfig) {
        self.config = config;
    }


    /// Update the data
    pub fn set_data(&mut self, data: EffectsData) {
        self.data = data;
    }

    /// Render the overlay
    pub fn render(&mut self) {
        let width = self.frame.width() as f32;

        let padding = self.frame.scaled(BASE_PADDING);
        let bar_height = self.frame.scaled(BASE_BAR_HEIGHT);
        let entry_spacing = self.frame.scaled(BASE_ENTRY_SPACING);
        let font_scale = self.config.font_scale.clamp(0.3, 3.0);
        let font_size = self.frame.scaled(BASE_FONT_SIZE * font_scale);

        let font_color = color_from_rgba(self.config.font_color);

        // Sort entries in place if needed
        if self.config.sort_by_remaining {
            self.data
                .entries
                .sort_by(|a, b| a.remaining_secs.partial_cmp(&b.remaining_secs).unwrap());
        }

        // Compute content height for dynamic background
        let max_display = self.config.max_display as usize;
        let num_entries = self.data.entries.iter().take(max_display).count();
        let content_height = if num_entries > 0 {
            padding * 2.0
                + num_entries as f32 * bar_height
                + (num_entries - 1) as f32 * entry_spacing
        } else {
            0.0
        };

        // Begin frame (clear, background, border)
        if self.config.dynamic_background {
            self.frame.begin_frame_with_content_height(content_height);
        } else {
            self.frame.begin_frame();
        }

        // Nothing to render if no effects
        if self.data.entries.is_empty() {
            self.frame.end_frame();
            return;
        }

        let content_width = width - padding * 2.0;
        let bar_radius = 3.0 * self.frame.scale_factor();

        let mut y = padding;

        for entry in self.data.entries.iter().take(max_display) {
            let bar_color = color_from_rgba(entry.color);
            let time_text = entry.format_time(self.european_number_format);

            // Draw effect bar with name on left, time on right
            ProgressBar::new(entry.display_name(), entry.progress())
                .with_fill_color(bar_color)
                .with_bg_color(colors::dps_bar_bg())
                .with_text_color(font_color)
                .with_right_text(time_text)
                .render(
                    &mut self.frame,
                    padding,
                    y,
                    content_width,
                    bar_height,
                    font_size,
                    bar_radius,
                );

            y += bar_height + entry_spacing;
        }

        // End frame (resize indicator, commit)
        self.frame.end_frame();
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Trait Implementation
// ─────────────────────────────────────────────────────────────────────────────

impl Overlay for EffectsOverlay {
    fn update_data(&mut self, data: OverlayData) -> bool {
        if let OverlayData::Effects(effects_data) = data {
            // Skip render only when transitioning empty → empty
            // Active effects need every frame for smooth bar animation
            let was_empty = self.data.entries.is_empty();
            let is_empty = effects_data.entries.is_empty();
            self.set_data(effects_data);
            !(was_empty && is_empty)
        } else {
            false
        }
    }

    fn update_config(&mut self, config: OverlayConfigUpdate) {
        if let OverlayConfigUpdate::Effects(effects_config, european) = config {
            self.set_config(effects_config);
            self.european_number_format = european;
        }
    }

    fn render(&mut self) {
        EffectsOverlay::render(self);
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
