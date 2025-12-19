//! Overlay window management
//!
//! Provides the OverlayWindow type which wraps platform-specific windows
//! with a high-level rendering API.

use crate::platform::{NativeOverlay, OverlayConfig, OverlayPlatform, PlatformError};
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
            self.renderer
                .fill_rect(buffer, width, height, x, y, w, h, color);
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
            self.renderer.stroke_rounded_rect(
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

    /// Measure text dimensions
    pub fn measure_text(&mut self, text: &str, font_size: f32) -> (f32, f32) {
        self.renderer.measure_text(text, font_size)
    }

    /// Commit the current frame to the screen
    pub fn commit(&mut self) {
        self.platform.commit();
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
