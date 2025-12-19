//! Windows platform implementation for overlay windows
//!
//! Uses Win32 API to create transparent, always-on-top overlay windows
//! with click-through support.

use std::mem;
use std::ptr;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, GetCurrentObject, GetDC, ReleaseDC, SetDIBits,
    SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HDC, HBITMAP, HGDIOBJ,
    OBJ_BITMAP,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, LoadCursorW, PeekMessageW,
    RegisterClassExW, SetWindowLongPtrW, SetWindowPos, ShowWindow, TranslateMessage,
    UpdateLayeredWindow, GetCursorPos,
    CS_HREDRAW, CS_VREDRAW, GWL_EXSTYLE, HTCLIENT, HWND_TOPMOST, IDC_ARROW, MSG, PM_REMOVE,
    SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, ULW_ALPHA, WM_DESTROY,
    WM_ERASEBKGND, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCHITTEST, WM_QUIT,
    WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_EX_TRANSPARENT, WS_POPUP,
};

use super::{OverlayConfig, OverlayPlatform, PlatformError};

/// Size constraints for overlays
const MIN_OVERLAY_SIZE: u32 = 100;
const MAX_OVERLAY_WIDTH: u32 = 300;
const MAX_OVERLAY_HEIGHT: u32 = 700;
const RESIZE_CORNER_SIZE: i32 = 20;

/// Windows overlay implementation
pub struct WindowsOverlay {
    hwnd: HWND,
    hdc_mem: HDC,
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    pixel_data: Vec<u8>,
    click_through: bool,
    position_dirty: bool,

    // Interaction state
    pointer_x: i32,
    pointer_y: i32,
    is_dragging: bool,
    is_resizing: bool,
    in_resize_corner: bool,
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
                .map_err(|e| PlatformError::BufferError(format!("CreateDIBSection failed: {}", e)))?;

            SelectObject(self.hdc_mem, hbitmap);
            ReleaseDC(HWND::default(), hdc_screen);

            // Resize pixel buffer
            let size = (self.width * self.height * 4) as usize;
            self.pixel_data.resize(size, 0);
        }
        Ok(())
    }

    fn update_layered_window(&self) {
        unsafe {
            // Copy RGBA pixel data to DIB (convert RGBA to BGRA)
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

            // Convert RGBA to BGRA and write directly
            let bgra_data: Vec<u8> = self
                .pixel_data
                .chunks(4)
                .flat_map(|chunk| {
                    if chunk.len() == 4 {
                        vec![chunk[2], chunk[1], chunk[0], chunk[3]] // BGRA
                    } else {
                        vec![0, 0, 0, 0]
                    }
                })
                .collect();

            // Get the bitmap handle from the DC
            let hgdiobj = GetCurrentObject(self.hdc_mem, OBJ_BITMAP);
            let hbitmap = HBITMAP(hgdiobj.0);
            SetDIBits(
                self.hdc_mem,
                hbitmap,
                0,
                self.height,
                bgra_data.as_ptr() as *const _,
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
                BlendOp: 0,              // AC_SRC_OVER
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: 1,          // AC_SRC_ALPHA
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
        x > (self.width as i32 - RESIZE_CORNER_SIZE) && y > (self.height as i32 - RESIZE_CORNER_SIZE)
    }

    fn update_extended_style(&self) {
        unsafe {
            let mut ex_style = WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW;
            if self.click_through {
                ex_style |= WS_EX_TRANSPARENT | WS_EX_NOACTIVATE;
            }
            SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, ex_style.0 as isize);
        }
    }
}

impl OverlayPlatform for WindowsOverlay {
    fn new(config: OverlayConfig) -> Result<Self, PlatformError> {
        Self::register_class()?;

        let hwnd = unsafe {
            let class_name = wide_string("BarasOverlayClass");
            let window_name = wide_string(&config.namespace);
            let hinstance = GetModuleHandleW(None)
                .map_err(|e| PlatformError::Other(format!("GetModuleHandleW failed: {}", e)))?;

            let mut ex_style = WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW;
            if config.click_through {
                ex_style |= WS_EX_TRANSPARENT | WS_EX_NOACTIVATE;
            }

            let hwnd = CreateWindowExW(
                ex_style,
                PCWSTR(class_name.as_ptr()),
                PCWSTR(window_name.as_ptr()),
                WS_POPUP,
                config.x,
                config.y,
                config.width as i32,
                config.height as i32,
                None,
                None,
                hinstance,
                None,
            )
            .map_err(|e| PlatformError::Other(format!("CreateWindowExW failed: {}", e)))?;

            hwnd
        };

        let mut overlay = Self {
            hwnd,
            hdc_mem: HDC::default(),
            width: config.width,
            height: config.height,
            x: config.x,
            y: config.y,
            pixel_data: vec![0u8; (config.width * config.height * 4) as usize],
            click_through: config.click_through,
            position_dirty: false,
            pointer_x: 0,
            pointer_y: 0,
            is_dragging: false,
            is_resizing: false,
            in_resize_corner: false,
            drag_start_screen_x: 0,
            drag_start_screen_y: 0,
            drag_start_win_x: config.x,
            drag_start_win_y: config.y,
            resize_start_x: 0,
            resize_start_y: 0,
            pending_width: config.width,
            pending_height: config.height,
            running: true,
        };

        overlay.create_dib_section()?;

        // Show window
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }

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
        self.x = x;
        self.y = y;
        self.position_dirty = true;
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                x,
                y,
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
        self.click_through = enabled;
        self.update_extended_style();

        if enabled {
            self.is_dragging = false;
            self.is_resizing = false;
            self.in_resize_corner = false;
        }
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
                    self.running = false;
                    return false;
                }

                // Handle mouse messages for drag/resize
                match msg.message {
                    WM_LBUTTONDOWN if !self.click_through => {
                        let x = (msg.lParam.0 & 0xFFFF) as i16 as i32;
                        let y = ((msg.lParam.0 >> 16) & 0xFFFF) as i16 as i32;

                        if self.is_in_resize_corner(x, y) {
                            self.is_resizing = true;
                            self.pending_width = self.width;
                            self.pending_height = self.height;
                            self.resize_start_x = x;
                            self.resize_start_y = y;
                        } else {
                            self.is_dragging = true;
                            // Use screen coordinates for stable drag
                            let mut pt = POINT::default();
                            let _ = GetCursorPos(&mut pt);
                            self.drag_start_screen_x = pt.x;
                            self.drag_start_screen_y = pt.y;
                            self.drag_start_win_x = self.x;
                            self.drag_start_win_y = self.y;
                        }
                        let _ = SetCapture(self.hwnd);
                    }
                    WM_LBUTTONUP => {
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
                            self.set_position(self.drag_start_win_x + dx, self.drag_start_win_y + dy);
                        } else if self.is_resizing {
                            // Resize uses client coordinates (size changes, position doesn't)
                            let dx = x - self.resize_start_x;
                            let dy = y - self.resize_start_y;
                            let new_w = (self.pending_width as i32 + dx).max(MIN_OVERLAY_SIZE as i32).min(MAX_OVERLAY_WIDTH as i32) as u32;
                            let new_h = (self.pending_height as i32 + dy).max(MIN_OVERLAY_SIZE as i32).min(MAX_OVERLAY_HEIGHT as i32) as u32;

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
                        self.running = false;
                        return false;
                    }
                    _ => {
                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                }
            }
        }
        self.running
    }
}

impl Drop for WindowsOverlay {
    fn drop(&mut self) {
        unsafe {
            if !self.hdc_mem.is_invalid() {
                DeleteDC(self.hdc_mem);
            }
            if !self.hwnd.is_invalid() {
                let _ = DestroyWindow(self.hwnd);
            }
        }
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
