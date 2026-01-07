//! Wayland platform implementation using layer-shell protocol
//!
//! This provides overlay windows on Wayland compositors that support
//! the wlr-layer-shell protocol (wlroots-based compositors like Hyprland, Sway, etc.)
//! Not GNOME though why are you trying to game on GNOME?
#![allow(clippy::too_many_arguments)]

use std::os::fd::AsFd;

use rustix::fs::{MemfdFlags, memfd_create};
use rustix::mm::{MapFlags, ProtFlags, mmap};
use wayland_client::globals::GlobalListContents;
use wayland_client::protocol::wl_buffer::WlBuffer;
use wayland_client::protocol::wl_compositor::WlCompositor;
use wayland_client::protocol::wl_output::{self, WlOutput};
use wayland_client::protocol::wl_pointer::{self, WlPointer};
use wayland_client::protocol::wl_region::WlRegion;
use wayland_client::protocol::wl_registry;
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::protocol::wl_shm::{Format, WlShm};
use wayland_client::protocol::wl_shm_pool::WlShmPool;
use wayland_client::protocol::wl_surface::{self, WlSurface};
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle};
use wayland_protocols::wp::relative_pointer::zv1::client::{
    zwp_relative_pointer_manager_v1::ZwpRelativePointerManagerV1,
    zwp_relative_pointer_v1::{self, ZwpRelativePointerV1},
};
use wayland_protocols::xdg::xdg_output::zv1::client::{
    zxdg_output_manager_v1::ZxdgOutputManagerV1,
    zxdg_output_v1::{self, ZxdgOutputV1},
};
use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::ZwlrLayerShellV1,
    zwlr_layer_surface_v1::{self, Anchor, KeyboardInteractivity, ZwlrLayerSurfaceV1},
};

use super::{MAX_OVERLAY_HEIGHT, MAX_OVERLAY_WIDTH, MIN_OVERLAY_SIZE, RESIZE_CORNER_SIZE};
use super::{MonitorInfo, OverlayConfig, OverlayPlatform, PlatformError};
// ─────────────────────────────────────────────────────────────────────────────
// Standalone Monitor Enumeration
// ─────────────────────────────────────────────────────────────────────────────

/// Minimal state for standalone monitor enumeration
struct MonitorEnumState {
    outputs: Vec<(WlOutput, OutputInfo)>,
    xdg_output_manager: Option<ZxdgOutputManagerV1>,
    xdg_outputs: Vec<ZxdgOutputV1>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for MonitorEnumState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _data: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "wl_output" => {
                    let output: WlOutput = registry.bind(name, version.min(4), qh, name);
                    state.outputs.push((
                        output,
                        OutputInfo {
                            name,
                            ..Default::default()
                        },
                    ));
                }
                "zxdg_output_manager_v1" => {
                    let manager: ZxdgOutputManagerV1 = registry.bind(name, version.min(3), qh, ());
                    state.xdg_output_manager = Some(manager);
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<WlOutput, u32> for MonitorEnumState {
    fn event(
        state: &mut Self,
        _output: &WlOutput,
        event: wl_output::Event,
        name: &u32,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // Find the output info by global name
        let Some((_, info)) = state.outputs.iter_mut().find(|(_, o)| o.name == *name) else {
            return;
        };

        match event {
            wl_output::Event::Geometry { x, y, model, .. } => {
                // Only use geometry position as fallback if xdg-output doesn't provide it
                // We don't override x,y if they're already set by xdg-output
                if info.xdg_logical_width == 0 {
                    info.x = x;
                    info.y = y;
                }
                info.model = model;
            }
            wl_output::Event::Mode {
                flags,
                width,
                height,
                ..
            } => {
                // Only use the current mode (physical pixels)
                if let wayland_client::WEnum::Value(mode_flags) = flags
                    && mode_flags.contains(wl_output::Mode::Current)
                {
                    info.physical_width = width;
                    info.physical_height = height;
                }
            }
            wl_output::Event::Scale { factor } => {
                info.scale = factor;
            }
            wl_output::Event::Name { name: output_name } => {
                // Use the output name (e.g., "DP-1", "HDMI-A-1") as the connector name
                info.connector_name = output_name;
            }
            wl_output::Event::Done => {
                // Default scale to 1 if not received
                if info.scale == 0 {
                    info.scale = 1;
                }
                info.wl_done = true;
                // In xdg-output v3+, Done is deprecated - xdg info piggybacks on wl_output.done
                // If we've received logical size from xdg-output, consider it done
                if info.xdg_logical_width > 0 {
                    info.xdg_done = true;
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<ZxdgOutputManagerV1, ()> for MonitorEnumState {
    fn event(
        _state: &mut Self,
        _proxy: &ZxdgOutputManagerV1,
        _event: <ZxdgOutputManagerV1 as wayland_client::Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // Manager doesn't send events
    }
}

/// xdg_output events - data contains the global name (u32) of the associated wl_output
impl Dispatch<ZxdgOutputV1, u32> for MonitorEnumState {
    fn event(
        state: &mut Self,
        _proxy: &ZxdgOutputV1,
        event: zxdg_output_v1::Event,
        name: &u32,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // Find the output info by global name
        let Some((_, info)) = state.outputs.iter_mut().find(|(_, o)| o.name == *name) else {
            return;
        };

        match event {
            zxdg_output_v1::Event::LogicalPosition { x, y } => {
                info.x = x;
                info.y = y;
            }
            zxdg_output_v1::Event::LogicalSize { width, height } => {
                info.xdg_logical_width = width;
                info.xdg_logical_height = height;
            }
            zxdg_output_v1::Event::Name { name: output_name } => {
                // xdg-output name is more reliable than wl_output name
                if !output_name.is_empty() {
                    info.connector_name = output_name;
                }
            }
            zxdg_output_v1::Event::Description { description } => {
                // Human-readable description with EDID info (e.g., "LG ULTRAWIDE (HDMI-A-1)")
                info.description = description;
            }
            zxdg_output_v1::Event::Done => {
                // Note: In xdg-output v3+, Done is deprecated and wl_output.done is used instead
                info.xdg_done = true;
            }
            _ => {}
        }
    }
}

/// Get all connected monitors without requiring an existing overlay window.
/// This is useful for converting saved relative positions to absolute before spawning.
pub fn get_all_monitors() -> Vec<MonitorInfo> {
    // Connect to the Wayland display
    let Ok(connection) = Connection::connect_to_env() else {
        return Vec::new();
    };

    let display = connection.display();
    let mut event_queue = connection.new_event_queue::<MonitorEnumState>();
    let qh = event_queue.handle();

    let mut state = MonitorEnumState {
        outputs: Vec::new(),
        xdg_output_manager: None,
        xdg_outputs: Vec::new(),
    };

    // Get the registry and bind globals (outputs and xdg_output_manager)
    let _registry = display.get_registry(&qh, ());

    // First roundtrip: registry globals (wl_output and zxdg_output_manager_v1)
    let _ = event_queue.roundtrip(&mut state);

    // Create xdg_output for each wl_output (if manager is available)
    if let Some(ref manager) = state.xdg_output_manager {
        for (output, info) in &state.outputs {
            let xdg_output = manager.get_xdg_output(output, &qh, info.name);
            state.xdg_outputs.push(xdg_output);
        }
    } else {
        // No xdg-output support, mark all as xdg_done so they're considered ready
        for (_, info) in &mut state.outputs {
            info.xdg_done = true;
        }
    }

    // Second roundtrip: wl_output info events
    let _ = event_queue.roundtrip(&mut state);
    // Third roundtrip: xdg_output events and done events
    let _ = event_queue.roundtrip(&mut state);
    // Fourth roundtrip: ensure all done events are processed
    let _ = event_queue.roundtrip(&mut state);

    // Convert to MonitorInfo (using logical dimensions from xdg-output if available)
    state
        .outputs
        .into_iter()
        .filter(|(_, info)| {
            info.is_ready() && info.logical_width() > 0 && info.logical_height() > 0
        })
        .enumerate()
        .map(|(idx, (_, info))| {
            let id = info.id();
            MonitorInfo {
                id: id.clone(),
                name: if info.connector_name.is_empty() && info.model.is_empty() {
                    format!("Monitor {}", idx + 1)
                } else {
                    id
                },
                x: info.x,
                y: info.y,
                width: info.logical_width() as u32,
                height: info.logical_height() as u32,
                is_primary: info.x == 0 && info.y == 0, // Monitor at origin is primary
            }
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// Wayland Overlay Implementation
// ─────────────────────────────────────────────────────────────────────────────

/// Wayland overlay implementation
pub struct WaylandOverlay {
    config: OverlayConfig,
    connection: Connection,
    event_queue: EventQueue<WaylandState>,
    state: WaylandState,
    qh: QueueHandle<WaylandState>,
}

/// Partial output info (built up from events before done)
#[derive(Debug, Clone, Default)]
struct OutputInfo {
    /// wl_output name (global name from registry)
    name: u32,
    /// Connector name from wl_output::Name event (e.g., "HDMI-A-1", "eDP-2")
    connector_name: String,
    /// Model description from wl_output::Geometry (fallback if no connector name)
    model: String,
    /// Human-readable description from xdg-output (includes EDID info like "LG ULTRAWIDE (HDMI-A-1)")
    description: String,
    /// Position in global coordinate space (from xdg-output if available, else wl_output)
    x: i32,
    y: i32,
    /// Physical dimensions (from Mode event)
    physical_width: i32,
    physical_height: i32,
    /// Logical dimensions from xdg-output (more accurate than physical/scale)
    xdg_logical_width: i32,
    xdg_logical_height: i32,
    /// Scale factor (from Scale event, defaults to 1)
    scale: i32,
    /// Whether wl_output done event received
    wl_done: bool,
    /// Whether xdg_output done event received (or not using xdg-output)
    xdg_done: bool,
}

impl OutputInfo {
    /// Check if output info is complete
    fn is_ready(&self) -> bool {
        self.wl_done && self.xdg_done
    }

    /// Get a stable identifier for this output.
    /// Prefers xdg-output description (includes EDID model info like "LG ULTRAWIDE (HDMI-A-1)"),
    /// falls back to connector name, then model, then synthesized ID.
    fn id(&self) -> String {
        if !self.description.is_empty() {
            // xdg-output description is most robust - includes monitor model + connector
            self.description.clone()
        } else if !self.connector_name.is_empty() {
            self.connector_name.clone()
        } else if !self.model.is_empty() {
            self.model.clone()
        } else {
            format!("output-{}", self.name)
        }
    }

    /// Get logical width - prefer xdg-output value, fall back to physical/scale
    fn logical_width(&self) -> i32 {
        if self.xdg_logical_width > 0 {
            self.xdg_logical_width
        } else if self.scale > 0 {
            self.physical_width / self.scale
        } else {
            self.physical_width
        }
    }

    /// Get logical height - prefer xdg-output value, fall back to physical/scale
    fn logical_height(&self) -> i32 {
        if self.xdg_logical_height > 0 {
            self.xdg_logical_height
        } else if self.scale > 0 {
            self.physical_height / self.scale
        } else {
            self.physical_height
        }
    }
}

/// Internal state for Wayland event handling
struct WaylandState {
    running: bool,
    configured: bool,
    width: u32,
    height: u32,

    // Wayland objects
    compositor: Option<WlCompositor>,
    layer_shell: Option<ZwlrLayerShellV1>,
    surface: Option<WlSurface>,
    layer_surface: Option<ZwlrLayerSurfaceV1>,
    shm: Option<WlShm>,
    buffer: Option<WlBuffer>,
    seat: Option<WlSeat>,
    pointer: Option<WlPointer>,
    relative_pointer_manager: Option<ZwpRelativePointerManagerV1>,
    relative_pointer: Option<ZwpRelativePointerV1>,

    // xdg-output for accurate monitor positions
    xdg_output_manager: Option<ZxdgOutputManagerV1>,
    xdg_outputs: Vec<ZxdgOutputV1>,

    // Monitor tracking
    outputs: Vec<(WlOutput, OutputInfo)>,
    /// The output this layer surface is bound to (for coordinate conversion)
    /// Contains (global_x, global_y, width, height) of the bound output
    bound_output_bounds: Option<(i32, i32, i32, i32)>,

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
    // Drag tracking - uses relative pointer motion for smooth movement
    drag_start_window_x: i32,
    drag_start_window_y: i32,
    drag_accum_x: f64, // Accumulated relative motion since drag start
    drag_accum_y: f64,
    // Track pending dimensions during resize (separate from actual state.width/height)
    pending_width: u32,
    pending_height: u32,
    position_dirty: bool,
    pending_resize: Option<(u32, u32)>, // (width, height) - applied on release

    // Mode tracking for optimization
    click_through: bool,
    drag_enabled: bool,
    pending_click: Option<(f32, f32)>,

    // Cross-monitor drag: pending rebind to a different output
    pending_output_rebind: Option<u32>, // Output name (global id) to rebind to
}

/// Resize corner detection
struct ResizeCorner;

impl ResizeCorner {
    /// Check if position is in the bottom-right resize corner
    fn is_in_corner(x: f64, y: f64, width: u32, height: u32) -> bool {
        x > (width as f64 - RESIZE_CORNER_SIZE as f64)
            && y > (height as f64 - RESIZE_CORNER_SIZE as f64)
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
            layer_shell: None,
            surface: None,
            layer_surface: None,
            shm: None,
            buffer: None,
            seat: None,
            pointer: None,
            relative_pointer_manager: None,
            relative_pointer: None,
            xdg_output_manager: None,
            xdg_outputs: Vec::new(),
            outputs: Vec::new(),
            bound_output_bounds: None,
            pixel_data: vec![0u8; pixel_count * 4],
            shm_data: None,
            pointer_x: 0.0,
            pointer_y: 0.0,
            is_dragging: false,
            is_resizing: false,
            in_resize_corner: false,
            window_x: x,
            window_y: y,
            drag_start_window_x: x,
            drag_start_window_y: y,
            drag_accum_x: 0.0,
            drag_accum_y: 0.0,
            pending_width: width,
            pending_height: height,
            position_dirty: false,
            pending_resize: None,
            click_through,
            drag_enabled: true,
            pending_click: None,
            pending_output_rebind: None,
        }
    }

    /// Clamp position to the bound output's local bounds.
    /// Layer-shell surfaces are bound to a specific output, so we clamp within that output.
    /// Coordinates are relative to the output's top-left corner (0,0 to width,height).
    fn clamp_position(&self, x: i32, y: i32) -> (i32, i32) {
        if let Some((_, _, out_width, out_height)) = self.bound_output_bounds {
            // Clamp to output's local bounds (0,0 to width,height)
            let max_x = (out_width - self.width as i32).max(0);
            let max_y = (out_height - self.height as i32).max(0);
            (x.clamp(0, max_x), y.clamp(0, max_y))
        } else {
            // No bound output info, just ensure non-negative
            (x.max(0), y.max(0))
        }
    }

    /// Update position directly - called from event handler
    fn update_position(&mut self, x: i32, y: i32) {
        // Clamp to monitor bounds
        let (clamped_x, clamped_y) = self.clamp_position(x, y);
        self.window_x = clamped_x;
        self.window_y = clamped_y;
        if let Some(layer_surface) = &self.layer_surface {
            layer_surface.set_margin(clamped_y, 0, 0, clamped_x);
        }
        if let Some(surface) = &self.surface {
            surface.commit();
        }
        self.position_dirty = true;
    }

    /// Check if an absolute position is on a different output than we're currently bound to.
    /// Returns the output's global name if we should rebind, None otherwise.
    fn check_cross_monitor(&self, abs_x: i32, abs_y: i32) -> Option<u32> {
        let (cur_x, cur_y, cur_w, cur_h) = self.bound_output_bounds?;

        // Check if position is still within current output
        if abs_x >= cur_x && abs_x < cur_x + cur_w && abs_y >= cur_y && abs_y < cur_y + cur_h {
            return None; // Still on same output
        }

        // Find which output contains this position
        for (_, info) in &self.outputs {
            if !info.is_ready() || info.logical_width() <= 0 || info.logical_height() <= 0 {
                continue;
            }
            let right = info.x + info.logical_width();
            let bottom = info.y + info.logical_height();
            if abs_x >= info.x && abs_x < right && abs_y >= info.y && abs_y < bottom {
                return Some(info.name);
            }
        }
        None // Position not on any known output
    }

    /// Convert current window position to absolute screen coordinates
    fn absolute_position(&self) -> (i32, i32) {
        if let Some((out_x, out_y, _, _)) = self.bound_output_bounds {
            (out_x + self.window_x, out_y + self.window_y)
        } else {
            (self.window_x, self.window_y)
        }
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

// Private methods for WaylandOverlay
impl WaylandOverlay {
    /// Rebind the layer surface to a different output (for cross-monitor dragging)
    fn rebind_to_output(&mut self, output_name: u32) {
        // Find the new output
        let new_output_info = self
            .state
            .outputs
            .iter()
            .find(|(_, info)| info.name == output_name)
            .map(|(output, info)| (output.clone(), info.clone()));

        let Some((new_output, new_info)) = new_output_info else {
            eprintln!("Rebind failed: output {} not found", output_name);
            return;
        };

        eprintln!(
            "Rebinding to output {} at ({}, {}) size {}x{}",
            new_info.id(),
            new_info.x,
            new_info.y,
            new_info.logical_width(),
            new_info.logical_height()
        );

        // Calculate absolute position before destroying old surface
        let (abs_x, abs_y) = self.state.absolute_position();

        // Convert to position relative to new output
        let new_rel_x = abs_x - new_info.x;
        let new_rel_y = abs_y - new_info.y;

        // Clamp to new output bounds
        let max_x = (new_info.logical_width() - self.state.width as i32).max(0);
        let max_y = (new_info.logical_height() - self.state.height as i32).max(0);
        let clamped_x = new_rel_x.clamp(0, max_x);
        let clamped_y = new_rel_y.clamp(0, max_y);

        eprintln!(
            "  Absolute ({}, {}) -> relative ({}, {}) -> clamped ({}, {})",
            abs_x, abs_y, new_rel_x, new_rel_y, clamped_x, clamped_y
        );

        // Destroy old surface and layer surface
        if let Some(old_layer) = self.state.layer_surface.take() {
            old_layer.destroy();
        }
        if let Some(old_surface) = self.state.surface.take() {
            old_surface.destroy();
        }

        // Get compositor and layer_shell for creating new surface
        let Some(compositor) = &self.state.compositor else {
            eprintln!("Rebind failed: no compositor");
            return;
        };
        let Some(layer_shell) = &self.state.layer_shell else {
            eprintln!("Rebind failed: no layer_shell");
            return;
        };

        // Create new surface
        let surface = compositor.create_surface(&self.qh, ());

        // Set up input region if click-through (but we're in move mode, so not click-through)
        // Actually during drag we're interactive, so no input region needed

        // Create new layer surface bound to the new output
        let layer_surface = layer_shell.get_layer_surface(
            &surface,
            Some(&new_output),
            wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::Layer::Overlay,
            self.config.namespace.clone(),
            &self.qh,
            (),
        );

        // Configure the new layer surface
        layer_surface.set_anchor(Anchor::Top | Anchor::Left);
        layer_surface.set_margin(clamped_y, 0, 0, clamped_x);
        layer_surface.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer_surface.set_size(self.state.width, self.state.height);
        surface.commit();

        // Update state
        self.state.surface = Some(surface);
        self.state.layer_surface = Some(layer_surface);
        self.state.window_x = clamped_x;
        self.state.window_y = clamped_y;
        self.state.bound_output_bounds = Some((
            new_info.x,
            new_info.y,
            new_info.logical_width(),
            new_info.logical_height(),
        ));

        // Reset drag tracking to new position
        self.state.drag_start_window_x = clamped_x;
        self.state.drag_start_window_y = clamped_y;
        self.state.drag_accum_x = 0.0;
        self.state.drag_accum_y = 0.0;

        // Mark as not yet configured - need to wait for configure event
        self.state.configured = false;

        // Recreate the buffer for the new surface
        self.state.create_shm_buffer(&self.qh);

        // Dispatch to get the configure event
        let _ = self.event_queue.roundtrip(&mut self.state);

        // Copy existing pixel data to new buffer and commit immediately
        // so the overlay is visible right away on the new monitor
        self.state.copy_pixels_to_shm();
        if let (Some(buffer), Some(surface)) = (&self.state.buffer, &self.state.surface) {
            surface.attach(Some(buffer), 0, 0);
            surface.damage_buffer(0, 0, self.state.width as i32, self.state.height as i32);
            surface.commit();
        }
        let _ = self.connection.flush();

        eprintln!(
            "Rebind complete, new bounds: {:?}",
            self.state.bound_output_bounds
        );
    }
}

impl OverlayPlatform for WaylandOverlay {
    fn new(config: OverlayConfig) -> Result<Self, PlatformError> {
        let connection = Connection::connect_to_env()
            .map_err(|e| PlatformError::ConnectionFailed(e.to_string()))?;

        let (globals, mut event_queue) =
            wayland_client::globals::registry_queue_init::<WaylandState>(&connection)
                .map_err(|e| PlatformError::ConnectionFailed(e.to_string()))?;

        let qh = event_queue.handle();
        let mut state = WaylandState::new(
            config.width,
            config.height,
            config.x,
            config.y,
            config.click_through,
        );

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

        // Bind relative pointer manager for smooth dragging (optional - graceful fallback)
        if let Ok(rpm) = globals.bind::<ZwpRelativePointerManagerV1, _, _>(&qh, 1..=1, ()) {
            state.relative_pointer_manager = Some(rpm);
        }

        // Bind xdg-output manager for accurate monitor positions (optional - graceful fallback)
        if let Ok(xom) = globals.bind::<ZxdgOutputManagerV1, _, _>(&qh, 1..=3, ()) {
            state.xdg_output_manager = Some(xom);
        }

        // Bind all outputs (monitors) for multi-monitor awareness
        // We store the global name as the output ID
        for global in globals.contents().clone_list() {
            if global.interface == "wl_output" {
                let output: WlOutput =
                    globals
                        .registry()
                        .bind(global.name, global.version.min(4), &qh, global.name);
                let info = OutputInfo {
                    name: global.name,
                    ..Default::default()
                };
                state.outputs.push((output, info));
            }
        }

        // Create xdg_output for each wl_output (if manager is available)
        if let Some(ref manager) = state.xdg_output_manager {
            for (output, info) in &state.outputs {
                let xdg_output = manager.get_xdg_output(output, &qh, info.name);
                state.xdg_outputs.push(xdg_output);
            }
        } else {
            // No xdg-output support, mark all as xdg_done so they're considered ready
            for (_, info) in &mut state.outputs {
                info.xdg_done = true;
            }
        }

        // Do roundtrips to receive output info
        // First roundtrip: wl_output events
        let _ = event_queue.roundtrip(&mut state);
        // Second roundtrip: xdg_output events
        let _ = event_queue.roundtrip(&mut state);
        // Third roundtrip: done events
        let _ = event_queue.roundtrip(&mut state);

        // Debug: print all outputs and their state
        eprintln!(
            "Looking for target_monitor_id: {:?}",
            config.target_monitor_id
        );
        for (_, info) in &state.outputs {
            eprintln!(
                "  Available: {} at ({}, {}) size {}x{} wl_done={} xdg_done={}",
                info.id(),
                info.x,
                info.y,
                info.logical_width(),
                info.logical_height(),
                info.wl_done,
                info.xdg_done
            );
        }

        // Output selection strategy:
        // 1. If monitor_id is set, bind to that specific output
        // 2. If monitor_id is None, let the compositor choose (active/focused output)
        let (target_output, margin_x, margin_y, bound_output_bounds) = if let Some(ref target_id) =
            config.target_monitor_id
        {
            // Find the specific output by ID
            if let Some((output, info)) = state
                .outputs
                .iter()
                .find(|(_, info)| info.is_ready() && info.id() == *target_id)
            {
                eprintln!(
                    "Binding to saved output {} at ({}, {}) size {}x{}",
                    info.id(),
                    info.x,
                    info.y,
                    info.logical_width(),
                    info.logical_height()
                );

                // Position is already relative to this monitor, just clamp it
                let max_x = (info.logical_width() - config.width as i32).max(0);
                let max_y = (info.logical_height() - config.height as i32).max(0);
                let clamped_x = config.x.clamp(0, max_x);
                let clamped_y = config.y.clamp(0, max_y);
                eprintln!(
                    "  Position ({}, {}) -> clamped ({}, {})",
                    config.x, config.y, clamped_x, clamped_y
                );

                let bounds = Some((info.x, info.y, info.logical_width(), info.logical_height()));
                (Some(output.clone()), clamped_x, clamped_y, bounds)
            } else {
                // Saved monitor not found, let compositor decide
                eprintln!(
                    "Saved monitor {} not found, letting compositor choose",
                    target_id
                );
                (None, config.x.max(0), config.y.max(0), None)
            }
        } else {
            // No saved monitor_id - let compositor choose (will be active/focused output)
            // The wl_surface::Enter event will tell us which output we ended up on
            eprintln!("No saved monitor_id, letting compositor choose active output");
            eprintln!("  Using position ({}, {}) as margins", config.x, config.y);
            (None, config.x.max(0), config.y.max(0), None)
        };
        eprintln!("Final bound_output_bounds: {:?}", bound_output_bounds);

        // Store bound output info for clamping
        state.bound_output_bounds = bound_output_bounds;

        // Only create pointer if interactive (not click-through)
        // This saves memory/CPU when overlay is locked
        if !config.click_through
            && let Some(seat) = &state.seat
        {
            let pointer = seat.get_pointer(&qh, ());
            // Create relative pointer if manager is available
            if let Some(rpm) = &state.relative_pointer_manager {
                let rel_pointer = rpm.get_relative_pointer(&pointer, &qh, ());
                state.relative_pointer = Some(rel_pointer);
            }
            state.pointer = Some(pointer);
        }

        // Create surface on the target output (or let compositor choose if None)
        let surface = compositor.create_surface(&qh, ());
        let layer_surface = layer_shell.get_layer_surface(
            &surface,
            target_output.as_ref(),
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

        // Configure layer surface with output-relative coordinates
        layer_surface.set_anchor(Anchor::Top | Anchor::Left);
        layer_surface.set_margin(margin_y, 0, 0, margin_x);
        layer_surface.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer_surface.set_size(config.width, config.height);
        surface.commit();
        eprintln!(
            "Layer surface configured: margin=({}, {}), size={}x{}",
            margin_x, margin_y, config.width, config.height
        );

        // window_x/window_y are stored as output-relative for internal use
        // The x()/y() trait methods will convert back to global for position saving
        state.window_x = margin_x;
        state.window_y = margin_y;

        state.compositor = Some(compositor);
        state.layer_shell = Some(layer_shell);
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
            overlay
                .event_queue
                .blocking_dispatch(&mut overlay.state)
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

    fn x(&self) -> i32 {
        // Convert output-relative to global by adding bound output's position
        if let Some((out_x, _, _, _)) = self.state.bound_output_bounds {
            self.state.window_x + out_x
        } else {
            self.state.window_x
        }
    }

    fn y(&self) -> i32 {
        // Convert output-relative to global by adding bound output's position
        if let Some((_, out_y, _, _)) = self.state.bound_output_bounds {
            self.state.window_y + out_y
        } else {
            self.state.window_y
        }
    }

    fn take_position_dirty(&mut self) -> bool {
        let dirty = self.state.position_dirty;
        self.state.position_dirty = false;
        dirty
    }

    fn set_position(&mut self, x: i32, y: i32) {
        // Convert global coordinates to output-relative for layer-shell
        let (rel_x, rel_y) = if let Some((out_x, out_y, _, _)) = self.state.bound_output_bounds {
            (x - out_x, y - out_y)
        } else {
            (x, y)
        };

        // update_position() handles clamping internally
        self.state.update_position(rel_x, rel_y);
        self.config.x = self.x(); // Store global position
        self.config.y = self.y();
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
        if !self.config.click_through
            && let (Some(compositor), Some(surface)) = (&self.state.compositor, &self.state.surface)
        {
            let region = compositor.create_region(&self.qh, ());
            region.add(0, 0, width as i32, height as i32);
            surface.set_input_region(Some(&region));
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
            // Locked mode: release pointers to save resources
            if let Some(rel_pointer) = self.state.relative_pointer.take() {
                rel_pointer.destroy();
            }
            if let Some(pointer) = self.state.pointer.take() {
                pointer.release();
            }
            // Reset interaction state
            self.state.is_dragging = false;
            self.state.is_resizing = false;
            self.state.in_resize_corner = false;
        } else {
            // Interactive mode: acquire pointer if we don't have one
            if self.state.pointer.is_none()
                && let Some(seat) = &self.state.seat
            {
                let pointer = seat.get_pointer(&self.qh, ());
                // Create relative pointer if manager is available
                if let Some(rpm) = &self.state.relative_pointer_manager {
                    let rel_pointer = rpm.get_relative_pointer(&pointer, &self.qh, ());
                    self.state.relative_pointer = Some(rel_pointer);
                }
                self.state.pointer = Some(pointer);
            }
        }
    }

    fn set_drag_enabled(&mut self, enabled: bool) {
        self.state.drag_enabled = enabled;
        if !enabled {
            // Cancel any in-progress drag
            self.state.is_dragging = false;
        }
    }

    fn is_drag_enabled(&self) -> bool {
        self.state.drag_enabled
    }

    fn take_pending_click(&mut self) -> Option<(f32, f32)> {
        self.state.pending_click.take()
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
                    Ok(0) => break,  // No events available
                    Ok(_) => {}      // Events read, continue
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

        // Handle cross-monitor rebind if requested
        if let Some(new_output_name) = self.state.pending_output_rebind.take() {
            self.rebind_to_output(new_output_name);
        }

        self.state.running
    }

    fn get_monitors(&self) -> Vec<MonitorInfo> {
        self.state
            .outputs
            .iter()
            .filter(|(_, info)| {
                info.is_ready() && info.logical_width() > 0 && info.logical_height() > 0
            })
            .enumerate()
            .map(|(idx, (_, info))| {
                let id = info.id();
                MonitorInfo {
                    id: id.clone(),
                    name: if info.connector_name.is_empty() && info.model.is_empty() {
                        format!("Monitor {}", idx + 1)
                    } else {
                        id
                    },
                    x: info.x,
                    y: info.y,
                    width: info.logical_width() as u32,
                    height: info.logical_height() as u32,
                    is_primary: info.x == 0 && info.y == 0, // Monitor at origin is primary
                }
            })
            .collect()
    }

    /// Override current_monitor to return the bound output's info.
    /// On Wayland with layer-shell, the surface is bound to a specific output,
    /// so we return that output's info rather than searching by global position.
    fn current_monitor(&self) -> Option<MonitorInfo> {
        if let Some((out_x, out_y, out_width, out_height)) = self.state.bound_output_bounds {
            // Find the output matching these bounds
            self.state
                .outputs
                .iter()
                .filter(|(_, info)| info.is_ready())
                .enumerate()
                .find(|(_, (_, info))| info.x == out_x && info.y == out_y)
                .map(|(idx, (_, info))| {
                    let id = info.id();
                    MonitorInfo {
                        id: id.clone(),
                        name: if info.connector_name.is_empty() && info.model.is_empty() {
                            format!("Monitor {}", idx + 1)
                        } else {
                            id
                        },
                        x: out_x,
                        y: out_y,
                        width: out_width as u32,
                        height: out_height as u32,
                        is_primary: out_x == 0 && out_y == 0,
                    }
                })
        } else {
            // Fall back to default implementation
            let monitors = self.get_monitors();
            let x = self.x();
            let y = self.y();
            monitors
                .iter()
                .find(|m| m.contains(x, y))
                .or_else(|| monitors.iter().find(|m| m.is_primary))
                .cloned()
        }
    }

    // Note: run() uses the default implementation from OverlayPlatform trait
}

// --- Wayland Dispatch implementations ---

/// Macro to implement empty Dispatch for protocols that don't need event handling
macro_rules! impl_empty_dispatch {
    ($proxy:ty, $data:ty, $state:ty) => {
        impl Dispatch<$proxy, $data> for $state {
            fn event(
                _: &mut Self,
                _: &$proxy,
                _: <$proxy as wayland_client::Proxy>::Event,
                _: &$data,
                _: &Connection,
                _: &QueueHandle<Self>,
            ) {
            }
        }
    };
}

// Empty dispatches for WaylandState - protocols that don't need event handling
impl_empty_dispatch!(wl_registry::WlRegistry, (), WaylandState);
impl_empty_dispatch!(wl_registry::WlRegistry, GlobalListContents, WaylandState);
impl_empty_dispatch!(WlCompositor, (), WaylandState);
/// WlSurface dispatch - handle Enter event to detect which output we're on
impl Dispatch<WlSurface, ()> for WaylandState {
    fn event(
        state: &mut Self,
        _proxy: &WlSurface,
        event: wl_surface::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let wl_surface::Event::Enter { output } = event {
            // Surface entered an output - update our bound_output_bounds
            if let Some((_, info)) = state.outputs.iter().find(|(o, _)| *o == output)
                && info.is_ready()
                && info.logical_width() > 0
                && info.logical_height() > 0
            {
                eprintln!(
                    "Surface entered output: {} at ({}, {}) size {}x{}",
                    info.id(),
                    info.x,
                    info.y,
                    info.logical_width(),
                    info.logical_height()
                );
                state.bound_output_bounds =
                    Some((info.x, info.y, info.logical_width(), info.logical_height()));
            }
        }
    }
}
impl_empty_dispatch!(WlRegion, (), WaylandState);

/// WlOutput dispatch - data contains the global name (u32)
impl Dispatch<WlOutput, u32> for WaylandState {
    fn event(
        state: &mut Self,
        proxy: &WlOutput,
        event: wl_output::Event,
        data: &u32,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // Find the output info for this output by matching the proxy
        let Some(info) = state
            .outputs
            .iter_mut()
            .find(|(o, _)| o == proxy)
            .map(|(_, i)| i)
        else {
            return;
        };

        match event {
            wl_output::Event::Geometry { x, y, model, .. } => {
                // Only use geometry position as fallback if xdg-output doesn't provide it
                if info.xdg_logical_width == 0 {
                    info.x = x;
                    info.y = y;
                }
                info.model = model;
            }
            wl_output::Event::Mode {
                flags,
                width,
                height,
                ..
            } => {
                // Only use the current mode (physical pixels)
                if let wayland_client::WEnum::Value(mode_flags) = flags
                    && mode_flags.contains(wl_output::Mode::Current)
                {
                    info.physical_width = width;
                    info.physical_height = height;
                }
            }
            wl_output::Event::Scale { factor } => {
                info.scale = factor;
            }
            wl_output::Event::Name { name } => {
                // Use the output name (e.g., "DP-1", "HDMI-A-1") as the connector name
                info.connector_name = name;
            }
            wl_output::Event::Done => {
                // Default scale to 1 if not received
                if info.scale == 0 {
                    info.scale = 1;
                }
                info.wl_done = true;
                // In xdg-output v3+, Done is deprecated - xdg info piggybacks on wl_output.done
                if info.xdg_logical_width > 0 {
                    info.xdg_done = true;
                }
                eprintln!(
                    "Output {}: {} at ({}, {}) size {}x{} (physical {}x{}, scale {})",
                    data,
                    info.id(),
                    info.x,
                    info.y,
                    info.logical_width(),
                    info.logical_height(),
                    info.physical_width,
                    info.physical_height,
                    info.scale
                );
            }
            _ => {}
        }
    }
}

impl_empty_dispatch!(WlShm, (), WaylandState);
impl_empty_dispatch!(WlShmPool, (), WaylandState);
impl_empty_dispatch!(WlBuffer, (), WaylandState);
impl_empty_dispatch!(ZwlrLayerShellV1, (), WaylandState);

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

impl_empty_dispatch!(WlSeat, (), WaylandState);

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
            wl_pointer::Event::Enter {
                surface_x,
                surface_y,
                ..
            } => {
                state.pointer_x = surface_x;
                state.pointer_y = surface_y;
                // Check if in resize corner for visual feedback
                state.in_resize_corner =
                    ResizeCorner::is_in_corner(surface_x, surface_y, state.width, state.height);
            }
            wl_pointer::Event::Motion {
                surface_x,
                surface_y,
                ..
            } => {
                // Only update resize corner state when not actively resizing
                // (during resize, keep it true so grip stays visible)
                if !state.is_resizing {
                    state.in_resize_corner =
                        ResizeCorner::is_in_corner(surface_x, surface_y, state.width, state.height);
                }

                if state.is_resizing {
                    // Use incremental delta for resize (works fine since we're changing size, not position)
                    let delta_x = surface_x - state.pointer_x;
                    let delta_y = surface_y - state.pointer_y;

                    let new_width = state.pending_width as i32 + delta_x as i32;
                    let new_height = state.pending_height as i32 + delta_y as i32;

                    // Clamp to min/max size constraints
                    let clamped_width =
                        (new_width as u32).clamp(MIN_OVERLAY_SIZE, MAX_OVERLAY_WIDTH);
                    let clamped_height =
                        (new_height as u32).clamp(MIN_OVERLAY_SIZE, MAX_OVERLAY_HEIGHT);

                    if new_width > 0 && new_height > 0 {
                        state.pending_width = clamped_width;
                        state.pending_height = clamped_height;
                        state.pending_resize = Some((clamped_width, clamped_height));
                    }

                    // Update pointer for resize delta calculation
                    state.pointer_x = surface_x;
                    state.pointer_y = surface_y;
                } else {
                    // Dragging is handled by relative pointer events for smooth movement
                    // Just track pointer position for corner detection
                    state.pointer_x = surface_x;
                    state.pointer_y = surface_y;
                }
            }
            wl_pointer::Event::Button {
                button,
                state: button_state,
                ..
            } => {
                use wayland_client::WEnum;
                // Button 272 is left mouse button (BTN_LEFT)
                if button == 272 {
                    match button_state {
                        WEnum::Value(wl_pointer::ButtonState::Pressed) => {
                            // Resize and drag are only available when drag_enabled (move mode)
                            // When drag_enabled=false (rearrange mode), all clicks go to the overlay
                            if state.drag_enabled {
                                if ResizeCorner::is_in_corner(
                                    state.pointer_x,
                                    state.pointer_y,
                                    state.width,
                                    state.height,
                                ) {
                                    state.is_resizing = true;
                                    state.pending_width = state.width;
                                    state.pending_height = state.height;
                                } else {
                                    // Initialize drag state - reset accumulator for relative motion
                                    state.is_dragging = true;
                                    state.drag_start_window_x = state.window_x;
                                    state.drag_start_window_y = state.window_y;
                                    state.drag_accum_x = 0.0;
                                    state.drag_accum_y = 0.0;
                                }
                            } else {
                                // Drag disabled (rearrange mode) - report click to overlay
                                state.pending_click =
                                    Some((state.pointer_x as f32, state.pointer_y as f32));
                            }
                        }
                        WEnum::Value(wl_pointer::ButtonState::Released) => {
                            state.is_dragging = false;
                            state.is_resizing = false;
                            // Recalculate corner state based on current pointer position
                            // and the potentially new window dimensions
                            state.in_resize_corner = ResizeCorner::is_in_corner(
                                state.pointer_x,
                                state.pointer_y,
                                state.pending_width,
                                state.pending_height,
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

impl_empty_dispatch!(ZwpRelativePointerManagerV1, (), WaylandState);

impl Dispatch<ZwpRelativePointerV1, ()> for WaylandState {
    fn event(
        state: &mut Self,
        _proxy: &ZwpRelativePointerV1,
        event: zwp_relative_pointer_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let zwp_relative_pointer_v1::Event::RelativeMotion { dx, dy, .. } = event {
            // dx, dy are in surface-local coordinates but represent actual cursor movement
            // This is the key: these deltas don't change when the window moves!
            if state.is_dragging {
                // Accumulate relative motion
                state.drag_accum_x += dx;
                state.drag_accum_y += dy;

                // Calculate new window position (relative to current output)
                let new_x = state.drag_start_window_x + state.drag_accum_x as i32;
                let new_y = state.drag_start_window_y + state.drag_accum_y as i32;

                // Check if this position would cross into a different monitor
                if let Some((out_x, out_y, _, _)) = state.bound_output_bounds {
                    let abs_x = out_x + new_x;
                    let abs_y = out_y + new_y;
                    if let Some(new_output_name) = state.check_cross_monitor(abs_x, abs_y) {
                        // Schedule rebind to new output (will be handled in poll_events)
                        state.pending_output_rebind = Some(new_output_name);
                    }
                }

                state.update_position(new_x, new_y);
            }
        }
    }
}

impl_empty_dispatch!(ZxdgOutputManagerV1, (), WaylandState);

/// xdg_output events - data contains the global name (u32) of the associated wl_output
impl Dispatch<ZxdgOutputV1, u32> for WaylandState {
    fn event(
        state: &mut Self,
        _proxy: &ZxdgOutputV1,
        event: zxdg_output_v1::Event,
        name: &u32,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // Find the output info by global name
        let Some(info) = state
            .outputs
            .iter_mut()
            .find(|(_, o)| o.name == *name)
            .map(|(_, i)| i)
        else {
            return;
        };

        match event {
            zxdg_output_v1::Event::LogicalPosition { x, y } => {
                info.x = x;
                info.y = y;
            }
            zxdg_output_v1::Event::LogicalSize { width, height } => {
                info.xdg_logical_width = width;
                info.xdg_logical_height = height;
            }
            zxdg_output_v1::Event::Name { name: output_name } => {
                // xdg-output name is more reliable than wl_output name
                if !output_name.is_empty() {
                    info.connector_name = output_name;
                }
            }
            zxdg_output_v1::Event::Description { description } => {
                // Human-readable description with EDID info (e.g., "LG ULTRAWIDE (HDMI-A-1)")
                info.description = description;
            }
            zxdg_output_v1::Event::Done => {
                info.xdg_done = true;
            }
            _ => {}
        }
    }
}
