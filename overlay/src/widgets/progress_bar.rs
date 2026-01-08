//! Progress bar widget for displaying metrics
#![allow(clippy::too_many_arguments)]
use tiny_skia::Color;

use crate::frame::OverlayFrame;
use crate::widgets::colors;

/// A horizontal progress bar with label and optional center/right text
///
/// Layout options:
/// - Label only: `| Name                    |`
/// - Label + right: `| Name              Value |`
/// - Label + center + right: `| Name    Center   Value |` (3-column, smaller font)
/// - Label + center: `| Name           Center   |`
#[derive(Debug, Clone)]
pub struct ProgressBar {
    pub label: String,
    pub progress: f32,
    pub fill_color: Color,
    pub bg_color: Color,
    pub text_color: Color,
    /// Optional text displayed in center (e.g., total value)
    pub center_text: Option<String>,
    /// Optional text displayed on right (e.g., per-second rate)
    pub right_text: Option<String>,
}

impl ProgressBar {
    pub fn new(label: impl Into<String>, progress: f32) -> Self {
        Self {
            label: label.into(),
            progress: progress.clamp(0.0, 1.0),
            fill_color: colors::dps_bar_fill(),
            bg_color: colors::dps_bar_bg(),
            text_color: colors::white(),
            center_text: None,
            right_text: None,
        }
    }

    pub fn with_fill_color(mut self, color: Color) -> Self {
        self.fill_color = color;
        self
    }

    pub fn with_bg_color(mut self, color: Color) -> Self {
        self.bg_color = color;
        self
    }

    pub fn with_text_color(mut self, color: Color) -> Self {
        self.text_color = color;
        self
    }

    /// Set center text (e.g., cumulative total)
    pub fn with_center_text(mut self, text: impl Into<String>) -> Self {
        self.center_text = Some(text.into());
        self
    }

    /// Set right text (e.g., per-second rate)
    pub fn with_right_text(mut self, text: impl Into<String>) -> Self {
        self.right_text = Some(text.into());
        self
    }

    /// Check if this is a 3-column layout (has both center and right text)
    fn is_three_column(&self) -> bool {
        self.center_text.is_some() && self.right_text.is_some()
    }

    /// Truncate label to fit within max_width, adding "..." if truncated
    /// Uses estimation + single verification instead of binary search to reduce measure_text calls
    fn truncate_label_to_width(
        &self,
        frame: &mut OverlayFrame,
        max_width: f32,
        font_size: f32,
    ) -> String {
        let (label_width, _) = frame.measure_text(&self.label, font_size);
        if label_width <= max_width {
            return self.label.clone();
        }

        let chars: Vec<char> = self.label.chars().collect();
        if chars.is_empty() {
            return "...".to_string();
        }

        // Estimate: assume roughly uniform character width
        // Calculate how many chars would fit based on ratio
        let (ellipsis_width, _) = frame.measure_text("...", font_size);
        let available_width = max_width - ellipsis_width;

        if available_width <= 0.0 {
            return "...".to_string();
        }

        // Estimate characters that fit (slightly conservative)
        let avg_char_width = label_width / chars.len() as f32;
        let estimated_fit = ((available_width / avg_char_width) * 0.9) as usize;
        let mut fit_count = estimated_fit.min(chars.len()).max(1);

        // Single verification pass - if too wide, back off linearly
        loop {
            let truncated: String = chars[..fit_count].iter().collect();
            let test = format!("{}...", truncated);
            let (test_width, _) = frame.measure_text(&test, font_size);

            if test_width <= max_width || fit_count <= 1 {
                return test;
            }
            fit_count -= 1;
        }
    }

    /// Render the progress bar to an OverlayFrame
    pub fn render(
        &self,
        frame: &mut OverlayFrame,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        font_size: f32,
        radius: f32,
    ) {
        // Draw background
        frame.fill_rounded_rect(x, y, width, height, radius, self.bg_color);

        // Draw fill
        let fill_width = width * self.progress;
        if fill_width > 0.0 {
            frame.fill_rounded_rect(x, y, fill_width, height, radius, self.fill_color);
        }

        let text_padding = 4.0 * frame.scale_factor();
        let is_three_col = self.is_three_column();

        // Use smaller font for 3-column layout to fit everything
        let effective_font_size = if is_three_col {
            font_size * 0.85
        } else {
            font_size
        };

        let text_y = y + height / 2.0 + effective_font_size / 3.0;

        // Calculate column widths for proper layout
        // 3-column: name gets ~45%, center gets ~27%, right gets ~28%
        // 2-column with right text: name gets remaining space after right text
        // 2-column with center only: name gets ~55%
        let (name_width, _center_start, right_start) = if is_three_col {
            let name_w = width * 0.42;
            let center_w = width * 0.29;
            (name_w, x + name_w, x + name_w + center_w)
        } else if let Some(ref right) = self.right_text {
            // Measure actual right text width and give the rest to name
            let (right_width, _) = frame.measure_text(right, effective_font_size);
            let right_reserved = right_width + text_padding * 3.0; // padding on both sides + gap
            let name_w = width - right_reserved;
            (name_w, x + name_w, x + name_w)
        } else if self.center_text.is_some() {
            let name_w = width * 0.55;
            (name_w, x + name_w, x + name_w)
        } else {
            (width - text_padding * 2.0, x, x)
        };

        // Draw label on the left (truncated to fit)
        let display_label = self.truncate_label_to_width(
            frame,
            name_width - text_padding * 2.0,
            effective_font_size,
        );
        frame.draw_text(
            &display_label,
            x + text_padding,
            text_y,
            effective_font_size,
            self.text_color,
        );

        // Draw right text (rightmost position)
        if let Some(ref right) = self.right_text {
            let (text_width, _) = frame.measure_text(right, effective_font_size);
            frame.draw_text(
                right,
                x + width - text_width - text_padding,
                text_y,
                effective_font_size,
                self.text_color,
            );
        }

        // Draw center text
        if let Some(ref center) = self.center_text {
            if is_three_col {
                // In 3-column mode, position center text right-aligned within its column
                let (center_width, _) = frame.measure_text(center, effective_font_size);
                let center_x = right_start - center_width - text_padding;
                frame.draw_text(
                    center,
                    center_x,
                    text_y,
                    effective_font_size,
                    self.text_color,
                );
            } else {
                // In 2-column mode (center only), right-align it
                let (center_width, _) = frame.measure_text(center, effective_font_size);
                frame.draw_text(
                    center,
                    x + width - center_width - text_padding,
                    text_y,
                    effective_font_size,
                    self.text_color,
                );
            }
        }
    }
}
