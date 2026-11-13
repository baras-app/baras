//! Progress bar widget for displaying metrics
#![allow(clippy::too_many_arguments)]
use tiny_skia::Color;

use crate::frame::OverlayFrame;
use crate::widgets::colors;

/// Lighten a color by blending it toward white
/// `amount` is 0.0 (no change) to 1.0 (full white)
pub fn lighten_color(color: Color, amount: f32) -> Color {
    let amount = amount.clamp(0.0, 1.0);
    Color::from_rgba(
        color.red() + (1.0 - color.red()) * amount,
        color.green() + (1.0 - color.green()) * amount,
        color.blue() + (1.0 - color.blue()) * amount,
        color.alpha(),
    )
    .unwrap_or(color)
}

/// Darken a color by blending it toward black, used as the trailing edge of a
/// single-color gradient. `amount` is 0.0 (no change) to 1.0 (full black).
pub fn darken_color(color: Color, amount: f32) -> Color {
    let factor = 1.0 - amount.clamp(0.0, 1.0);
    Color::from_rgba(
        color.red() * factor,
        color.green() * factor,
        color.blue() * factor,
        color.alpha(),
    )
    .unwrap_or(color)
}

/// Draw the placeholder for a reserved icon slot: a square tinted darker than
/// the paired bar with a small lighter diamond, so icon-less bars read as one
/// piece with the icon column instead of leaving a hole or a detached gray
/// block. Shared by every bar layout that reserves an icon column.
pub fn draw_icon_placeholder(
    frame: &mut OverlayFrame,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    radius: f32,
    bar_color: Color,
) {
    frame.fill_rounded_rect(x, y, w, h, radius, darken_color(bar_color, 0.5));
    frame.fill_diamond(
        x + w / 2.0,
        y + h / 2.0,
        w.min(h) * 0.22,
        lighten_color(bar_color, 0.35),
    );
}

/// How much the trailing edge of a gradient fill is darkened relative to the base
/// color, used as the default when no explicit intensity is set.
pub const GRADIENT_DARKEN: f32 = 0.32;
/// Shallower darkening for the secondary (overdrawn) segment of a split bar, so
/// the boss/adds boundary is a gentle step rather than a hard shadow where the
/// primary's bright edge meets the secondary's dark edge.
const SEAM_DARKEN: f32 = 0.25;

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
    /// Optional split progress for boss/add visualization (0.0-1.0 relative to total progress)
    /// When set, draws primary portion first, then secondary portion
    pub split_progress: Option<f32>,
    /// Optional custom color for the secondary (right) portion of split bars
    /// If None, uses a lightened version of fill_color
    pub split_color: Option<Color>,
    /// Optional offset for label text start position (for icon space)
    pub label_offset: f32,
    /// Whether to render text in bold (default: false)
    pub bold_text: bool,
    /// Whether to render text with full surrounding glow instead of drop shadow (default: false)
    pub text_glow: bool,
    /// Fade the fill from `fill_color` (left) to a darkened version (right),
    /// spanning the filled portion. Details-style single-color gradient.
    pub gradient: bool,
    /// How strongly the gradient darkens the leading edge (0.0 = flat).
    /// Defaults to `GRADIENT_DARKEN`.
    pub gradient_intensity: f32,
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
            split_progress: None,
            split_color: None,
            label_offset: 0.0,
            bold_text: false,
            text_glow: false,
            gradient: false,
            gradient_intensity: GRADIENT_DARKEN,
        }
    }

    /// Enable a single-color gradient fill (base color fading to a darker shade)
    pub fn with_gradient(mut self, gradient: bool) -> Self {
        self.gradient = gradient;
        self
    }

    /// Override how strongly the gradient darkens the leading edge (0.0 = flat).
    pub fn with_gradient_intensity(mut self, intensity: f32) -> Self {
        self.gradient_intensity = intensity.clamp(0.0, 1.0);
        self
    }

    /// Set offset for label text (to make room for icon)
    pub fn with_label_offset(mut self, offset: f32) -> Self {
        self.label_offset = offset;
        self
    }

    /// Enable bold text rendering
    pub fn with_bold_text(mut self) -> Self {
        self.bold_text = true;
        self
    }

    /// Enable full surrounding text glow instead of simple drop shadow
    pub fn with_text_glow(mut self) -> Self {
        self.text_glow = true;
        self
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

    /// Set split progress for boss/add visualization
    /// The value represents the primary portion as a fraction of total progress (0.0-1.0)
    pub fn with_split(mut self, primary_fraction: f32) -> Self {
        self.split_progress = Some(primary_fraction.clamp(0.0, 1.0));
        self
    }

    /// Set custom color for the secondary (right) portion of split bars
    pub fn with_split_color(mut self, color: Color) -> Self {
        self.split_color = Some(color);
        self
    }

    /// Check if this is a 3-column layout (has both center and right text)
    fn is_three_column(&self) -> bool {
        self.center_text.is_some() && self.right_text.is_some()
    }

    /// Draw a fill segment, applying the single-color gradient when enabled.
    /// The gradient spans the rect's own width.
    fn draw_fill(
        &self,
        frame: &mut OverlayFrame,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        color: Color,
    ) {
        self.draw_fill_span(frame, x, y, w, h, radius, color, x, x + w, self.gradient_intensity);
    }

    /// Draw a fill segment whose gradient spans `[grad_x0, grad_x1]` rather than
    /// the rect itself, darkening the leading edge by `darken`. Used for the
    /// secondary (overdrawn) portion of a split bar so its visible slice shows
    /// its own dark→light fade with a softened seam.
    fn draw_fill_span(
        &self,
        frame: &mut OverlayFrame,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        color: Color,
        grad_x0: f32,
        grad_x1: f32,
        darken: f32,
    ) {
        if self.gradient {
            frame.fill_rounded_rect_gradient(
                x,
                y,
                w,
                h,
                radius,
                grad_x0,
                grad_x1,
                darken_color(color, darken),
                color,
            );
        } else {
            frame.fill_rounded_rect(x, y, w, h, radius, color);
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

        // Draw fill (with optional split for primary/secondary visualization)
        let fill_width = width * self.progress;
        if fill_width > 0.0 {
            if let Some(primary_fraction) = self.split_progress {
                // Split bar: draw full fill as secondary color, then primary on top
                let secondary_color = self
                    .split_color
                    .unwrap_or_else(|| lighten_color(self.fill_color, 0.4));
                let primary_width = fill_width * primary_fraction;

                // Secondary fills the full width (so outer corners stay rounded),
                // but its gradient spans only the visible right segment so the
                // secondary (e.g. blue shield) shows its own dark→light fade
                // instead of just the light tail of a full-width gradient.
                self.draw_fill_span(
                    frame,
                    x,
                    y,
                    fill_width,
                    height,
                    radius,
                    secondary_color,
                    x + primary_width,
                    x + fill_width,
                    SEAM_DARKEN,
                );

                // Draw primary segment on top (covers the secondary's left/padded region)
                if primary_width > 0.0 {
                    self.draw_fill(frame, x, y, primary_width, height, radius, self.fill_color);
                }
            } else {
                // Normal single-color fill
                self.draw_fill(frame, x, y, fill_width, height, radius, self.fill_color);
            }
        }

        let text_padding = 4.0 * frame.scale_factor();
        let is_three_col = self.is_three_column();
        let bold = self.bold_text;

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
            let (right_width, _) =
                frame.measure_text_styled(right, effective_font_size, bold, false);
            let right_reserved = right_width + text_padding * 3.0; // padding on both sides + gap
            let name_w = width - right_reserved;
            (name_w, x + name_w, x + name_w)
        } else if self.center_text.is_some() {
            let name_w = width * 0.55;
            (name_w, x + name_w, x + name_w)
        } else {
            (width - text_padding * 2.0, x, x)
        };

        // Helper: draw text with either full glow or simple drop shadow
        let use_glow = self.text_glow;
        let text_color = self.text_color;
        let draw_bar_text = |frame: &mut OverlayFrame, text: &str, tx: f32, ty: f32| {
            if use_glow {
                frame.draw_text_with_glow(
                    text,
                    tx,
                    ty,
                    effective_font_size,
                    text_color,
                    bold,
                    false,
                );
            } else {
                // Simple 1px drop shadow
                frame.draw_text_styled(
                    text,
                    tx + 1.0,
                    ty + 1.0,
                    effective_font_size,
                    colors::text_shadow(),
                    bold,
                    false,
                );
                frame.draw_text_styled(text, tx, ty, effective_font_size, text_color, bold, false);
            }
        };

        // Draw label on the left (truncated to fit, with optional offset for icon)
        let label_start = x + text_padding + self.label_offset;
        let available_for_label = name_width - text_padding * 2.0 - self.label_offset;
        let display_label = frame.truncate_to_width(
            &self.label,
            available_for_label.max(0.0),
            effective_font_size,
            bold,
        );
        draw_bar_text(frame, &display_label, label_start, text_y);

        // Draw right text (rightmost position)
        if let Some(ref right) = self.right_text {
            let (text_width, _) =
                frame.measure_text_styled(right, effective_font_size, bold, false);
            let right_x = x + width - text_width - 8.0 * frame.scale_factor();
            draw_bar_text(frame, right, right_x, text_y);
        }

        // Draw center text
        if let Some(ref center) = self.center_text {
            if is_three_col {
                // In 3-column mode, position center text right-aligned within its column
                let (center_width, _) =
                    frame.measure_text_styled(center, effective_font_size, bold, false);
                let center_x = right_start - center_width - text_padding;
                draw_bar_text(frame, center, center_x, text_y);
            } else {
                // In 2-column mode (center only), right-align it
                let (center_width, _) =
                    frame.measure_text_styled(center, effective_font_size, bold, false);
                let center_pos = x + width - center_width - text_padding;
                draw_bar_text(frame, center, center_pos, text_y);
            }
        }
    }
}
