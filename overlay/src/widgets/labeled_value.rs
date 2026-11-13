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
    pub fn render(&self, frame: &mut OverlayFrame, x: f32, y: f32, width: f32, font_size: f32) {
        frame.draw_text_with_glow(
            &self.label,
            x,
            y,
            font_size,
            self.label_color,
            self.label_bold,
            false,
        );

        let (text_width, _) =
            frame.measure_text_styled(&self.value, font_size, self.value_bold, false);
        frame.draw_text_with_glow(
            &self.value,
            x + width - text_width,
            y,
            font_size,
            self.value_color,
            self.value_bold,
            false,
        );
    }
}
