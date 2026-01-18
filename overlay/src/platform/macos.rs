//! macOS platform implementation for overlay windows
//!
//! Uses Cocoa/AppKit for transparent, always-on-top overlay windows
//! with click-through support.

use std::ffi::c_void;

// Keep cocoa imports for now (will be removed in Plan 15-03)
use cocoa::appkit::{
    NSApp, NSApplication, NSApplicationActivationPolicyAccessory, NSBackingStoreBuffered,
    NSColor, NSEvent, NSEventMask, NSEventType, NSScreen, NSView, NSWindow,
    NSWindowCollectionBehavior, NSWindowStyleMask,
};
use cocoa::base::{id, nil, NO, YES};
use cocoa::foundation::{NSArray, NSDate, NSString};

// New objc2 foundation types (replace cocoa::foundation geometry)
use objc2_foundation::{NSPoint, NSRect, NSSize};

// Keep core-graphics
use core_graphics::base::kCGImageAlphaPremultipliedFirst;
use core_graphics::color_space::CGColorSpace;
use core_graphics::context::CGContext;

// Keep old objc for ClassDecl (will be migrated in Plan 15-02)
use objc::declare::ClassDecl;
use objc::runtime::{Class, Object, Sel, BOOL};

// New objc2 for msg_send! - use both during transition
use objc2::{class, msg_send, sel};

use super::{MonitorInfo, OverlayConfig, OverlayPlatform, PlatformError};
use super::{MAX_OVERLAY_HEIGHT, MAX_OVERLAY_WIDTH, MIN_OVERLAY_SIZE, RESIZE_CORNER_SIZE};

// ─────────────────────────────────────────────────────────────────────────────
// Standalone Monitor Enumeration
// ─────────────────────────────────────────────────────────────────────────────

pub fn get_all_monitors() -> Vec<MonitorInfo> {
    unsafe {
        let screens = NSScreen::screens(nil);
        let count = NSArray::count(screens) as usize;
        let main_screen = NSScreen::mainScreen(nil);
        let main_frame = NSScreen::frame(main_screen);

        (0..count)
            .map(|i| {
                let screen: id = msg_send![screens, objectAtIndex: i];
                let frame = NSScreen::frame(screen);
                let name: id = msg_send![screen, localizedName];
                let name_str = nsstring_to_string(name);

                // macOS origin is bottom-left, convert to top-left
                let y = main_frame.size.height - frame.origin.y - frame.size.height;

                MonitorInfo {
                    id: format!("screen-{}", i),
                    name: name_str,
                    x: frame.origin.x as i32,
                    y: y as i32,
                    width: frame.size.width as u32,
                    height: frame.size.height as u32,
                    is_primary: i == 0,
                }
            })
            .collect()
    }
}

fn nsstring_to_string(nsstring: id) -> String {
    if nsstring == nil {
        return String::new();
    }
    unsafe {
        let cstr: *const i8 = msg_send![nsstring, UTF8String];
        if cstr.is_null() {
            String::new()
        } else {
            std::ffi::CStr::from_ptr(cstr)
                .to_string_lossy()
                .into_owned()
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Custom NSView for rendering
// ─────────────────────────────────────────────────────────────────────────────

static mut OVERLAY_VIEW_CLASS: Option<&'static Class> = None;

fn get_overlay_view_class() -> &'static Class {
    unsafe {
        if let Some(cls) = OVERLAY_VIEW_CLASS {
            return cls;
        }

        let superclass = class!(NSView);
        let mut decl = ClassDecl::new("BarasOverlayView", superclass).unwrap();

        decl.add_ivar::<*mut c_void>("pixelData");
        decl.add_ivar::<u32>("bufferWidth");
        decl.add_ivar::<u32>("bufferHeight");

        extern "C" fn draw_rect(this: &Object, _sel: Sel, _dirty_rect: NSRect) {
            unsafe {
                let pixel_ptr: *mut c_void = *this.get_ivar("pixelData");
                let width: u32 = *this.get_ivar("bufferWidth");
                let height: u32 = *this.get_ivar("bufferHeight");

                if pixel_ptr.is_null() || width == 0 || height == 0 {
                    return;
                }

                let bounds: NSRect = msg_send![this, bounds];
                let color_space = CGColorSpace::create_device_rgb();

                // Create CGContext from our pixel buffer (BGRA format)
                let ctx = CGContext::create_bitmap_context(
                    Some(pixel_ptr), // Already *mut c_void, no cast needed
                    width as usize,
                    height as usize,
                    8,
                    (width * 4) as usize,
                    &color_space,
                    kCGImageAlphaPremultipliedFirst, // BGRA
                );

                // create_image returns Option<CGImage>
                if let Some(image) = ctx.create_image() {
                    // Get current graphics context
                    let ns_ctx: id = msg_send![class!(NSGraphicsContext), currentContext];
                    if ns_ctx != nil {
                        let cg_ctx_ptr: *mut c_void = msg_send![ns_ctx, CGContext];

                        if !cg_ctx_ptr.is_null() {
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

        extern "C" fn is_opaque(_this: &Object, _sel: Sel) -> BOOL {
            NO
        }

        decl.add_method(
            sel!(drawRect:),
            draw_rect as extern "C" fn(&Object, Sel, NSRect),
        );
        decl.add_method(
            sel!(isOpaque),
            is_opaque as extern "C" fn(&Object, Sel) -> BOOL,
        );

        let cls = decl.register();
        OVERLAY_VIEW_CLASS = Some(cls);
        cls
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// macOS Overlay Implementation
// ─────────────────────────────────────────────────────────────────────────────

pub struct MacOSOverlay {
    window: id,
    view: id,
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
        unsafe {
            let view = self.view;
            (*view).set_ivar("pixelData", self.bgra_buffer.as_mut_ptr() as *mut c_void);
            (*view).set_ivar("bufferWidth", self.width);
            (*view).set_ivar("bufferHeight", self.height);
        }
    }

    fn is_in_resize_corner(&self, x: f64, y: f64) -> bool {
        x > (self.width as f64 - RESIZE_CORNER_SIZE as f64)
            && y > (self.height as f64 - RESIZE_CORNER_SIZE as f64)
    }
}

impl OverlayPlatform for MacOSOverlay {
    fn new(config: OverlayConfig) -> Result<Self, PlatformError> {
        unsafe {
            // Initialize app if needed
            let app = NSApp();
            app.setActivationPolicy_(NSApplicationActivationPolicyAccessory);

            // Get main screen height for coordinate conversion
            let main_screen = NSScreen::mainScreen(nil);
            let main_frame = NSScreen::frame(main_screen);
            let main_screen_height = main_frame.size.height;

            // Convert position from top-left to bottom-left origin
            let macos_y = main_screen_height - config.y as f64 - config.height as f64;

            let rect = NSRect::new(
                NSPoint::new(config.x as f64, macos_y),
                NSSize::new(config.width as f64, config.height as f64),
            );

            // Create borderless window
            let window = NSWindow::alloc(nil).initWithContentRect_styleMask_backing_defer_(
                rect,
                NSWindowStyleMask::NSBorderlessWindowMask,
                NSBackingStoreBuffered,
                NO,
            );

            if window == nil {
                return Err(PlatformError::Other("Failed to create NSWindow".into()));
            }

            // Configure window for overlay behavior
            window.setLevel_(
                cocoa::appkit::NSMainMenuWindowLevel as i64 + 1, // Above most windows
            );
            window.setBackgroundColor_(NSColor::clearColor(nil));
            window.setOpaque_(NO);
            window.setHasShadow_(NO);
            window.setIgnoresMouseEvents_(if config.click_through { YES } else { NO });

            // Prevent window from being hidden when app is deactivated
            window.setCollectionBehavior_(
                NSWindowCollectionBehavior::NSWindowCollectionBehaviorCanJoinAllSpaces
                    | NSWindowCollectionBehavior::NSWindowCollectionBehaviorStationary
                    | NSWindowCollectionBehavior::NSWindowCollectionBehaviorIgnoresCycle,
            );

            // Create custom view
            let view_class = get_overlay_view_class();
            let view: id = msg_send![view_class, alloc];
            let view: id = msg_send![view, initWithFrame: rect];

            if view == nil {
                return Err(PlatformError::Other("Failed to create NSView".into()));
            }

            window.setContentView_(view);
            window.makeKeyAndOrderFront_(nil);

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
            let _: () = msg_send![self.view, setFrame: view_rect];
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
            let _: () = msg_send![self.view, setNeedsDisplay: true];
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
