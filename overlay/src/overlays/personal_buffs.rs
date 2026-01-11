//! Personal Buffs Overlay
//!
//! Displays active buffs/procs on the local player as a horizontal row of icons.
//! Each buff shows a countdown timer with optional stack count.

use std::collections::HashMap;
use std::sync::Arc;

use super::{Overlay, OverlayConfigUpdate, OverlayData};
use crate::frame::OverlayFrame;
use crate::platform::{OverlayConfig, PlatformError};
use crate::utils::color_from_rgba;
use crate::widgets::colors;

/// Cache for pre-scaled icons to avoid re-scaling every frame
type ScaledIconCache = HashMap<(u64, u32), Vec<u8>>; // (ability_id, size) -> scaled RGBA

/// A single buff entry for display
#[derive(Debug, Clone)]
pub struct PersonalBuff {
    /// Effect ID for identification
    pub effect_id: u64,
    /// Ability ID for icon lookup
    pub icon_ability_id: u64,
    /// Display name of the buff
    pub name: String,
    /// Remaining time in seconds
    pub remaining_secs: f32,
    /// Total duration in seconds (for progress calculation)
    pub total_secs: f32,
    /// Color (RGBA) - used as fallback if no icon
    pub color: [u8; 4],
    /// Stack count (0 = don't show)
    pub stacks: u8,
    /// Source entity name
    pub source_name: String,
    /// Target entity name (self)
    pub target_name: String,
    /// Pre-loaded icon RGBA data (width, height, rgba_bytes) - Arc for cheap cloning
    pub icon: Option<Arc<(u32, u32, Vec<u8>)>>,
}

impl PersonalBuff {
    /// Progress as 0.0 (expired) to 1.0 (full)
    pub fn progress(&self) -> f32 {
        if self.total_secs <= 0.0 {
            return 1.0;
        }
        (self.remaining_secs / self.total_secs).clamp(0.0, 1.0)
    }

    /// Format remaining time
    pub fn format_time(&self) -> String {
        if self.remaining_secs <= 0.0 {
            return "0".to_string();
        }
        let secs = self.remaining_secs;
        if secs >= 60.0 {
            let mins = (secs / 60.0).floor() as u32;
            format!("{}m", mins)
        } else if secs >= 10.0 {
            format!("{:.0}", secs)
        } else {
            format!("{:.1}", secs)
        }
    }
}

/// Data sent from service to personal buffs overlay
#[derive(Debug, Clone, Default)]
pub struct PersonalBuffsData {
    pub buffs: Vec<PersonalBuff>,
}

/// Configuration for personal buffs overlay
#[derive(Debug, Clone)]
pub struct PersonalBuffsConfig {
    pub icon_size: u8,
    pub max_display: u8,
    pub show_effect_names: bool,
    pub show_countdown: bool,
    pub show_source_name: bool,
    pub show_target_name: bool,
    /// When true, stacks are shown large and centered; timer is secondary
    pub stack_priority: bool,
}

impl Default for PersonalBuffsConfig {
    fn default() -> Self {
        Self {
            icon_size: 32,
            max_display: 8,
            show_effect_names: false,
            show_countdown: true,
            show_source_name: false,
            show_target_name: false,
            stack_priority: false,
        }
    }
}

/// Base dimensions
const BASE_WIDTH: f32 = 300.0;
const BASE_HEIGHT: f32 = 60.0;
const BASE_PADDING: f32 = 4.0;
const BASE_ICON_SPACING: f32 = 4.0;
const BASE_FONT_SIZE: f32 = 10.0;

/// Personal buffs overlay - horizontal row of buff icons
pub struct PersonalBuffsOverlay {
    frame: OverlayFrame,
    config: PersonalBuffsConfig,
    background_alpha: u8,
    data: PersonalBuffsData,
    /// Cache of pre-scaled icons (ability_id, size) -> scaled RGBA
    icon_cache: ScaledIconCache,
    /// Last rendered state for dirty checking: (effect_id, time_string, stacks)
    last_rendered: Vec<(u64, String, u8)>,
}

impl PersonalBuffsOverlay {
    /// Create a new personal buffs overlay
    pub fn new(
        window_config: OverlayConfig,
        config: PersonalBuffsConfig,
        background_alpha: u8,
    ) -> Result<Self, PlatformError> {
        let mut frame = OverlayFrame::new(window_config, BASE_WIDTH, BASE_HEIGHT)?;
        frame.set_background_alpha(background_alpha);
        frame.set_label("Personal Buffs");

        Ok(Self {
            frame,
            config,
            background_alpha,
            data: PersonalBuffsData::default(),
            icon_cache: HashMap::new(),
            last_rendered: Vec::new(),
        })
    }

    /// Update the config
    pub fn set_config(&mut self, config: PersonalBuffsConfig) {
        self.config = config;
    }

    /// Update background alpha
    pub fn set_background_alpha(&mut self, alpha: u8) {
        self.background_alpha = alpha;
        self.frame.set_background_alpha(alpha);
    }

    /// Update the data and pre-cache any new icons
    pub fn set_data(&mut self, data: PersonalBuffsData) {
        // Pre-cache icons at current display size
        let icon_size = self.frame.scaled(self.config.icon_size as f32) as u32;

        for buff in &data.buffs {
            if let Some(ref icon_arc) = buff.icon {
                let cache_key = (buff.icon_ability_id, icon_size);
                if !self.icon_cache.contains_key(&cache_key) {
                    let (src_w, src_h, ref src_data) = **icon_arc;
                    let scaled = scale_icon(src_data, src_w, src_h, icon_size);
                    self.icon_cache.insert(cache_key, scaled);
                }
            }
        }

        self.data = data;
    }

    /// Render the overlay
    pub fn render(&mut self) {
        let max_display = self.config.max_display as usize;

        // Build current visible state for dirty check
        let current_state: Vec<(u64, String, u8)> = self
            .data
            .buffs
            .iter()
            .take(max_display)
            .map(|b| (b.effect_id, b.format_time(), b.stacks))
            .collect();

        // Skip render if nothing changed (but always render at least once)
        if current_state == self.last_rendered && !self.last_rendered.is_empty() {
            return;
        }
        self.last_rendered = current_state;

        let padding = self.frame.scaled(BASE_PADDING);
        let icon_spacing = self.frame.scaled(BASE_ICON_SPACING);
        let font_size = self.frame.scaled(BASE_FONT_SIZE);
        let icon_size = self.frame.scaled(self.config.icon_size as f32);

        self.frame.begin_frame();

        if self.data.buffs.is_empty() {
            self.frame.end_frame();
            return;
        }

        let mut x = padding;
        let y = padding;

        let icon_size_u32 = icon_size as u32;

        for buff in self.data.buffs.iter().take(max_display) {
            // Draw icon from cache or colored square fallback
            let cache_key = (buff.icon_ability_id, icon_size_u32);
            if let Some(scaled_icon) = self.icon_cache.get(&cache_key) {
                // Draw pre-scaled icon (no scaling needed)
                self.frame.draw_image(
                    scaled_icon,
                    icon_size_u32,
                    icon_size_u32,
                    x,
                    y,
                    icon_size,
                    icon_size,
                );
            } else if let Some(ref icon_arc) = buff.icon {
                // Fallback if cache miss (shouldn't happen)
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
            } else {
                // Fallback: colored square
                let bg_color = color_from_rgba(buff.color);
                self.frame.fill_rounded_rect(
                    x,
                    y,
                    icon_size,
                    icon_size,
                    3.0,
                    bg_color,
                );
            }

            // Border
            let border_color = colors::white();
            self.frame.stroke_rounded_rect(
                x,
                y,
                icon_size,
                icon_size,
                3.0,
                1.0,
                border_color,
            );

            // Clock wipe - dark overlay grows from top as time runs out
            // progress = remaining/total: 1 at start (bright), 0 when expired (dark)
            let progress = buff.progress();
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

            // Stack priority mode: stacks centered & large, timer in corner
            // Normal mode: timer centered, stacks in corner
            if self.config.stack_priority && buff.stacks >= 1 {
                // STACK PRIORITY: Big stacks centered
                let stack_text = format!("{}", buff.stacks);
                let stack_font_size = font_size * 1.9;
                let text_width = self.frame.measure_text(&stack_text, stack_font_size).0;
                let text_x = x + (icon_size - text_width) / 2.0;
                let text_y = y + icon_size / 2.0 + stack_font_size / 3.0;

                // Shadow
                self.frame.draw_text(
                    &stack_text,
                    text_x + 1.0,
                    text_y + 1.0,
                    stack_font_size,
                    colors::text_shadow(),
                );
                self.frame.draw_text(
                    &stack_text,
                    text_x,
                    text_y,
                    stack_font_size,
                    colors::white(),
                );

                // Timer small in top-right corner
                if self.config.show_countdown && buff.total_secs > 0.0 {
                    let time_text = buff.format_time();
                    let time_font_size = font_size * 0.8;
                    let time_x = x + icon_size - self.frame.measure_text(&time_text, time_font_size).0 - 2.0;
                    let time_y = y + time_font_size + 2.0;

                    self.frame.draw_text(
                        &time_text,
                        time_x + 1.0,
                        time_y + 1.0,
                        time_font_size,
                        colors::text_shadow(),
                    );
                    self.frame.draw_text(
                        &time_text,
                        time_x,
                        time_y,
                        time_font_size,
                        colors::label_dim(),
                    );
                }
            } else {
                // NORMAL MODE: Timer centered, stacks in corner
                if self.config.show_countdown && buff.total_secs > 0.0 {
                    let time_text = buff.format_time();
                    let text_width = self.frame.measure_text(&time_text, font_size).0;
                    let text_x = x + (icon_size - text_width) / 2.0;
                    let text_y = y + icon_size / 2.0 + font_size / 3.0;

                    self.frame.draw_text(
                        &time_text,
                        text_x,
                        text_y,
                        font_size,
                        colors::white(),
                    );
                }

                // Stack count in bottom-right corner
                if buff.stacks >= 1 {
                    let stack_text = format!("{}", buff.stacks);
                    let stack_font_size = font_size * 1.3;
                    let stack_x = x + icon_size - self.frame.measure_text(&stack_text, stack_font_size).0 - 2.0;
                    let stack_y = y + icon_size - 2.0;

                    self.frame.draw_text(
                        &stack_text,
                        stack_x + 1.0,
                        stack_y + 1.0,
                        stack_font_size,
                        colors::text_shadow(),
                    );
                    self.frame.draw_text(
                        &stack_text,
                        stack_x,
                        stack_y,
                        stack_font_size,
                        colors::white(),
                    );
                }
            }

            // Effect name below icon
            if self.config.show_effect_names {
                let name_font_size = font_size * 0.85;
                let name = truncate_name(&buff.name, 8);
                let name_width = self.frame.measure_text(&name, name_font_size).0;
                let name_x = x + (icon_size - name_width) / 2.0;
                let name_y = y + icon_size + name_font_size + 2.0;

                self.frame.draw_text(
                    &name,
                    name_x + 1.0,
                    name_y + 1.0,
                    name_font_size,
                    colors::text_shadow(),
                );
                self.frame.draw_text(
                    &name,
                    name_x,
                    name_y,
                    name_font_size,
                    colors::white(),
                );
            }

            x += icon_size + icon_spacing;
        }

        self.frame.end_frame();
    }
}

/// Truncate a name to fit within a character limit
fn truncate_name(name: &str, max_chars: usize) -> String {
    if name.chars().count() <= max_chars {
        name.to_string()
    } else {
        let truncated: String = name.chars().take(max_chars - 1).collect();
        format!("{}…", truncated)
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

impl Overlay for PersonalBuffsOverlay {
    fn update_data(&mut self, data: OverlayData) -> bool {
        if let OverlayData::PersonalBuffs(buffs_data) = data {
            let was_empty = self.data.buffs.is_empty();
            let is_empty = buffs_data.buffs.is_empty();
            self.set_data(buffs_data);
            !(was_empty && is_empty)
        } else {
            false
        }
    }

    fn update_config(&mut self, config: OverlayConfigUpdate) {
        if let OverlayConfigUpdate::PersonalBuffs(cfg, alpha) = config {
            self.set_config(cfg);
            self.set_background_alpha(alpha);
        }
    }

    fn render(&mut self) {
        PersonalBuffsOverlay::render(self);
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
