//! Wayland platform implementation using layer-shell protocol
//!
//! This provides overlay windows on Wayland compositors that support
//! the wlr-layer-shell protocol (wlroots-based compositors like Hyprland, Sway, etc.)

use std::os::fd::AsFd;

use rustix::fs::{MemfdFlags, memfd_create};
use rustix::mm::{MapFlags, ProtFlags, mmap};
use wayland_client::globals::GlobalListContents;
use wayland_client::protocol::wl_buffer::WlBuffer;
use wayland_client::protocol::wl_compositor::WlCompositor;
use wayland_client::protocol::wl_pointer::{self, WlPointer};
use wayland_client::protocol::wl_region::WlRegion;
use wayland_client::protocol::wl_registry;
use wayland_client::protocol::wl_seat::{self, WlSeat};
use wayland_client::protocol::wl_shm::{self, Format, WlShm};
use wayland_client::protocol::wl_shm_pool::WlShmPool;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle};
use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::ZwlrLayerShellV1,
    zwlr_layer_surface_v1::{self, Anchor, KeyboardInteractivity, ZwlrLayerSurfaceV1},
};

use super::{OverlayConfig, OverlayPlatform, PlatformError};

/// Wayland overlay implementation
pub struct WaylandOverlay {
    config: OverlayConfig,
    connection: Connection,
    event_queue: EventQueue<WaylandState>,
    state: WaylandState,
    qh: QueueHandle<WaylandState>,
}

/// Internal state for Wayland event handling
struct WaylandState {
    running: bool,
    configured: bool,
    width: u32,
    height: u32,

    // Wayland objects
    compositor: Option<WlCompositor>,
    surface: Option<WlSurface>,
    layer_surface: Option<ZwlrLayerSurfaceV1>,
    shm: Option<WlShm>,
    buffer: Option<WlBuffer>,
    seat: Option<WlSeat>,
    pointer: Option<WlPointer>,

    // Pixel buffer (RGBA format for rendering, converted to ARGB for Wayland)
    pixel_data: Vec<u8>,
    shm_data: Option<ShmBuffer>,

    // Drag/resize state
    pointer_x: f64,
    pointer_y: f64,
    is_dragging: bool,
    is_resizing: bool,
    in_resize_corner: bool, // true when pointer is in resize corner (for visual feedback)
    window_x: i32,
    window_y: i32,
    // Track pending dimensions during resize (separate from actual state.width/height)
    pending_width: u32,
    pending_height: u32,
    position_dirty: bool,
    pending_resize: Option<(u32, u32)>, // (width, height) - applied on release

    // Mode tracking for optimization
    click_through: bool,
}

/// Overlay size constraints
const MIN_OVERLAY_SIZE: u32 = 100;
const MAX_OVERLAY_WIDTH: u32 = 300;
const MAX_OVERLAY_HEIGHT: u32 = 700;

/// Resize corner detection
struct ResizeCorner;

impl ResizeCorner {
    const CORNER_SIZE: f64 = 20.0; // pixels from bottom-right corner

    /// Check if position is in the bottom-right resize corner
    fn is_in_corner(x: f64, y: f64, width: u32, height: u32) -> bool {
        x > (width as f64 - Self::CORNER_SIZE) && y > (height as f64 - Self::CORNER_SIZE)
    }
}

struct ShmBuffer {
    ptr: *mut u8,
    size: usize,
}

// SAFETY: We only access shm_data from the main thread
unsafe impl Send for ShmBuffer {}

impl WaylandState {
    fn new(width: u32, height: u32, x: i32, y: i32, click_through: bool) -> Self {
        let pixel_count = (width * height) as usize;
        Self {
            running: true,
            configured: false,
            width,
            height,
            compositor: None,
            surface: None,
            layer_surface: None,
            shm: None,
            buffer: None,
            seat: None,
            pointer: None,
            pixel_data: vec![0u8; pixel_count * 4],
            shm_data: None,
            pointer_x: 0.0,
            pointer_y: 0.0,
            is_dragging: false,
            is_resizing: false,
            in_resize_corner: false,
            window_x: x,
            window_y: y,
            pending_width: width,
            pending_height: height,
            position_dirty: false,
            pending_resize: None,
            click_through,
        }
    }

    /// Update position directly - called from event handler
    fn update_position(&mut self, x: i32, y: i32) {
        self.window_x = x;
        self.window_y = y;
        if let Some(layer_surface) = &self.layer_surface {
            layer_surface.set_margin(y, 0, 0, x);
        }
        if let Some(surface) = &self.surface {
            surface.commit();
        }
        self.position_dirty = true;
    }

    fn create_shm_buffer(&mut self, qh: &QueueHandle<WaylandState>) {
        let shm = match &self.shm {
            Some(s) => s,
            None => return,
        };

        // Clean up old buffer first
        if let Some(old_buffer) = self.buffer.take() {
            old_buffer.destroy();
        }
        if let Some(old_shm) = self.shm_data.take() {
            unsafe {
                rustix::mm::munmap(old_shm.ptr as *mut _, old_shm.size).ok();
            }
        }

        let stride = self.width * 4;
        let size = (stride * self.height) as usize;

        // Create anonymous shared memory
        let fd = memfd_create(c"baras-overlay-buffer", MemfdFlags::CLOEXEC)
            .expect("Failed to create memfd");

        rustix::fs::ftruncate(&fd, size as u64).expect("Failed to set memfd size");

        // Memory map it
        let ptr = unsafe {
            mmap(
                std::ptr::null_mut(),
                size,
                ProtFlags::READ | ProtFlags::WRITE,
                MapFlags::SHARED,
                fd.as_fd(),
                0,
            )
            .expect("Failed to mmap")
        };

        self.shm_data = Some(ShmBuffer {
            ptr: ptr as *mut u8,
            size,
        });

        // Create wayland shm pool and buffer
        let pool = shm.create_pool(fd.as_fd(), size as i32, qh, ());
        self.buffer = Some(pool.create_buffer(
            0,
            self.width as i32,
            self.height as i32,
            stride as i32,
            Format::Argb8888,
            qh,
            (),
        ));
    }

    fn copy_pixels_to_shm(&mut self) {
        let shm = match &self.shm_data {
            Some(s) => s,
            None => return,
        };

        let shm_slice = unsafe { std::slice::from_raw_parts_mut(shm.ptr, shm.size) };

        // Convert RGBA to BGRA (Wayland ARGB8888 is BGRA in little-endian)
        for (i, chunk) in self.pixel_data.chunks(4).enumerate() {
            let offset = i * 4;
            if offset + 3 < shm_slice.len() && chunk.len() == 4 {
                shm_slice[offset] = chunk[2]; // B
                shm_slice[offset + 1] = chunk[1]; // G
                shm_slice[offset + 2] = chunk[0]; // R
                shm_slice[offset + 3] = chunk[3]; // A
            }
        }
    }

    fn commit_frame(&self) {
        if let (Some(surface), Some(buffer)) = (&self.surface, &self.buffer) {
            surface.attach(Some(buffer), 0, 0);
            surface.damage_buffer(0, 0, self.width as i32, self.height as i32);
            surface.commit();
        }
    }
}

impl OverlayPlatform for WaylandOverlay {
    fn new(config: OverlayConfig) -> Result<Self, PlatformError> {
        let connection = Connection::connect_to_env()
            .map_err(|e| PlatformError::ConnectionFailed(e.to_string()))?;

        let (globals, event_queue) =
            wayland_client::globals::registry_queue_init::<WaylandState>(&connection)
                .map_err(|e| PlatformError::ConnectionFailed(e.to_string()))?;

        let qh = event_queue.handle();
        let mut state = WaylandState::new(config.width, config.height, config.x, config.y, config.click_through);

        // Bind globals
        let _registry = connection.display().get_registry(&qh, ());

        let compositor: WlCompositor = globals
            .bind(&qh, 4..=6, ())
            .map_err(|_| PlatformError::UnsupportedFeature("wl_compositor".to_string()))?;

        let layer_shell: ZwlrLayerShellV1 = globals
            .bind(&qh, 1..=4, ())
            .map_err(|_| PlatformError::UnsupportedFeature("zwlr_layer_shell_v1".to_string()))?;

        let shm: WlShm = globals
            .bind(&qh, 1..=1, ())
            .map_err(|_| PlatformError::UnsupportedFeature("wl_shm".to_string()))?;

        state.shm = Some(shm);

        // Bind seat for pointer input (stored for later use when toggling modes)
        let seat: WlSeat = globals
            .bind(&qh, 1..=9, ())
            .map_err(|_| PlatformError::UnsupportedFeature("wl_seat".to_string()))?;
        state.seat = Some(seat);

        // Only create pointer if interactive (not click-through)
        // This saves memory/CPU when overlay is locked
        if !config.click_through {
            if let Some(seat) = &state.seat {
                let pointer = seat.get_pointer(&qh, ());
                state.pointer = Some(pointer);
            }
        }

        // Create surface
        let surface = compositor.create_surface(&qh, ());
        let layer_surface = layer_shell.get_layer_surface(
            &surface,
            None,
            wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::Layer::Overlay,
            config.namespace.clone(),
            &qh,
            (),
        );

        // Set up click-through if requested
        if config.click_through {
            let region = compositor.create_region(&qh, ());
            surface.set_input_region(Some(&region));
        }

        // Configure layer surface
        layer_surface.set_anchor(Anchor::Top | Anchor::Left);
        layer_surface.set_margin(config.y, 0, 0, config.x);
        layer_surface.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer_surface.set_size(config.width, config.height);
        surface.commit();

        state.compositor = Some(compositor);
        state.surface = Some(surface);
        state.layer_surface = Some(layer_surface);

        // Create shared memory buffer
        state.create_shm_buffer(&qh);

        let mut overlay = Self {
            config,
            connection,
            event_queue,
            state,
            qh,
        };

        // Wait for the initial configure event before returning
        // This is required by the layer-shell protocol
        while !overlay.state.configured {
            overlay.event_queue.blocking_dispatch(&mut overlay.state)
                .map_err(|e| PlatformError::ConnectionFailed(e.to_string()))?;
        }

        Ok(overlay)
    }

    fn width(&self) -> u32 {
        self.state.width
    }

    fn height(&self) -> u32 {
        self.state.height
    }

    fn set_position(&mut self, x: i32, y: i32) {
        self.config.x = x;
        self.config.y = y;
        self.state.update_position(x, y);
        let _ = self.connection.flush();
    }

    fn set_size(&mut self, width: u32, height: u32) {
        if width == self.state.width && height == self.state.height {
            return;
        }

        self.config.width = width;
        self.config.height = height;
        self.state.width = width;
        self.state.height = height;

        // Resize pixel buffer
        let pixel_count = (width * height) as usize;
        self.state.pixel_data.resize(pixel_count * 4, 0);

        // Recreate shm buffer
        self.state.create_shm_buffer(&self.qh);

        if let Some(layer_surface) = &self.state.layer_surface {
            layer_surface.set_size(width, height);
        }

        // Update input region if click-through is disabled (interactive mode)
        if !self.config.click_through {
            if let (Some(compositor), Some(surface)) = (&self.state.compositor, &self.state.surface) {
                let region = compositor.create_region(&self.qh, ());
                region.add(0, 0, width as i32, height as i32);
                surface.set_input_region(Some(&region));
            }
        }

        if let Some(surface) = &self.state.surface {
            surface.commit();
        }
    }

    fn set_click_through(&mut self, enabled: bool) {
        self.config.click_through = enabled;
        self.state.click_through = enabled;

        if let (Some(compositor), Some(surface)) = (&self.state.compositor, &self.state.surface) {
            let region = compositor.create_region(&self.qh, ());
            if !enabled {
                // Full surface is interactive
                region.add(0, 0, self.state.width as i32, self.state.height as i32);
            }
            surface.set_input_region(Some(&region));
            surface.commit();
        }

        // Manage pointer lifecycle based on mode
        if enabled {
            // Locked mode: release pointer to save resources
            if let Some(pointer) = self.state.pointer.take() {
                pointer.release();
            }
            // Reset interaction state
            self.state.is_dragging = false;
            self.state.is_resizing = false;
            self.state.in_resize_corner = false;
        } else {
            // Interactive mode: acquire pointer if we don't have one
            if self.state.pointer.is_none() {
                if let Some(seat) = &self.state.seat {
                    let pointer = seat.get_pointer(&self.qh, ());
                    self.state.pointer = Some(pointer);
                }
            }
        }
    }

    fn in_resize_corner(&self) -> bool {
        self.state.in_resize_corner
    }

    fn is_resizing(&self) -> bool {
        self.state.is_resizing
    }

    fn pending_size(&self) -> Option<(u32, u32)> {
        if self.state.is_resizing {
            Some((self.state.pending_width, self.state.pending_height))
        } else {
            None
        }
    }

    fn is_interactive(&self) -> bool {
        !self.state.click_through
    }

    fn pixel_buffer(&mut self) -> Option<&mut [u8]> {
        Some(&mut self.state.pixel_data)
    }

    fn commit(&mut self) {
        self.state.copy_pixels_to_shm();
        self.state.commit_frame();
    }

    fn poll_events(&mut self) -> bool {
        // Flush outgoing requests first
        if self.connection.flush().is_err() {
            return false;
        }

        // Read and dispatch all available events in a tight loop
        loop {
            // Try to read events from the socket
            if let Some(guard) = self.event_queue.prepare_read() {
                match guard.read() {
                    Ok(0) => break, // No events available
                    Ok(_) => {}     // Events read, continue
                    Err(_) => break, // Error or would block
                }
            } else {
                // Events already pending, dispatch them
            }

            // Dispatch all pending events
            match self.event_queue.dispatch_pending(&mut self.state) {
                Ok(0) => break, // No events dispatched
                Ok(_) => {}     // Events dispatched, check for more
                Err(_) => return false,
            }
        }

        // Apply pending resize immediately for real-time feedback
        if let Some((width, height)) = self.state.pending_resize.take() {
            self.set_size(width, height);
            // Update pending dimensions to match actual size (for next delta calculation)
            self.state.pending_width = width;
            self.state.pending_height = height;
        }

        // Final flush if any position updates happened
        if self.state.position_dirty {
            let _ = self.connection.flush();
            self.state.position_dirty = false;
        }

        self.state.running
    }

    fn run<F>(&mut self, mut render_callback: F)
    where
        F: FnMut(&mut Self),
    {
        while self.state.running {
            // Block waiting for events
            if self.event_queue.blocking_dispatch(&mut self.state).is_err() {
                break;
            }

            if self.state.configured {
                render_callback(self);
            }
        }
    }
}

// --- Wayland Dispatch implementations ---

impl Dispatch<wl_registry::WlRegistry, ()> for WaylandState {
    fn event(
        _state: &mut Self,
        _proxy: &wl_registry::WlRegistry,
        _event: wl_registry::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for WaylandState {
    fn event(
        _state: &mut Self,
        _proxy: &wl_registry::WlRegistry,
        _event: wl_registry::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlCompositor, ()> for WaylandState {
    fn event(
        _state: &mut Self,
        _proxy: &WlCompositor,
        _event: wayland_client::protocol::wl_compositor::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlSurface, ()> for WaylandState {
    fn event(
        _state: &mut Self,
        _proxy: &WlSurface,
        _event: wayland_client::protocol::wl_surface::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlRegion, ()> for WaylandState {
    fn event(
        _state: &mut Self,
        _proxy: &WlRegion,
        _event: wayland_client::protocol::wl_region::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlShm, ()> for WaylandState {
    fn event(
        _state: &mut Self,
        _proxy: &WlShm,
        _event: wl_shm::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlShmPool, ()> for WaylandState {
    fn event(
        _state: &mut Self,
        _proxy: &WlShmPool,
        _event: wayland_client::protocol::wl_shm_pool::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlBuffer, ()> for WaylandState {
    fn event(
        _state: &mut Self,
        _proxy: &WlBuffer,
        _event: wayland_client::protocol::wl_buffer::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrLayerShellV1, ()> for WaylandState {
    fn event(
        _state: &mut Self,
        _proxy: &ZwlrLayerShellV1,
        _event: wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrLayerSurfaceV1, ()> for WaylandState {
    fn event(
        state: &mut Self,
        proxy: &ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_layer_surface_v1::Event::Configure {
                serial,
                width,
                height,
            } => {
                proxy.ack_configure(serial);

                if width > 0 && height > 0 {
                    state.width = width;
                    state.height = height;
                }

                state.configured = true;
            }
            zwlr_layer_surface_v1::Event::Closed => {
                state.running = false;
            }
            _ => {}
        }
    }
}

impl Dispatch<WlSeat, ()> for WaylandState {
    fn event(
        _state: &mut Self,
        _proxy: &WlSeat,
        _event: wl_seat::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // We don't need to handle seat events for basic pointer functionality
    }
}

impl Dispatch<WlPointer, ()> for WaylandState {
    fn event(
        state: &mut Self,
        _proxy: &WlPointer,
        event: wl_pointer::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Enter { surface_x, surface_y, .. } => {
                state.pointer_x = surface_x;
                state.pointer_y = surface_y;
                // Check if in resize corner for visual feedback
                state.in_resize_corner = ResizeCorner::is_in_corner(
                    surface_x, surface_y, state.width, state.height
                );
            }
            wl_pointer::Event::Motion { surface_x, surface_y, .. } => {
                let delta_x = surface_x - state.pointer_x;
                let delta_y = surface_y - state.pointer_y;

                // Only update resize corner state when not actively resizing
                // (during resize, keep it true so grip stays visible)
                if !state.is_resizing {
                    state.in_resize_corner = ResizeCorner::is_in_corner(
                        surface_x, surface_y, state.width, state.height
                    );
                }

                if state.is_resizing {
                    // Bottom-right corner resize - apply immediately for real-time feedback
                    let new_width = state.pending_width as i32 + delta_x as i32;
                    let new_height = state.pending_height as i32 + delta_y as i32;

                    // Clamp to min/max size constraints
                    let clamped_width = (new_width as u32)
                        .max(MIN_OVERLAY_SIZE)
                        .min(MAX_OVERLAY_WIDTH);
                    let clamped_height = (new_height as u32)
                        .max(MIN_OVERLAY_SIZE)
                        .min(MAX_OVERLAY_HEIGHT);

                    // Only update if within valid range
                    if new_width > 0 && new_height > 0 {
                        state.pending_width = clamped_width;
                        state.pending_height = clamped_height;
                        state.pending_resize = Some((clamped_width, clamped_height));
                    }
                } else if state.is_dragging {
                    // Move window
                    let new_x = state.window_x + delta_x as i32;
                    let new_y = state.window_y + delta_y as i32;
                    state.update_position(new_x, new_y);
                }

                // Always update pointer position for next delta calculation
                state.pointer_x = surface_x;
                state.pointer_y = surface_y;
            }
            wl_pointer::Event::Button { button, state: button_state, .. } => {
                use wayland_client::WEnum;
                // Button 272 is left mouse button (BTN_LEFT)
                if button == 272 {
                    match button_state {
                        WEnum::Value(wl_pointer::ButtonState::Pressed) => {
                            // Check if in bottom-right corner for resize
                            if ResizeCorner::is_in_corner(
                                state.pointer_x, state.pointer_y,
                                state.width, state.height
                            ) {
                                state.is_resizing = true;
                                state.pending_width = state.width;
                                state.pending_height = state.height;
                            } else {
                                state.is_dragging = true;
                            }
                        }
                        WEnum::Value(wl_pointer::ButtonState::Released) => {
                            state.is_dragging = false;
                            state.is_resizing = false;
                            // Recalculate corner state based on current pointer position
                            // and the potentially new window dimensions
                            state.in_resize_corner = ResizeCorner::is_in_corner(
                                state.pointer_x, state.pointer_y,
                                state.pending_width, state.pending_height
                            );
                        }
                        _ => {}
                    }
                }
            }
            wl_pointer::Event::Leave { .. } => {
                // Only reset corner state if not actively resizing
                // (during resize, keep grip visible)
                if !state.is_resizing {
                    state.in_resize_corner = false;
                }
                // Don't cancel drag/resize on leave - user might move fast
            }
            _ => {}
        }
    }
}
