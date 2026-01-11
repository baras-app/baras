//! Cooldown Tracker Overlay
//!
//! Displays ability cooldowns as a vertical list of icons with countdown timers.
//! Shows cooldowns sorted by remaining time with optional ability names.

use std::collections::HashMap;
use std::sync::Arc;

use super::{Overlay, OverlayConfigUpdate, OverlayData};
use crate::frame::OverlayFrame;
use crate::platform::{OverlayConfig, PlatformError};
use crate::utils::color_from_rgba;
use crate::widgets::colors;

/// Cache for pre-scaled icons
type ScaledIconCache = HashMap<(u64, u32), Vec<u8>>;

/// A single cooldown entry for display
#[derive(Debug, Clone)]
pub struct CooldownEntry {
    /// Ability ID for identification
    pub ability_id: u64,
    /// Display name of the ability
    pub name: String,
    /// Remaining cooldown in seconds
    pub remaining_secs: f32,
    /// Total cooldown duration in seconds
    pub total_secs: f32,
    /// Ability ID for icon lookup
    pub icon_ability_id: u64,
    /// Charge count (0 = don't show, 1 = single charge, 2+ = show count)
    pub charges: u8,
    /// Max charges (for display purposes)
    pub max_charges: u8,
    /// Color (RGBA) - used as fallback if no icon
    pub color: [u8; 4],
    /// Source entity name (who cast)
    pub source_name: String,
    /// Target entity name (target of ability)
    pub target_name: String,
    /// Pre-loaded icon RGBA data (width, height, rgba_bytes) - Arc for cheap cloning
    pub icon: Option<Arc<(u32, u32, Vec<u8>)>>,
}

impl CooldownEntry {
    /// Progress as 0.0 (just started) to 1.0 (ready)
    pub fn progress(&self) -> f32 {
        if self.total_secs <= 0.0 {
            return 1.0;
        }
        let elapsed = self.total_secs - self.remaining_secs;
        (elapsed / self.total_secs).clamp(0.0, 1.0)
    }

    /// Format remaining time
    pub fn format_time(&self) -> String {
        if self.remaining_secs <= 0.0 {
            return "Ready".to_string();
        }
        let secs = self.remaining_secs;
        if secs >= 60.0 {
            let mins = (secs / 60.0).floor() as u32;
            let remaining_secs = (secs % 60.0).floor() as u32;
            format!("{}:{:02}", mins, remaining_secs)
        } else if secs >= 10.0 {
            format!("{:.0}s", secs)
        } else {
            format!("{:.1}s", secs)
        }
    }

    /// Is the cooldown ready (off cooldown)?
    pub fn is_ready(&self) -> bool {
        self.remaining_secs <= 0.0
    }
}

/// Data sent from service to cooldown overlay
#[derive(Debug, Clone, Default)]
pub struct CooldownData {
    pub entries: Vec<CooldownEntry>,
}

/// Configuration for cooldown overlay
#[derive(Debug, Clone)]
pub struct CooldownConfig {
    pub icon_size: u8,
    pub max_display: u8,
    pub show_ability_names: bool,
    pub sort_by_remaining: bool,
    pub show_source_name: bool,
    pub show_target_name: bool,
}

impl Default for CooldownConfig {
    fn default() -> Self {
        Self {
            icon_size: 32,
            max_display: 10,
            show_ability_names: true,
            sort_by_remaining: true,
            show_source_name: false,
            show_target_name: false,
        }
    }
}

/// Base dimensions
const BASE_WIDTH: f32 = 180.0;
const BASE_HEIGHT: f32 = 300.0;
const BASE_PADDING: f32 = 4.0;
const BASE_ROW_SPACING: f32 = 2.0;
const BASE_FONT_SIZE: f32 = 11.0;

/// Cooldown overlay - vertical list of ability cooldowns
pub struct CooldownOverlay {
    frame: OverlayFrame,
    config: CooldownConfig,
    background_alpha: u8,
    data: CooldownData,
    icon_cache: ScaledIconCache,
    /// Last rendered state for dirty checking: (ability_id, time_string, charges)
    last_rendered: Vec<(u64, String, u8)>,
}

impl CooldownOverlay {
    /// Create a new cooldown overlay
    pub fn new(
        window_config: OverlayConfig,
        config: CooldownConfig,
        background_alpha: u8,
    ) -> Result<Self, PlatformError> {
        let mut frame = OverlayFrame::new(window_config, BASE_WIDTH, BASE_HEIGHT)?;
        frame.set_background_alpha(background_alpha);
        frame.set_label("Cooldowns");

        Ok(Self {
            frame,
            config,
            background_alpha,
            data: CooldownData::default(),
            icon_cache: HashMap::new(),
            last_rendered: Vec::new(),
        })
    }

    /// Update the config
    pub fn set_config(&mut self, config: CooldownConfig) {
        self.config = config;
    }

    /// Update background alpha
    pub fn set_background_alpha(&mut self, alpha: u8) {
        self.background_alpha = alpha;
        self.frame.set_background_alpha(alpha);
    }

    /// Update the data and pre-cache icons
    pub fn set_data(&mut self, mut data: CooldownData) {
        let icon_size = self.frame.scaled(self.config.icon_size as f32) as u32;

        // Pre-cache icons at display size
        for entry in &data.entries {
            if let Some(ref icon_arc) = entry.icon {
                let cache_key = (entry.icon_ability_id, icon_size);
                if !self.icon_cache.contains_key(&cache_key) {
                    let (src_w, src_h, ref src_data) = **icon_arc;
                    let scaled = scale_icon(src_data, src_w, src_h, icon_size);
                    self.icon_cache.insert(cache_key, scaled);
                }
            }
        }

        // Sort by remaining time if configured
        if self.config.sort_by_remaining {
            data.entries.sort_by(|a, b| {
                a.remaining_secs
                    .partial_cmp(&b.remaining_secs)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        self.data = data;
    }

    /// Render the overlay
    pub fn render(&mut self) {
        let max_display = self.config.max_display as usize;

        // Build current visible state for dirty check
        let current_state: Vec<(u64, String, u8)> = self
            .data
            .entries
            .iter()
            .take(max_display)
            .map(|e| (e.ability_id, e.format_time(), e.charges))
            .collect();

        // Skip render if nothing changed (but always render at least once)
        if current_state == self.last_rendered && !self.last_rendered.is_empty() {
            return;
        }
        self.last_rendered = current_state;

        let padding = self.frame.scaled(BASE_PADDING);
        let row_spacing = self.frame.scaled(BASE_ROW_SPACING);
        let font_size = self.frame.scaled(BASE_FONT_SIZE);
        let icon_size = self.frame.scaled(self.config.icon_size as f32);
        let row_height = icon_size + row_spacing;

        self.frame.begin_frame();

        if self.data.entries.is_empty() {
            self.frame.end_frame();
            return;
        }

        let mut y = padding;

        let icon_size_u32 = icon_size as u32;

        for entry in self.data.entries.iter().take(max_display) {
            let x = padding;

            // Draw icon from cache or colored square fallback
            let cache_key = (entry.icon_ability_id, icon_size_u32);
            let has_icon = if let Some(scaled_icon) = self.icon_cache.get(&cache_key) {
                self.frame.draw_image(
                    scaled_icon,
                    icon_size_u32,
                    icon_size_u32,
                    x,
                    y,
                    icon_size,
                    icon_size,
                );
                true
            } else if let Some(ref icon_arc) = entry.icon {
                // Fallback if cache miss
                let (img_w, img_h, ref rgba) = **icon_arc;
                self.frame.draw_image(
                    rgba,
                    img_w,
                    img_h,
                    x,
                    y,
                    icon_size,
                    icon_size,
                );
                true
            } else {
                false
            };

            if !has_icon {
                // Fallback: colored square
                let bg_color = color_from_rgba(entry.color);
                self.frame.fill_rounded_rect(
                    x,
                    y,
                    icon_size,
                    icon_size,
                    3.0,
                    bg_color,
                );
            }

            // Decreasing clock wipe - overlay shrinks from top, revealing icon
            // progress: 0 = just used, 1 = ready
            // Overlay starts full (when progress=0), shrinks as progress→1
            let progress = entry.progress();
            let overlay_height = icon_size * (1.0 - progress);
            if overlay_height > 1.0 {
                self.frame.fill_rect(
                    x,
                    y,
                    icon_size,
                    overlay_height,
                    color_from_rgba([0, 0, 0, 140]),
                );
            }

            // Border
            let border_color = if entry.is_ready() {
                colors::effect_buff() // Green/bright when ready
            } else {
                colors::white()
            };
            self.frame.stroke_rounded_rect(
                x,
                y,
                icon_size,
                icon_size,
                3.0,
                1.0,
                border_color,
            );

            // Charge count in corner (if > 1 or showing max charges)
            if entry.charges > 1 || (entry.max_charges > 1 && entry.charges > 0) {
                let charge_text = format!("{}", entry.charges);
                let charge_font_size = font_size * 0.9;
                let charge_x = x + icon_size - self.frame.measure_text(&charge_text, charge_font_size).0 - 2.0;
                let charge_y = y + charge_font_size + 2.0;

                self.frame.draw_text(
                    &charge_text,
                    charge_x + 1.0,
                    charge_y + 1.0,
                    charge_font_size,
                    colors::text_shadow(),
                );
                self.frame.draw_text(
                    &charge_text,
                    charge_x,
                    charge_y,
                    charge_font_size,
                    colors::effect_buff(),
                );
            }

            // Ability name and countdown text
            let text_x = x + icon_size + padding;
            let text_y = y + icon_size / 2.0;

            if self.config.show_ability_names {
                // Ability name on top
                let name_y = text_y - font_size * 0.3;
                self.frame.draw_text(
                    &entry.name,
                    text_x,
                    name_y,
                    font_size,
                    colors::white(),
                );

                // Countdown below
                let time_text = entry.format_time();
                let time_color = if entry.is_ready() {
                    colors::effect_buff()
                } else {
                    colors::label_dim()
                };
                self.frame.draw_text(
                    &time_text,
                    text_x,
                    name_y + font_size + 2.0,
                    font_size * 0.9,
                    time_color,
                );
            } else {
                // Just countdown centered
                let time_text = entry.format_time();
                let time_color = if entry.is_ready() {
                    colors::effect_buff()
                } else {
                    colors::white()
                };
                self.frame.draw_text(
                    &time_text,
                    text_x,
                    text_y + font_size / 3.0,
                    font_size,
                    time_color,
                );
            }

            y += row_height;
        }

        self.frame.end_frame();
    }
}

/// Scale icon to target size (nearest neighbor for speed)
fn scale_icon(src: &[u8], src_w: u32, src_h: u32, target_size: u32) -> Vec<u8> {
    let mut dest = vec![0u8; (target_size * target_size * 4) as usize];
    let scale_x = src_w as f32 / target_size as f32;
    let scale_y = src_h as f32 / target_size as f32;

    for dy in 0..target_size {
        for dx in 0..target_size {
            let sx = ((dx as f32 * scale_x) as u32).min(src_w - 1);
            let sy = ((dy as f32 * scale_y) as u32).min(src_h - 1);
            let src_idx = ((sy * src_w + sx) * 4) as usize;
            let dest_idx = ((dy * target_size + dx) * 4) as usize;

            dest[dest_idx] = src[src_idx];
            dest[dest_idx + 1] = src[src_idx + 1];
            dest[dest_idx + 2] = src[src_idx + 2];
            dest[dest_idx + 3] = src[src_idx + 3];
        }
    }
    dest
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Trait Implementation
// ─────────────────────────────────────────────────────────────────────────────

impl Overlay for CooldownOverlay {
    fn update_data(&mut self, data: OverlayData) -> bool {
        if let OverlayData::Cooldowns(cooldown_data) = data {
            let was_empty = self.data.entries.is_empty();
            let is_empty = cooldown_data.entries.is_empty();
            self.set_data(cooldown_data);
            !(was_empty && is_empty)
        } else {
            false
        }
    }

    fn update_config(&mut self, config: OverlayConfigUpdate) {
        if let OverlayConfigUpdate::Cooldowns(cfg, alpha) = config {
            self.set_config(cfg);
            self.set_background_alpha(alpha);
        }
    }

    fn render(&mut self) {
        CooldownOverlay::render(self);
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
