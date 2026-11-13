//! Compound row widget for displaying a label with multiple values
//!
//! Renders a category label on the left and multiple values in right-aligned
//! columns. Rows that share a [`CompoundLayout`] line their columns up and use
//! the same value font size. Used for grouped personal stats like
//! "Damage  6.5K  45.2K"

use tiny_skia::Color;

use crate::frame::OverlayFrame;
use crate::widgets::colors;

/// Smallest row font scale before labels start truncating
const MIN_VALUE_SCALE: f32 = 0.55;
/// Gap between label and first value column, relative to font size
const LABEL_GAP: f32 = 0.4;
/// Gap between value columns, relative to value font size
const COLUMN_GAP: f32 = 0.5;

/// A value with an optional prefix drawn directly before it (e.g. "*" before "32.5%")
#[derive(Debug, Clone)]
pub struct CompoundValue {
    pub prefix: Option<&'static str>,
    /// Prefix font size relative to the value font size
    pub prefix_scale: f32,
    pub value: String,
}

impl CompoundValue {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            prefix: None,
            prefix_scale: 1.0,
            value: value.into(),
        }
    }

    pub fn with_prefix(mut self, prefix: &'static str) -> Self {
        self.prefix = Some(prefix);
        self
    }

    pub fn with_prefix_scale(mut self, scale: f32) -> Self {
        self.prefix_scale = scale;
        self
    }

    fn prefix_width(&self, frame: &mut OverlayFrame, vfs: f32) -> f32 {
        self.prefix.map_or(0.0, |p| {
            frame
                .measure_text_styled(p, vfs * self.prefix_scale, false, false)
                .0
        })
    }

    fn width(&self, frame: &mut OverlayFrame, vfs: f32) -> f32 {
        self.prefix_width(frame, vfs) + frame.measure_text_styled(&self.value, vfs, true, false).0
    }
}

/// Shared column widths and row scale for a set of compound rows
#[derive(Debug, Clone, Default)]
pub struct CompoundLayout {
    /// Column widths measured at the unscaled font size
    col_widths: Vec<f32>,
    scale: f32,
}

impl CompoundLayout {
    /// Measure all rows and pick the largest scale (down to `MIN_VALUE_SCALE`)
    /// at which the widest label plus the value grid fits. Labels and values
    /// shrink together so their proportions stay fixed.
    pub fn new<'a>(
        frame: &mut OverlayFrame,
        rows: impl IntoIterator<Item = &'a CompoundRow>,
        width: f32,
        font_size: f32,
    ) -> Self {
        let mut col_widths: Vec<f32> = Vec::new();
        let mut max_label = 0.0f32;
        for row in rows {
            max_label = max_label.max(
                frame
                    .measure_text_styled(&row.label, font_size, false, false)
                    .0,
            );
            for (i, cv) in row.values.iter().enumerate() {
                let w = cv.width(frame, font_size);
                match col_widths.get_mut(i) {
                    Some(cw) => *cw = cw.max(w),
                    None => col_widths.push(w),
                }
            }
        }

        let needed =
            max_label + font_size * LABEL_GAP + Self::grid_width(&col_widths, 1.0, font_size);
        let scale = if col_widths.is_empty() {
            1.0
        } else {
            (width / needed).clamp(MIN_VALUE_SCALE, 1.0)
        };
        Self { col_widths, scale }
    }

    /// Font scale applied to rows using this layout
    pub fn scale(&self) -> f32 {
        self.scale
    }

    fn grid_width(col_widths: &[f32], scale: f32, font_size: f32) -> f32 {
        let gap = font_size * scale * COLUMN_GAP;
        col_widths.iter().sum::<f32>() * scale + gap * col_widths.len().saturating_sub(1) as f32
    }
}

/// A row displaying a label and multiple right-aligned value columns
#[derive(Debug, Clone)]
pub struct CompoundRow {
    pub label: String,
    pub values: Vec<CompoundValue>,
    pub label_color: Color,
    pub value_color: Color,
}

impl CompoundRow {
    pub fn new(label: impl Into<String>, values: Vec<CompoundValue>) -> Self {
        Self {
            label: label.into(),
            values,
            label_color: colors::label_dim(),
            value_color: colors::white(),
        }
    }

    pub fn with_label_color(mut self, color: Color) -> Self {
        self.label_color = color;
        self
    }

    pub fn with_value_color(mut self, color: Color) -> Self {
        self.value_color = color;
        self
    }

    /// Render the row with `y` as the text baseline.
    ///
    /// The value grid sits flush with the right edge; each value is
    /// right-aligned in its column. Rows with fewer values than the layout
    /// leave their trailing columns empty. Label and values render at the
    /// layout's scale.
    pub fn render(
        &self,
        frame: &mut OverlayFrame,
        x: f32,
        y: f32,
        width: f32,
        font_size: f32,
        layout: &CompoundLayout,
    ) {
        let s = layout.scale;
        let vfs = font_size * s;
        let gap = vfs * COLUMN_GAP;
        let grid_left = x + width - CompoundLayout::grid_width(&layout.col_widths, s, font_size);

        // Truncation only kicks in once the layout hit MIN_VALUE_SCALE
        let label_max = grid_left - x - vfs * LABEL_GAP;
        if label_max > 0.0 {
            let label = frame.truncate_to_width(&self.label, label_max, vfs, false);
            frame.draw_text_with_glow(&label, x, y, vfs, self.label_color, false, false);
        }

        let prefix_color = colors::label_dim();
        let mut col_left = grid_left;
        for (cv, cw) in self.values.iter().zip(&layout.col_widths) {
            let col_right = col_left + cw * s;
            col_left = col_right + gap;

            let text_x = col_right - cv.width(frame, vfs);
            if text_x < x {
                continue;
            }
            if let Some(prefix) = cv.prefix {
                let pfs = vfs * cv.prefix_scale;
                frame.draw_text_with_glow(prefix, text_x, y, pfs, prefix_color, false, false);
            }
            let value_x = text_x + cv.prefix_width(frame, vfs);
            frame.draw_text_with_glow(&cv.value, value_x, y, vfs, self.value_color, true, false);
        }
    }
}
