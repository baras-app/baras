//! X11 platform implementation for overlay windows
//!
//! Uses XCB via x11rb for transparent, always-on-top overlay windows
//! with click-through support. Requires a compositor for transparency.

use std::fs::File;
use std::os::fd::AsFd;

use rustix::fs::{memfd_create, MemfdFlags};
use rustix::mm::{mmap, MapFlags, ProtFlags};
use x11rb::atom_manager;
use x11rb::connection::Connection;
use x11rb::protocol::randr::ConnectionExt as _;
use x11rb::protocol::shape::{self, ConnectionExt as _};
use x11rb::protocol::shm::{self, ConnectionExt as _};
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

use super::{MonitorInfo, OverlayConfig, OverlayPlatform, PlatformError};
use super::{MAX_OVERLAY_HEIGHT, MAX_OVERLAY_WIDTH, MIN_OVERLAY_SIZE, RESIZE_CORNER_SIZE};

// Atoms needed for EWMH hints
atom_manager! {
    pub AtomCollection: AtomCollectionCookie {
        _NET_WM_WINDOW_TYPE,
        _NET_WM_WINDOW_TYPE_DOCK,
        _NET_WM_STATE,
        _NET_WM_STATE_ABOVE,
        _NET_WM_STATE_SKIP_TASKBAR,
        _NET_WM_STATE_SKIP_PAGER,
        ATOM,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Standalone Monitor Enumeration
// ─────────────────────────────────────────────────────────────────────────────

pub fn get_all_monitors() -> Vec<MonitorInfo> {
    let Ok((conn, screen_num)) = x11rb::connect(None) else {
        return Vec::new();
    };

    let setup = conn.setup();
    let screen = &setup.roots[screen_num];
    let root = screen.root;

    let Ok(monitors) = conn.randr_get_monitors(root, true) else {
        return Vec::new();
    };
    let Ok(monitors) = monitors.reply() else {
        return Vec::new();
    };

    monitors
        .monitors
        .iter()
        .enumerate()
        .map(|(idx, mon)| {
            let name = conn
                .get_atom_name(mon.name)
                .ok()
                .and_then(|r| r.reply().ok())
                .map(|r| String::from_utf8_lossy(&r.name).to_string())
                .unwrap_or_else(|| format!("Monitor {}", idx + 1));

            MonitorInfo {
                id: name.clone(),
                name,
                x: mon.x as i32,
                y: mon.y as i32,
                width: mon.width as u32,
                height: mon.height as u32,
                is_primary: mon.primary,
            }
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// X11 Overlay Implementation
// ─────────────────────────────────────────────────────────────────────────────

/// SHM buffer for efficient pixel transfer
struct ShmBuffer {
    seg_id: shm::Seg,
    ptr: *mut u8,
    size: usize,
}

// SAFETY: We only access shm_data from the main thread
unsafe impl Send for ShmBuffer {}

pub struct X11Overlay {
    conn: RustConnection,
    window: Window,
    gc: Gcontext,
    atoms: AtomCollection,
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    depth: u8,

    // Pixel buffers
    pixel_data: Vec<u8>,  // RGBA from renderer
    shm_buffer: ShmBuffer,

    // Interaction state
    click_through: bool,
    drag_enabled: bool,
    is_dragging: bool,
    is_resizing: bool,
    in_resize_corner: bool,
    position_dirty: bool,
    pending_click: Option<(f32, f32)>,

    // Drag tracking (root coordinates for stability)
    drag_start_root_x: i32,
    drag_start_root_y: i32,
    drag_start_win_x: i32,
    drag_start_win_y: i32,

    // Resize tracking
    resize_start_x: i32,
    resize_start_y: i32,
    pending_width: u32,
    pending_height: u32,

    running: bool,
}

impl X11Overlay {
    /// Find a 32-bit ARGB visual for transparency
    fn find_argb_visual(screen: &Screen) -> Option<(Visualid, u8)> {
        for depth in &screen.allowed_depths {
            if depth.depth == 32 {
                for visual in &depth.visuals {
                    if visual.class == VisualClass::TRUE_COLOR {
                        return Some((visual.visual_id, depth.depth));
                    }
                }
            }
        }
        None
    }

    /// Create a shared memory buffer for efficient pixel transfer
    fn create_shm_buffer(
        conn: &RustConnection,
        width: u32,
        height: u32,
    ) -> Result<ShmBuffer, PlatformError> {
        let size = (width * height * 4) as usize;

        // Create anonymous shared memory
        let fd = memfd_create(c"baras-x11-buffer", MemfdFlags::CLOEXEC)
            .map_err(|e| PlatformError::BufferError(format!("memfd_create failed: {}", e)))?;

        rustix::fs::ftruncate(&fd, size as u64)
            .map_err(|e| PlatformError::BufferError(format!("ftruncate failed: {}", e)))?;

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
            .map_err(|e| PlatformError::BufferError(format!("mmap failed: {}", e)))?
        };

        // Attach to X server
        let seg_id = conn
            .generate_id()
            .map_err(|e| PlatformError::BufferError(e.to_string()))?;

        // x11rb shm_attach_fd takes ownership of the fd
        let file = File::from(fd);
        conn.shm_attach_fd(seg_id, file, false)
            .map_err(|e| PlatformError::BufferError(format!("shm_attach_fd failed: {}", e)))?;

        Ok(ShmBuffer {
            seg_id,
            ptr: ptr as *mut u8,
            size,
        })
    }

    /// Recreate SHM buffer after resize
    fn recreate_shm_buffer(&mut self) -> Result<(), PlatformError> {
        // Detach old segment
        let _ = self.conn.shm_detach(self.shm_buffer.seg_id);
        unsafe {
            rustix::mm::munmap(self.shm_buffer.ptr as *mut _, self.shm_buffer.size).ok();
        }

        // Create new buffer
        self.shm_buffer = Self::create_shm_buffer(&self.conn, self.width, self.height)?;
        self.pixel_data
            .resize((self.width * self.height * 4) as usize, 0);

        Ok(())
    }

    /// Set EWMH hints for overlay behavior
    fn setup_window_hints(&self) -> Result<(), PlatformError> {
        // Window type: dock (stays on top, no decorations)
        self.conn
            .change_property32(
                PropMode::REPLACE,
                self.window,
                self.atoms._NET_WM_WINDOW_TYPE,
                self.atoms.ATOM,
                &[self.atoms._NET_WM_WINDOW_TYPE_DOCK],
            )
            .map_err(|e| PlatformError::Other(e.to_string()))?;

        // Window state: above, skip taskbar/pager
        self.conn
            .change_property32(
                PropMode::REPLACE,
                self.window,
                self.atoms._NET_WM_STATE,
                self.atoms.ATOM,
                &[
                    self.atoms._NET_WM_STATE_ABOVE,
                    self.atoms._NET_WM_STATE_SKIP_TASKBAR,
                    self.atoms._NET_WM_STATE_SKIP_PAGER,
                ],
            )
            .map_err(|e| PlatformError::Other(e.to_string()))?;

        Ok(())
    }

    /// Update input shape for click-through
    fn update_input_shape(&self) {
        if self.click_through {
            // Empty input region - clicks pass through
            let _ = self.conn.shape_rectangles(
                shape::SO::SET,
                shape::SK::INPUT,
                ClipOrdering::UNSORTED,
                self.window,
                0,
                0,
                &[],
            );
        } else {
            // Full window is interactive
            let rect = Rectangle {
                x: 0,
                y: 0,
                width: self.width as u16,
                height: self.height as u16,
            };
            let _ = self.conn.shape_rectangles(
                shape::SO::SET,
                shape::SK::INPUT,
                ClipOrdering::UNSORTED,
                self.window,
                0,
                0,
                &[rect],
            );
        }
        let _ = self.conn.flush();
    }

    fn is_in_resize_corner(&self, x: i32, y: i32) -> bool {
        x > (self.width as i32 - RESIZE_CORNER_SIZE)
            && y > (self.height as i32 - RESIZE_CORNER_SIZE)
    }
}

impl OverlayPlatform for X11Overlay {
    fn new(config: OverlayConfig) -> Result<Self, PlatformError> {
        let (conn, screen_num) =
            x11rb::connect(None).map_err(|e| PlatformError::ConnectionFailed(e.to_string()))?;

        // Intern atoms
        let atoms = AtomCollection::new(&conn)
            .map_err(|e| PlatformError::Other(e.to_string()))?
            .reply()
            .map_err(|e| PlatformError::Other(e.to_string()))?;

        let setup = conn.setup();
        let screen = &setup.roots[screen_num];
        let root = screen.root;

        // Check for required extensions
        conn.shape_query_version()
            .map_err(|_| PlatformError::UnsupportedFeature("Shape extension".into()))?
            .reply()
            .map_err(|_| PlatformError::UnsupportedFeature("Shape extension".into()))?;

        conn.shm_query_version()
            .map_err(|_| PlatformError::UnsupportedFeature("SHM extension".into()))?
            .reply()
            .map_err(|_| PlatformError::UnsupportedFeature("SHM extension".into()))?;

        // Find 32-bit visual for transparency
        let (visual, depth) = Self::find_argb_visual(screen)
            .ok_or_else(|| PlatformError::UnsupportedFeature("32-bit ARGB visual".into()))?;

        // Create colormap for 32-bit visual
        let colormap = conn
            .generate_id()
            .map_err(|e| PlatformError::Other(e.to_string()))?;
        conn.create_colormap(ColormapAlloc::NONE, colormap, root, visual)
            .map_err(|e| PlatformError::Other(e.to_string()))?;

        // Resolve absolute position
        let monitors = get_all_monitors();
        let (abs_x, abs_y) = super::resolve_absolute_position(
            config.x,
            config.y,
            config.target_monitor_id.as_deref(),
            &monitors,
        );

        // Create window
        let window = conn
            .generate_id()
            .map_err(|e| PlatformError::Other(e.to_string()))?;

        let win_aux = CreateWindowAux::new()
            .background_pixel(0)
            .border_pixel(0)
            .colormap(colormap)
            .event_mask(
                EventMask::EXPOSURE
                    | EventMask::BUTTON_PRESS
                    | EventMask::BUTTON_RELEASE
                    | EventMask::POINTER_MOTION
                    | EventMask::ENTER_WINDOW
                    | EventMask::LEAVE_WINDOW
                    | EventMask::STRUCTURE_NOTIFY,
            )
            .override_redirect(1);

        conn.create_window(
            depth,
            window,
            root,
            abs_x as i16,
            abs_y as i16,
            config.width as u16,
            config.height as u16,
            0,
            WindowClass::INPUT_OUTPUT,
            visual,
            &win_aux,
        )
        .map_err(|e| PlatformError::Other(e.to_string()))?;

        // Create graphics context
        let gc = conn
            .generate_id()
            .map_err(|e| PlatformError::Other(e.to_string()))?;
        conn.create_gc(gc, window, &CreateGCAux::new())
            .map_err(|e| PlatformError::Other(e.to_string()))?;

        // Create SHM buffer
        let shm_buffer = Self::create_shm_buffer(&conn, config.width, config.height)?;

        let overlay = Self {
            conn,
            window,
            gc,
            atoms,
            width: config.width,
            height: config.height,
            x: abs_x,
            y: abs_y,
            depth,
            pixel_data: vec![0u8; (config.width * config.height * 4) as usize],
            shm_buffer,
            click_through: config.click_through,
            drag_enabled: true,
            is_dragging: false,
            is_resizing: false,
            in_resize_corner: false,
            position_dirty: false,
            pending_click: None,
            drag_start_root_x: 0,
            drag_start_root_y: 0,
            drag_start_win_x: abs_x,
            drag_start_win_y: abs_y,
            resize_start_x: 0,
            resize_start_y: 0,
            pending_width: config.width,
            pending_height: config.height,
            running: true,
        };

        overlay.setup_window_hints()?;
        overlay.update_input_shape();

        // Map window
        overlay
            .conn
            .map_window(window)
            .map_err(|e| PlatformError::Other(e.to_string()))?;
        overlay
            .conn
            .flush()
            .map_err(|e| PlatformError::Other(e.to_string()))?;

        Ok(overlay)
    }

    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }

    fn x(&self) -> i32 {
        self.x
    }

    fn y(&self) -> i32 {
        self.y
    }

    fn take_position_dirty(&mut self) -> bool {
        std::mem::take(&mut self.position_dirty)
    }

    fn set_position(&mut self, x: i32, y: i32) {
        let monitors = self.get_monitors();
        let (cx, cy) = super::clamp_to_virtual_screen(x, y, self.width, self.height, &monitors);

        if cx == self.x && cy == self.y {
            return;
        }

        self.x = cx;
        self.y = cy;
        self.position_dirty = true;

        let _ = self
            .conn
            .configure_window(self.window, &ConfigureWindowAux::new().x(cx).y(cy));
        let _ = self.conn.flush();
    }

    fn set_size(&mut self, width: u32, height: u32) {
        if width == self.width && height == self.height {
            return;
        }

        self.width = width;
        self.height = height;
        self.pending_width = width;
        self.pending_height = height;

        // Recreate SHM buffer for new size
        let _ = self.recreate_shm_buffer();

        let _ = self.conn.configure_window(
            self.window,
            &ConfigureWindowAux::new().width(width).height(height),
        );

        if !self.click_through {
            self.update_input_shape();
        }

        let _ = self.conn.flush();
    }

    fn set_click_through(&mut self, enabled: bool) {
        self.click_through = enabled;
        self.update_input_shape();

        if enabled {
            self.is_dragging = false;
            self.is_resizing = false;
            self.in_resize_corner = false;
        }
    }

    fn set_drag_enabled(&mut self, enabled: bool) {
        self.drag_enabled = enabled;
        if !enabled {
            self.is_dragging = false;
        }
    }

    fn is_drag_enabled(&self) -> bool {
        self.drag_enabled
    }

    fn take_pending_click(&mut self) -> Option<(f32, f32)> {
        self.pending_click.take()
    }

    fn in_resize_corner(&self) -> bool {
        self.in_resize_corner
    }

    fn is_resizing(&self) -> bool {
        self.is_resizing
    }

    fn pending_size(&self) -> Option<(u32, u32)> {
        self.is_resizing
            .then_some((self.pending_width, self.pending_height))
    }

    fn is_interactive(&self) -> bool {
        !self.click_through
    }

    fn pixel_buffer(&mut self) -> Option<&mut [u8]> {
        Some(&mut self.pixel_data)
    }

    fn commit(&mut self) {
        // Convert RGBA to BGRA directly into SHM buffer
        let shm_slice =
            unsafe { std::slice::from_raw_parts_mut(self.shm_buffer.ptr, self.shm_buffer.size) };

        for (i, chunk) in self.pixel_data.chunks(4).enumerate() {
            let offset = i * 4;
            if chunk.len() == 4 && offset + 3 < shm_slice.len() {
                shm_slice[offset] = chunk[2];     // B
                shm_slice[offset + 1] = chunk[1]; // G
                shm_slice[offset + 2] = chunk[0]; // R
                shm_slice[offset + 3] = chunk[3]; // A
            }
        }

        let _ = self.conn.shm_put_image(
            self.window,
            self.gc,
            self.width as u16,
            self.height as u16,
            0,
            0,
            self.width as u16,
            self.height as u16,
            0,
            0,
            self.depth,
            ImageFormat::Z_PIXMAP.into(),
            false,
            self.shm_buffer.seg_id,
            0,
        );
        let _ = self.conn.flush();
    }

    fn poll_events(&mut self) -> bool {
        while let Ok(Some(event)) = self.conn.poll_for_event() {
            match event {
                x11rb::protocol::Event::ButtonPress(e) if !self.click_through => {
                    let x = e.event_x as i32;
                    let y = e.event_y as i32;

                    if e.detail == 1 {
                        if self.drag_enabled {
                            if self.is_in_resize_corner(x, y) {
                                self.is_resizing = true;
                                self.pending_width = self.width;
                                self.pending_height = self.height;
                                self.resize_start_x = x;
                                self.resize_start_y = y;
                            } else {
                                self.is_dragging = true;
                                self.drag_start_root_x = e.root_x as i32;
                                self.drag_start_root_y = e.root_y as i32;
                                self.drag_start_win_x = self.x;
                                self.drag_start_win_y = self.y;
                            }
                        } else {
                            self.pending_click = Some((x as f32, y as f32));
                        }
                    }
                }
                x11rb::protocol::Event::ButtonRelease(e) if e.detail == 1 => {
                    self.is_dragging = false;
                    self.is_resizing = false;
                }
                x11rb::protocol::Event::MotionNotify(e) if !self.click_through => {
                    let x = e.event_x as i32;
                    let y = e.event_y as i32;

                    if !self.is_resizing {
                        self.in_resize_corner = self.is_in_resize_corner(x, y);
                    }

                    if self.is_dragging {
                        let dx = e.root_x as i32 - self.drag_start_root_x;
                        let dy = e.root_y as i32 - self.drag_start_root_y;
                        self.set_position(self.drag_start_win_x + dx, self.drag_start_win_y + dy);
                    } else if self.is_resizing {
                        let dx = x - self.resize_start_x;
                        let dy = y - self.resize_start_y;

                        let new_w = (self.pending_width as i32 + dx)
                            .clamp(MIN_OVERLAY_SIZE as i32, MAX_OVERLAY_WIDTH as i32)
                            as u32;
                        let new_h = (self.pending_height as i32 + dy)
                            .clamp(MIN_OVERLAY_SIZE as i32, MAX_OVERLAY_HEIGHT as i32)
                            as u32;

                        if new_w != self.width || new_h != self.height {
                            self.set_size(new_w, new_h);
                            self.resize_start_x = x;
                            self.resize_start_y = y;
                        }
                    }
                }
                x11rb::protocol::Event::LeaveNotify(_) => {
                    if !self.is_resizing {
                        self.in_resize_corner = false;
                    }
                }
                x11rb::protocol::Event::DestroyNotify(e) if e.window == self.window => {
                    self.running = false;
                    return false;
                }
                _ => {}
            }
        }
        self.running
    }

    fn get_monitors(&self) -> Vec<MonitorInfo> {
        get_all_monitors()
    }
}

impl Drop for X11Overlay {
    fn drop(&mut self) {
        // Clean up SHM
        let _ = self.conn.shm_detach(self.shm_buffer.seg_id);
        unsafe {
            rustix::mm::munmap(self.shm_buffer.ptr as *mut _, self.shm_buffer.size).ok();
        }

        let _ = self.conn.destroy_window(self.window);
        let _ = self.conn.free_gc(self.gc);
        let _ = self.conn.flush();
    }
}
