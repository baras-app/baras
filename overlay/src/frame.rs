//! Overlay frame abstraction
//!
//! `OverlayFrame` encapsulates the common chrome shared by all overlay types:
//! - Rounded background with configurable alpha
//! - Interactive border when in move mode
//! - Resize indicator in the corner
//! - Scaling calculations based on window dimensions
//!
//! This allows overlay implementations to focus solely on their content rendering.

#![allow(clippy::too_many_arguments)]
use crate::manager::OverlayWindow;
use crate::platform::{OverlayConfig, PlatformError};
use crate::widgets::colors;
use tiny_skia::Color;

/// A frame wrapper around an overlay window that handles common rendering
pub struct OverlayFrame {
    window: OverlayWindow,
    background_alpha: u8,
    base_width: f32,
    base_height: f32,
    /// Optional label shown in move mode to identify the overlay
    label: Option<String>,
}

impl OverlayFrame {
    /// Create a new overlay frame
    ///
    /// # Arguments
    /// * `config` - Window configuration
    /// * `base_width` - Reference width for scaling calculations
    /// * `base_height` - Reference height for scaling calculations
    pub fn new(
        config: OverlayConfig,
        base_width: f32,
        base_height: f32,
    ) -> Result<Self, PlatformError> {
        let window = OverlayWindow::new(config)?;

        Ok(Self {
            window,
            background_alpha: 180,
            base_width,
            base_height,
            label: None,
        })
    }

    /// Set the background alpha (0-255)
    pub fn set_background_alpha(&mut self, alpha: u8) {
        self.background_alpha = alpha;
    }

    /// Get the background alpha
    pub fn background_alpha(&self) -> u8 {
        self.background_alpha
    }

    /// Set the overlay label (shown in move mode)
    pub fn set_label(&mut self, label: impl Into<String>) {
        self.label = Some(label.into());
    }

    /// Set the font family used for all text rendering on this overlay
    pub fn set_font_family(&mut self, family: &str) {
        self.window.set_font_family(family);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Scaling
    // ─────────────────────────────────────────────────────────────────────────

    /// Calculate scale factor based on current window size vs base dimensions
    ///
    /// Uses geometric mean of width and height ratios for balanced scaling
    pub fn scale_factor(&self) -> f32 {
        let width = self.window.width() as f32;
        let height = self.window.height() as f32;
        let width_ratio = width / self.base_width;
        let height_ratio = height / self.base_height;
        (width_ratio * height_ratio).sqrt()
    }

    /// Scale a base value by the current scale factor
    #[inline]
    pub fn scaled(&self, base_value: f32) -> f32 {
        base_value * self.scale_factor()
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Frame rendering
    // ─────────────────────────────────────────────────────────────────────────

    /// Begin a new frame: clear and draw background + border
    ///
    /// Call this at the start of render(), then draw your content,
    /// then call `end_frame()`.
    pub fn begin_frame(&mut self) {
        let height = self.window.height() as f32;
        self.begin_frame_with_content_height(height);
    }

    /// Begin a new frame with the background sized to the given content height
    /// instead of the full window height.
    ///
    /// Use this when the overlay knows its actual content height and wants the
    /// background to only cover the content area. The rest of the window remains
    /// fully transparent, giving the visual effect of auto-sizing without
    /// actually resizing the window.
    ///
    /// In move mode, the background always covers the full window so the user
    /// can see the true window bounds for dragging/resizing.
    pub fn begin_frame_with_content_height(&mut self, content_height: f32) {
        self.begin_frame_with_content_rect(0.0, content_height);
    }

    /// Begin a new frame with the background positioned at a specific Y offset
    /// and sized to the given content height.
    ///
    /// This is useful when content is rendered at a non-zero Y position (e.g.
    /// "stack from bottom" in the metrics overlay) and the dynamic background
    /// needs to align with where the content actually appears.
    ///
    /// In move mode, the background always covers the full window so the user
    /// can see the true window bounds for dragging/resizing.
    pub fn begin_frame_with_content_rect(&mut self, content_y: f32, content_height: f32) {
        let width = self.window.width() as f32;
        let height = self.window.height() as f32;
        let corner_radius = self.scaled(6.0);
        let in_move_mode = self.window.is_interactive() && self.window.is_drag_enabled();

        // Clear with transparent
        self.window.clear(colors::transparent());

        // Calculate background alpha
        // In move mode: use 20% of normal alpha, but always at least 20% visible for draggability
        let alpha = if in_move_mode {
            (self.background_alpha as f32 * 0.20).round().max(51.0) as u8
        } else {
            self.background_alpha
        };

        // Draw background if there's any alpha to show
        // In move mode: always fill the full window so the user can see the bounds
        // In normal mode: only fill to the content height at the content position
        if alpha > 0 {
            let bg_color = Color::from_rgba8(30, 30, 30, alpha);
            let (bg_y, bg_height) = if in_move_mode {
                (0.0, height)
            } else {
                (content_y, content_height.min(height - content_y))
            };
            self.window
                .fill_rounded_rect(0.0, bg_y, width, bg_height, corner_radius, bg_color);
        }

        // Draw border only in move mode (interactive AND drag enabled)
        // Rearrange mode is interactive but drag disabled - no border
        if in_move_mode {
            self.window.stroke_rounded_rect(
                1.0,
                1.0,
                width - 2.0,
                height - 2.0,
                corner_radius - 1.0,
                2.0,
                colors::frame_border(),
            );

            // Draw overlay label centered in move mode
            if let Some(label) = self.label.clone() {
                let font_size = self.scaled(12.0).max(10.0);
                let label_color = Color::from_rgba8(180, 180, 180, 200);
                let (text_width, text_height) = self.window.measure_text(&label, font_size);
                let x = (width - text_width) / 2.0;
                let y = (height + text_height) / 2.0; // baseline-centered
                self.draw_text_glowed(&label, x, y, font_size, label_color);
            }
        }
    }

    /// End the frame: draw resize indicator and commit
    ///
    /// Call this after drawing your content.
    pub fn end_frame(&mut self) {
        self.draw_resize_indicator();
        self.window.commit();
    }

    /// Draw the resize grip indicator in the bottom-right corner
    /// Only shown in move mode (interactive AND drag enabled)
    fn draw_resize_indicator(&mut self) {
        // Only show resize grip in move mode, not rearrange mode
        if !self.window.is_drag_enabled() {
            return;
        }
        if !self.window.in_resize_corner() && !self.window.is_interactive() {
            return;
        }

        let width = self.window.width() as f32;
        let height = self.window.height() as f32;
        let indicator_size = self.scaled(16.0).max(16.0);

        let highlight = if self.window.is_resizing() {
            colors::white()
        } else {
            colors::resize_indicator()
        };

        // Draw filled triangle in bottom-right corner using scanlines
        // Triangle goes from top-right to bottom-left to bottom-right
        let num_lines = indicator_size as i32;
        for i in 0..num_lines {
            let line_width = (i + 1) as f32;
            let y = height - indicator_size + i as f32;
            let x = width - line_width;
            self.window.fill_rect(x, y, line_width, 1.0, highlight);
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Drawing helpers (delegate to window)
    // ─────────────────────────────────────────────────────────────────────────

    /// Draw text with bold/italic styling
    pub fn draw_text_styled(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        font_size: f32,
        color: Color,
        bold: bool,
        italic: bool,
    ) {
        self.window
            .draw_text_styled(text, x, y, font_size, color, bold, italic);
    }

    /// Draw styled text with a full surrounding dark glow for readability.
    /// Renders text at all 8 cardinal/diagonal offsets in shadow color, then the real text on top.
    pub fn draw_text_with_glow(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        font_size: f32,
        color: Color,
        bold: bool,
        italic: bool,
    ) {
        let shadow_color = crate::widgets::colors::text_shadow();
        let d = 1.0_f32;
        for &(dx, dy) in &[
            (-d, -d),
            (0.0, -d),
            (d, -d),
            (-d, 0.0),
            (d, 0.0),
            (-d, d),
            (0.0, d),
            (d, d),
        ] {
            self.draw_text_styled(text, x + dx, y + dy, font_size, shadow_color, bold, italic);
        }
        self.draw_text_styled(text, x, y, font_size, color, bold, italic);
    }

    /// Draw text with a full surrounding dark glow (non-styled convenience method).
    pub fn draw_text_glowed(&mut self, text: &str, x: f32, y: f32, font_size: f32, color: Color) {
        self.draw_text_with_glow(text, x, y, font_size, color, false, false);
    }

    /// Measure text dimensions
    pub fn measure_text(&mut self, text: &str, font_size: f32) -> (f32, f32) {
        self.window.measure_text(text, font_size)
    }

    /// Measure text dimensions with style options
    pub fn measure_text_styled(
        &mut self,
        text: &str,
        font_size: f32,
        bold: bool,
        italic: bool,
    ) -> (f32, f32) {
        self.window
            .measure_text_styled(text, font_size, bold, italic)
    }

    /// Truncate `text` with "…" so it fits `max_width`, measured in the same
    /// weight it will be drawn with. Returns an empty string if not even the
    /// ellipsis fits.
    pub fn truncate_to_width(
        &mut self,
        text: &str,
        max_width: f32,
        font_size: f32,
        bold: bool,
    ) -> String {
        if self.measure_text_styled(text, font_size, bold, false).0 <= max_width {
            return text.to_string();
        }
        let chars: Vec<char> = text.chars().collect();
        let mut buf = String::with_capacity(text.len() + 3);
        // Binary search the longest prefix that fits with the ellipsis appended
        let (mut lo, mut hi) = (0usize, chars.len().saturating_sub(1));
        while lo < hi {
            let mid = (lo + hi).div_ceil(2);
            buf.clear();
            buf.extend(&chars[..mid]);
            buf.push('…');
            if self.measure_text_styled(&buf, font_size, bold, false).0 <= max_width {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        buf.clear();
        buf.extend(&chars[..lo]);
        buf.truncate(buf.trim_end().len());
        buf.push('…');
        if lo == 0 && self.measure_text_styled(&buf, font_size, bold, false).0 > max_width {
            buf.clear();
        }
        buf
    }

    /// Draw an RGBA image at the specified position with scaling
    pub fn draw_image(
        &mut self,
        image_data: &[u8],
        image_width: u32,
        image_height: u32,
        dest_x: f32,
        dest_y: f32,
        dest_width: f32,
        dest_height: f32,
    ) {
        self.window.draw_image(
            image_data,
            image_width,
            image_height,
            dest_x,
            dest_y,
            dest_width,
            dest_height,
        );
    }

    /// Draw an RGBA image with a full surrounding shadow outline for visibility.
    /// Renders dark semi-transparent copies at all 8 cardinal/diagonal offsets,
    /// then the original image on top — similar to SWTOR's icon shadow style.
    pub fn draw_image_with_shadow(
        &mut self,
        image_data: &[u8],
        image_width: u32,
        image_height: u32,
        dest_x: f32,
        dest_y: f32,
        dest_width: f32,
        dest_height: f32,
    ) {
        // Build shadow version: black with reduced alpha
        let mut shadow = image_data.to_vec();
        for chunk in shadow.chunks_exact_mut(4) {
            let alpha = chunk[3] as u16;
            chunk[0] = 0;
            chunk[1] = 0;
            chunk[2] = 0;
            chunk[3] = (alpha * 180 / 255) as u8;
        }

        // Draw shadow at all 8 surrounding offsets for a full 1px outline
        let d = 1.0_f32;
        for &(dx, dy) in &[
            (-d, -d),
            (0.0, -d),
            (d, -d),
            (-d, 0.0),
            (d, 0.0),
            (-d, d),
            (0.0, d),
            (d, d),
        ] {
            self.draw_image(
                &shadow,
                image_width,
                image_height,
                dest_x + dx,
                dest_y + dy,
                dest_width,
                dest_height,
            );
        }

        // Draw the actual image on top
        self.draw_image(
            image_data,
            image_width,
            image_height,
            dest_x,
            dest_y,
            dest_width,
            dest_height,
        );
    }

    /// Draw a filled rectangle
    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color) {
        self.window.fill_rect(x, y, w, h, color);
    }

    /// Draw a filled rounded rectangle
    /// Draw a filled diamond centered at (cx, cy)
    pub fn fill_diamond(&mut self, cx: f32, cy: f32, radius: f32, color: Color) {
        self.window.fill_diamond(cx, cy, radius, color);
    }

    pub fn fill_rounded_rect(&mut self, x: f32, y: f32, w: f32, h: f32, radius: f32, color: Color) {
        self.window.fill_rounded_rect(x, y, w, h, radius, color);
    }

    /// Draw a filled rectangle with independent top/bottom corner radii
    pub fn fill_corner_rounded_rect(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        top_radius: f32,
        bottom_radius: f32,
        color: Color,
    ) {
        self.window
            .fill_corner_rounded_rect(x, y, w, h, top_radius, bottom_radius, color);
    }

    /// Draw a filled rounded rectangle with a horizontal linear gradient
    /// fading `start_color` to `end_color`. `grad_x0`/`grad_x1` set the
    /// gradient span independently of the rect bounds.
    pub fn fill_rounded_rect_gradient(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        grad_x0: f32,
        grad_x1: f32,
        start_color: Color,
        end_color: Color,
    ) {
        self.window
            .fill_rounded_rect_gradient(x, y, w, h, radius, grad_x0, grad_x1, start_color, end_color);
    }

    /// Draw a rounded rectangle outline
    pub fn stroke_rounded_rect(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        stroke_width: f32,
        color: Color,
    ) {
        self.window
            .stroke_rounded_rect(x, y, w, h, radius, stroke_width, color);
    }

    /// Stroke an open folder-tab outline (rounded top + sides, open bottom)
    pub fn stroke_tab_outline(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        stroke_width: f32,
        color: Color,
    ) {
        self.window
            .stroke_tab_outline(x, y, w, h, radius, stroke_width, color);
    }

    /// Draw a dashed rounded rectangle outline (useful for alignment guides)
    pub fn stroke_rounded_rect_dashed(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        stroke_width: f32,
        color: Color,
        dash_length: f32,
        gap_length: f32,
    ) {
        self.window.stroke_rounded_rect_dashed(
            x,
            y,
            w,
            h,
            radius,
            stroke_width,
            color,
            dash_length,
            gap_length,
        );
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Window access
    // ─────────────────────────────────────────────────────────────────────────

    /// Get immutable access to the underlying window
    pub fn window(&self) -> &OverlayWindow {
        &self.window
    }

    /// Get mutable access to the underlying window
    pub fn window_mut(&mut self) -> &mut OverlayWindow {
        &mut self.window
    }

    /// Get the window width
    pub fn width(&self) -> u32 {
        self.window.width()
    }

    /// Get the window height
    pub fn height(&self) -> u32 {
        self.window.height()
    }

    /// Get the current X position
    pub fn x(&self) -> i32 {
        self.window.x()
    }

    /// Get the current Y position
    pub fn y(&self) -> i32 {
        self.window.y()
    }

    /// Poll for events (non-blocking), returns false if should close
    pub fn poll_events(&mut self) -> bool {
        self.window.poll_events()
    }

    /// Check if position/size changed since last check
    pub fn take_position_dirty(&mut self) -> bool {
        self.window.take_position_dirty()
    }

    /// Check if currently in interactive mode (move mode)
    pub fn is_interactive(&self) -> bool {
        self.window.is_interactive()
    }

    /// Check if in move mode (interactive AND drag enabled)
    /// This is the state where overlays show preview content and can be repositioned
    pub fn is_in_move_mode(&self) -> bool {
        self.window.is_interactive() && self.window.is_drag_enabled()
    }

    /// Check if pointer is in the resize corner
    pub fn in_resize_corner(&self) -> bool {
        self.window.in_resize_corner()
    }

    /// Check if currently resizing
    pub fn is_resizing(&self) -> bool {
        self.window.is_resizing()
    }

    /// Enable or disable click-through mode
    pub fn set_click_through(&mut self, enabled: bool) {
        self.window.set_click_through(enabled);
    }

    /// Keep one sub-rectangle clickable while the rest stays click-through
    pub fn set_interactive_region(&mut self, region: Option<(i32, i32, u32, u32)>) {
        self.window.set_interactive_region(region);
    }

    /// Enable or disable window dragging when interactive
    pub fn set_drag_enabled(&mut self, enabled: bool) {
        self.window.set_drag_enabled(enabled);
    }

    /// Check if dragging is enabled
    pub fn is_drag_enabled(&self) -> bool {
        self.window.is_drag_enabled()
    }

    /// Take a pending click position (if any)
    pub fn take_pending_click(&mut self) -> Option<(f32, f32)> {
        self.window.take_pending_click()
    }

    /// Set the window position
    pub fn set_position(&mut self, x: i32, y: i32) {
        self.window.set_position(x, y);
    }

    /// Set the window size
    pub fn set_size(&mut self, width: u32, height: u32) {
        self.window.set_size(width, height);
    }
}
