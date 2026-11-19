//! Overlay window management
//!
//! Provides the OverlayWindow type which wraps platform-specific windows
//! with a high-level rendering API.
#![allow(clippy::too_many_arguments)]

use crate::platform::{MonitorInfo, NativeOverlay, OverlayConfig, OverlayPlatform, PlatformError};
use crate::renderer::Renderer;
use tiny_skia::Color;

/// A managed overlay window with its own renderer
pub struct OverlayWindow {
    platform: NativeOverlay,
    renderer: Renderer,
}

impl OverlayWindow {
    /// Create a new overlay window
    pub fn new(config: OverlayConfig) -> Result<Self, PlatformError> {
        let platform = NativeOverlay::new(config)?;
        let renderer = Renderer::new();

        Ok(Self { platform, renderer })
    }

    /// Get the window width
    pub fn width(&self) -> u32 {
        self.platform.width()
    }

    /// Get the window height
    pub fn height(&self) -> u32 {
        self.platform.height()
    }

    /// Get the current X position
    pub fn x(&self) -> i32 {
        self.platform.x()
    }

    /// Get the current Y position
    pub fn y(&self) -> i32 {
        self.platform.y()
    }

    /// Check if position has changed since last check (clears the dirty flag)
    pub fn take_position_dirty(&mut self) -> bool {
        self.platform.take_position_dirty()
    }

    /// Set the window position
    pub fn set_position(&mut self, x: i32, y: i32) {
        self.platform.set_position(x, y);
    }

    /// Set the window size
    pub fn set_size(&mut self, width: u32, height: u32) {
        self.platform.set_size(width, height);
    }

    /// Enable or disable click-through mode
    pub fn set_click_through(&mut self, enabled: bool) {
        self.platform.set_click_through(enabled);
    }

    /// Keep one sub-rectangle clickable while the rest stays click-through
    pub fn set_interactive_region(&mut self, region: Option<(i32, i32, u32, u32)>) {
        self.platform.set_interactive_region(region);
    }

    /// Enable or disable window dragging when interactive
    pub fn set_drag_enabled(&mut self, enabled: bool) {
        self.platform.set_drag_enabled(enabled);
    }

    /// Check if dragging is enabled
    pub fn is_drag_enabled(&self) -> bool {
        self.platform.is_drag_enabled()
    }

    /// Take a pending click position (if any)
    pub fn take_pending_click(&mut self) -> Option<(f32, f32)> {
        self.platform.take_pending_click()
    }

    /// Set the font family used for all text rendering on this window
    pub fn set_font_family(&mut self, family: &str) {
        self.renderer.set_font_family(family);
    }

    /// Clear the overlay with a color
    pub fn clear(&mut self, color: Color) {
        let width = self.platform.width();
        let height = self.platform.height();
        if let Some(buffer) = self.platform.pixel_buffer() {
            self.renderer.clear(buffer, width, height, color);
        }
    }

    /// Draw a filled rectangle
    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color) {
        let width = self.platform.width();
        let height = self.platform.height();
        if let Some(buffer) = self.platform.pixel_buffer() {
            self.renderer.fill_rect(buffer, width, height, x, y, w, h, color);
        }
    }

    /// Draw a filled diamond centered at (cx, cy)
    pub fn fill_diamond(&mut self, cx: f32, cy: f32, radius: f32, color: Color) {
        let width = self.platform.width();
        let height = self.platform.height();
        if let Some(buffer) = self.platform.pixel_buffer() {
            self.renderer.fill_diamond(buffer, width, height, cx, cy, radius, color);
        }
    }

    /// Draw a filled rounded rectangle
    pub fn fill_rounded_rect(&mut self, x: f32, y: f32, w: f32, h: f32, radius: f32, color: Color) {
        let width = self.platform.width();
        let height = self.platform.height();
        if let Some(buffer) = self.platform.pixel_buffer() {
            self.renderer
                .fill_rounded_rect(buffer, width, height, x, y, w, h, radius, color);
        }
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
        let width = self.platform.width();
        let height = self.platform.height();
        if let Some(buffer) = self.platform.pixel_buffer() {
            self.renderer
                .fill_corner_rounded_rect(buffer, width, height, x, y, w, h, top_radius, bottom_radius, color);
        }
    }

    /// Draw a filled rounded rectangle with a horizontal linear gradient.
    /// `grad_x0`/`grad_x1` set the gradient span independently of the rect.
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
        let width = self.platform.width();
        let height = self.platform.height();
        if let Some(buffer) = self.platform.pixel_buffer() {
            self.renderer.fill_rounded_rect_gradient(
                buffer,
                width,
                height,
                x,
                y,
                w,
                h,
                radius,
                grad_x0,
                grad_x1,
                start_color,
                end_color,
            );
        }
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
        let width = self.platform.width();
        let height = self.platform.height();
        if let Some(buffer) = self.platform.pixel_buffer() {
            self.renderer
                .stroke_rounded_rect(buffer, width, height, x, y, w, h, radius, stroke_width, color);
        }
    }

    /// Stroke an open folder-tab outline (top + sides, open bottom)
    pub fn stroke_tab_outline(&mut self, x: f32, y: f32, w: f32, h: f32, radius: f32, stroke_width: f32, color: Color) {
        let width = self.platform.width();
        let height = self.platform.height();
        if let Some(buffer) = self.platform.pixel_buffer() {
            self.renderer
                .stroke_tab_outline(buffer, width, height, x, y, w, h, radius, stroke_width, color);
        }
    }

    /// Draw a dashed rounded rectangle outline
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
        let width = self.platform.width();
        let height = self.platform.height();
        if let Some(buffer) = self.platform.pixel_buffer() {
            self.renderer.stroke_rounded_rect_dashed(
                buffer,
                width,
                height,
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
    }

    /// Draw text at the specified position
    pub fn draw_text(&mut self, text: &str, x: f32, y: f32, font_size: f32, color: Color) {
        let width = self.platform.width();
        let height = self.platform.height();
        if let Some(buffer) = self.platform.pixel_buffer() {
            self.renderer
                .draw_text(buffer, width, height, text, x, y, font_size, color);
        }
    }

    /// Draw text at the specified position with bold/italic styling
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
        let width = self.platform.width();
        let height = self.platform.height();
        if let Some(buffer) = self.platform.pixel_buffer() {
            self.renderer
                .draw_text_styled(buffer, width, height, text, x, y, font_size, color, bold, italic);
        }
    }

    /// Measure text dimensions
    pub fn measure_text(&mut self, text: &str, font_size: f32) -> (f32, f32) {
        self.renderer.measure_text(text, font_size)
    }

    /// Measure text dimensions with style options
    pub fn measure_text_styled(&mut self, text: &str, font_size: f32, bold: bool, italic: bool) -> (f32, f32) {
        self.renderer.measure_text_styled(text, font_size, bold, italic)
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
        let width = self.platform.width();
        let height = self.platform.height();
        if let Some(buffer) = self.platform.pixel_buffer() {
            self.renderer.draw_image(
                buffer,
                width,
                height,
                image_data,
                image_width,
                image_height,
                dest_x,
                dest_y,
                dest_width,
                dest_height,
            );
        }
    }

    /// Commit the current frame to the screen
    pub fn commit(&mut self) {
        self.platform.commit();
    }

    /// Copy the final premultiplied RGBA pixels for non-window consumers.
    pub fn snapshot_rgba(&mut self) -> Option<Vec<u8>> {
        self.platform.pixel_buffer().map(|pixels| pixels.to_vec())
    }

    /// Poll for events (non-blocking)
    /// Returns false if the window should close
    pub fn poll_events(&mut self) -> bool {
        self.platform.poll_events()
    }

    /// Check if pointer is in the resize corner
    pub fn in_resize_corner(&self) -> bool {
        self.platform.in_resize_corner()
    }

    /// Check if currently resizing
    pub fn is_resizing(&self) -> bool {
        self.platform.is_resizing()
    }

    /// Get pending resize dimensions (if resizing)
    pub fn pending_size(&self) -> Option<(u32, u32)> {
        self.platform.pending_size()
    }

    /// Check if overlay is in interactive mode (not click-through)
    pub fn is_interactive(&self) -> bool {
        self.platform.is_interactive()
    }

    /// Get the monitor that contains the overlay's current position
    pub fn current_monitor(&self) -> Option<MonitorInfo> {
        self.platform.current_monitor()
    }

    /// Run the window event loop with a render callback
    pub fn run<F>(&mut self, mut render_callback: F)
    where
        F: FnMut(&mut Self),
    {
        while self.poll_events() {
            render_callback(self);
        }
    }
}
