//! Labeled value widget for displaying key-value pairs
//!
//! Renders a label on the left and a value right-aligned on the right.

use tiny_skia::Color;

use crate::frame::OverlayFrame;
use crate::widgets::colors;

/// A row displaying a label and right-aligned value
#[derive(Debug, Clone)]
pub struct LabeledValue {
    pub label: String,
    pub value: String,
    pub label_color: Color,
    pub value_color: Color,
    pub label_bold: bool,
    pub value_bold: bool,
}

impl LabeledValue {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            label_color: colors::label_dim(),
            value_color: colors::white(),
            label_bold: true,
            value_bold: true,
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

    pub fn with_label_bold(mut self, bold: bool) -> Self {
        self.label_bold = bold;
        self
    }

    pub fn with_value_bold(mut self, bold: bool) -> Self {
        self.value_bold = bold;
        self
    }

    /// Render the row with `y` as the text baseline and `width` as the total row width.
    /// The value keeps priority; the label is truncated to the space left beside it.
    pub fn render(&self, frame: &mut OverlayFrame, x: f32, y: f32, width: f32, font_size: f32) {
        let value = frame.truncate_to_width(&self.value, width, font_size, self.value_bold);
        let (value_width, _) = frame.measure_text_styled(&value, font_size, self.value_bold, false);

        let label_max = width - value_width - font_size * 0.4;
        if label_max > 0.0 {
            let label = frame.truncate_to_width(&self.label, label_max, font_size, self.label_bold);
            frame.draw_text_with_glow(
                &label,
                x,
                y,
                font_size,
                self.label_color,
                self.label_bold,
                false,
            );
        }

        frame.draw_text_with_glow(
            &value,
            x + width - value_width,
            y,
            font_size,
            self.value_color,
            self.value_bold,
            false,
        );
    }
}
