//! Header widget for section titles with separator lines
//!
//! Renders a title with an optional separator line below.

use tiny_skia::Color;

use crate::frame::OverlayFrame;
use crate::widgets::colors;

/// A section header with title and optional separator
#[derive(Debug, Clone)]
pub struct Header {
    pub title: String,
    pub color: Color,
    pub show_separator: bool,
}

impl Header {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            color: colors::white(),
            show_separator: true,
        }
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub fn with_separator(mut self, show: bool) -> Self {
        self.show_separator = show;
        self
    }

    /// Render the header and return the y position after rendering
    ///
    /// # Arguments
    /// * `frame` - The overlay frame to render to
    /// * `x` - Left edge x position
    /// * `y` - Top y position
    /// * `width` - Total width available
    /// * `font_size` - Font size for title
    /// * `spacing` - Spacing after separator
    ///
    /// # Returns
    /// The y position after the header (ready for next content)
    pub fn render(
        &self,
        frame: &mut OverlayFrame,
        x: f32,
        y: f32,
        width: f32,
        font_size: f32,
        spacing: f32,
    ) -> f32 {
        let title_y = y + font_size;
        frame.draw_text(&self.title, x, title_y, font_size, self.color);

        if self.show_separator {
            let sep_y = title_y + spacing + 2.0;
            let line_height = 0.2 * frame.scale_factor();
            frame.fill_rect(x, sep_y, width, line_height, self.color);

            sep_y + spacing + 4.0 * frame.scale_factor()
        } else {
            title_y + spacing
        }
    }

    /// Calculate the total height this header will use
    pub fn height(&self, font_size: f32, spacing: f32, scale: f32) -> f32 {
        if self.show_separator {
            font_size + spacing + 2.0 + spacing + 4.0 * scale
        } else {
            font_size + spacing
        }
    }
}

/// A footer with separator and one or two right-aligned values
///
/// Supports both single-column (just rate or just total) and two-column
/// (total + rate) layouts to match the progress bar display.
#[derive(Debug, Clone)]
pub struct Footer {
    /// Primary value displayed on the right (e.g., rate sum)
    pub value: String,
    /// Secondary value displayed in center (e.g., total sum)
    pub secondary_value: Option<String>,
    pub color: Color,
}

impl Footer {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            secondary_value: None,
            color: colors::white(),
        }
    }

    /// Set secondary value (displayed in center, e.g., total sum)
    pub fn with_secondary(mut self, value: impl Into<String>) -> Self {
        self.secondary_value = Some(value.into());
        self
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Check if this is a two-column layout
    fn is_two_column(&self) -> bool {
        self.secondary_value.is_some()
    }

    /// Render the footer
    ///
    /// # Arguments
    /// * `frame` - The overlay frame to render to
    /// * `x` - Left edge x position
    /// * `y` - Top y position (where separator starts)
    /// * `width` - Total width available
    /// * `font_size` - Font size for value
    /// * `spacing` - Spacing between separator and value
    pub fn render(&self, frame: &mut OverlayFrame, x: f32, y: f32, width: f32, font_size: f32) {
        let text_padding = 4.0 * frame.scale_factor();
        let text_y = y + font_size;
        let is_two_col = self.is_two_column();

        // Use smaller font for two-column layout
        let effective_font_size = if is_two_col {
            font_size * 0.85
        } else {
            font_size
        };

        // Draw primary value right-aligned
        let (text_width, _) = frame.measure_text(&self.value, effective_font_size);
        frame.draw_text(
            &self.value,
            x + width - text_width - text_padding,
            text_y,
            effective_font_size,
            self.color,
        );

        // Draw secondary value in center area (if present)
        if let Some(ref secondary) = self.secondary_value {
            // Position secondary value in the center-right area (similar to progress bar layout)
            let right_start = x + width * 0.71; // Match progress bar column layout
            let (secondary_width, _) = frame.measure_text(secondary, effective_font_size);
            let secondary_x = right_start - secondary_width - text_padding;
            frame.draw_text(
                secondary,
                secondary_x,
                text_y,
                effective_font_size,
                self.color,
            );
        }
    }
}
