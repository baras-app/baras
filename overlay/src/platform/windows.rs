//! Windows platform implementation for overlay windows
//!
//! Uses Win32 API to create transparent, always-on-top overlay windows
//! with click-through support.
#![allow(clippy::too_many_arguments)]

// Debug logging macro - prints to stderr with [WIN-OVERLAY] prefix
macro_rules! overlay_log {
    ($($arg:tt)*) => {
        eprintln!("[WIN-OVERLAY] {}", format!($($arg)*));
    };
}

use std::mem;
use std::ptr;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS,
    DeleteDC, EnumDisplayMonitors, GetCurrentObject, GetDC, GetMonitorInfoW, HBITMAP, HDC,
    HMONITOR, MONITORINFOEXW, OBJ_BITMAP, ReleaseDC, SelectObject, SetDIBits,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GWL_EXSTYLE, GetCursorPos, HTCLIENT, HWND_TOPMOST, IDC_ARROW, LoadCursorW, MSG, PM_REMOVE,
    PeekMessageW, RegisterClassExW, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SetWindowLongPtrW, SetWindowPos, ShowWindow, TranslateMessage, ULW_ALPHA, UpdateLayeredWindow,
    WM_DESTROY, WM_ERASEBKGND, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCHITTEST, WM_QUIT,
    WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::PCWSTR;

use windows::Win32::Foundation::RECT;

use super::{MAX_OVERLAY_HEIGHT, MAX_OVERLAY_WIDTH, MIN_OVERLAY_SIZE, RESIZE_CORNER_SIZE};
use super::{MonitorInfo, OverlayConfig, OverlayPlatform, PlatformError};

// ─────────────────────────────────────────────────────────────────────────────
// Standalone Monitor Enumeration
// ─────────────────────────────────────────────────────────────────────────────

/// Raw monitor data collected during enumeration.
/// Defined at module level so it can be used by both the enum callback and processing code.
struct RawMonitor {
    device_name: String,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    is_primary: bool,
}

/// Callback for EnumDisplayMonitors - collects monitor info into a Vec<RawMonitor>
unsafe extern "system" fn enum_monitors_callback(
    hmonitor: HMONITOR,
    _hdc: HDC,
    _rect: *mut RECT,
    lparam: LPARAM,
) -> windows::Win32::Foundation::BOOL {
    unsafe {
        let raw_monitors = &mut *(lparam.0 as *mut Vec<RawMonitor>);

        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = mem::size_of::<MONITORINFOEXW>() as u32;

        if GetMonitorInfoW(hmonitor, &mut info.monitorInfo).as_bool() {
            let rc = info.monitorInfo.rcMonitor;

            // Convert device name (wide string) to String
            let name_len = info
                .szDevice
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(info.szDevice.len());
            let device_name = String::from_utf16_lossy(&info.szDevice[..name_len]);

            raw_monitors.push(RawMonitor {
                device_name,
                x: rc.left,
                y: rc.top,
                width: (rc.right - rc.left) as u32,
                height: (rc.bottom - rc.top) as u32,
                is_primary: info.monitorInfo.dwFlags & 1 != 0,
            });
        }

        windows::Win32::Foundation::BOOL::from(true)
    }
}

/// Convert raw monitor data to MonitorInfo with stable device-name-based IDs
fn raw_monitors_to_info(raw_monitors: Vec<RawMonitor>) -> Vec<MonitorInfo> {
    raw_monitors
        .into_iter()
        .map(|raw| MonitorInfo {
            id: raw.device_name.clone(),
            name: raw.device_name,
            x: raw.x,
            y: raw.y,
            width: raw.width,
            height: raw.height,
            is_primary: raw.is_primary,
        })
        .collect()
}

/// Get all connected monitors without requiring an existing overlay window.
/// This is useful for converting saved relative positions to absolute before spawning.
pub fn get_all_monitors() -> Vec<MonitorInfo> {
    overlay_log!("get_all_monitors: enumerating displays...");
    let mut raw_monitors: Vec<RawMonitor> = Vec::new();

    unsafe {
        let raw_ptr = &mut raw_monitors as *mut Vec<RawMonitor>;
        let result = EnumDisplayMonitors(
            None,
            None,
            Some(enum_monitors_callback),
            LPARAM(raw_ptr as isize),
        );
        overlay_log!(
            "get_all_monitors: EnumDisplayMonitors returned {:?}",
            result
        );
    }

    let monitors = raw_monitors_to_info(raw_monitors);
    for m in &monitors {
        overlay_log!(
            "  Monitor: id='{}' pos=({},{}) size={}x{} primary={}",
            m.id,
            m.x,
            m.y,
            m.width,
            m.height,
            m.is_primary
        );
    }
    overlay_log!("get_all_monitors: found {} monitors", monitors.len());
    monitors
}

/// Windows overlay implementation
pub struct WindowsOverlay {
    hwnd: HWND,
    hdc_mem: HDC,
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    pixel_data: Vec<u8>,
    bgra_buffer: Vec<u8>, // Pre-allocated buffer for RGBA->BGRA conversion
    content_dirty: bool,  // Track if pixel content changed
    click_through: bool,
    position_dirty: bool,

    // Interaction state
    pointer_x: i32,
    pointer_y: i32,
    is_dragging: bool,
    is_resizing: bool,
    in_resize_corner: bool,
    drag_enabled: bool,
    pending_click: Option<(f32, f32)>,
    // Drag tracking - uses screen coordinates for stable movement
    drag_start_screen_x: i32,
    drag_start_screen_y: i32,
    drag_start_win_x: i32,
    drag_start_win_y: i32,
    // Resize tracking - uses client coordinates (size changes, not position)
    resize_start_x: i32,
    resize_start_y: i32,
    pending_width: u32,
    pending_height: u32,
    running: bool,
}

// NOTE: WindowsOverlay intentionally does NOT implement Send.
// Win32 HWND handles must be used from the thread that created them.
// The message queue is tied to the creating thread, so SetWindowLongPtrW,
// PeekMessageW, and other window operations fail when called from a different thread.
//
// The spawn_overlay_with_factory function creates the overlay INSIDE the spawned
// thread to ensure correct threading.

impl WindowsOverlay {
    fn register_class() -> Result<(), PlatformError> {
        unsafe {
            let class_name = wide_string("BarasOverlayClass");
            let hinstance = GetModuleHandleW(None)
                .map_err(|e| PlatformError::Other(format!("GetModuleHandleW failed: {}", e)))?;

            let wc = WNDCLASSEXW {
                cbSize: mem::size_of::<WNDCLASSEXW>() as u32,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(window_proc),
                hInstance: hinstance.into(),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                lpszClassName: PCWSTR(class_name.as_ptr()),
                ..Default::default()
            };

            let atom = RegisterClassExW(&wc);
            if atom == 0 {
                // Class may already be registered, which is fine
                let err = std::io::Error::last_os_error();
                if err.raw_os_error() != Some(1410) {
                    // ERROR_CLASS_ALREADY_EXISTS
                    return Err(PlatformError::Other(format!(
                        "RegisterClassExW failed: {}",
                        err
                    )));
                }
            }
        }
        Ok(())
    }

    fn create_dib_section(&mut self) -> Result<(), PlatformError> {
        unsafe {
            let hdc_screen = GetDC(HWND::default());

            if !self.hdc_mem.is_invalid() {
                let _ = DeleteDC(self.hdc_mem);
            }

            self.hdc_mem = CreateCompatibleDC(hdc_screen);
            if self.hdc_mem.is_invalid() {
                ReleaseDC(HWND::default(), hdc_screen);
                return Err(PlatformError::BufferError(
                    "CreateCompatibleDC failed".to_string(),
                ));
            }

            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: self.width as i32,
                    biHeight: -(self.height as i32), // Top-down DIB
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };

            let mut bits: *mut std::ffi::c_void = ptr::null_mut();
            let hbitmap = CreateDIBSection(hdc_screen, &bmi, DIB_RGB_COLORS, &mut bits, None, 0)
                .map_err(|e| {
                    PlatformError::BufferError(format!("CreateDIBSection failed: {}", e))
                })?;

            SelectObject(self.hdc_mem, hbitmap);
            ReleaseDC(HWND::default(), hdc_screen);

            // Resize pixel buffers
            let size = (self.width * self.height * 4) as usize;
            self.pixel_data.resize(size, 0);
            self.bgra_buffer.resize(size, 0);
            self.content_dirty = true;
        }
        Ok(())
    }

    fn update_layered_window(&mut self) {
        // Skip expensive pixel operations if content hasn't changed
        if !self.content_dirty {
            return;
        }
        self.content_dirty = false;

        unsafe {
            let hdc_screen = GetDC(HWND::default());

            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: self.width as i32,
                    biHeight: -(self.height as i32),
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };

            // Convert RGBA to BGRA using pre-allocated buffer (no allocation!)
            for (i, chunk) in self.pixel_data.chunks(4).enumerate() {
                let offset = i * 4;
                if chunk.len() == 4 && offset + 3 < self.bgra_buffer.len() {
                    self.bgra_buffer[offset] = chunk[2]; // B
                    self.bgra_buffer[offset + 1] = chunk[1]; // G
                    self.bgra_buffer[offset + 2] = chunk[0]; // R
                    self.bgra_buffer[offset + 3] = chunk[3]; // A
                }
            }

            // Get the bitmap handle from the DC
            let hgdiobj = GetCurrentObject(self.hdc_mem, OBJ_BITMAP);
            let hbitmap = HBITMAP(hgdiobj.0);
            SetDIBits(
                self.hdc_mem,
                hbitmap,
                0,
                self.height,
                self.bgra_buffer.as_ptr() as *const _,
                &bmi,
                DIB_RGB_COLORS,
            );

            // Use UpdateLayeredWindow for per-pixel alpha
            let pt_src = POINT { x: 0, y: 0 };
            let pt_dst = POINT {
                x: self.x,
                y: self.y,
            };
            let size = windows::Win32::Foundation::SIZE {
                cx: self.width as i32,
                cy: self.height as i32,
            };
            let blend = windows::Win32::Graphics::Gdi::BLENDFUNCTION {
                BlendOp: 0, // AC_SRC_OVER
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: 1, // AC_SRC_ALPHA
            };

            let _ = UpdateLayeredWindow(
                self.hwnd,
                hdc_screen,
                Some(&pt_dst),
                Some(&size),
                self.hdc_mem,
                Some(&pt_src),
                windows::Win32::Foundation::COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );

            ReleaseDC(HWND::default(), hdc_screen);
        }
    }

    fn is_in_resize_corner(&self, x: i32, y: i32) -> bool {
        x > (self.width as i32 - RESIZE_CORNER_SIZE)
            && y > (self.height as i32 - RESIZE_CORNER_SIZE)
    }

    fn update_extended_style(&self) {
        overlay_log!(
            "HWND={:?}: update_extended_style called, click_through={}",
            self.hwnd,
            self.click_through
        );
        unsafe {
            let mut ex_style = WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW;
            if self.click_through {
                ex_style |= WS_EX_TRANSPARENT | WS_EX_NOACTIVATE;
            }
            overlay_log!("  Setting extended style to {:#x}", ex_style.0);
            SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, ex_style.0 as isize);
            overlay_log!("  SetWindowLongPtrW completed");
        }
    }
}

impl OverlayPlatform for WindowsOverlay {
    fn new(config: OverlayConfig) -> Result<Self, PlatformError> {
        overlay_log!("=== Creating new overlay: '{}' ===", config.namespace);
        overlay_log!(
            "  Config: pos=({},{}) size={}x{} click_through={} target_monitor={:?}",
            config.x,
            config.y,
            config.width,
            config.height,
            config.click_through,
            config.target_monitor_id
        );

        Self::register_class()?;
        overlay_log!("  Window class registered");

        // Convert relative position to absolute screen coordinates
        // Position is stored relative to the target monitor
        let monitors = get_all_monitors();
        let (abs_x, abs_y, monitor_found) = if let Some(ref target_id) = config.target_monitor_id {
            // Find the target monitor and add its position
            if let Some(mon) = monitors.iter().find(|m| m.id == *target_id) {
                overlay_log!(
                    "  Found target monitor '{}' at ({},{})",
                    target_id,
                    mon.x,
                    mon.y
                );
                (config.x + mon.x, config.y + mon.y, true)
            } else {
                overlay_log!(
                    "  WARNING: Target monitor '{}' not found! Falling back to primary",
                    target_id
                );
                // Monitor not found, use primary
                let result = monitors
                    .iter()
                    .find(|m| m.is_primary)
                    .map(|m| (config.x + m.x, config.y + m.y))
                    .unwrap_or((config.x, config.y));
                (result.0, result.1, false)
            }
        } else {
            overlay_log!("  No target monitor specified, using primary");
            // No monitor ID, use primary monitor
            let result = monitors
                .iter()
                .find(|m| m.is_primary)
                .map(|m| (config.x + m.x, config.y + m.y))
                .unwrap_or((config.x, config.y));
            (result.0, result.1, true)
        };
        overlay_log!(
            "  Absolute position: ({},{}) monitor_found={}",
            abs_x,
            abs_y,
            monitor_found
        );

        let hwnd = unsafe {
            let class_name = wide_string("BarasOverlayClass");
            let window_name = wide_string(&config.namespace);
            let hinstance = GetModuleHandleW(None).map_err(|e| {
                overlay_log!("  ERROR: GetModuleHandleW failed: {}", e);
                PlatformError::Other(format!("GetModuleHandleW failed: {}", e))
            })?;

            let mut ex_style = WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW;
            if config.click_through {
                ex_style |= WS_EX_TRANSPARENT | WS_EX_NOACTIVATE;
            }
            overlay_log!("  Extended style: {:#x}", ex_style.0);

            let hwnd = CreateWindowExW(
                ex_style,
                PCWSTR(class_name.as_ptr()),
                PCWSTR(window_name.as_ptr()),
                WS_POPUP,
                abs_x,
                abs_y,
                config.width as i32,
                config.height as i32,
                None,
                None,
                hinstance,
                None,
            )
            .map_err(|e| {
                overlay_log!("  ERROR: CreateWindowExW failed: {}", e);
                PlatformError::Other(format!("CreateWindowExW failed: {}", e))
            })?;

            overlay_log!("  Window created: HWND={:?}", hwnd);
            hwnd
        };

        let mut overlay = Self {
            hwnd,
            hdc_mem: HDC::default(),
            width: config.width,
            height: config.height,
            x: abs_x,
            y: abs_y,
            pixel_data: vec![0u8; (config.width * config.height * 4) as usize],
            bgra_buffer: vec![0u8; (config.width * config.height * 4) as usize],
            content_dirty: true, // Initial render needed
            click_through: config.click_through,
            position_dirty: false,
            pointer_x: 0,
            pointer_y: 0,
            is_dragging: false,
            is_resizing: false,
            in_resize_corner: false,
            drag_enabled: true,
            pending_click: None,
            drag_start_screen_x: 0,
            drag_start_screen_y: 0,
            drag_start_win_x: abs_x,
            drag_start_win_y: abs_y,
            resize_start_x: 0,
            resize_start_y: 0,
            pending_width: config.width,
            pending_height: config.height,
            running: true,
        };

        overlay.create_dib_section()?;
        overlay_log!("  DIB section created");

        // Show window
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        overlay_log!("  Window shown");
        overlay_log!(
            "=== Overlay '{}' created successfully ===",
            config.namespace
        );

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
        let dirty = self.position_dirty;
        self.position_dirty = false;
        dirty
    }

    fn set_position(&mut self, x: i32, y: i32) {
        // Clamp position to virtual screen bounds (all monitors combined)
        let monitors = self.get_monitors();
        let (clamped_x, clamped_y) =
            super::clamp_to_virtual_screen(x, y, self.width, self.height, &monitors);

        // Skip if position unchanged
        if clamped_x == self.x && clamped_y == self.y {
            return;
        }
        self.x = clamped_x;
        self.y = clamped_y;
        self.position_dirty = true;
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                clamped_x,
                clamped_y,
                0,
                0,
                SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }

    fn set_size(&mut self, width: u32, height: u32) {
        if width == self.width && height == self.height {
            return;
        }
        self.width = width;
        self.height = height;
        self.pending_width = width;
        self.pending_height = height;

        let _ = self.create_dib_section();

        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                0,
                0,
                width as i32,
                height as i32,
                SWP_NOMOVE | SWP_NOACTIVATE,
            );
        }
    }

    fn set_click_through(&mut self, enabled: bool) {
        overlay_log!(
            "HWND={:?}: set_click_through({}) - was {}",
            self.hwnd,
            enabled,
            self.click_through
        );
        self.click_through = enabled;
        self.update_extended_style();

        if enabled {
            self.is_dragging = false;
            self.is_resizing = false;
            self.in_resize_corner = false;
        }
        overlay_log!(
            "HWND={:?}: click_through mode now {}",
            self.hwnd,
            if enabled { "LOCKED" } else { "INTERACTIVE" }
        );
    }

    fn set_drag_enabled(&mut self, enabled: bool) {
        overlay_log!("HWND={:?}: set_drag_enabled({})", self.hwnd, enabled);
        self.drag_enabled = enabled;
        if !enabled {
            // Cancel any in-progress drag
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
        if self.is_resizing {
            Some((self.pending_width, self.pending_height))
        } else {
            None
        }
    }

    fn is_interactive(&self) -> bool {
        !self.click_through
    }

    fn pixel_buffer(&mut self) -> Option<&mut [u8]> {
        self.content_dirty = true; // Assume caller will modify the buffer
        Some(&mut self.pixel_data)
    }

    fn commit(&mut self) {
        self.update_layered_window();
    }

    fn poll_events(&mut self) -> bool {
        unsafe {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, self.hwnd, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_QUIT {
                    overlay_log!("HWND={:?}: Received WM_QUIT - exiting", self.hwnd);
                    self.running = false;
                    return false;
                }

                // Handle mouse messages for drag/resize
                match msg.message {
                    WM_LBUTTONDOWN if !self.click_through => {
                        let x = (msg.lParam.0 & 0xFFFF) as i16 as i32;
                        let y = ((msg.lParam.0 >> 16) & 0xFFFF) as i16 as i32;
                        overlay_log!(
                            "HWND={:?}: WM_LBUTTONDOWN at ({},{}) drag_enabled={}",
                            self.hwnd,
                            x,
                            y,
                            self.drag_enabled
                        );

                        // Resize and drag are only available when drag_enabled (move mode)
                        // When drag_enabled=false (rearrange mode), all clicks go to the overlay
                        if self.drag_enabled {
                            if self.is_in_resize_corner(x, y) {
                                overlay_log!("  Starting resize");
                                self.is_resizing = true;
                                self.pending_width = self.width;
                                self.pending_height = self.height;
                                self.resize_start_x = x;
                                self.resize_start_y = y;
                                let _ = SetCapture(self.hwnd);
                            } else {
                                overlay_log!("  Starting drag");
                                self.is_dragging = true;
                                // Use screen coordinates for stable drag
                                let mut pt = POINT::default();
                                let _ = GetCursorPos(&mut pt);
                                self.drag_start_screen_x = pt.x;
                                self.drag_start_screen_y = pt.y;
                                self.drag_start_win_x = self.x;
                                self.drag_start_win_y = self.y;
                                let _ = SetCapture(self.hwnd);
                            }
                        } else {
                            // Drag disabled (rearrange mode) - report click to overlay
                            overlay_log!("  Storing pending click for overlay");
                            self.pending_click = Some((x as f32, y as f32));
                        }
                    }
                    WM_LBUTTONUP => {
                        if self.is_dragging || self.is_resizing {
                            overlay_log!("HWND={:?}: WM_LBUTTONUP - ending drag/resize", self.hwnd);
                        }
                        // Size is updated live during resize, no need to apply on release
                        self.is_dragging = false;
                        self.is_resizing = false;
                        let _ = ReleaseCapture();
                    }
                    WM_MOUSEMOVE if !self.click_through => {
                        let x = (msg.lParam.0 & 0xFFFF) as i16 as i32;
                        let y = ((msg.lParam.0 >> 16) & 0xFFFF) as i16 as i32;
                        self.pointer_x = x;
                        self.pointer_y = y;

                        if !self.is_resizing {
                            self.in_resize_corner = self.is_in_resize_corner(x, y);
                        }

                        if self.is_dragging {
                            // Use screen coordinates for stable drag (no oscillation)
                            let mut pt = POINT::default();
                            let _ = GetCursorPos(&mut pt);
                            let dx = pt.x - self.drag_start_screen_x;
                            let dy = pt.y - self.drag_start_screen_y;
                            self.set_position(
                                self.drag_start_win_x + dx,
                                self.drag_start_win_y + dy,
                            );
                        } else if self.is_resizing {
                            // Resize uses client coordinates (size changes, position doesn't)
                            let dx = x - self.resize_start_x;
                            let dy = y - self.resize_start_y;
                            let new_w = (self.pending_width as i32 + dx)
                                .max(MIN_OVERLAY_SIZE as i32)
                                .min(MAX_OVERLAY_WIDTH as i32)
                                as u32;
                            let new_h = (self.pending_height as i32 + dy)
                                .max(MIN_OVERLAY_SIZE as i32)
                                .min(MAX_OVERLAY_HEIGHT as i32)
                                as u32;

                            // Live resize - update immediately for visual feedback
                            if new_w != self.width || new_h != self.height {
                                self.set_size(new_w, new_h);
                                // Update resize start for next delta
                                self.resize_start_x = x;
                                self.resize_start_y = y;
                            }
                        }
                    }
                    WM_DESTROY => {
                        overlay_log!("HWND={:?}: Received WM_DESTROY - exiting!", self.hwnd);
                        self.running = false;
                        return false;
                    }
                    _ => {
                        // Log unexpected messages for debugging (but not too verbose)
                        // Common messages to ignore: WM_NCHITTEST (132), WM_SETCURSOR (32), WM_PAINT (15)
                        if msg.message != 132
                            && msg.message != 32
                            && msg.message != 15
                            && msg.message != 512
                        {
                            overlay_log!(
                                "HWND={:?}: Unhandled message: {}",
                                self.hwnd,
                                msg.message
                            );
                        }
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                }
            }
        }
        self.running
    }

    fn get_monitors(&self) -> Vec<MonitorInfo> {
        // Reuse the standalone function for consistency
        get_all_monitors()
    }
}

impl Drop for WindowsOverlay {
    fn drop(&mut self) {
        overlay_log!("HWND={:?}: Drop called - cleaning up overlay", self.hwnd);
        unsafe {
            if !self.hdc_mem.is_invalid() {
                overlay_log!("  Deleting memory DC");
                let _ = DeleteDC(self.hdc_mem);
            }
            if !self.hwnd.is_invalid() {
                overlay_log!("  Destroying window");
                let _ = DestroyWindow(self.hwnd);
            }
        }
        overlay_log!("HWND={:?}: Drop complete", self.hwnd);
    }
}

/// Window procedure for overlay windows
unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_NCHITTEST => {
            // Return HTTRANSPARENT for click-through when in locked mode
            // The actual click-through is handled by WS_EX_TRANSPARENT style
            LRESULT(HTCLIENT as isize)
        }
        WM_ERASEBKGND => LRESULT(1), // Don't erase background
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// Convert a &str to a null-terminated wide string
fn wide_string(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
