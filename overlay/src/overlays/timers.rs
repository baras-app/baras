//! Timer Bar Overlay
//!
//! Displays countdown timers for boss mechanics, ability cooldowns, etc.

use baras_core::context::TimerOverlayConfig;

use super::{Overlay, OverlayConfigUpdate, OverlayData};
use crate::frame::OverlayFrame;
use crate::platform::{OverlayConfig, PlatformError};
use crate::utils::color_from_rgba;
use crate::widgets::{ProgressBar, colors};

/// A single timer entry for display
#[derive(Debug, Clone)]
pub struct TimerEntry {
    /// Timer display name
    pub name: String,
    /// Remaining time in seconds
    pub remaining_secs: f32,
    /// Total duration in seconds (for progress calculation)
    pub total_secs: f32,
    /// Bar color (RGBA)
    pub color: [u8; 4],
}

impl TimerEntry {
    /// Progress as 0.0 (expired) to 1.0 (full)
    pub fn progress(&self) -> f32 {
        if self.total_secs <= 0.0 {
            return 0.0;
        }
        (self.remaining_secs / self.total_secs).clamp(0.0, 1.0)
    }

    /// Format remaining time as MM:SS or S.s
    pub fn format_time(&self) -> String {
        if self.remaining_secs <= 0.0 {
            return "0:00".to_string();
        }

        let secs = self.remaining_secs;
        if secs >= 60.0 {
            let mins = (secs / 60.0).floor() as u32;
            let remaining_secs = (secs % 60.0).floor() as u32;
            format!("{}:{:02}", mins, remaining_secs)
        } else if secs >= 10.0 {
            format!("{:.0}", secs)
        } else {
            format!("{:.1}", secs)
        }
    }
}

/// Data sent from service to timer overlay
#[derive(Debug, Clone, Default)]
pub struct TimerData {
    /// Current active timers
    pub entries: Vec<TimerEntry>,
}

/// Base dimensions for scaling calculations
const BASE_WIDTH: f32 = 220.0;
const BASE_HEIGHT: f32 = 150.0;

/// Base layout values (at BASE_WIDTH x BASE_HEIGHT)
const BASE_BAR_HEIGHT: f32 = 18.0;
const BASE_ENTRY_SPACING: f32 = 4.0;
const BASE_PADDING: f32 = 6.0;
const BASE_FONT_SIZE: f32 = 11.0;

/// Timer bar overlay
pub struct TimerOverlay {
    frame: OverlayFrame,
    config: TimerOverlayConfig,
    data: TimerData,
}

impl TimerOverlay {
    /// Create a new timer overlay
    pub fn new(
        window_config: OverlayConfig,
        config: TimerOverlayConfig,
        background_alpha: u8,
    ) -> Result<Self, PlatformError> {
        let mut frame = OverlayFrame::new(window_config, BASE_WIDTH, BASE_HEIGHT)?;
        frame.set_background_alpha(background_alpha);
        frame.set_label("Timers");

        Ok(Self {
            frame,
            config,
            data: TimerData::default(),
        })
    }

    /// Update the config
    pub fn set_config(&mut self, config: TimerOverlayConfig) {
        self.config = config;
    }

    /// Update background alpha
    pub fn set_background_alpha(&mut self, alpha: u8) {
        self.frame.set_background_alpha(alpha);
    }

    /// Update the data
    pub fn set_data(&mut self, data: TimerData) {
        self.data = data;
    }

    /// Render the overlay
    pub fn render(&mut self) {
        let width = self.frame.width() as f32;

        let padding = self.frame.scaled(BASE_PADDING);
        let bar_height = self.frame.scaled(BASE_BAR_HEIGHT);
        let entry_spacing = self.frame.scaled(BASE_ENTRY_SPACING);
        let font_size = self.frame.scaled(BASE_FONT_SIZE);

        let font_color = color_from_rgba(self.config.font_color);

        // Begin frame (clear, background, border)
        self.frame.begin_frame();

        // Sort entries in place if needed
        if self.config.sort_by_remaining {
            self.data
                .entries
                .sort_by(|a, b| a.remaining_secs.partial_cmp(&b.remaining_secs).unwrap());
        }

        // Nothing to render if no timers
        let max_display = self.config.max_display as usize;
        if self.data.entries.is_empty() {
            self.frame.end_frame();
            return;
        }

        let content_width = width - padding * 2.0;
        let bar_radius = 3.0 * self.frame.scale_factor();

        let mut y = padding;

        for entry in self.data.entries.iter().take(max_display) {
            let bar_color = color_from_rgba(entry.color);
            let time_text = entry.format_time();

            // Draw timer bar with name on left, time on right
            ProgressBar::new(&entry.name, entry.progress())
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

impl Overlay for TimerOverlay {
    fn update_data(&mut self, data: OverlayData) -> bool {
        if let OverlayData::Timers(timer_data) = data {
            // Skip render only when transitioning empty → empty
            // Active timers need every frame for smooth bar animation
            let was_empty = self.data.entries.is_empty();
            let is_empty = timer_data.entries.is_empty();
            self.set_data(timer_data);
            !(was_empty && is_empty)
        } else {
            false
        }
    }

    fn update_config(&mut self, config: OverlayConfigUpdate) {
        if let OverlayConfigUpdate::Timers(timer_config, alpha) = config {
            self.set_config(timer_config);
            self.set_background_alpha(alpha);
        }
    }

    fn render(&mut self) {
        TimerOverlay::render(self);
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
