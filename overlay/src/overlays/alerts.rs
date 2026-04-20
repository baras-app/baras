//! Alerts Text Overlay
//!
//! Displays triggered alert text in a chat-like window.
//! Alerts stack from top (newest first) and fade out after their duration.

use std::sync::Arc;
use std::time::Instant;

use baras_core::context::AlertsOverlayConfig;

use super::{Overlay, OverlayConfigUpdate, OverlayData};
use crate::frame::OverlayFrame;
use crate::platform::{OverlayConfig, PlatformError};
use crate::utils::{color_from_rgba, shared_scaled_icons};
use crate::widgets::colors;

/// A single alert entry for display
#[derive(Debug, Clone)]
pub struct AlertEntry {
    /// Alert display text
    pub text: String,
    /// Text color (RGBA)
    pub color: [u8; 4],
    /// When this alert was created
    pub created_at: Instant,
    /// Duration to show at full opacity (seconds)
    pub duration_secs: f32,
    /// Optional ability ID for icon lookup
    pub icon_ability_id: Option<u64>,
    /// Pre-loaded icon RGBA data (width, height, rgba_bytes)
    pub icon: Option<Arc<(u32, u32, Vec<u8>)>>,
}

impl AlertEntry {
    /// Create a new alert entry with current timestamp
    pub fn new(text: String, color: [u8; 4], duration_secs: f32) -> Self {
        Self {
            text,
            color,
            created_at: Instant::now(),
            duration_secs,
            icon_ability_id: None,
            icon: None,
        }
    }

    /// Calculate opacity based on elapsed time and fade duration
    /// Returns 1.0 during duration, then fades to 0.0 over fade_duration
    pub fn opacity(&self, fade_duration: f32) -> f32 {
        let elapsed = self.created_at.elapsed().as_secs_f32();
        if elapsed < self.duration_secs {
            1.0 // Full opacity during main duration
        } else {
            let fade_elapsed = elapsed - self.duration_secs;
            (1.0 - fade_elapsed / fade_duration).max(0.0)
        }
    }

    /// Check if this alert has fully expired (past duration + fade)
    pub fn is_expired(&self, fade_duration: f32) -> bool {
        self.created_at.elapsed().as_secs_f32() > self.duration_secs + fade_duration
    }
}

/// Data sent from service to alerts overlay
/// Contains new alerts to append (not replace)
#[derive(Debug, Clone, Default)]
pub struct AlertsData {
    /// New alerts to display
    pub entries: Vec<AlertEntry>,
}

/// Base dimensions for scaling calculations
const BASE_WIDTH: f32 = 220.0;
const BASE_HEIGHT: f32 = 120.0;

/// Base layout values (at BASE_WIDTH x BASE_HEIGHT)
const BASE_LINE_HEIGHT: f32 = 16.0;
const BASE_ENTRY_SPACING: f32 = 2.0;
const BASE_PADDING: f32 = 6.0;

/// Alerts text overlay
pub struct AlertsOverlay {
    frame: OverlayFrame,
    config: AlertsOverlayConfig,
    /// Active alerts (managed internally, newest first)
    entries: Vec<AlertEntry>,
    european_number_format: bool,
}

impl AlertsOverlay {
    /// Create a new alerts overlay
    pub fn new(
        window_config: OverlayConfig,
        config: AlertsOverlayConfig,
        background_alpha: u8,
    ) -> Result<Self, PlatformError> {
        let mut frame = OverlayFrame::new(window_config, BASE_WIDTH, BASE_HEIGHT)?;
        frame.set_background_alpha(background_alpha);
        frame.set_label("Alerts");

        Ok(Self {
            frame,
            config,
            entries: Vec::new(),
            european_number_format: false,
        })
    }

    /// Update the config
    pub fn set_config(&mut self, config: AlertsOverlayConfig) {
        self.config = config;
    }

    /// Update background alpha
    pub fn set_background_alpha(&mut self, alpha: u8) {
        self.frame.set_background_alpha(alpha);
    }

    /// Add new alerts (prepends to show newest first) and pre-cache icons
    pub fn add_alerts(&mut self, new_alerts: Vec<AlertEntry>) {
        // Pre-cache icons at current display size before inserting
        let icon_size = self.frame.scaled(self.config.font_size as f32) as u32;
        for alert in &new_alerts {
            if let (Some(ability_id), Some(icon_arc)) = (alert.icon_ability_id, &alert.icon) {
                let (src_w, src_h, ref src_data) = **icon_arc;
                let _ = shared_scaled_icons()
                    .get_or_scale(ability_id, icon_size, src_data, src_w, src_h);
            }
        }

        // Prepend new alerts (newest at top)
        for alert in new_alerts.into_iter().rev() {
            self.entries.insert(0, alert);
        }
        // Trim to max display count
        let max = self.config.max_display as usize;
        if self.entries.len() > max {
            self.entries.truncate(max);
        }
    }

    /// Remove expired alerts
    fn prune_expired(&mut self) {
        let fade_duration = self.config.fade_duration;
        self.entries.retain(|e| !e.is_expired(fade_duration));
    }

    /// Render a skeleton preview when in move mode
    fn render_preview(&mut self) {
        let padding = self.frame.scaled(BASE_PADDING);
        let line_height = self.frame.scaled(BASE_LINE_HEIGHT);
        let entry_spacing = self.frame.scaled(BASE_ENTRY_SPACING);
        let font_size = self.frame.scaled(self.config.font_size as f32);
        let icon_size = font_size;
        let icon_gap = self.frame.scaled(4.0);

        self.frame.begin_frame();

        let previews: [(&str, [u8; 4]); 3] = [
            ("Stack!", [255, 80, 80, 255]),
            ("Move Away!", [255, 210, 50, 255]),
            ("Spread Out", [255, 255, 255, 255]),
        ];

        let show_icons = self.config.show_icons;
        let frame_width = self.frame.width() as f32;

        let mut y = padding + font_size;

        for (text, color) in &previews {
            let shadow = colors::text_shadow();

            // Center the icon+text group to match live render path.
            let (text_width, _) = self.frame.measure_text_styled(text, font_size, true, false);
            let group_width = if show_icons {
                icon_size + icon_gap + text_width
            } else {
                text_width
            };
            let group_x = ((frame_width - group_width) / 2.0).max(padding);

            let text_x = if show_icons {
                let icon_y = y - icon_size + (icon_size - font_size) / 2.0;
                self.frame.fill_rounded_rect(
                    group_x,
                    icon_y,
                    icon_size,
                    icon_size,
                    2.0,
                    colors::effect_icon_bg(),
                );
                self.frame.stroke_rounded_rect_dashed(
                    group_x,
                    icon_y,
                    icon_size,
                    icon_size,
                    2.0,
                    1.0,
                    colors::preview_border(),
                    3.0,
                    2.0,
                );
                group_x + icon_size + icon_gap
            } else {
                group_x
            };

            self.frame.draw_text_styled(
                text,
                text_x + 1.0,
                y + 1.0,
                font_size,
                shadow,
                true,
                false,
            );
            self.frame.draw_text_styled(
                text,
                text_x,
                y,
                font_size,
                color_from_rgba(*color),
                true,
                false,
            );

            y += line_height + entry_spacing;
        }

        self.frame.end_frame();
    }

    /// Render the overlay
    pub fn render(&mut self) {
        if self.frame.is_in_move_mode() {
            self.render_preview();
            return;
        }

        // Remove expired alerts first
        self.prune_expired();

        let padding = self.frame.scaled(BASE_PADDING);
        let line_height = self.frame.scaled(BASE_LINE_HEIGHT);
        let entry_spacing = self.frame.scaled(BASE_ENTRY_SPACING);
        let font_size = self.frame.scaled(self.config.font_size as f32);

        // Begin frame (clear, background, border)
        self.frame.begin_frame();

        // Nothing to render if no alerts
        if self.entries.is_empty() {
            self.frame.end_frame();
            return;
        }

        let max_display = self.config.max_display as usize;
        let fade_duration = self.config.fade_duration;
        let show_icons = self.config.show_icons;
        let icon_size = font_size;
        let icon_gap = self.frame.scaled(4.0);
        let icon_size_u32 = icon_size.round() as u32;
        let frame_width = self.frame.width() as f32;

        // Start below top padding + font height (text draws from baseline)
        let mut y = padding + font_size;

        for entry in self.entries.iter().take(max_display) {
            let opacity = entry.opacity(fade_duration);

            // Apply opacity to the alert's color
            let mut color = entry.color;
            color[3] = (color[3] as f32 * opacity) as u8;

            // Apply opacity to shadow color
            let mut shadow = colors::text_shadow();
            shadow.set_alpha(shadow.alpha() * opacity);

            let has_icon = show_icons && entry.icon_ability_id.is_some() && entry.icon.is_some();

            // Center the icon+text group horizontally within the overlay.
            let (text_width, _) =
                self.frame
                    .measure_text_styled(&entry.text, font_size, true, false);
            let group_width = if has_icon {
                icon_size + icon_gap + text_width
            } else {
                text_width
            };
            let group_x = ((frame_width - group_width) / 2.0).max(padding);

            let text_x = if has_icon {
                let icon_y = y - icon_size;
                if let Some(ability_id) = entry.icon_ability_id {
                    if let Some(scaled_icon) =
                        shared_scaled_icons().get(ability_id, icon_size_u32)
                    {
                        self.frame.draw_image(
                            &scaled_icon,
                            icon_size_u32,
                            icon_size_u32,
                            group_x,
                            icon_y,
                            icon_size,
                            icon_size,
                        );
                    } else if let Some(ref icon_arc) = entry.icon {
                        let (img_w, img_h, ref rgba) = **icon_arc;
                        self.frame
                            .draw_image(rgba, img_w, img_h, group_x, icon_y, icon_size, icon_size);
                    }
                }
                group_x + icon_size + icon_gap
            } else {
                group_x
            };

            self.frame.draw_text_styled(
                &entry.text,
                text_x + 1.0,
                y + 1.0,
                font_size,
                shadow,
                true,
                false,
            );
            self.frame.draw_text_styled(
                &entry.text,
                text_x,
                y,
                font_size,
                color_from_rgba(color),
                true,
                false,
            );

            y += line_height + entry_spacing;
        }

        // End frame (resize indicator, commit)
        self.frame.end_frame();
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Trait Implementation
// ─────────────────────────────────────────────────────────────────────────────

impl Overlay for AlertsOverlay {
    fn update_data(&mut self, data: OverlayData) -> bool {
        if let OverlayData::Alerts(alerts_data) = data {
            if alerts_data.entries.is_empty() {
                // No new alerts, but may need to render for fade updates
                !self.entries.is_empty()
            } else {
                self.add_alerts(alerts_data.entries);
                true
            }
        } else {
            false
        }
    }

    fn update_config(&mut self, config: OverlayConfigUpdate) {
        if let OverlayConfigUpdate::Alerts(alerts_config, alpha, european) = config {
            self.set_config(alerts_config);
            self.set_background_alpha(alpha);
            self.european_number_format = european;
        }
    }

    fn render(&mut self) {
        AlertsOverlay::render(self);
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

    /// Alerts need continuous render while fading
    fn needs_render(&self) -> bool {
        !self.entries.is_empty()
    }
}
