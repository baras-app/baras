//! macOS platform implementation for overlay windows
//!
//! Uses objc2-app-kit for transparent, always-on-top overlay windows
//! with click-through support.

use std::cell::Cell;
use std::ffi::c_void;

// objc2 core
use objc2::rc::Retained;
use objc2::{define_class, msg_send, DeclaredClass};

// objc2-foundation types
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

// objc2-app-kit types
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSColor, NSEvent,
    NSEventMask, NSEventType, NSGraphicsContext, NSScreen, NSView, NSWindow,
    NSWindowCollectionBehavior, NSWindowLevel, NSWindowStyleMask,
};

// Keep core-graphics for CGContext operations
use core_graphics::base::kCGImageAlphaPremultipliedFirst;
use core_graphics::color_space::CGColorSpace;
use core_graphics::context::CGContext;

use super::{MonitorInfo, OverlayConfig, OverlayPlatform, PlatformError};
use super::{MAX_OVERLAY_HEIGHT, MAX_OVERLAY_WIDTH, MIN_OVERLAY_SIZE, RESIZE_CORNER_SIZE};

// ─────────────────────────────────────────────────────────────────────────────
// Standalone Monitor Enumeration
// ─────────────────────────────────────────────────────────────────────────────

pub fn get_all_monitors() -> Vec<MonitorInfo> {
    let screens = NSScreen::screens();
    let Some(main_screen) = NSScreen::mainScreen() else {
        return Vec::new();
    };
    let main_frame = main_screen.frame();

    screens
        .iter()
        .enumerate()
        .map(|(i, screen)| {
            let frame = screen.frame();
            let name = screen
                .localizedName()
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("Display {}", i + 1));

            // macOS origin is bottom-left, convert to top-left
            let y = main_frame.size.height - frame.origin.y - frame.size.height;

            MonitorInfo {
                id: format!("screen-{}", i),
                name,
                x: frame.origin.x as i32,
                y: y as i32,
                width: frame.size.width as u32,
                height: frame.size.height as u32,
                is_primary: i == 0,
            }
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// Custom NSView for rendering
// ─────────────────────────────────────────────────────────────────────────────

/// Instance variables for BarasOverlayView.
/// Uses Cell<T> for interior mutability since objc2 methods take &self.
#[derive(Default)]
struct BarasOverlayViewIvars {
    pixel_data: Cell<*mut c_void>,
    buffer_width: Cell<u32>,
    buffer_height: Cell<u32>,
}

// SAFETY: BarasOverlayView is only used on the main thread (AppKit requirement)
// and pixel_data pointer is only accessed during drawRect: which is single-threaded.
// The raw pointer points to MacOSOverlay's bgra_buffer which lives for the overlay lifetime.
unsafe impl Send for BarasOverlayViewIvars {}
unsafe impl Sync for BarasOverlayViewIvars {}

define_class!(
    // SAFETY: NSView permits subclassing for custom drawing.
    // We override drawRect: and isOpaque, both designed to be overridden.
    // We do not implement Drop - cleanup happens via Objective-C mechanisms.
    #[unsafe(super(objc2_app_kit::NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "BarasOverlayView"]
    #[ivars = BarasOverlayViewIvars]
    pub struct BarasOverlayView;

    impl BarasOverlayView {
        /// Draw the overlay content from the pixel buffer.
        /// NSRect implements Encode trait - validated at compile time by objc2.
        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty_rect: NSRect) {
            let ivars = self.ivars();
            let pixel_ptr = ivars.pixel_data.get();
            let width = ivars.buffer_width.get();
            let height = ivars.buffer_height.get();

            if pixel_ptr.is_null() || width == 0 || height == 0 {
                return;
            }

            unsafe {
                let bounds: NSRect = msg_send![self, bounds];
                let color_space = CGColorSpace::create_device_rgb();

                // Create CGContext from our pixel buffer (BGRA format)
                let ctx = CGContext::create_bitmap_context(
                    Some(pixel_ptr),
                    width as usize,
                    height as usize,
                    8,
                    (width * 4) as usize,
                    &color_space,
                    kCGImageAlphaPremultipliedFirst,
                );

                // create_image returns Option<CGImage>
                if let Some(image) = ctx.create_image() {
                    // Get current graphics context using objc2-app-kit
                    // NSGraphicsContext::currentContext() returns Option<Retained<NSGraphicsContext>>
                    if let Some(ns_ctx) = NSGraphicsContext::currentContext() {
                        // Bridge to core-graphics CGContext:
                        // NSGraphicsContext.CGContext returns the underlying CGContextRef
                        let cg_ctx_ptr: *mut c_void = msg_send![&*ns_ctx, CGContext];

                        if !cg_ctx_ptr.is_null() {
                            // core-graphics CGContext wraps the raw CGContextRef
                            let cg_ctx = CGContext::from_existing_context_ptr(
                                cg_ctx_ptr as *mut core_graphics::sys::CGContext,
                            );

                            cg_ctx.draw_image(
                                core_graphics::geometry::CGRect::new(
                                    &core_graphics::geometry::CGPoint::new(0.0, 0.0),
                                    &core_graphics::geometry::CGSize::new(
                                        bounds.size.width,
                                        bounds.size.height,
                                    ),
                                ),
                                &image,
                            );
                        }
                    }
                }
            }
        }

        /// Report that the view is not opaque to enable transparency.
        #[unsafe(method(isOpaque))]
        fn is_opaque(&self) -> bool {
            false
        }
    }
);

impl BarasOverlayView {
    /// Create a new BarasOverlayView with the given frame.
    fn new(frame: NSRect) -> Retained<Self> {
        let this = Self::alloc();
        let ivars = BarasOverlayViewIvars::default();
        // SAFETY: Calling NSView's initWithFrame: designated initializer
        unsafe { msg_send![super(this.set_ivars(ivars)), initWithFrame: frame] }
    }

    /// Update the pixel buffer pointer and dimensions.
    fn set_pixel_data(&self, data: *mut c_void, width: u32, height: u32) {
        let ivars = self.ivars();
        ivars.pixel_data.set(data);
        ivars.buffer_width.set(width);
        ivars.buffer_height.set(height);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// macOS Overlay Implementation
// ─────────────────────────────────────────────────────────────────────────────

pub struct MacOSOverlay {
    window: Retained<NSWindow>,
    view: Retained<BarasOverlayView>,
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    pixel_data: Vec<u8>, // RGBA from renderer
    bgra_buffer: Vec<u8>, // BGRA for Core Graphics

    // Interaction state
    click_through: bool,
    drag_enabled: bool,
    is_dragging: bool,
    is_resizing: bool,
    in_resize_corner: bool,
    position_dirty: bool,
    pending_click: Option<(f32, f32)>,

    // Drag tracking
    drag_start_x: f64,
    drag_start_y: f64,
    drag_start_win_x: i32,
    drag_start_win_y: i32,

    // Resize tracking
    resize_start_x: f64,
    resize_start_y: f64,
    pending_width: u32,
    pending_height: u32,

    running: bool,
    main_screen_height: f64, // For coordinate conversion
}

impl MacOSOverlay {
    fn convert_y(&self, y: i32, height: u32) -> f64 {
        // Convert top-left origin to bottom-left origin
        self.main_screen_height - y as f64 - height as f64
    }

    fn convert_y_back(&self, y: f64, height: u32) -> i32 {
        // Convert bottom-left origin to top-left origin
        (self.main_screen_height - y - height as f64) as i32
    }

    fn update_view_buffer(&mut self) {
        self.view.set_pixel_data(
            self.bgra_buffer.as_mut_ptr() as *mut c_void,
            self.width,
            self.height,
        );
    }

    fn is_in_resize_corner(&self, x: f64, y: f64) -> bool {
        x > (self.width as f64 - RESIZE_CORNER_SIZE as f64)
            && y > (self.height as f64 - RESIZE_CORNER_SIZE as f64)
    }
}

impl OverlayPlatform for MacOSOverlay {
    fn new(config: OverlayConfig) -> Result<Self, PlatformError> {
        // objc2-app-kit operations require running on the main thread
        // For overlay use cases, we're always on the main thread during init

        unsafe {
            // Initialize app if needed - use objc2-app-kit
            let app = NSApplication::sharedApplication();
            app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

            // Get main screen height for coordinate conversion
            let main_screen = NSScreen::mainScreen()
                .ok_or_else(|| PlatformError::Other("No main screen".into()))?;
            let main_frame = main_screen.frame();
            let main_screen_height = main_frame.size.height;

            // Convert position from top-left to bottom-left origin
            let macos_y = main_screen_height - config.y as f64 - config.height as f64;

            let rect = NSRect::new(
                NSPoint::new(config.x as f64, macos_y),
                NSSize::new(config.width as f64, config.height as f64),
            );

            // Create borderless window using objc2-app-kit
            let window: Retained<NSWindow> = {
                let alloc = NSWindow::alloc();
                NSWindow::initWithContentRect_styleMask_backing_defer(
                    alloc,
                    rect,
                    NSWindowStyleMask::Borderless,
                    NSBackingStoreType::Buffered,
                    false,
                )
            };

            // CRITICAL: Prevent window from being released when closed (MAC-04)
            // This is required for correct memory management when not using a window controller
            window.setReleasedWhenClosed(false);

            // Configure window for overlay behavior
            // NSMainMenuWindowLevel (24) + 1 = 25
            window.setLevel(NSWindowLevel(25));
            window.setBackgroundColor(Some(&NSColor::clearColor()));
            window.setOpaque(false);
            window.setHasShadow(false);
            window.setIgnoresMouseEvents(config.click_through);

            // Prevent window from being hidden when app is deactivated
            window.setCollectionBehavior(
                NSWindowCollectionBehavior::CanJoinAllSpaces
                    | NSWindowCollectionBehavior::Stationary
                    | NSWindowCollectionBehavior::IgnoresCycle,
            );

            // Create custom view using our define_class! defined view
            let view = BarasOverlayView::new(rect);

            // Set view as window's content
            window.setContentView(Some(&view));
            window.makeKeyAndOrderFront(None);

            let size = (config.width * config.height * 4) as usize;
            let mut overlay = MacOSOverlay {
                window,
                view,
                width: config.width,
                height: config.height,
                x: config.x,
                y: config.y,
                pixel_data: vec![0u8; size],
                bgra_buffer: vec![0u8; size],
                click_through: config.click_through,
                drag_enabled: true,
                is_dragging: false,
                is_resizing: false,
                in_resize_corner: false,
                position_dirty: false,
                pending_click: None,
                drag_start_x: 0.0,
                drag_start_y: 0.0,
                drag_start_win_x: config.x,
                drag_start_win_y: config.y,
                resize_start_x: 0.0,
                resize_start_y: 0.0,
                pending_width: config.width,
                pending_height: config.height,
                running: true,
                main_screen_height,
            };

            overlay.update_view_buffer();

            Ok(overlay)
        }
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

        unsafe {
            let macos_y = self.convert_y(cy, self.height);
            let point = NSPoint::new(cx as f64, macos_y);
            self.window.setFrameOrigin_(point);
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

        let size = (width * height * 4) as usize;
        self.pixel_data.resize(size, 0);
        self.bgra_buffer.resize(size, 0);

        unsafe {
            let macos_y = self.convert_y(self.y, height);
            let rect = NSRect::new(
                NSPoint::new(self.x as f64, macos_y),
                NSSize::new(width as f64, height as f64),
            );
            self.window.setFrame_display_(rect, YES);

            // Update view frame
            let view_rect = NSRect::new(
                NSPoint::new(0.0, 0.0),
                NSSize::new(width as f64, height as f64),
            );
            let _: () = msg_send![&*self.view, setFrame: view_rect];
        }

        self.update_view_buffer();
    }

    fn set_click_through(&mut self, enabled: bool) {
        self.click_through = enabled;
        unsafe {
            self.window
                .setIgnoresMouseEvents_(if enabled { YES } else { NO });
        }

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
        // Convert RGBA to BGRA (Core Graphics expects BGRA with premultiplied alpha)
        for (i, chunk) in self.pixel_data.chunks(4).enumerate() {
            let offset = i * 4;
            if chunk.len() == 4 && offset + 3 < self.bgra_buffer.len() {
                let a = chunk[3] as u32;
                // Premultiply alpha
                self.bgra_buffer[offset] = ((chunk[2] as u32 * a) / 255) as u8; // B
                self.bgra_buffer[offset + 1] = ((chunk[1] as u32 * a) / 255) as u8; // G
                self.bgra_buffer[offset + 2] = ((chunk[0] as u32 * a) / 255) as u8; // R
                self.bgra_buffer[offset + 3] = chunk[3]; // A
            }
        }

        unsafe {
            let _: () = msg_send![&*self.view, setNeedsDisplay: true];
        }
    }

    fn poll_events(&mut self) -> bool {
        unsafe {
            // Process all pending events
            loop {
                let event: id = msg_send![
                    NSApp(),
                    nextEventMatchingMask: NSEventMask::NSAnyEventMask,
                    untilDate: nil,
                    inMode: NSString::alloc(nil).init_str("kCFRunLoopDefaultMode"),
                    dequeue: true
                ];

                if event == nil {
                    break;
                }

                let event_type = event.eventType();

                // Handle events for our window when interactive
                if !self.click_through {
                    let event_window: id = msg_send![event, window];
                    if event_window == self.window {
                        match event_type {
                            NSEventType::NSLeftMouseDown => {
                                let loc = event.locationInWindow();
                                // Convert from bottom-left to top-left within window
                                let x = loc.x;
                                let y = self.height as f64 - loc.y;

                                if self.drag_enabled {
                                    if self.is_in_resize_corner(x, y) {
                                        self.is_resizing = true;
                                        self.pending_width = self.width;
                                        self.pending_height = self.height;
                                        self.resize_start_x = loc.x;
                                        self.resize_start_y = loc.y;
                                    } else {
                                        self.is_dragging = true;
                                        let mouse_loc = NSEvent::mouseLocation(nil);
                                        self.drag_start_x = mouse_loc.x;
                                        self.drag_start_y = mouse_loc.y;
                                        self.drag_start_win_x = self.x;
                                        self.drag_start_win_y = self.y;
                                    }
                                } else {
                                    self.pending_click = Some((x as f32, y as f32));
                                }
                            }
                            NSEventType::NSLeftMouseUp => {
                                self.is_dragging = false;
                                self.is_resizing = false;
                            }
                            NSEventType::NSMouseMoved | NSEventType::NSLeftMouseDragged => {
                                let loc = event.locationInWindow();
                                let x = loc.x;
                                let y = self.height as f64 - loc.y;

                                if !self.is_resizing {
                                    self.in_resize_corner = self.is_in_resize_corner(x, y);
                                }

                                if self.is_dragging {
                                    let mouse_loc = NSEvent::mouseLocation(nil);
                                    let dx = mouse_loc.x - self.drag_start_x;
                                    let dy = self.drag_start_y - mouse_loc.y; // Flip Y
                                    self.set_position(
                                        self.drag_start_win_x + dx as i32,
                                        self.drag_start_win_y + dy as i32,
                                    );
                                } else if self.is_resizing {
                                    let dx = loc.x - self.resize_start_x;
                                    let dy = self.resize_start_y - loc.y; // Flip Y

                                    let new_w = (self.pending_width as i32 + dx as i32)
                                        .clamp(MIN_OVERLAY_SIZE as i32, MAX_OVERLAY_WIDTH as i32)
                                        as u32;
                                    let new_h = (self.pending_height as i32 + dy as i32)
                                        .clamp(MIN_OVERLAY_SIZE as i32, MAX_OVERLAY_HEIGHT as i32)
                                        as u32;

                                    if new_w != self.width || new_h != self.height {
                                        self.set_size(new_w, new_h);
                                        self.resize_start_x = loc.x;
                                        self.resize_start_y = loc.y;
                                    }
                                }
                            }
                            NSEventType::NSMouseExited => {
                                if !self.is_resizing {
                                    self.in_resize_corner = false;
                                }
                            }
                            _ => {}
                        }
                    }
                }

                // Forward event to app
                let _: () = msg_send![NSApp(), sendEvent: event];
            }
        }
        self.running
    }

    fn get_monitors(&self) -> Vec<MonitorInfo> {
        get_all_monitors()
    }
}

impl Drop for MacOSOverlay {
    fn drop(&mut self) {
        unsafe {
            self.window.close();
        }
    }
}
