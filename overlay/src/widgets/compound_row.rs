//! Compound row widget for displaying a label with multiple values
//!
//! Renders a category label on the left and multiple values distributed
//! evenly across the remaining width. Used for grouped personal stats
//! like "Damage: 6,541  45.2K  Crit: 32.5%"

use tiny_skia::Color;

use crate::frame::OverlayFrame;
use crate::widgets::colors;

/// Offset in pixels for text drop shadow
const SHADOW_OFFSET: f32 = 1.0;

/// A value with an optional prefix label (e.g., "Crit:" before "32.5%")
#[derive(Debug, Clone)]
pub struct CompoundValue {
    /// Optional prefix label (e.g., "Crit:", "Eff:")
    pub prefix: Option<String>,
    /// The value text
    pub value: String,
}

impl CompoundValue {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            prefix: None,
            value: value.into(),
        }
    }

    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = Some(prefix.into());
        self
    }
}

/// A row displaying a label and multiple right-distributed values
#[derive(Debug, Clone)]
pub struct CompoundRow {
    pub label: String,
    pub values: Vec<CompoundValue>,
    pub label_color: Color,
    pub value_color: Color,
    pub text_glow: bool,
}

impl CompoundRow {
    pub fn new(label: impl Into<String>, values: Vec<CompoundValue>) -> Self {
        Self {
            label: label.into(),
            values,
            label_color: colors::label_dim(),
            value_color: colors::white(),
            text_glow: false,
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

    /// Enable full surrounding text glow instead of simple drop shadow
    pub fn with_text_glow(mut self) -> Self {
        self.text_glow = true;
        self
    }

    fn draw_text(
        frame: &mut OverlayFrame,
        text: &str,
        x: f32,
        y: f32,
        font_size: f32,
        color: Color,
        bold: bool,
        glow: bool,
    ) {
        if glow {
            frame.draw_text_with_glow(text, x, y, font_size, color, bold, false);
        } else {
            let shadow = colors::text_shadow();
            frame.draw_text_styled(
                text,
                x + SHADOW_OFFSET,
                y + SHADOW_OFFSET,
                font_size,
                shadow,
                bold,
                false,
            );
            frame.draw_text_styled(text, x, y, font_size, color, bold, false);
        }
    }

    /// Measure the total width needed for all values at a given font size,
    /// including gaps between them.
    fn measure_values_width(
        frame: &mut OverlayFrame,
        values: &[CompoundValue],
        vfs: f32,
        gap: f32,
    ) -> (Vec<f32>, f32) {
        let mut widths = Vec::with_capacity(values.len());
        let mut total = 0.0f32;
        for (i, cv) in values.iter().enumerate() {
            let w = if let Some(ref prefix) = cv.prefix {
                let (pw, _) = frame.measure_text_styled(prefix, vfs, false, false);
                let (sw, _) = frame.measure_text_styled(" ", vfs, false, false);
                let (vw, _) = frame.measure_text_styled(&cv.value, vfs, true, false);
                pw + sw + vw
            } else {
                let (vw, _) = frame.measure_text_styled(&cv.value, vfs, true, false);
                vw
            };
            widths.push(w);
            total += w;
            if i > 0 {
                total += gap;
            }
        }
        (widths, total)
    }

    /// Render the compound row
    ///
    /// Layout: label left-aligned, values right-aligned to the row edge.
    /// Values are laid out right-to-left with a gap between each.
    /// If the values + label don't fit, the value font size is scaled down
    /// automatically (down to 0.55x) until everything fits.
    ///
    /// # Arguments
    /// * `frame` - The overlay frame to render to
    /// * `x` - Left edge x position
    /// * `y` - Baseline y position for text
    /// * `width` - Total width available
    /// * `font_size` - Font size for label and values
    pub fn render(&self, frame: &mut OverlayFrame, x: f32, y: f32, width: f32, font_size: f32) {
        let prefix_color = colors::label_dim();
        let glow = self.text_glow;

        // Draw label on left (regular weight, same size as values)
        Self::draw_text(frame, &self.label, x, y, font_size, self.label_color, false, glow);

        if self.values.is_empty() {
            return;
        }

        // Measure label to find available space for values
        let (label_width, _) = frame.measure_text_styled(&self.label, font_size, false, false);
        let label_gap = font_size * 0.4;
        let available = width - label_width - label_gap;

        // Values start at 1.0x (same size as label); auto-shrink only if needed.
        let base_value_scale = 1.0f32;
        let min_scale = 0.55f32;
        let base_vfs = font_size * base_value_scale;
        let base_gap = base_vfs * 0.5;
        let (mut measured, total_w) =
            Self::measure_values_width(frame, &self.values, base_vfs, base_gap);

        // If it still doesn't fit at 0.8x, shrink further down to min_scale
        let scale = if available <= 0.0 || total_w <= 0.0 {
            min_scale / base_value_scale
        } else if total_w <= available {
            1.0
        } else {
            (available / total_w).clamp(min_scale / base_value_scale, 1.0)
        };

        let gap;
        if scale < 1.0 {
            let vfs = base_vfs * scale;
            gap = vfs * 0.5;
            let result = Self::measure_values_width(frame, &self.values, vfs, gap);
            measured = result.0;
        } else {
            gap = base_gap;
        }

        let scale = base_value_scale * scale;

        let value_font_size = font_size * scale;

        // Layout right-to-left: last value flush right, others placed leftward
        let right_edge = x + width;
        let num = self.values.len();
        let mut positions: Vec<f32> = vec![0.0; num];

        let mut cursor = right_edge;
        for i in (0..num).rev() {
            positions[i] = cursor - measured[i];
            cursor = positions[i] - gap;
        }

        // Values sit on the same baseline as the label
        let vy = y;

        // Draw each value
        for (i, cv) in self.values.iter().enumerate() {
            let text_x = positions[i];

            if let Some(ref prefix) = cv.prefix {
                let (prefix_width, _) =
                    frame.measure_text_styled(prefix, value_font_size, false, false);
                let (space_width, _) =
                    frame.measure_text_styled(" ", value_font_size, false, false);

                Self::draw_text(frame, prefix, text_x, vy, value_font_size, prefix_color, false, glow);

                let value_x = text_x + prefix_width + space_width;
                Self::draw_text(frame, &cv.value, value_x, vy, value_font_size, self.value_color, true, glow);
            } else {
                Self::draw_text(frame, &cv.value, text_x, vy, value_font_size, self.value_color, true, glow);
            }
        }
    }
}
