//! Bar row with a reserved square icon column on the left
//!
//! Shared by every bar-style list (timers, cooldowns, DOT tracker, effects
//! A/B) and their move-mode previews. The icon column is always reserved so
//! bars stay aligned whether or not an entry has an icon; icon-less rows get
//! a tinted placeholder. The bar overlaps the icon's right edge by 1px so the
//! pair reads as one shape under a single outline.

use std::sync::Arc;

use tiny_skia::Color;

use crate::frame::OverlayFrame;
use crate::utils::shared_scaled_icons;
use crate::widgets::{ProgressBar, draw_icon_placeholder};

/// Source RGBA icon as carried on overlay entries: (width, height, rgba)
pub type IconRgba = (u32, u32, Vec<u8>);

/// Icon to draw in a row's reserved column
pub enum RowIcon<'a> {
    /// Pre-scaled RGBA at `IconBarRow::icon_px()` (shared cache hit)
    Scaled(Arc<Vec<u8>>),
    /// Full-size source image, scaled on blit (cache miss fallback)
    Source(&'a IconRgba),
    /// No icon: draw the tinted placeholder
    None,
}

impl<'a> RowIcon<'a> {
    /// Shared-cache lookup at `size`, falling back to the source image
    pub fn lookup(ability_id: u64, size: u32, source: Option<&'a IconRgba>) -> Self {
        match shared_scaled_icons().get(ability_id, size) {
            Some(scaled) => Self::Scaled(scaled),
            None => source.map_or(Self::None, Self::Source),
        }
    }
}

/// Geometry for a column of icon + bar rows
#[derive(Debug, Clone, Copy)]
pub struct IconBarRow {
    /// Left edge of the icon column
    pub x: f32,
    /// Total width of icon + bar
    pub width: f32,
    /// Row height; the icon is a square of this size
    pub height: f32,
    pub radius: f32,
    overlap: f32,
}

impl IconBarRow {
    pub fn new(x: f32, width: f32, height: f32, radius: f32, scale: f32) -> Self {
        Self { x, width, height, radius, overlap: scale }
    }

    /// Icon pixel size, used as the shared icon cache key
    pub fn icon_px(&self) -> u32 {
        self.height.round() as u32
    }

    /// Draw the icon (or placeholder tinted from `bar_color`), then the bar.
    /// Returns true if a real icon was drawn.
    pub fn render(
        &self,
        frame: &mut OverlayFrame,
        y: f32,
        icon: RowIcon,
        bar_color: Color,
        bar: &ProgressBar,
        font_size: f32,
    ) -> bool {
        let size = self.height;
        let drawn = match icon {
            RowIcon::Scaled(rgba) => {
                let px = self.icon_px();
                frame.draw_image(&rgba, px, px, self.x, y, size, size);
                true
            }
            RowIcon::Source((w, h, rgba)) => {
                frame.draw_image(rgba, *w, *h, self.x, y, size, size);
                true
            }
            RowIcon::None => {
                draw_icon_placeholder(frame, self.x, y, size, size, self.radius, bar_color);
                false
            }
        };

        let bar_x = self.x + size - self.overlap;
        let bar_width = self.width - size + self.overlap;
        bar.render(frame, bar_x, y, bar_width, size, font_size, self.radius);
        drawn
    }

    /// Stroke one continuous outline around icon + bar
    pub fn outline(&self, frame: &mut OverlayFrame, y: f32, stroke_width: f32, color: Color) {
        frame.stroke_rounded_rect(
            self.x,
            y,
            self.width,
            self.height,
            self.radius,
            stroke_width,
            color,
        );
    }
}
