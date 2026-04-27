//! Labeled value widget for displaying key-value pairs
//!
//! Renders a label on the left and a value right-aligned on the right.

use tiny_skia::Color;

use crate::frame::OverlayFrame;
use crate::widgets::colors;

/// Offset in pixels for text drop shadow
const SHADOW_OFFSET: f32 = 1.0;

/// A row displaying a label and right-aligned value
#[derive(Debug, Clone)]
pub struct LabeledValue {
    pub label: String,
    pub value: String,
    pub label_color: Color,
    pub value_color: Color,
    pub label_bold: bool,
    pub value_bold: bool,
    pub text_glow: bool,
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

    pub fn with_label_bold(mut self, bold: bool) -> Self {
        self.label_bold = bold;
        self
    }

    pub fn with_value_bold(mut self, bold: bool) -> Self {
        self.value_bold = bold;
        self
    }

    /// Enable full surrounding text glow instead of simple drop shadow
    pub fn with_text_glow(mut self) -> Self {
        self.text_glow = true;
        self
    }

    /// Render the labeled value row
    ///
    /// # Arguments
    /// * `frame` - The overlay frame to render to
    /// * `x` - Left edge x position
    /// * `y` - Baseline y position for text
    /// * `width` - Total width available
    /// * `font_size` - Font size for both label and value
    pub fn render(&self, frame: &mut OverlayFrame, x: f32, y: f32, width: f32, font_size: f32) {
        // Draw label on left
        self.draw_text(frame, &self.label, x, y, font_size, self.label_color, self.label_bold);

        // Draw value on right (right-aligned)
        let (text_width, _) =
            frame.measure_text_styled(&self.value, font_size, self.value_bold, false);
        let value_x = x + width - text_width;
        self.draw_text(frame, &self.value, value_x, y, font_size, self.value_color, self.value_bold);
    }

    fn draw_text(
        &self,
        frame: &mut OverlayFrame,
        text: &str,
        x: f32,
        y: f32,
        font_size: f32,
        color: Color,
        bold: bool,
    ) {
        if self.text_glow {
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

    /// Render with separate font sizes for label and value.
    /// Value is right-aligned, label is left-aligned.
    pub fn render_scaled(
        &self,
        frame: &mut OverlayFrame,
        x: f32,
        y: f32,
        width: f32,
        label_font_size: f32,
        value_font_size: f32,
    ) {
        let shadow = colors::text_shadow();

        // Draw label on left
        frame.draw_text_styled(
            &self.label,
            x + SHADOW_OFFSET,
            y + SHADOW_OFFSET,
            label_font_size,
            shadow,
            self.label_bold,
            false,
        );
        frame.draw_text_styled(
            &self.label,
            x,
            y,
            label_font_size,
            self.label_color,
            self.label_bold,
            false,
        );

        // Draw value on right (right-aligned)
        let (text_width, _) =
            frame.measure_text_styled(&self.value, value_font_size, self.value_bold, false);
        let value_x = x + width - text_width;
        frame.draw_text_styled(
            &self.value,
            value_x + SHADOW_OFFSET,
            y + SHADOW_OFFSET,
            value_font_size,
            shadow,
            self.value_bold,
            false,
        );
        frame.draw_text_styled(
            &self.value,
            value_x,
            y,
            value_font_size,
            self.value_color,
            self.value_bold,
            false,
        );
    }

    /// Calculate the height this widget needs (just the line height)
    pub fn height(&self, line_height: f32) -> f32 {
        line_height
    }
}
