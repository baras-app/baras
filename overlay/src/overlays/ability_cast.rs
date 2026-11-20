//! Ability Cast Overlay
//!
//! Displays the abilities the local player has recently activated as a
//! newest-first list of `[icon] Ability Name` rows. Entries are pushed by the
//! service per cast and expire on the overlay thread after `prune_secs`.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;

use super::{Overlay, OverlayConfigUpdate, OverlayData};
use crate::frame::OverlayFrame;
use crate::platform::{OverlayConfig, PlatformError};
use crate::utils::{color_from_rgba, shared_scaled_icons};
use crate::widgets::colors;

// ─────────────────────────────────────────────────────────────────────────────
// Configuration & Data
// ─────────────────────────────────────────────────────────────────────────────

/// Runtime configuration for the ability cast overlay
#[derive(Debug, Clone)]
pub struct AbilityCastConfig {
    /// Maximum number of rows to keep and display
    pub max_display: u8,
    /// Seconds after which a cast is removed from the list
    pub prune_secs: f32,
    /// Icon edge size in pixels (row height follows the icon)
    pub icon_size: u8,
    /// Font scale multiplier (0.3 - 3.0)
    pub font_scale: f32,
    /// Font color (RGBA)
    pub font_color: [u8; 4],
    /// When true, background shrinks to fit content
    pub dynamic_background: bool,
    /// When true, rows anchor to the bottom edge with the newest cast lowest
    pub stack_from_bottom: bool,
}

impl Default for AbilityCastConfig {
    fn default() -> Self {
        Self {
            max_display: 8,
            prune_secs: 10.0,
            icon_size: 24,
            font_scale: 1.0,
            font_color: [255, 255, 255, 255],
            dynamic_background: true,
            stack_from_bottom: false,
        }
    }
}

/// A single cast pushed from the service
#[derive(Debug, Clone)]
pub struct AbilityCastEntry {
    pub ability_id: u64,
    pub name: String,
    pub cast_at: Instant,
    /// Pre-loaded icon RGBA data (width, height, rgba_bytes)
    pub icon: Option<Arc<(u32, u32, Vec<u8>)>>,
}

/// Data sent from the service to the ability cast overlay.
/// Non-empty appends new casts; empty clears the list.
#[derive(Debug, Clone, Default)]
pub struct AbilityCastData {
    pub entries: Vec<AbilityCastEntry>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Layout Constants
// ─────────────────────────────────────────────────────────────────────────────

const BASE_WIDTH: f32 = 200.0;
const BASE_HEIGHT: f32 = 240.0;
const BASE_PADDING: f32 = 6.0;
const BASE_ROW_SPACING: f32 = 3.0;
const BASE_ICON_GAP: f32 = 6.0;
const BASE_FONT_SIZE: f32 = 12.0;

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Implementation
// ─────────────────────────────────────────────────────────────────────────────

pub struct AbilityCastOverlay {
    frame: OverlayFrame,
    config: AbilityCastConfig,
    /// Newest first
    entries: VecDeque<AbilityCastEntry>,
}

impl AbilityCastOverlay {
    pub fn new(window_config: OverlayConfig, config: AbilityCastConfig) -> Result<Self, PlatformError> {
        let mut frame = OverlayFrame::new(window_config, BASE_WIDTH, BASE_HEIGHT)?;
        frame.set_label("Abilities Cast");

        Ok(Self {
            frame,
            config,
            entries: VecDeque::new(),
        })
    }

    pub fn set_config(&mut self, config: AbilityCastConfig) {
        self.config = config;
        self.truncate();
    }

    fn icon_px(&self) -> f32 {
        self.frame.scaled(self.config.icon_size as f32)
    }

    fn font_px(&self) -> f32 {
        self.frame.scaled(BASE_FONT_SIZE) * self.config.font_scale
    }

    /// Row height is the taller of the icon and the text line
    fn row_height(&self) -> f32 {
        self.icon_px().max(self.font_px() * 1.2)
    }

    fn truncate(&mut self) {
        self.entries.truncate(self.config.max_display as usize);
    }

    /// Prepend new casts (newest first) and pre-scale their icons
    fn add_casts(&mut self, new_entries: Vec<AbilityCastEntry>) {
        let icon_size = self.icon_px().round() as u32;
        for entry in new_entries {
            if let Some(icon_arc) = &entry.icon {
                let (w, h, ref rgba) = **icon_arc;
                let _ = shared_scaled_icons().get_or_scale(entry.ability_id, icon_size, rgba, w, h);
            }
            self.entries.push_front(entry);
        }
        self.truncate();
    }

    fn prune_expired(&mut self) {
        let prune = self.config.prune_secs;
        // Newest first, so expired entries are always at the back
        while self
            .entries
            .back()
            .is_some_and(|e| e.cast_at.elapsed().as_secs_f32() >= prune)
        {
            self.entries.pop_back();
        }
    }

    /// Y of the first row and of the background rect for `rows` rows,
    /// honouring the stack direction.
    fn rows_origin(&self, rows: usize) -> (f32, f32) {
        let padding = self.frame.scaled(BASE_PADDING);
        if !self.config.stack_from_bottom || rows == 0 {
            return (padding, 0.0);
        }
        let content_h = self.content_height(rows);
        let bg_y = (self.frame.height() as f32 - content_h).max(0.0);
        (bg_y + padding, bg_y)
    }

    fn content_height(&self, rows: usize) -> f32 {
        if rows == 0 {
            return 0.0;
        }
        let padding = self.frame.scaled(BASE_PADDING);
        let spacing = self.frame.scaled(BASE_ROW_SPACING);
        padding * 2.0 + rows as f32 * self.row_height() + (rows - 1) as f32 * spacing
    }

    /// Draw one `[icon] name` row at `y`. `icon` is the scaled cache key
    /// (ability id) plus a fallback source image when the cache misses.
    fn draw_row(&mut self, y: f32, name: &str, icon: Option<(u64, Option<&Arc<(u32, u32, Vec<u8>)>>)>, color: [u8; 4]) {
        let padding = self.frame.scaled(BASE_PADDING);
        let icon_size = self.icon_px();
        let icon_size_u32 = icon_size.round() as u32;
        let font_size = self.font_px();
        let row_h = self.row_height();
        let icon_gap = self.frame.scaled(BASE_ICON_GAP);

        let icon_y = y + (row_h - icon_size) / 2.0;
        match icon {
            Some((ability_id, fallback)) => {
                if let Some(scaled) = shared_scaled_icons().get(ability_id, icon_size_u32) {
                    self.frame.draw_image(
                        &scaled,
                        icon_size_u32,
                        icon_size_u32,
                        padding,
                        icon_y,
                        icon_size,
                        icon_size,
                    );
                } else if let Some(icon_arc) = fallback {
                    let (w, h, ref rgba) = **icon_arc;
                    self.frame.draw_image(rgba, w, h, padding, icon_y, icon_size, icon_size);
                } else {
                    self.frame
                        .fill_rounded_rect(padding, icon_y, icon_size, icon_size, 2.0, colors::effect_icon_bg());
                }
            }
            None => {
                self.frame
                    .fill_rounded_rect(padding, icon_y, icon_size, icon_size, 2.0, colors::effect_icon_bg());
                self.frame.stroke_rounded_rect_dashed(
                    padding,
                    icon_y,
                    icon_size,
                    icon_size,
                    2.0,
                    1.0,
                    colors::preview_border(),
                    3.0,
                    2.0,
                );
            }
        }

        // Text baseline vertically centred on the row
        let text_x = padding + icon_size + icon_gap;
        let text_y = y + (row_h + font_size * 0.7) / 2.0;
        self.frame.draw_text_styled(
            name,
            text_x + 1.0,
            text_y + 1.0,
            font_size,
            colors::text_shadow(),
            true,
            false,
        );
        self.frame
            .draw_text_styled(name, text_x, text_y, font_size, color_from_rgba(color), true, false);
    }

    fn render_preview(&mut self) {
        const PREVIEW: [&str; 3] = ["Force Leap", "Sundering Assault", "Blade Storm"];
        let spacing = self.frame.scaled(BASE_ROW_SPACING);
        let row_h = self.row_height();
        let color = self.config.font_color;

        self.frame.begin_frame();
        let (mut y, _) = self.rows_origin(PREVIEW.len());
        for name in PREVIEW {
            self.draw_row(y, name, None, color);
            y += row_h + spacing;
        }
        self.frame.end_frame();
    }

    pub fn render(&mut self) {
        if self.frame.is_in_move_mode() {
            self.render_preview();
            return;
        }

        self.prune_expired();

        let rows = self.entries.len();
        let (mut y, bg_y) = self.rows_origin(rows);
        if self.config.dynamic_background {
            let h = self.content_height(rows);
            self.frame.begin_frame_with_content_rect(bg_y, h);
        } else {
            self.frame.begin_frame();
        }

        if rows == 0 {
            self.frame.end_frame();
            return;
        }

        let spacing = self.frame.scaled(BASE_ROW_SPACING);
        let row_h = self.row_height();
        let color = self.config.font_color;

        // Detach entries so rows can borrow the frame mutably. Entries are
        // newest-first; bottom-up draws them oldest-first so the newest cast
        // sits on the bottom row.
        let entries = std::mem::take(&mut self.entries);
        let ordered: Box<dyn Iterator<Item = &AbilityCastEntry>> = if self.config.stack_from_bottom {
            Box::new(entries.iter().rev())
        } else {
            Box::new(entries.iter())
        };
        for entry in ordered {
            self.draw_row(y, &entry.name, Some((entry.ability_id, entry.icon.as_ref())), color);
            y += row_h + spacing;
        }
        self.entries = entries;

        self.frame.end_frame();
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Trait Implementation
// ─────────────────────────────────────────────────────────────────────────────

impl Overlay for AbilityCastOverlay {
    fn update_data(&mut self, data: OverlayData) -> bool {
        let OverlayData::AbilityCast(data) = data else {
            return false;
        };
        if data.entries.is_empty() {
            let had = !self.entries.is_empty();
            self.entries.clear();
            had
        } else {
            self.add_casts(data.entries);
            true
        }
    }

    fn update_config(&mut self, config: OverlayConfigUpdate) {
        if let OverlayConfigUpdate::AbilityCast(cfg) = config {
            self.set_config(cfg);
        }
    }

    fn render(&mut self) {
        AbilityCastOverlay::render(self);
    }

    fn frame(&self) -> &OverlayFrame {
        &self.frame
    }

    fn frame_mut(&mut self) -> &mut OverlayFrame {
        &mut self.frame
    }

    /// Keep ticking while entries exist so pruning happens on time
    fn needs_render(&self) -> bool {
        !self.entries.is_empty()
    }
}
