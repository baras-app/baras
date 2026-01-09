//! Boss Health Bar Overlay
//!
//! Displays real-time health bars for boss NPCs in the current encounter.

use baras_core::OverlayHealthEntry;
use baras_core::context::BossHealthConfig;

use super::{Overlay, OverlayConfigUpdate, OverlayData};
use crate::frame::OverlayFrame;
use crate::platform::{OverlayConfig, PlatformError};
use crate::utils::{color_from_rgba, format_number};
use crate::widgets::ProgressBar;
use crate::widgets::colors;

/// Data sent from service to boss health overlay
#[derive(Debug, Clone, Default)]
pub struct BossHealthData {
    /// Current boss health entries (sorted by encounter order)
    pub entries: Vec<OverlayHealthEntry>,
}

/// Base dimensions for scaling calculations
const BASE_WIDTH: f32 = 250.0;
const BASE_HEIGHT: f32 = 100.0;

/// Base layout values (at BASE_WIDTH x BASE_HEIGHT)
const BASE_BAR_HEIGHT: f32 = 20.0;
const BASE_LABEL_HEIGHT: f32 = 16.0;
const BASE_ENTRY_SPACING: f32 = 8.0;
const BASE_LABEL_BAR_GAP: f32 = 2.0;
const BASE_PADDING: f32 = 8.0;
const BASE_FONT_SIZE: f32 = 13.0;
const BASE_LABEL_FONT_SIZE: f32 = 8.5;

/// Maximum number of bosses we optimize scaling for
const MAX_SUPPORTED_BOSSES: usize = 7;
/// Minimum compression factor to keep entries readable
const MIN_COMPRESSION: f32 = 0.4;

/// Boss health bar overlay
pub struct BossHealthOverlay {
    frame: OverlayFrame,
    config: BossHealthConfig,
    data: BossHealthData,
}

impl BossHealthOverlay {
    /// Create a new boss health overlay
    pub fn new(
        window_config: OverlayConfig,
        config: BossHealthConfig,
        background_alpha: u8,
    ) -> Result<Self, PlatformError> {
        let mut frame = OverlayFrame::new(window_config, BASE_WIDTH, BASE_HEIGHT)?;
        frame.set_background_alpha(background_alpha);
        frame.set_label("Boss Health");

        Ok(Self {
            frame,
            config,
            data: BossHealthData::default(),
        })
    }

    /// Update the config
    pub fn set_config(&mut self, config: BossHealthConfig) {
        self.config = config;
    }

    /// Update background alpha
    pub fn set_background_alpha(&mut self, alpha: u8) {
        self.frame.set_background_alpha(alpha);
    }

    /// Update the data
    pub fn set_data(&mut self, data: BossHealthData) {
        self.data = data;
    }

    /// Calculate scaled font size so text fits within max_width
    fn scaled_font_for_text(&mut self, text: &str, max_width: f32, base_font_size: f32) -> f32 {
        let (text_width, _) = self.frame.measure_text(text, base_font_size);
        if text_width <= max_width {
            return base_font_size;
        }

        // Scale font proportionally to fit
        let scale = max_width / text_width;
        let min_font = base_font_size * 0.6; // Don't go below 60% of base size
        (base_font_size * scale).max(min_font)
    }

    /// Calculate compression factor to fit entries in available height
    fn compression_factor(&self, entry_count: usize, has_targets: bool) -> f32 {
        let height = self.frame.height() as f32;
        let padding = self.frame.scaled(BASE_PADDING);

        // Calculate height of one entry at base scale
        let bar_height = self.frame.scaled(BASE_BAR_HEIGHT);
        let label_height = self.frame.scaled(BASE_LABEL_HEIGHT);
        let entry_spacing = self.frame.scaled(BASE_ENTRY_SPACING);
        let label_bar_gap = self.frame.scaled(BASE_LABEL_BAR_GAP);
        let label_font_size = self.frame.scaled(BASE_LABEL_FONT_SIZE);

        // Per-entry height: label + gap + bar + spacing
        let mut entry_height = label_height + label_bar_gap + bar_height + entry_spacing;

        // Add target line height if targets are shown
        if has_targets {
            let target_font_size = label_font_size * 0.85;
            entry_height += target_font_size + 2.0;
        }

        // Total height needed for all entries
        let total_needed = padding * 2.0 + entry_height * entry_count as f32 - entry_spacing;
        let available = height;

        if total_needed <= available {
            1.0
        } else {
            (available / total_needed).max(MIN_COMPRESSION)
        }
    }

    /// Render the overlay
    pub fn render(&mut self) {
        let width = self.frame.width() as f32;

        // Begin frame (clear, background, border)
        self.frame.begin_frame();

        // Filter out dead bosses (0% health) and collect living ones
        let entries: Vec<_> = self
            .data
            .entries
            .iter()
            .filter(|e| e.percent() > 0.0)
            .take(MAX_SUPPORTED_BOSSES)
            .cloned()
            .collect();

        // Nothing to render if no living bosses
        if entries.is_empty() {
            self.frame.end_frame();
            return;
        }

        // Check if any entry has a target (for compression calculation)
        let has_targets =
            self.config.show_target && entries.iter().any(|e| e.target_name.is_some());

        // Calculate compression factor based on entry count
        let compression = self.compression_factor(entries.len(), has_targets);

        // Apply compression to entry-specific dimensions
        let padding = self.frame.scaled(BASE_PADDING);
        let bar_height = self.frame.scaled(BASE_BAR_HEIGHT) * compression;
        let label_height = self.frame.scaled(BASE_LABEL_HEIGHT) * compression;
        let entry_spacing = self.frame.scaled(BASE_ENTRY_SPACING) * compression;
        let label_bar_gap = self.frame.scaled(BASE_LABEL_BAR_GAP) * compression;
        let font_size = self.frame.scaled(BASE_FONT_SIZE) * compression;
        let label_font_size = self.frame.scaled(BASE_LABEL_FONT_SIZE) * compression;

        let bar_color = color_from_rgba(self.config.bar_color);
        let font_color = color_from_rgba(self.config.font_color);

        let content_width = width - padding * 2.0;
        let bar_radius = 4.0 * self.frame.scale_factor() * compression;

        let mut y = padding;

        for entry in &entries {
            let progress = entry.percent() / 100.0;

            // Scale font to fit boss name if too wide
            let actual_font_size =
                self.scaled_font_for_text(&entry.name, content_width, label_font_size);

            // Draw boss name above bar (y is baseline, so offset by font size)
            self.frame.draw_text(
                &entry.name,
                padding,
                y + actual_font_size,
                actual_font_size,
                font_color,
            );

            y += label_height + label_bar_gap;

            // Format health text for inside bar: "(1.5M/2.0M)"
            let health_text = format_number(entry.current as i64);

            // Format percentage for right side
            let percent_text = if self.config.show_percent {
                format!("{:.1}%", entry.percent())
            } else {
                String::new()
            };

            // Draw bar with health on left, percentage on right (smaller font)
            let bar_font_size = font_size * 0.70;
            ProgressBar::new(&health_text, progress)
                .with_fill_color(bar_color)
                .with_bg_color(colors::dps_bar_bg())
                .with_text_color(font_color)
                .with_right_text(percent_text)
                .render(
                    &mut self.frame,
                    padding,
                    y,
                    content_width,
                    bar_height,
                    bar_font_size,
                    bar_radius,
                );

            y += bar_height;

            // Draw target name below bar, right-aligned
            if self.config.show_target
                && let Some(ref target) = entry.target_name
            {
                let target_font_size = label_font_size * 0.85;
                let target_text = format!("⌖ {}", target);
                let (text_width, _) = self.frame.measure_text(&target_text, target_font_size);
                self.frame.draw_text(
                    &target_text,
                    padding + content_width - text_width,
                    y + target_font_size + 1.0,
                    target_font_size,
                    font_color,
                );
                y += target_font_size + 2.0;
            }

            y += entry_spacing;
        }

        // End frame (resize indicator, commit)
        self.frame.end_frame();
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Trait Implementation
// ─────────────────────────────────────────────────────────────────────────────

impl Overlay for BossHealthOverlay {
    fn update_data(&mut self, data: OverlayData) -> bool {
        if let OverlayData::BossHealth(boss_data) = data {
            self.set_data(boss_data);
            true // Boss health always renders when updated (only sent during combat)
        } else {
            false
        }
    }

    fn update_config(&mut self, config: OverlayConfigUpdate) {
        if let OverlayConfigUpdate::BossHealth(boss_config, alpha) = config {
            self.set_config(boss_config);
            self.set_background_alpha(alpha);
        }
    }

    fn render(&mut self) {
        BossHealthOverlay::render(self);
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
