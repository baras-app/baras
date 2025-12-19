//! Platform abstraction for overlay windows
//!
//! This module defines the trait that all platform backends must implement,
//! allowing the overlay rendering code to be platform-agnostic.

#[cfg(all(unix, not(target_os = "macos")))]
pub mod wayland;

// TODO: Future platform implementations
// #[cfg(all(unix, not(target_os = "macos")))]
// pub mod x11;
// #[cfg(target_os = "windows")]
// pub mod windows;
// #[cfg(target_os = "macos")]
// pub mod macos;

/// Configuration for creating an overlay window
#[derive(Debug, Clone)]
pub struct OverlayConfig {
    /// Initial X position (from left edge of screen)
    pub x: i32,
    /// Initial Y position (from top edge of screen)
    pub y: i32,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Unique identifier for this overlay (used for window rules)
    pub namespace: String,
    /// Whether clicks pass through the overlay
    pub click_through: bool,
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self {
            x: 20,
            y: 20,
            width: 300,
            height: 150,
            namespace: "baras-overlay".to_string(),
            click_through: true,
        }
    }
}

/// Errors that can occur in platform operations
#[derive(Debug)]
pub enum PlatformError {
    /// Failed to connect to display server
    ConnectionFailed(String),
    /// Required protocol/feature not available
    UnsupportedFeature(String),
    /// Buffer/memory allocation failed
    BufferError(String),
    /// Generic platform error
    Other(String),
}

impl std::fmt::Display for PlatformError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlatformError::ConnectionFailed(s) => write!(f, "Connection failed: {}", s),
            PlatformError::UnsupportedFeature(s) => write!(f, "Unsupported feature: {}", s),
            PlatformError::BufferError(s) => write!(f, "Buffer error: {}", s),
            PlatformError::Other(s) => write!(f, "Platform error: {}", s),
        }
    }
}

impl std::error::Error for PlatformError {}

/// Trait that all platform backends must implement
pub trait OverlayPlatform: Sized {
    /// Create a new overlay window with the given configuration
    fn new(config: OverlayConfig) -> Result<Self, PlatformError>;

    /// Get the current width of the overlay
    fn width(&self) -> u32;

    /// Get the current height of the overlay
    fn height(&self) -> u32;

    /// Get the current X position
    fn x(&self) -> i32;

    /// Get the current Y position
    fn y(&self) -> i32;

    /// Check if position has changed since last check (clears the dirty flag)
    fn take_position_dirty(&mut self) -> bool;

    /// Update the overlay position
    fn set_position(&mut self, x: i32, y: i32);

    /// Resize the overlay
    fn set_size(&mut self, width: u32, height: u32);

    /// Enable or disable click-through mode
    fn set_click_through(&mut self, enabled: bool);

    /// Check if pointer is in the resize corner (for visual feedback)
    fn in_resize_corner(&self) -> bool;

    /// Check if currently resizing (for preview)
    fn is_resizing(&self) -> bool;

    /// Get pending resize dimensions during drag (for preview)
    fn pending_size(&self) -> Option<(u32, u32)>;

    /// Check if overlay is in interactive mode (not click-through)
    /// Callers can use this to adjust poll frequency - locked overlays need less frequent updates
    fn is_interactive(&self) -> bool;

    /// Get mutable access to the pixel buffer (RGBA format)
    /// Returns None if buffer is not ready
    fn pixel_buffer(&mut self) -> Option<&mut [u8]>;

    /// Commit the current pixel buffer to the screen
    fn commit(&mut self);

    /// Process pending platform events (non-blocking)
    /// Returns false if the overlay should close
    fn poll_events(&mut self) -> bool;

    /// Run the event loop, calling render_callback before each frame
    /// The callback receives mutable access to self and should:
    /// 1. Get pixel_buffer()
    /// 2. Render to it
    /// 3. Call commit()
    fn run<F>(&mut self, mut render_callback: F)
    where
        F: FnMut(&mut Self),
    {
        while self.poll_events() {
            render_callback(self);
        }
    }
}

/// Re-export the appropriate platform for the current target
#[cfg(all(unix, not(target_os = "macos")))]
pub use wayland::WaylandOverlay as NativeOverlay;

// TODO: Add re-exports for other platforms
// #[cfg(target_os = "windows")]
// pub use windows::WindowsOverlay as NativeOverlay;
// #[cfg(target_os = "macos")]
// pub use macos::MacOSOverlay as NativeOverlay;
