//! Compound row widget for displaying a label with multiple values
//!
//! Renders a category label on the left and multiple values right-aligned
//! on the right. Used for grouped personal stats like
//! "Damage: 6,541  45.2K  Crit: 32.5%"

use tiny_skia::Color;

use crate::frame::OverlayFrame;
use crate::widgets::colors;

/// Smallest value font scale before values stop shrinking
const MIN_VALUE_SCALE: f32 = 0.55;

/// A value with an optional prefix label (e.g., "Crit:" before "32.5%")
#[derive(Debug, Clone)]
pub struct CompoundValue {
    pub prefix: Option<String>,
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

/// A row displaying a label and multiple right-aligned values
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

    /// Width of the optional "prefix " part of a value (0 when no prefix).
    fn prefix_width(frame: &mut OverlayFrame, cv: &CompoundValue, vfs: f32) -> f32 {
        cv.prefix.as_deref().map_or(0.0, |prefix| {
            frame.measure_text_styled(prefix, vfs, false, false).0
                + frame.measure_text_styled(" ", vfs, false, false).0
        })
    }

    /// Per-value widths and total width (including gaps) at a given font size.
    fn measure_values(frame: &mut OverlayFrame, values: &[CompoundValue], vfs: f32) -> (Vec<f32>, f32) {
        let gap = vfs * 0.5;
        let widths: Vec<f32> = values
            .iter()
            .map(|cv| {
                Self::prefix_width(frame, cv, vfs)
                    + frame.measure_text_styled(&cv.value, vfs, true, false).0
            })
            .collect();
        let total = widths.iter().sum::<f32>() + gap * values.len().saturating_sub(1) as f32;
        (widths, total)
    }

    /// Render the row with `y` as the text baseline.
    ///
    /// Label is left-aligned; values are laid out right-to-left from the row
    /// edge. Values shrink (down to `MIN_VALUE_SCALE`) when they don't fit
    /// beside the label.
    pub fn render(&self, frame: &mut OverlayFrame, x: f32, y: f32, width: f32, font_size: f32) {
        frame.draw_text_with_glow(&self.label, x, y, font_size, self.label_color, false, false);

        if self.values.is_empty() {
            return;
        }

        let (label_width, _) = frame.measure_text_styled(&self.label, font_size, false, false);
        let available = width - label_width - font_size * 0.4;

        let (mut widths, total) = Self::measure_values(frame, &self.values, font_size);
        let scale = if available <= 0.0 || total <= 0.0 {
            MIN_VALUE_SCALE
        } else {
            (available / total).clamp(MIN_VALUE_SCALE, 1.0)
        };
        let vfs = font_size * scale;
        if scale < 1.0 {
            widths = Self::measure_values(frame, &self.values, vfs).0;
        }
        let gap = vfs * 0.5;

        // Last value flush right, earlier values placed leftward
        let mut cursor = x + width;
        let prefix_color = colors::label_dim();
        for (cv, w) in self.values.iter().zip(&widths).rev() {
            let text_x = cursor - w;
            cursor = text_x - gap;

            if let Some(prefix) = cv.prefix.as_deref() {
                frame.draw_text_with_glow(prefix, text_x, y, vfs, prefix_color, false, false);
            }
            let value_x = text_x + Self::prefix_width(frame, cv, vfs);
            frame.draw_text_with_glow(&cv.value, value_x, y, vfs, self.value_color, true, false);
        }
    }
}
