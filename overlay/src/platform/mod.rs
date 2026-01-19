//! Platform abstraction for overlay windows
//!
//! This module defines the trait that all platform backends must implement,
//! allowing the overlay rendering code to be platform-agnostic.
/// Size constraints for overlays
pub const MIN_OVERLAY_SIZE: u32 = 50;
pub const MAX_OVERLAY_WIDTH: u32 = 1280;
pub const MAX_OVERLAY_HEIGHT: u32 = 1024;
pub const RESIZE_CORNER_SIZE: i32 = 20;

#[cfg(all(unix, not(target_os = "macos")))]
pub mod wayland;

#[cfg(all(unix, not(target_os = "macos")))]
pub mod x11;

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "macos")]
pub mod macos;

/// Information about a connected monitor
#[derive(Debug, Clone)]
pub struct MonitorInfo {
    /// Unique identifier for this monitor (platform-specific)
    pub id: String,
    /// Human-readable name/description
    pub name: String,
    /// X position of the monitor in virtual screen space
    pub x: i32,
    /// Y position of the monitor in virtual screen space
    pub y: i32,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Whether this is the primary monitor
    pub is_primary: bool,
}

impl MonitorInfo {
    /// Check if a point is within this monitor's bounds
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x
            && x < self.x + self.width as i32
            && y >= self.y
            && y < self.y + self.height as i32
    }

    /// Check if a rectangle overlaps with this monitor
    pub fn overlaps(&self, x: i32, y: i32, width: u32, height: u32) -> bool {
        let rect_right = x + width as i32;
        let rect_bottom = y + height as i32;
        let mon_right = self.x + self.width as i32;
        let mon_bottom = self.y + self.height as i32;

        x < mon_right && rect_right > self.x && y < mon_bottom && rect_bottom > self.y
    }

    /// Convert absolute screen coordinates to relative monitor coordinates
    pub fn to_relative(&self, abs_x: i32, abs_y: i32) -> (i32, i32) {
        (abs_x - self.x, abs_y - self.y)
    }

    /// Convert relative monitor coordinates to absolute screen coordinates
    pub fn to_absolute(&self, rel_x: i32, rel_y: i32) -> (i32, i32) {
        (rel_x + self.x, rel_y + self.y)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Virtual Screen (Multi-Monitor) Utilities
// ─────────────────────────────────────────────────────────────────────────────

/// Bounding box of the virtual screen (all monitors combined)
#[derive(Debug, Clone, Copy)]
pub struct VirtualScreenBounds {
    pub min_x: i32,
    pub min_y: i32,
    pub max_x: i32,
    pub max_y: i32,
}

impl VirtualScreenBounds {
    /// Calculate the bounding box that encompasses all monitors
    pub fn from_monitors(monitors: &[MonitorInfo]) -> Option<Self> {
        if monitors.is_empty() {
            return None;
        }

        let mut min_x = i32::MAX;
        let mut min_y = i32::MAX;
        let mut max_x = i32::MIN;
        let mut max_y = i32::MIN;

        for m in monitors {
            min_x = min_x.min(m.x);
            min_y = min_y.min(m.y);
            max_x = max_x.max(m.x + m.width as i32);
            max_y = max_y.max(m.y + m.height as i32);
        }

        Some(Self {
            min_x,
            min_y,
            max_x,
            max_y,
        })
    }

    /// Clamp a window position to stay within the virtual screen bounds
    pub fn clamp_position(&self, x: i32, y: i32, width: u32, height: u32) -> (i32, i32) {
        let clamped_x = x.clamp(self.min_x, (self.max_x - width as i32).max(self.min_x));
        let clamped_y = y.clamp(self.min_y, (self.max_y - height as i32).max(self.min_y));
        (clamped_x, clamped_y)
    }
}

/// Clamp a window position to the virtual screen (all monitors combined).
/// This allows windows to be dragged freely across all connected monitors.
pub fn clamp_to_virtual_screen(
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    monitors: &[MonitorInfo],
) -> (i32, i32) {
    VirtualScreenBounds::from_monitors(monitors)
        .map(|bounds| bounds.clamp_position(x, y, width, height))
        .unwrap_or((x, y))
}

/// Find the monitor that contains the center of the given rectangle.
/// Falls back to the monitor with the most overlap, then primary, then first.
pub fn find_monitor_at(
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    monitors: &[MonitorInfo],
) -> Option<&MonitorInfo> {
    if monitors.is_empty() {
        return None;
    }

    // Calculate center point
    let center_x = x + (width as i32 / 2);
    let center_y = y + (height as i32 / 2);

    // First, try to find the monitor containing the center
    if let Some(m) = monitors.iter().find(|m| m.contains(center_x, center_y)) {
        return Some(m);
    }

    // Fall back to the monitor with the most overlap
    let mut best_monitor = None;
    let mut best_overlap = 0i64;

    for m in monitors {
        if m.overlaps(x, y, width, height) {
            // Calculate overlap area
            let overlap_x = (x + width as i32).min(m.x + m.width as i32) - x.max(m.x);
            let overlap_y = (y + height as i32).min(m.y + m.height as i32) - y.max(m.y);
            let overlap_area = (overlap_x.max(0) as i64) * (overlap_y.max(0) as i64);

            if overlap_area > best_overlap {
                best_overlap = overlap_area;
                best_monitor = Some(m);
            }
        }
    }

    if best_monitor.is_some() {
        return best_monitor;
    }

    // Fall back to primary or first monitor
    monitors.iter().find(|m| m.is_primary).or(monitors.first())
}

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
    /// Target monitor ID for multi-monitor support.
    /// On Wayland, this is used to select which output to render on.
    /// If None or not found, the compositor chooses (typically primary).
    pub target_monitor_id: Option<String>,
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
            target_monitor_id: None,
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

    /// Finalize resize after drag completes (updates buffers)
    /// Default is no-op since most platforms update buffers in set_size.
    /// macOS defers buffer updates to reduce tearing during live resize.
    fn finalize_size(&mut self) {}

    /// Enable or disable click-through mode
    fn set_click_through(&mut self, enabled: bool);

    /// Enable or disable window dragging when interactive
    ///
    /// When drag is disabled but click-through is also disabled, clicks are
    /// captured and reported via `take_pending_click()` instead of initiating
    /// a window drag. This is used for rearrange mode in raid overlays.
    fn set_drag_enabled(&mut self, enabled: bool);

    /// Check if dragging is enabled
    fn is_drag_enabled(&self) -> bool;

    /// Take a pending click position (if any)
    ///
    /// Returns the coordinates of the last click when drag is disabled.
    /// The click is consumed (subsequent calls return None until next click).
    fn take_pending_click(&mut self) -> Option<(f32, f32)>;

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

    /// Get information about all connected monitors
    fn get_monitors(&self) -> Vec<MonitorInfo>;

    /// Get the monitor that contains the overlay's current position
    /// Returns the primary monitor if the overlay is not on any monitor
    fn current_monitor(&self) -> Option<MonitorInfo> {
        let monitors = self.get_monitors();
        let x = self.x();
        let y = self.y();

        // Find the monitor containing the overlay's top-left corner
        monitors
            .iter()
            .find(|m| m.contains(x, y))
            .or_else(|| monitors.iter().find(|m| m.is_primary))
            .cloned()
    }

    /// Get the monitor ID for the overlay's current position
    fn current_monitor_id(&self) -> Option<String> {
        self.current_monitor().map(|m| m.id)
    }

    /// Clamp the overlay position to stay within the virtual screen bounds.
    /// This is called to re-apply clamping if needed (e.g., after resize).
    fn clamp_to_virtual_screen(&mut self) {
        // set_position handles clamping internally, so just re-set the current position
        let (x, y) = (self.x(), self.y());
        self.set_position(x, y);
    }

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
pub use linux::LinuxOverlay as NativeOverlay;

#[cfg(target_os = "windows")]
pub use windows::WindowsOverlay as NativeOverlay;

#[cfg(target_os = "macos")]
pub use macos::MacOSOverlay as NativeOverlay;

/// Get all connected monitors without requiring an existing overlay window.
/// This is useful for converting saved relative positions to absolute before spawning.
#[cfg(all(unix, not(target_os = "macos")))]
pub fn get_all_monitors() -> Vec<MonitorInfo> {
    linux::get_all_monitors()
}

#[cfg(target_os = "windows")]
pub fn get_all_monitors() -> Vec<MonitorInfo> {
    windows::get_all_monitors()
}

#[cfg(target_os = "macos")]
pub fn get_all_monitors() -> Vec<MonitorInfo> {
    macos::get_all_monitors()
}

// ─────────────────────────────────────────────────────────────────────────────
// Linux Runtime Detection (Wayland vs X11)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(all(unix, not(target_os = "macos")))]
mod linux {
    use super::*;

    /// Detect whether to use Wayland or X11 at runtime
    fn use_wayland() -> bool {
        std::env::var("WAYLAND_DISPLAY").is_ok()
    }

    /// Get all monitors using the appropriate backend
    pub fn get_all_monitors() -> Vec<MonitorInfo> {
        if use_wayland() {
            wayland::get_all_monitors()
        } else {
            x11::get_all_monitors()
        }
    }

    /// Linux overlay that wraps either Wayland or X11 backend
    pub enum LinuxOverlay {
        Wayland(wayland::WaylandOverlay),
        X11(x11::X11Overlay),
    }

    impl OverlayPlatform for LinuxOverlay {
        fn new(config: OverlayConfig) -> Result<Self, PlatformError> {
            if use_wayland() {
                wayland::WaylandOverlay::new(config).map(LinuxOverlay::Wayland)
            } else {
                x11::X11Overlay::new(config).map(LinuxOverlay::X11)
            }
        }

        fn width(&self) -> u32 {
            match self {
                Self::Wayland(w) => w.width(),
                Self::X11(x) => x.width(),
            }
        }

        fn height(&self) -> u32 {
            match self {
                Self::Wayland(w) => w.height(),
                Self::X11(x) => x.height(),
            }
        }

        fn x(&self) -> i32 {
            match self {
                Self::Wayland(w) => w.x(),
                Self::X11(x) => x.x(),
            }
        }

        fn y(&self) -> i32 {
            match self {
                Self::Wayland(w) => w.y(),
                Self::X11(x) => x.y(),
            }
        }

        fn take_position_dirty(&mut self) -> bool {
            match self {
                Self::Wayland(w) => w.take_position_dirty(),
                Self::X11(x) => x.take_position_dirty(),
            }
        }

        fn set_position(&mut self, x: i32, y: i32) {
            match self {
                Self::Wayland(w) => w.set_position(x, y),
                Self::X11(xo) => xo.set_position(x, y),
            }
        }

        fn set_size(&mut self, width: u32, height: u32) {
            match self {
                Self::Wayland(w) => w.set_size(width, height),
                Self::X11(x) => x.set_size(width, height),
            }
        }

        fn set_click_through(&mut self, enabled: bool) {
            match self {
                Self::Wayland(w) => w.set_click_through(enabled),
                Self::X11(x) => x.set_click_through(enabled),
            }
        }

        fn set_drag_enabled(&mut self, enabled: bool) {
            match self {
                Self::Wayland(w) => w.set_drag_enabled(enabled),
                Self::X11(x) => x.set_drag_enabled(enabled),
            }
        }

        fn is_drag_enabled(&self) -> bool {
            match self {
                Self::Wayland(w) => w.is_drag_enabled(),
                Self::X11(x) => x.is_drag_enabled(),
            }
        }

        fn take_pending_click(&mut self) -> Option<(f32, f32)> {
            match self {
                Self::Wayland(w) => w.take_pending_click(),
                Self::X11(x) => x.take_pending_click(),
            }
        }

        fn in_resize_corner(&self) -> bool {
            match self {
                Self::Wayland(w) => w.in_resize_corner(),
                Self::X11(x) => x.in_resize_corner(),
            }
        }

        fn is_resizing(&self) -> bool {
            match self {
                Self::Wayland(w) => w.is_resizing(),
                Self::X11(x) => x.is_resizing(),
            }
        }

        fn pending_size(&self) -> Option<(u32, u32)> {
            match self {
                Self::Wayland(w) => w.pending_size(),
                Self::X11(x) => x.pending_size(),
            }
        }

        fn is_interactive(&self) -> bool {
            match self {
                Self::Wayland(w) => w.is_interactive(),
                Self::X11(x) => x.is_interactive(),
            }
        }

        fn pixel_buffer(&mut self) -> Option<&mut [u8]> {
            match self {
                Self::Wayland(w) => w.pixel_buffer(),
                Self::X11(x) => x.pixel_buffer(),
            }
        }

        fn commit(&mut self) {
            match self {
                Self::Wayland(w) => w.commit(),
                Self::X11(x) => x.commit(),
            }
        }

        fn poll_events(&mut self) -> bool {
            match self {
                Self::Wayland(w) => w.poll_events(),
                Self::X11(x) => x.poll_events(),
            }
        }

        fn get_monitors(&self) -> Vec<MonitorInfo> {
            match self {
                Self::Wayland(w) => w.get_monitors(),
                Self::X11(x) => x.get_monitors(),
            }
        }
    }
}

/// Find a monitor by ID, or fall back to the primary monitor
pub fn find_monitor_by_id<'a>(
    monitors: &'a [MonitorInfo],
    id: Option<&str>,
) -> Option<&'a MonitorInfo> {
    if let Some(id) = id {
        // Try to find exact match
        if let Some(monitor) = monitors.iter().find(|m| m.id == id) {
            return Some(monitor);
        }
    }
    // Fall back to primary monitor
    monitors.iter().find(|m| m.is_primary).or(monitors.first())
}

/// Convert a relative position to absolute screen coordinates.
/// Uses the monitor_id to find the correct monitor, falling back to primary.
pub fn resolve_absolute_position(
    relative_x: i32,
    relative_y: i32,
    monitor_id: Option<&str>,
    monitors: &[MonitorInfo],
) -> (i32, i32) {
    if let Some(monitor) = find_monitor_by_id(monitors, monitor_id) {
        monitor.to_absolute(relative_x, relative_y)
    } else {
        // No monitors available, use position as-is (will likely be wrong but functional)
        (relative_x, relative_y)
    }
}
