# Features Research: macOS Overlay APIs

**Domain:** macOS overlay window rendering for BARAS
**Researched:** 2026-01-18
**Confidence:** HIGH (verified against official Apple documentation and objc2 crate docs)

## Executive Summary

The existing BARAS macOS implementation uses deprecated crates (`cocoa` 0.26, `objc` 0.2). The recommended migration path is to the `objc2` ecosystem, which provides modern, type-safe bindings generated from Xcode 16.4 SDKs. The core APIs remain the same conceptually, but the Rust interface is significantly improved.

---

## Window Creation

### Required APIs

| API | Purpose | objc2 Crate |
|-----|---------|-------------|
| `NSWindow::initWithContentRect_styleMask_backing_defer` | Create borderless window | `objc2-app-kit` |
| `NSWindowStyleMask::Borderless` | Remove window chrome | `objc2-app-kit` |
| `NSBackingStoreType::Buffered` | Double-buffered rendering | `objc2-app-kit` |
| `NSApplication::sharedApplication` | Get app singleton | `objc2-app-kit` |
| `NSApplication::setActivationPolicy` | Set as accessory (no dock icon) | `objc2-app-kit` |

### Implementation Notes

```rust
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType,
    NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{NSPoint, NSRect, NSSize};

// Create borderless window
let rect = NSRect::new(
    NSPoint::new(x as f64, y as f64),
    NSSize::new(width as f64, height as f64),
);

// SAFETY: Window not in window controller, must set releasedWhenClosed(false)
let window = unsafe {
    NSWindow::initWithContentRect_styleMask_backing_defer(
        NSWindow::alloc(),
        rect,
        NSWindowStyleMask::Borderless,
        NSBackingStoreType::Buffered,
        false,
    )
};
window.setReleasedWhenClosed(false); // Critical for memory safety
```

### Gotchas

1. **Memory Management:** When creating `NSWindow` outside a window controller, you MUST call `window.setReleasedWhenClosed(false)` to prevent use-after-free. The objc2-app-kit docs explicitly warn about this.

2. **Activation Policy:** Use `NSApplicationActivationPolicy::Accessory` to prevent dock icon and app switcher appearance.

---

## Transparency & Layering

### Required APIs

| API | Purpose | objc2 Crate |
|-----|---------|-------------|
| `NSWindow::setLevel` | Set window z-order | `objc2-app-kit` |
| `NSWindow::setBackgroundColor` | Set to clear for transparency | `objc2-app-kit` |
| `NSWindow::setOpaque` | Disable opaque optimization | `objc2-app-kit` |
| `NSWindow::setHasShadow` | Disable window shadow | `objc2-app-kit` |
| `NSWindow::setIgnoresMouseEvents` | Enable click-through | `objc2-app-kit` |
| `NSWindow::setCollectionBehavior` | Multi-space/stationary behavior | `objc2-app-kit` |
| `NSColor::clearColor` | Transparent background color | `objc2-app-kit` |
| `CGWindowLevelForKey` | Get overlay window level | `objc2-core-graphics` |

### Window Level Configuration

```rust
use objc2_app_kit::{NSWindow, NSWindowLevel, NSWindowCollectionBehavior};
use objc2_core_graphics::{CGWindowLevelKey, CGWindowLevelForKey};

// Window levels (numeric values for reference):
// - kCGNormalWindowLevel = 0
// - kCGFloatingWindowLevel = 3
// - kCGMainMenuWindowLevel = 24
// - kCGStatusWindowLevel = 25
// - kCGOverlayWindowLevel = 102  <-- Recommended for overlays
// - kCGScreenSaverWindowLevel = 1000

// Set window to overlay level (above most windows)
let level = unsafe { CGWindowLevelForKey(CGWindowLevelKey::OverlayWindow) };
window.setLevel(NSWindowLevel(level as isize));

// Alternative: mainMenuWindowLevel + 1 (current implementation uses this)
// This is level 25, which may be insufficient for some use cases
```

### Collection Behavior for Multi-Space Support

```rust
// Make window visible on all Spaces and unaffected by Expose/Mission Control
window.setCollectionBehavior(
    NSWindowCollectionBehavior::CanJoinAllSpaces |
    NSWindowCollectionBehavior::Stationary |
    NSWindowCollectionBehavior::IgnoresCycle
);

// Bit values:
// - CanJoinAllSpaces = 1 << 0 (window appears in all Spaces)
// - MoveToActiveSpace = 1 << 1
// - Managed = 1 << 2
// - Transient = 1 << 3
// - Stationary = 1 << 4 (unaffected by Mission Control)
// - ParticipatesInCycle = 1 << 5
// - IgnoresCycle = 1 << 6 (not in Cmd+` window cycle)
```

### Click-Through Mode

```rust
// Enable click-through (events pass to windows below)
window.setIgnoresMouseEvents(true);

// Disable click-through (window receives events)
window.setIgnoresMouseEvents(false);
```

### Gotchas

1. **macOS Sonoma Bug:** There's a known issue where after multiple calls to `setNeedsDisplay:YES`, transparent windows may stop being click-through. Monitor for this in testing.

2. **Full-Screen Apps:** `CanJoinAllSpaces` does NOT make the overlay appear over full-screen app windows. This is a macOS limitation - full-screen apps use a separate space.

3. **Window Level Selection:** `kCGOverlayWindowLevel` (102) is recommended over `kCGMainMenuWindowLevel + 1` (25) for more reliable overlay behavior.

---

## Pixel Buffer Rendering

### Required APIs

| API | Purpose | objc2 Crate |
|-----|---------|-------------|
| `define_class!` macro | Create custom NSView subclass | `objc2` |
| `NSView::setNeedsDisplay` | Trigger redraw | `objc2-app-kit` |
| `NSGraphicsContext::currentContext` | Get drawing context | `objc2-app-kit` |
| `NSGraphicsContext::CGContext` | Get CGContext from NSGraphicsContext | `objc2-app-kit` |
| `CGBitmapContextCreate` | Create bitmap context from buffer | `objc2-core-graphics` |
| `CGBitmapContextCreateImage` | Create CGImage from bitmap context | `objc2-core-graphics` |
| `CGContextDrawImage` | Draw image to context | `objc2-core-graphics` |
| `CGColorSpaceCreateDeviceRGB` | Create RGB color space | `objc2-core-graphics` |

### Pixel Format Configuration

tiny-skia outputs RGBA (unpremultiplied). Core Graphics requires premultiplied alpha in specific formats:

| tiny-skia Format | Core Graphics Format | CGBitmapInfo |
|------------------|---------------------|--------------|
| RGBA | RGBA premultiplied | `kCGBitmapByteOrder32Big \| kCGImageAlphaPremultipliedLast` |
| RGBA | BGRA premultiplied | `kCGBitmapByteOrder32Little \| kCGImageAlphaPremultipliedFirst` |

**Current implementation uses BGRA premultiplied** (`kCGImageAlphaPremultipliedFirst`), which requires manual conversion from RGBA.

### Custom NSView Subclass with objc2

```rust
use objc2::{define_class, msg_send, rc::Retained, sel};
use objc2_app_kit::{NSGraphicsContext, NSView};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGBitmapContextCreateImage, CGBitmapInfo,
    CGColorSpace, CGColorSpaceCreateDeviceRGB, CGContextDrawImage,
    CGImageAlphaInfo, CGRect,
};
use objc2_foundation::NSRect;
use std::cell::Cell;
use std::ptr::NonNull;

#[derive(Default)]
struct OverlayViewIvars {
    pixel_data: Cell<*mut u8>,
    buffer_width: Cell<u32>,
    buffer_height: Cell<u32>,
}

define_class! {
    #[unsafe(super(NSView))]
    #[name = "BarasOverlayView"]
    #[ivars = OverlayViewIvars]
    pub struct OverlayView;

    impl OverlayView {
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
                let color_space = CGColorSpaceCreateDeviceRGB();

                // Create bitmap context from pixel buffer
                // Using BGRA with premultiplied alpha (matches current impl)
                let bitmap_info = CGBitmapInfo(
                    CGImageAlphaInfo::PremultipliedFirst as u32 |
                    CGBitmapInfo::ByteOrder32Little.0
                );

                let ctx = CGBitmapContextCreate(
                    NonNull::new(pixel_ptr as *mut _),
                    width as usize,
                    height as usize,
                    8,                          // bits per component
                    (width * 4) as usize,       // bytes per row
                    Some(&color_space),
                    bitmap_info,
                );

                if let Some(ctx) = ctx {
                    if let Some(image) = CGBitmapContextCreateImage(&ctx) {
                        // Get current graphics context
                        if let Some(ns_ctx) = NSGraphicsContext::currentContext() {
                            let cg_ctx = ns_ctx.CGContext();
                            let bounds = self.bounds();

                            let rect = CGRect::new(
                                CGPoint::new(0.0, 0.0),
                                CGSize::new(bounds.size.width, bounds.size.height),
                            );

                            CGContextDrawImage(&cg_ctx, rect, &image);
                        }
                    }
                }
            }
        }

        #[unsafe(method(isOpaque))]
        fn is_opaque(&self) -> bool {
            false  // Required for transparency
        }
    }
}
```

### RGBA to BGRA Premultiplied Conversion

```rust
fn convert_rgba_to_bgra_premultiplied(rgba: &[u8], bgra: &mut [u8]) {
    for (i, chunk) in rgba.chunks_exact(4).enumerate() {
        let offset = i * 4;
        let r = chunk[0] as u32;
        let g = chunk[1] as u32;
        let b = chunk[2] as u32;
        let a = chunk[3] as u32;

        // Premultiply alpha and swap R/B
        bgra[offset]     = ((b * a) / 255) as u8;  // B
        bgra[offset + 1] = ((g * a) / 255) as u8;  // G
        bgra[offset + 2] = ((r * a) / 255) as u8;  // R
        bgra[offset + 3] = a as u8;                 // A
    }
}
```

### Gotchas

1. **Premultiplied Alpha Required:** Core Graphics ONLY accepts premultiplied alpha for RGB color spaces. Non-premultiplied (`kCGImageAlphaLast`) will fail.

2. **Supported Combinations:** Valid 32-bit per pixel combinations:
   - `kCGImageAlphaPremultipliedFirst` (ARGB/BGRA)
   - `kCGImageAlphaPremultipliedLast` (RGBA)
   - `kCGImageAlphaNoneSkipFirst` (XRGB)
   - `kCGImageAlphaNoneSkipLast` (RGBX)

3. **Byte Order Matters:**
   - `kCGBitmapByteOrder32Big` = RGBA/ARGB (network byte order)
   - `kCGBitmapByteOrder32Little` = BGRA/ABGR (little-endian, common on Intel/Apple Silicon)

4. **View Must Return `false` from `isOpaque`:** Otherwise AppKit may optimize away transparency.

---

## Event Handling

### Required APIs

| API | Purpose | objc2 Crate |
|-----|---------|-------------|
| `NSApplication::nextEventMatchingMask_untilDate_inMode_dequeue` | Poll for events | `objc2-app-kit` |
| `NSApplication::sendEvent` | Dispatch event to window | `objc2-app-kit` |
| `NSEvent::type` | Get event type | `objc2-app-kit` |
| `NSEvent::locationInWindow` | Get mouse position in window coords | `objc2-app-kit` |
| `NSEvent::mouseLocation` | Get global mouse position | `objc2-app-kit` |
| `NSEvent::window` | Get event's target window | `objc2-app-kit` |

### Non-Blocking Event Loop

```rust
use objc2_app_kit::{NSApplication, NSEvent, NSEventMask, NSEventType};
use objc2_foundation::{NSDate, ns_string};

fn poll_events(app: &NSApplication, window: &NSWindow) -> bool {
    loop {
        // Poll for next event (non-blocking with nil date)
        let event = unsafe {
            app.nextEventMatchingMask_untilDate_inMode_dequeue(
                NSEventMask::Any,
                None,                                    // nil = don't wait
                ns_string!("kCFRunLoopDefaultMode"),
                true,                                    // dequeue
            )
        };

        let Some(event) = event else {
            break; // No more events
        };

        let event_type = unsafe { event.type_() };
        let event_window = event.window();

        // Handle events for our window when interactive
        if event_window.as_ref() == Some(window) {
            match event_type {
                NSEventType::LeftMouseDown => {
                    let loc = event.locationInWindow();
                    // loc.y is from bottom-left, convert to top-left:
                    let y = window.frame().size.height - loc.y;
                    handle_mouse_down(loc.x, y);
                }
                NSEventType::LeftMouseUp => {
                    handle_mouse_up();
                }
                NSEventType::LeftMouseDragged | NSEventType::MouseMoved => {
                    let loc = event.locationInWindow();
                    handle_mouse_move(loc.x, loc.y);
                }
                _ => {}
            }
        }

        // Forward event to app for standard handling
        unsafe { app.sendEvent(&event) };
    }

    true // Continue running
}
```

### Coordinate System Conversion

macOS uses bottom-left origin; most UI frameworks use top-left:

```rust
// Window coordinates: bottom-left origin
let loc = event.locationInWindow();

// Convert to top-left origin (for overlay rendering)
let window_height = window.frame().size.height;
let y_top_left = window_height - loc.y;

// For global coordinates (screen space)
let global_loc = NSEvent::mouseLocation();
let main_screen_height = NSScreen::mainScreen().unwrap().frame().size.height;
let global_y_top_left = main_screen_height - global_loc.y;
```

### Gotchas

1. **Coordinate Origin:** AppKit uses bottom-left origin. Convert to top-left for consistency with other platforms and typical UI expectations.

2. **Y Coordinates Are 1-Based:** A click at the absolute bottom-left returns `(0, 1)` not `(0, 0)`. This is a Cocoa quirk.

3. **locationInWindow with nil window:** If `event.window()` returns `None`, `locationInWindow()` returns screen coordinates, not window coordinates.

4. **Must Call sendEvent:** Events must be forwarded to `NSApplication::sendEvent` after processing, or standard AppKit behaviors (window dragging, etc.) won't work.

5. **Mouse Moved Events:** To receive `NSEventType::MouseMoved`, you must call `window.setAcceptsMouseMovedEvents(true)`.

---

## Multi-Monitor Support

### Required APIs

| API | Purpose | objc2 Crate |
|-----|---------|-------------|
| `NSScreen::screens` | Get all connected screens | `objc2-app-kit` |
| `NSScreen::mainScreen` | Get screen with key window | `objc2-app-kit` |
| `NSScreen::frame` | Get screen bounds | `objc2-app-kit` |
| `NSScreen::localizedName` | Get human-readable screen name | `objc2-app-kit` |
| `NSWindow::setFrameOrigin` | Move window to position | `objc2-app-kit` |

### Screen Enumeration

```rust
use objc2_app_kit::NSScreen;
use objc2_foundation::NSArray;

fn get_all_monitors() -> Vec<MonitorInfo> {
    let screens = NSScreen::screens();
    let main_screen = NSScreen::mainScreen().expect("No main screen");
    let main_frame = main_screen.frame();

    screens
        .iter()
        .enumerate()
        .map(|(i, screen)| {
            let frame = screen.frame();
            let name = screen.localizedName();

            // Convert from bottom-left to top-left origin
            let y = main_frame.size.height - frame.origin.y - frame.size.height;

            MonitorInfo {
                id: format!("screen-{}", i),
                name: name.to_string(),
                x: frame.origin.x as i32,
                y: y as i32,
                width: frame.size.width as u32,
                height: frame.size.height as u32,
                is_primary: i == 0, // First screen in array is primary
            }
        })
        .collect()
}
```

### Multi-Monitor Coordinate System

```
                    Screen Arrangement Example

    +-------------------+-------------------+
    | Secondary (left)  | Primary (right)   |
    | x: -1920, y: 0    | x: 0, y: 0        |
    | 1920x1080         | 3440x1440         |
    +-------------------+-------------------+
                        |
                        | (origin 0,0)
                        v

    - Primary screen defines coordinate origin (0,0) at bottom-left
    - Screens to the left have negative x values
    - Screens above have positive y values (macOS bottom-left origin)
    - After conversion to top-left: screens below have positive y values
```

### Window Positioning Across Monitors

```rust
fn set_position(&mut self, x: i32, y: i32) {
    // Convert from top-left to bottom-left origin for macOS
    let macos_y = self.main_screen_height - y as f64 - self.height as f64;

    let point = NSPoint::new(x as f64, macos_y);
    self.window.setFrameOrigin(point);
}
```

### Gotchas

1. **Main Screen vs Primary Screen:**
   - **Primary Screen:** Contains menu bar, defines coordinate origin
   - **Main Screen:** Contains the key window (currently active)
   - `NSScreen::mainScreen()` returns the screen with the key window, NOT necessarily the primary screen
   - First element of `NSScreen::screens()` is always the primary screen

2. **Screen Change Notifications:** Use `NSApplicationDidChangeScreenParametersNotification` to detect:
   - Monitor connected/disconnected
   - Resolution changes
   - Arrangement changes

3. **Coordinate Conversion Required:** All screen frames use bottom-left origin. Convert to top-left for cross-platform consistency.

4. **Virtual Screen Bounds:** With multiple monitors, the coordinate space can extend into negative values (monitors to the left) or large positive values.

---

## Recommended Crate Dependencies

Replace the current deprecated crates with the objc2 ecosystem:

```toml
# Platform: macOS
[target.'cfg(target_os = "macos")'.dependencies]
objc2 = "0.6"
objc2-foundation = { version = "0.3", features = ["NSArray", "NSDate", "NSString", "NSGeometry"] }
objc2-app-kit = { version = "0.3", features = [
    "NSApplication",
    "NSColor",
    "NSEvent",
    "NSGraphicsContext",
    "NSResponder",
    "NSRunningApplication",
    "NSScreen",
    "NSView",
    "NSWindow",
] }
objc2-core-graphics = { version = "0.3", features = [
    "CGBitmapContext",
    "CGColorSpace",
    "CGContext",
    "CGGeometry",
    "CGImage",
    "CGWindowLevel",
] }
```

---

## Migration Checklist

- [ ] Replace `cocoa` crate with `objc2-app-kit`
- [ ] Replace `objc` crate with `objc2`
- [ ] Replace `core-graphics` crate with `objc2-core-graphics`
- [ ] Update `ClassDecl` to `define_class!` macro
- [ ] Update `msg_send!` calls to objc2 syntax
- [ ] Add `setReleasedWhenClosed(false)` call after window creation
- [ ] Update CGBitmapContext creation to use objc2-core-graphics types
- [ ] Test click-through behavior on macOS Sonoma
- [ ] Test multi-monitor behavior with various arrangements

---

## Sources

### Official Documentation
- [NSWindow.Level](https://developer.apple.com/documentation/appkit/nswindow/level)
- [NSWindow.CollectionBehavior](https://developer.apple.com/documentation/appkit/nswindow/collectionbehavior-swift.struct)
- [CGWindowLevelForKey](https://developer.apple.com/documentation/coregraphics/1454084-cgwindowlevelforkey)
- [NSGraphicsContext](https://developer.apple.com/documentation/appkit/nsgraphicscontext)
- [NSScreen](https://developer.apple.com/documentation/appkit/nsscreen)
- [NSApplication nextEvent](https://developer.apple.com/documentation/appkit/nsapplication/1428485-nextevent)
- [locationInWindow](https://developer.apple.com/documentation/appkit/nsevent/1529068-locationinwindow)
- [Handling Mouse Events](https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/EventOverview/HandlingMouseEvents/HandlingMouseEvents.html)

### Rust Crate Documentation
- [objc2 GitHub](https://github.com/madsmtm/objc2)
- [objc2-app-kit NSWindow](https://docs.rs/objc2-app-kit/latest/objc2_app_kit/struct.NSWindow.html)
- [objc2-core-graphics](https://docs.rs/objc2-core-graphics)
- [define_class! macro](https://docs.rs/objc2/latest/objc2/macro.define_class.html)

### Community Resources
- [NSWindowLevel values](https://jameshfisher.com/2020/08/03/what-is-the-order-of-nswindow-levels/)
- [macOS Translucent Overlay Window](https://gaitatzis.medium.com/create-a-translucent-overlay-window-on-macos-in-swift-67d5e000ce90)
- [winit click-through issue](https://github.com/rust-windowing/winit/issues/1434)
- [macOS Sonoma click-through bug](https://developer.apple.com/forums/thread/737584)
- [Multi-monitor programming](https://www.thinkandbuild.it/deal-with-multiple-screens-programming/)
- [CGBitmapContext pixel ordering](http://www.krugerheavyindustries.com/pebble/tags/cgbitmapcontext/)
