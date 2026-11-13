//! Combat Time Overlay
//!
//! A standalone overlay displaying the current encounter's combat time.
//! Supports an optional title with separator, glow-rendered text for
//! transparent background readability, and clears immediately on combat end.

use baras_types::formatting;

use super::{Overlay, OverlayConfigUpdate, OverlayData};
use crate::frame::OverlayFrame;
use crate::platform::{OverlayConfig, PlatformError};
use crate::utils::color_from_rgba;
use crate::widgets::Header;

// ─────────────────────────────────────────────────────────────────────────────
// Configuration & Data
// ─────────────────────────────────────────────────────────────────────────────

/// Runtime configuration for the combat time overlay
#[derive(Debug, Clone)]
pub struct CombatTimeConfig {
    /// Whether to show the "Combat Time" title and separator
    pub show_title: bool,
    /// Font scale multiplier (0.3 - 3.0) — applies to the time digits only
    pub font_scale: f32,
    /// Font color (RGBA)
    pub font_color: [u8; 4],
    /// When true, background shrinks to fit content
    pub dynamic_background: bool,
    /// When true, overlay clears when combat ends; otherwise keeps last time
    pub clear_after_combat: bool,
}

impl Default for CombatTimeConfig {
    fn default() -> Self {
        Self {
            show_title: true,
            font_scale: 1.0,
            font_color: [255, 255, 255, 255],
            dynamic_background: false,
            clear_after_combat: true,
        }
    }
}

/// Data sent from the service layer to the combat time overlay
#[derive(Debug, Clone, Default)]
pub struct CombatTimeData {
    /// Duration of the current encounter in seconds (0 = cleared / no combat)
    pub encounter_time_secs: u64,
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
const BASE_HEADER_SPACING: f32 = 3.0;

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Implementation
// ─────────────────────────────────────────────────────────────────────────────

/// Standalone combat time overlay
pub struct CombatTimeOverlay {
    frame: OverlayFrame,
    config: CombatTimeConfig,
    data: CombatTimeData,
    european_number_format: bool,
}

impl CombatTimeOverlay {
    /// Create a new combat time overlay
    pub fn new(
        window_config: OverlayConfig,
        config: CombatTimeConfig,
        background_alpha: u8,
    ) -> Result<Self, PlatformError> {
        let mut frame = OverlayFrame::new(window_config, BASE_WIDTH, BASE_HEIGHT)?;
        frame.set_background_alpha(background_alpha);
        frame.set_label("Combat Time");

        Ok(Self {
            frame,
            config,
            data: CombatTimeData::default(),
            european_number_format: false,
        })
    }

    /// Update the overlay configuration
    pub fn set_config(&mut self, config: CombatTimeConfig) {
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
        let header_font_size = base_font_size * 0.85;
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

        // Nothing to render when cleared (combat ended)
        if self.data.encounter_time_secs == 0 {
            self.frame.end_frame();
            return;
        }

        let mut y = padding;

        // Optional title header with separator
        if self.config.show_title {
            let content_width = self.content_width(padding);
            y = Header::new("Combat Time")
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
        let time_str = formatting::format_duration_u64(self.data.encounter_time_secs);
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
        if self.data.encounter_time_secs == 0 {
            return 0.0;
        }

        let mut h = padding;

        if self.config.show_title {
            let scale = self.frame.scale_factor();
            h += Header::height(header_font_size, header_spacing, scale);
        }

        // Time value row
        h += time_font_size;
        h += padding;
        h
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Trait Implementation
// ─────────────────────────────────────────────────────────────────────────────

impl Overlay for CombatTimeOverlay {
    fn update_data(&mut self, data: OverlayData) -> bool {
        if let OverlayData::CombatTime(combat_time_data) = data {
            // When clear_after_combat is disabled, ignore zero-time clears
            // so the last combat time remains visible
            if combat_time_data.encounter_time_secs == 0 && !self.config.clear_after_combat {
                return false;
            }
            let changed = self.data.encounter_time_secs != combat_time_data.encounter_time_secs;
            self.data = combat_time_data;
            changed
        } else {
            false
        }
    }

    fn update_config(&mut self, config: OverlayConfigUpdate) {
        if let OverlayConfigUpdate::CombatTime(ct_config, alpha, european) = config {
            self.set_config(ct_config);
            self.set_background_alpha(alpha);
            self.european_number_format = european;
        }
    }

    fn render(&mut self) {
        CombatTimeOverlay::render(self);
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
