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
use crate::utils::color_from_rgba;
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
        let width = self.window.width() as f32;
        let height = self.window.height() as f32;
        let corner_radius = self.scaled(6.0);
        let in_move_mode = self.window.is_interactive() && self.window.is_drag_enabled();

        // Clear with transparent
        self.window.clear(colors::transparent());

        // Draw background only if alpha > 0 (fully transparent overlays skip this)
        if self.background_alpha > 0 {
            // In move mode, use reduced alpha (20%) to make overlay semi-transparent
            let alpha = if in_move_mode {
                (self.background_alpha as f32 * 0.20).round() as u8
            } else {
                self.background_alpha
            };
            let bg_color = Color::from_rgba8(30, 30, 30, alpha);
            self.window
                .fill_rounded_rect(0.0, 0.0, width, height, corner_radius, bg_color);
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
            if let Some(ref label) = self.label {
                let font_size = self.scaled(12.0).max(10.0);
                let label_color = Color::from_rgba8(180, 180, 180, 200);
                let (text_width, text_height) = self.window.measure_text(label, font_size);
                let x = (width - text_width) / 2.0;
                let y = (height + text_height) / 2.0; // baseline-centered
                self.window.draw_text(label, x, y, font_size, label_color);
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
        let indicator_size = self.scaled(12.0).max(12.0);
        let corner_x = width - indicator_size - 4.0;
        let corner_y = height - indicator_size - 4.0;

        let highlight = if self.window.is_resizing() {
            colors::white()
        } else {
            colors::resize_indicator()
        };

        // Draw diagonal grip lines
        for i in 0..3 {
            let offset = i as f32 * 4.0;
            self.window.fill_rect(
                corner_x + offset,
                corner_y + indicator_size - 2.0,
                indicator_size - offset,
                2.0,
                highlight,
            );
            self.window.fill_rect(
                corner_x + indicator_size - 2.0,
                corner_y + offset,
                2.0,
                indicator_size - offset,
                highlight,
            );
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Drawing helpers (delegate to window)
    // ─────────────────────────────────────────────────────────────────────────

    /// Draw text at the specified position
    pub fn draw_text(&mut self, text: &str, x: f32, y: f32, font_size: f32, color: Color) {
        self.window.draw_text(text, x, y, font_size, color);
    }

    /// Draw text with color from RGBA array
    pub fn draw_text_rgba(&mut self, text: &str, x: f32, y: f32, font_size: f32, rgba: [u8; 4]) {
        self.window
            .draw_text(text, x, y, font_size, color_from_rgba(rgba));
    }

    /// Measure text dimensions
    pub fn measure_text(&mut self, text: &str, font_size: f32) -> (f32, f32) {
        self.window.measure_text(text, font_size)
    }

    /// Draw a filled rectangle
    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color) {
        self.window.fill_rect(x, y, w, h, color);
    }

    /// Draw a filled rounded rectangle
    pub fn fill_rounded_rect(&mut self, x: f32, y: f32, w: f32, h: f32, radius: f32, color: Color) {
        self.window.fill_rounded_rect(x, y, w, h, radius, color);
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
