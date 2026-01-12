//! Effects Overlay (A/B)
//!
//! Consolidated overlay for displaying effect icons with countdowns.
//! Supports both horizontal (row) and vertical (column) layouts.
//! Used for Effects A and Effects B overlays.

use std::collections::HashMap;
use std::sync::Arc;

use super::{Overlay, OverlayConfigUpdate, OverlayData};
use crate::frame::OverlayFrame;
use crate::platform::{OverlayConfig, PlatformError};
use crate::utils::color_from_rgba;
use crate::widgets::colors;

/// Cache for pre-scaled icons to avoid re-scaling every frame
type ScaledIconCache = HashMap<(u64, u32), Vec<u8>>;

/// Layout direction for effects display
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EffectsLayout {
    /// Horizontal row of icons
    #[default]
    Horizontal,
    /// Vertical column of icons with text
    Vertical,
}

/// A single effect entry for display
#[derive(Debug, Clone)]
pub struct EffectABEntry {
    /// Effect ID for identification
    pub effect_id: u64,
    /// Ability ID for icon lookup
    pub icon_ability_id: u64,
    /// Display name of the effect
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
    /// Target entity name
    pub target_name: String,
    /// Pre-loaded icon RGBA data (width, height, rgba_bytes) - Arc for cheap cloning
    pub icon: Option<Arc<(u32, u32, Vec<u8>)>>,
    /// Whether to show the icon (true) or use colored square (false)
    pub show_icon: bool,
    /// Whether to display the source entity name
    pub display_source: bool,
    /// Whether this is a cleansable effect (for highlight)
    pub is_cleansable: bool,
}

impl EffectABEntry {
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

/// Data sent from service to effects overlay
#[derive(Debug, Clone, Default)]
pub struct EffectsABData {
    pub effects: Vec<EffectABEntry>,
}

/// Configuration for effects overlay
#[derive(Debug, Clone)]
pub struct EffectsABConfig {
    pub icon_size: u8,
    pub max_display: u8,
    pub layout: EffectsLayout,
    pub show_effect_names: bool,
    pub show_countdown: bool,
    /// Highlight cleansable effects (purple border)
    pub highlight_cleansable: bool,
    /// When true, stacks are shown large and centered; timer is secondary
    pub stack_priority: bool,
}

impl Default for EffectsABConfig {
    fn default() -> Self {
        Self {
            icon_size: 32,
            max_display: 8,
            layout: EffectsLayout::Horizontal,
            show_effect_names: false,
            show_countdown: true,
            highlight_cleansable: false,
            stack_priority: false,
        }
    }
}

/// Base dimensions
const BASE_WIDTH: f32 = 300.0;
const BASE_HEIGHT: f32 = 300.0;
const BASE_PADDING: f32 = 4.0;
const BASE_SPACING: f32 = 4.0;
const BASE_FONT_SIZE: f32 = 10.0;

/// Cleansable highlight color (purple glow)
const CLEANSABLE_HIGHLIGHT: [u8; 4] = [180, 80, 200, 255];

/// Effects overlay - displays effect icons in horizontal or vertical layout
pub struct EffectsABOverlay {
    frame: OverlayFrame,
    config: EffectsABConfig,
    background_alpha: u8,
    data: EffectsABData,
    /// Cache of pre-scaled icons (ability_id, size) -> scaled RGBA
    icon_cache: ScaledIconCache,
    /// Last rendered state for dirty checking: (effect_id, time_string, stacks)
    last_rendered: Vec<(u64, String, u8)>,
    /// Label for this overlay instance
    label: String,
}

impl EffectsABOverlay {
    /// Create a new effects overlay
    pub fn new(
        window_config: OverlayConfig,
        config: EffectsABConfig,
        background_alpha: u8,
        label: &str,
    ) -> Result<Self, PlatformError> {
        let mut frame = OverlayFrame::new(window_config, BASE_WIDTH, BASE_HEIGHT)?;
        frame.set_background_alpha(background_alpha);
        frame.set_label(label);

        Ok(Self {
            frame,
            config,
            background_alpha,
            data: EffectsABData::default(),
            icon_cache: HashMap::new(),
            last_rendered: Vec::new(),
            label: label.to_string(),
        })
    }

    /// Update the config
    pub fn set_config(&mut self, config: EffectsABConfig) {
        self.config = config;
    }

    /// Update background alpha
    pub fn set_background_alpha(&mut self, alpha: u8) {
        self.background_alpha = alpha;
        self.frame.set_background_alpha(alpha);
    }

    /// Update the data and pre-cache any new icons
    pub fn set_data(&mut self, data: EffectsABData) {
        // Pre-cache icons at current display size
        let icon_size = self.frame.scaled(self.config.icon_size as f32) as u32;

        for effect in &data.effects {
            if let Some(ref icon_arc) = effect.icon {
                let cache_key = (effect.icon_ability_id, icon_size);
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
        match self.config.layout {
            EffectsLayout::Horizontal => self.render_horizontal(),
            EffectsLayout::Vertical => self.render_vertical(),
        }
    }

    /// Render horizontal layout (row of icons)
    fn render_horizontal(&mut self) {
        let max_display = self.config.max_display as usize;

        // Build current visible state for dirty check
        let current_state: Vec<(u64, String, u8)> = self
            .data
            .effects
            .iter()
            .take(max_display)
            .map(|e| (e.effect_id, e.format_time(), e.stacks))
            .collect();

        // Skip render if nothing changed
        if current_state == self.last_rendered && !self.last_rendered.is_empty() {
            return;
        }
        self.last_rendered = current_state;

        let padding = self.frame.scaled(BASE_PADDING);
        let spacing = self.frame.scaled(BASE_SPACING);
        let font_size = self.frame.scaled(BASE_FONT_SIZE);
        let icon_size = self.frame.scaled(self.config.icon_size as f32);

        self.frame.begin_frame();

        if self.data.effects.is_empty() {
            self.frame.end_frame();
            return;
        }

        let mut x = padding;
        let y = padding;
        let icon_size_u32 = icon_size as u32;

        // Clone effects to avoid borrow issues
        let effects: Vec<_> = self.data.effects.iter().take(max_display).cloned().collect();

        for effect in &effects {
            // Cleansable highlight border
            if self.config.highlight_cleansable && effect.is_cleansable {
                let highlight_color = color_from_rgba(CLEANSABLE_HIGHLIGHT);
                self.frame.stroke_rounded_rect(
                    x - 2.0,
                    y - 2.0,
                    icon_size + 4.0,
                    icon_size + 4.0,
                    4.0,
                    2.0,
                    highlight_color,
                );
            }

            // Draw icon
            self.draw_icon(effect, x, y, icon_size, icon_size_u32);

            // Border
            self.frame.stroke_rounded_rect(
                x,
                y,
                icon_size,
                icon_size,
                3.0,
                1.0,
                colors::white(),
            );

            // Clock wipe overlay
            let progress = effect.progress();
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

            // Stack priority vs normal mode
            if self.config.stack_priority && effect.stacks >= 1 {
                self.draw_stack_priority(effect, x, y, icon_size, font_size);
            } else {
                self.draw_normal_mode(effect, x, y, icon_size, font_size);
            }

            // Effect name below icon
            let mut text_y_offset = 0.0;
            if self.config.show_effect_names {
                let name_font_size = font_size * 0.85;
                let name = truncate_name(&effect.name, 8);
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
                text_y_offset = name_font_size + 2.0;
            }

            // Source name below effect name
            if effect.display_source && !effect.source_name.is_empty() {
                let source_font_size = font_size * 0.75;
                let source = truncate_name(&effect.source_name, 10);
                let source_width = self.frame.measure_text(&source, source_font_size).0;
                let source_x = x + (icon_size - source_width) / 2.0;
                let source_y = y + icon_size + source_font_size + 2.0 + text_y_offset;

                self.frame.draw_text(
                    &source,
                    source_x + 1.0,
                    source_y + 1.0,
                    source_font_size,
                    colors::text_shadow(),
                );
                self.frame.draw_text(
                    &source,
                    source_x,
                    source_y,
                    source_font_size,
                    colors::label_dim(),
                );
            }

            x += icon_size + spacing;
        }

        self.frame.end_frame();
    }

    /// Render vertical layout (column with text beside icons)
    fn render_vertical(&mut self) {
        let max_display = self.config.max_display as usize;

        // Build current visible state for dirty check
        let current_state: Vec<(u64, String, u8)> = self
            .data
            .effects
            .iter()
            .take(max_display)
            .map(|e| (e.effect_id, e.format_time(), e.stacks))
            .collect();

        // Skip render if nothing changed
        if current_state == self.last_rendered && !self.last_rendered.is_empty() {
            return;
        }
        self.last_rendered = current_state;

        let padding = self.frame.scaled(BASE_PADDING);
        let row_spacing = self.frame.scaled(BASE_SPACING);
        let font_size = self.frame.scaled(BASE_FONT_SIZE);
        let icon_size = self.frame.scaled(self.config.icon_size as f32);
        let row_height = icon_size + row_spacing;

        self.frame.begin_frame();

        if self.data.effects.is_empty() {
            self.frame.end_frame();
            return;
        }

        let mut y = padding;
        let icon_size_u32 = icon_size as u32;

        // Clone effects to avoid borrow issues
        let effects: Vec<_> = self.data.effects.iter().take(max_display).cloned().collect();

        for effect in &effects {
            let x = padding;

            // Cleansable highlight border
            if self.config.highlight_cleansable && effect.is_cleansable {
                let highlight_color = color_from_rgba(CLEANSABLE_HIGHLIGHT);
                self.frame.stroke_rounded_rect(
                    x - 2.0,
                    y - 2.0,
                    icon_size + 4.0,
                    icon_size + 4.0,
                    4.0,
                    2.0,
                    highlight_color,
                );
            }

            // Draw icon
            self.draw_icon(effect, x, y, icon_size, icon_size_u32);

            // Clock wipe overlay
            let progress = effect.progress();
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
            self.frame.stroke_rounded_rect(
                x,
                y,
                icon_size,
                icon_size,
                3.0,
                1.0,
                colors::white(),
            );

            // Stack count in corner
            if effect.stacks >= 1 {
                let stack_text = format!("{}", effect.stacks);
                let stack_font_size = font_size * 0.9;
                let stack_x =
                    x + icon_size - self.frame.measure_text(&stack_text, stack_font_size).0 - 2.0;
                let stack_y = y + stack_font_size + 2.0;

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
                    colors::effect_buff(),
                );
            }

            // Text to the right of icon
            let text_x = x + icon_size + padding;
            let text_y = y + icon_size / 2.0;

            if self.config.show_effect_names {
                // Effect name on top
                let name_y = text_y - font_size * 0.3;
                self.frame.draw_text(
                    &effect.name,
                    text_x,
                    name_y,
                    font_size,
                    colors::white(),
                );

                // Countdown below
                if self.config.show_countdown && effect.total_secs > 0.0 {
                    let time_text = effect.format_time();
                    let time_y = name_y + font_size + 2.0;
                    self.frame.draw_text(
                        &time_text,
                        text_x,
                        time_y,
                        font_size * 0.9,
                        colors::label_dim(),
                    );

                    // Source name below countdown
                    if effect.display_source && !effect.source_name.is_empty() {
                        let source_font_size = font_size * 0.8;
                        self.frame.draw_text(
                            &effect.source_name,
                            text_x,
                            time_y + font_size * 0.9 + 2.0,
                            source_font_size,
                            colors::label_dim(),
                        );
                    }
                }
            } else {
                // Just countdown centered
                if self.config.show_countdown && effect.total_secs > 0.0 {
                    let time_text = effect.format_time();
                    self.frame.draw_text(
                        &time_text,
                        text_x,
                        text_y + font_size / 3.0,
                        font_size,
                        colors::white(),
                    );

                    // Source name below countdown
                    if effect.display_source && !effect.source_name.is_empty() {
                        let source_font_size = font_size * 0.8;
                        self.frame.draw_text(
                            &effect.source_name,
                            text_x,
                            text_y + font_size / 3.0 + font_size + 2.0,
                            source_font_size,
                            colors::label_dim(),
                        );
                    }
                }
            }

            y += row_height;
        }

        self.frame.end_frame();
    }

    /// Draw icon or colored square fallback
    fn draw_icon(&mut self, effect: &EffectABEntry, x: f32, y: f32, icon_size: f32, icon_size_u32: u32) {
        let cache_key = (effect.icon_ability_id, icon_size_u32);
        let has_icon = if effect.show_icon {
            if let Some(scaled_icon) = self.icon_cache.get(&cache_key) {
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
            } else if let Some(ref icon_arc) = effect.icon {
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
            }
        } else {
            false
        };

        if !has_icon {
            // Fallback: colored square
            let bg_color = color_from_rgba(effect.color);
            self.frame.fill_rounded_rect(
                x,
                y,
                icon_size,
                icon_size,
                3.0,
                bg_color,
            );
        }
    }

    /// Draw stack-priority mode (big stacks centered, timer in corner)
    fn draw_stack_priority(&mut self, effect: &EffectABEntry, x: f32, y: f32, icon_size: f32, font_size: f32) {
        let stack_text = format!("{}", effect.stacks);
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
        if self.config.show_countdown && effect.total_secs > 0.0 {
            let time_text = effect.format_time();
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
    }

    /// Draw normal mode (timer centered, stacks in corner)
    fn draw_normal_mode(&mut self, effect: &EffectABEntry, x: f32, y: f32, icon_size: f32, font_size: f32) {
        if self.config.show_countdown && effect.total_secs > 0.0 {
            let time_text = effect.format_time();
            let text_width = self.frame.measure_text(&time_text, font_size).0;
            let text_x = x + (icon_size - text_width) / 2.0;
            let text_y = y + icon_size / 2.0 + font_size / 3.0;

            self.frame.draw_text(
                &time_text,
                text_x + 1.0,
                text_y + 1.0,
                font_size,
                colors::text_shadow(),
            );
            self.frame.draw_text(
                &time_text,
                text_x,
                text_y,
                font_size,
                colors::white(),
            );
        }

        // Stack count in bottom-right corner
        if effect.stacks >= 1 {
            let stack_text = format!("{}", effect.stacks);
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

impl Overlay for EffectsABOverlay {
    fn update_data(&mut self, data: OverlayData) -> bool {
        match data {
            OverlayData::EffectsA(effects_data) | OverlayData::EffectsB(effects_data) => {
                let was_empty = self.data.effects.is_empty();
                let is_empty = effects_data.effects.is_empty();
                self.set_data(effects_data);
                !(was_empty && is_empty)
            }
            _ => false,
        }
    }

    fn update_config(&mut self, config: OverlayConfigUpdate) {
        match config {
            OverlayConfigUpdate::EffectsA(cfg, alpha) | OverlayConfigUpdate::EffectsB(cfg, alpha) => {
                self.set_config(cfg);
                self.set_background_alpha(alpha);
            }
            _ => {}
        }
    }

    fn render(&mut self) {
        EffectsABOverlay::render(self);
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
