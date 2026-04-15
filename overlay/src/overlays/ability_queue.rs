//! Ability Queue Overlay
//!
//! Displays a three-tier ability queue:
//! - Tier 1 (pinned top): GCD countdown bar — thicker, gold border
//! - Tier 2 (middle): Queued/ready entries — light blue border, "READY" label
//! - Tier 3 (bottom): Active countdown entries — standard progress bar

use std::collections::HashMap;

use super::{AbilityQueueData, Overlay, OverlayConfigUpdate, OverlayData};
use crate::frame::OverlayFrame;
use crate::platform::{OverlayConfig, PlatformError};
use crate::utils::{color_from_rgba, scale_icon};
use crate::widgets::{colors, ProgressBar};

/// Cache for pre-scaled icons keyed by (ability_id, display_size)
type ScaledIconCache = HashMap<(u64, u32), Vec<u8>>;

// ─────────────────────────────────────────────────────────────────────────────
// Configuration
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AbilityQueueConfig {
    pub max_display: u8,
    pub font_scale: f32,
    pub font_color: [u8; 4],
    /// GCD bar fill color (RGBA)
    pub gcd_color: [u8; 4],
    pub dynamic_background: bool,
}

impl Default for AbilityQueueConfig {
    fn default() -> Self {
        Self {
            max_display: 12,
            font_scale: 1.0,
            font_color: [255, 255, 255, 255],
            gcd_color: [120, 200, 255, 255],
            dynamic_background: true,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Layout Constants
// ─────────────────────────────────────────────────────────────────────────────

const BASE_WIDTH: f32 = 220.0;
const BASE_HEIGHT: f32 = 200.0;
/// Standard bar height (READY + countdown entries, matches timers A/B)
const BASE_BAR_HEIGHT: f32 = 18.0;
/// GCD bar is noticeably taller than regular entries
const BASE_GCD_BAR_HEIGHT: f32 = 26.0;
/// Vertical gap after the GCD bar before the first regular entry
const BASE_GCD_GAP: f32 = 6.0;
/// Spacing between regular (non-GCD) entries — matches timers A/B
const BASE_ENTRY_SPACING: f32 = 4.0;
const BASE_PADDING: f32 = 6.0;
const BASE_FONT_SIZE: f32 = 11.0;

/// Gold border color for the GCD bar
const GCD_BORDER: [u8; 4] = [220, 170, 50, 255];
/// Light blue border color for READY entries
const READY_BORDER: [u8; 4] = [100, 180, 255, 200];

// ─────────────────────────────────────────────────────────────────────────────
// Rendering data (owned, extracted before mut frame calls)
// ─────────────────────────────────────────────────────────────────────────────

enum RowKind {
    Gcd,
    Ready,
    Countdown,
}

struct RenderRow {
    kind: RowKind,
    name: String,
    progress: f32,
    right_text: String,
    bar_color: [u8; 4],
    icon_ability_id: Option<u64>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Implementation
// ─────────────────────────────────────────────────────────────────────────────

pub struct AbilityQueueOverlay {
    frame: OverlayFrame,
    config: AbilityQueueConfig,
    data: AbilityQueueData,
    icon_cache: ScaledIconCache,
}

impl AbilityQueueOverlay {
    pub fn new(
        window_config: OverlayConfig,
        config: AbilityQueueConfig,
        background_alpha: u8,
    ) -> Result<Self, PlatformError> {
        let mut frame = OverlayFrame::new(window_config, BASE_WIDTH, BASE_HEIGHT)?;
        frame.set_background_alpha(background_alpha);
        frame.set_label("Ability Queue");
        Ok(Self { frame, config, data: AbilityQueueData::default(), icon_cache: ScaledIconCache::new() })
    }

    pub fn set_config(&mut self, config: AbilityQueueConfig) {
        self.config = config;
    }

    pub fn set_background_alpha(&mut self, alpha: u8) {
        self.frame.set_background_alpha(alpha);
    }

    pub fn set_data(&mut self, data: AbilityQueueData) {
        let bar_height = self.frame.scaled(BASE_BAR_HEIGHT);
        let icon_size = (bar_height - 4.0 * self.frame.scale_factor()).round() as u32;
        for entry in &data.entries {
            if let (Some(ability_id), Some(icon_arc)) = (entry.icon_ability_id, &entry.icon) {
                let cache_key = (ability_id, icon_size);
                if !self.icon_cache.contains_key(&cache_key) {
                    let (src_w, src_h, ref src_data) = **icon_arc;
                    self.icon_cache.insert(cache_key, scale_icon(src_data, src_w, src_h, icon_size));
                }
            }
        }
        self.data = data;
    }

    fn render_preview(&mut self) {
        let width = self.frame.width() as f32;
        let padding = self.frame.scaled(BASE_PADDING);
        let gcd_h = self.frame.scaled(BASE_GCD_BAR_HEIGHT);
        let bar_h = self.frame.scaled(BASE_BAR_HEIGHT);
        let entry_spacing = self.frame.scaled(BASE_ENTRY_SPACING);
        let gcd_gap = self.frame.scaled(BASE_GCD_GAP);
        let font_scale = self.config.font_scale.clamp(1.0, 2.0);
        let font_size = self.frame.scaled(BASE_FONT_SIZE * font_scale);
        let font_color = color_from_rgba(self.config.font_color);
        let gcd_fill = color_from_rgba(self.config.gcd_color);
        let bar_radius = 3.0 * self.frame.scale_factor();
        let content_width = width - padding * 2.0;
        let accent = colors::effect_icon_bg();

        self.frame.begin_frame();
        let mut y = padding;

        ProgressBar::new("GCD", 0.45)
            .with_fill_color(gcd_fill)
            .with_bg_color(colors::dps_bar_bg())
            .with_text_color(font_color)
            .with_right_text("0.9")
            .with_bold_text()
            .with_text_glow()
            .render(&mut self.frame, padding, y, content_width, gcd_h, font_size, bar_radius);
        draw_bar_border(&mut self.frame, padding, y, content_width, gcd_h, bar_radius, GCD_BORDER);
        y += gcd_h + gcd_gap;

        ProgressBar::new("Ability A", 1.0)
            .with_fill_color(accent)
            .with_bg_color(colors::dps_bar_bg())
            .with_text_color(font_color)
            .with_right_text("READY")
            .with_bold_text()
            .with_text_glow()
            .render(&mut self.frame, padding, y, content_width, bar_h, font_size, bar_radius);
        draw_bar_border(&mut self.frame, padding, y, content_width, bar_h, bar_radius, READY_BORDER);
        y += bar_h + entry_spacing;

        ProgressBar::new("Ability B", 0.55)
            .with_fill_color(accent)
            .with_bg_color(colors::dps_bar_bg())
            .with_text_color(font_color)
            .with_right_text("4.2")
            .with_bold_text()
            .with_text_glow()
            .render(&mut self.frame, padding, y, content_width, bar_h, font_size, bar_radius);

        let _ = y;
        self.frame.end_frame();
    }

    pub fn render(&mut self) {
        if self.frame.is_in_move_mode() {
            self.render_preview();
            return;
        }

        let width = self.frame.width() as f32;
        let padding = self.frame.scaled(BASE_PADDING);
        let gcd_h = self.frame.scaled(BASE_GCD_BAR_HEIGHT);
        let bar_h = self.frame.scaled(BASE_BAR_HEIGHT);
        let entry_spacing = self.frame.scaled(BASE_ENTRY_SPACING);
        let gcd_gap = self.frame.scaled(BASE_GCD_GAP);
        let font_scale = self.config.font_scale.clamp(1.0, 2.0);
        let font_size = self.frame.scaled(BASE_FONT_SIZE * font_scale);
        let font_color = color_from_rgba(self.config.font_color);
        let gcd_color = self.config.gcd_color;
        let bar_radius = 3.0 * self.frame.scale_factor();
        let content_width = width - padding * 2.0;
        let max = self.config.max_display as usize;
        let icon_size = bar_h - 4.0 * self.frame.scale_factor();
        let icon_padding = 2.0 * self.frame.scale_factor();
        let icon_size_u32 = icon_size.round() as u32;

        // ── Collect owned render rows ──────────────────────────────────────────
        let rows: Vec<RenderRow> = {
            let pinned: Vec<_> = self.data.entries.iter().filter(|e| e.is_pinned).collect();
            let mut queued: Vec<_> = self.data.entries.iter().filter(|e| !e.is_pinned && e.is_queued).collect();
            let mut active: Vec<_> = self.data.entries.iter().filter(|e| !e.is_pinned && !e.is_queued).collect();

            queued.sort_by(|x, y| y.queue_priority.cmp(&x.queue_priority));
            active.sort_by(|x, y| x.remaining_secs.partial_cmp(&y.remaining_secs).unwrap_or(std::cmp::Ordering::Equal));

            let mut rows = Vec::new();
            let mut rendered = 0;

            for e in &pinned {
                if rendered >= max { break; }
                rows.push(RenderRow {
                    kind: RowKind::Gcd,
                    name: "GCD".to_string(),
                    progress: 1.0 - e.progress(),
                    right_text: format!("{:.1}", e.remaining_secs.max(0.0)),
                    bar_color: gcd_color,
                    icon_ability_id: None,
                });
                rendered += 1;
            }
            for e in &queued {
                if rendered >= max { break; }
                rows.push(RenderRow {
                    kind: RowKind::Ready,
                    name: e.name.clone(),
                    progress: 1.0,
                    right_text: "READY".to_string(),
                    bar_color: e.color,
                    icon_ability_id: e.icon_ability_id,
                });
                rendered += 1;
            }
            for e in &active {
                if rendered >= max { break; }
                let time_text = baras_types::formatting::format_countdown(e.remaining_secs, "", "0:00", false);
                rows.push(RenderRow {
                    kind: RowKind::Countdown,
                    name: e.name.clone(),
                    progress: e.progress(),
                    right_text: time_text,
                    bar_color: e.color,
                    icon_ability_id: e.icon_ability_id,
                });
                rendered += 1;
            }

            let _ = (pinned.len(), queued.len(), active.len());
            rows
        };

        // ── Content height ─────────────────────────────────────────────────────
        // GCD rows use gcd_h + gcd_gap; all others use bar_h + entry_spacing.
        // Subtract one entry_spacing for the last row (no trailing gap).
        let gcd_count = rows.iter().filter(|r| matches!(r.kind, RowKind::Gcd)).count();
        let other_count = rows.len() - gcd_count;
        let content_height = if rows.is_empty() {
            0.0
        } else {
            let gcd_height = gcd_count as f32 * (gcd_h + gcd_gap);
            let other_height = other_count as f32 * (bar_h + entry_spacing)
                - if other_count > 0 { entry_spacing } else { 0.0 }
                - if gcd_count > 0 && other_count > 0 { 0.0 } else { 0.0 };
            padding * 2.0 + gcd_height + other_height
        };

        if self.config.dynamic_background {
            self.frame.begin_frame_with_content_height(content_height);
        } else {
            self.frame.begin_frame();
        }

        if rows.is_empty() {
            self.frame.end_frame();
            return;
        }

        let mut y = padding;

        for (i, row) in rows.iter().enumerate() {
            let is_gcd = matches!(row.kind, RowKind::Gcd);
            let is_ready = matches!(row.kind, RowKind::Ready);
            let row_h = if is_gcd { gcd_h } else { bar_h };
            let has_icon = !is_gcd && row.icon_ability_id.is_some();

            let fill_color = color_from_rgba(row.bar_color);
            let mut bar = ProgressBar::new(&row.name, row.progress)
                .with_fill_color(fill_color)
                .with_bg_color(colors::dps_bar_bg())
                .with_text_color(font_color)
                .with_right_text(&row.right_text)
                .with_bold_text()
                .with_text_glow();

            if has_icon {
                bar = bar.with_label_offset(icon_size + icon_padding);
            }

            bar.render(&mut self.frame, padding, y, content_width, row_h, font_size, bar_radius);

            // GCD: gold border; READY: light blue border
            if is_gcd {
                draw_bar_border(&mut self.frame, padding, y, content_width, row_h, bar_radius, GCD_BORDER);
            } else if is_ready {
                draw_bar_border(&mut self.frame, padding, y, content_width, row_h, bar_radius, READY_BORDER);
            }

            // Draw icon
            if let Some(ability_id) = row.icon_ability_id {
                let icon_x = padding + icon_padding;
                let icon_y = y + icon_padding;
                let cache_key = (ability_id, icon_size_u32);

                let icon_drawn = if let Some(scaled) = self.icon_cache.get(&cache_key) {
                    self.frame.draw_image(scaled, icon_size_u32, icon_size_u32, icon_x, icon_y, icon_size, icon_size);
                    true
                } else if let Some(entry) = self.data.entries.iter().find(|e| e.icon_ability_id == Some(ability_id)) {
                    if let Some(ref icon_arc) = entry.icon {
                        let (img_w, img_h, ref rgba) = **icon_arc;
                        self.frame.draw_image(rgba, img_w, img_h, icon_x, icon_y, icon_size, icon_size);
                        true
                    } else { false }
                } else { false };

                if icon_drawn {
                    let icon_radius = 2.0 * self.frame.scale_factor();
                    let glow_expand = 1.0 * self.frame.scale_factor();
                    let outer_glow = tiny_skia::Color::from_rgba(1.0, 1.0, 1.0, 0.25).unwrap();
                    self.frame.stroke_rounded_rect(
                        icon_x - glow_expand, icon_y - glow_expand,
                        icon_size + glow_expand * 2.0, icon_size + glow_expand * 2.0,
                        icon_radius + glow_expand, 1.5 * self.frame.scale_factor(), outer_glow,
                    );
                    let inner_border = tiny_skia::Color::from_rgba(1.0, 1.0, 1.0, 0.6).unwrap();
                    self.frame.stroke_rounded_rect(
                        icon_x, icon_y, icon_size, icon_size,
                        icon_radius, 1.0 * self.frame.scale_factor(), inner_border,
                    );
                }
            }

            // Advance y: GCD uses gcd_gap, others use entry_spacing (skip trailing gap on last row)
            if i + 1 < rows.len() {
                y += row_h + if is_gcd { gcd_gap } else { entry_spacing };
            }
        }

        self.frame.end_frame();
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn draw_bar_border(
    frame: &mut OverlayFrame,
    x: f32, y: f32, w: f32, h: f32,
    radius: f32,
    color: [u8; 4],
) {
    let c = tiny_skia::Color::from_rgba(
        color[0] as f32 / 255.0,
        color[1] as f32 / 255.0,
        color[2] as f32 / 255.0,
        color[3] as f32 / 255.0,
    ).unwrap_or(tiny_skia::Color::WHITE);
    frame.stroke_rounded_rect(x, y, w, h, radius, 1.5 * frame.scale_factor(), c);
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Trait Implementation
// ─────────────────────────────────────────────────────────────────────────────

impl Overlay for AbilityQueueOverlay {
    fn update_data(&mut self, data: OverlayData) -> bool {
        if let OverlayData::AbilityQueue(aq_data) = data {
            let was_empty = self.data.entries.is_empty();
            let is_empty = aq_data.entries.is_empty();
            self.set_data(aq_data);
            !(was_empty && is_empty)
        } else {
            false
        }
    }

    fn update_config(&mut self, config: OverlayConfigUpdate) {
        if let OverlayConfigUpdate::AbilityQueue(aq_config, alpha) = config {
            self.set_config(aq_config);
            self.set_background_alpha(alpha);
        }
    }

    fn render(&mut self) {
        AbilityQueueOverlay::render(self);
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
