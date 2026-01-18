# Stack Research: macOS objc2 Migration

**Project:** BARAS Overlay Renderer
**Domain:** macOS overlay window rendering via Objective-C bindings
**Researched:** 2026-01-18
**Overall Confidence:** HIGH

## Executive Summary

The `cocoa` crate (v0.26) used by the existing macOS overlay implementation is officially deprecated. The Servo project (original maintainers) has marked it deprecated in favor of the `objc2` ecosystem. Migration to `objc2` is the correct path forward.

The objc2 ecosystem provides:
- Automatically generated bindings from Apple SDK headers (Xcode 16.4)
- Improved type safety and memory management
- Active maintenance aligned with Xcode releases
- Feature-gated granular imports

## Recommended Crates

### Core Dependencies

| Crate | Version | Purpose | Confidence |
|-------|---------|---------|------------|
| `objc2` | 0.6.3 | Core Objective-C runtime bindings, `define_class!`, `msg_send!` | HIGH |
| `objc2-foundation` | 0.3.2 | Foundation types: NSString, NSArray, NSRect, NSPoint, NSSize | HIGH |
| `objc2-app-kit` | 0.3.2 | AppKit types: NSWindow, NSView, NSScreen, NSApplication, NSEvent, NSColor | HIGH |
| `objc2-core-graphics` | 0.3.2 | Core Graphics: CGContext, CGColorSpace, CGImage, bitmap contexts | HIGH |
| `block2` | 0.6.1 | Objective-C blocks (if needed for callbacks) | HIGH |

### Cargo.toml Configuration

```toml
[target.'cfg(target_os = "macos")'.dependencies]
objc2 = "0.6"
objc2-foundation = { version = "0.3", features = [
    "NSGeometry",
    "NSString",
    "NSArray",
    "NSDate",
    "NSObject",
] }
objc2-app-kit = { version = "0.3", features = [
    "NSApplication",
    "NSWindow",
    "NSView",
    "NSScreen",
    "NSEvent",
    "NSColor",
    "NSGraphics",
    "NSResponder",
] }
objc2-core-graphics = { version = "0.3", features = [
    "CGContext",
    "CGColorSpace",
    "CGImage",
    "CGBitmapContext",
    "CGGeometry",
] }
```

**Note:** Most features are enabled by default in these crates. You can use `default-features = false` and enable only what you need for minimal compile times, but for this overlay use case, defaults work fine.

## API Mapping: cocoa to objc2

### Foundation Types

| Old (cocoa) | New (objc2-foundation) | Notes |
|-------------|------------------------|-------|
| `cocoa::foundation::NSRect` | `objc2_foundation::NSRect` | Same structure, different module |
| `cocoa::foundation::NSPoint` | `objc2_foundation::NSPoint` | Same structure |
| `cocoa::foundation::NSSize` | `objc2_foundation::NSSize` | Same structure |
| `cocoa::foundation::NSString` | `objc2_foundation::NSString` | Now a proper Rust type with methods |
| `cocoa::foundation::NSArray` | `objc2_foundation::NSArray` | Generic `NSArray<T>` |
| `cocoa::foundation::NSDate` | `objc2_foundation::NSDate` | Same |
| `nil` | `None` | Use `Option<&T>` or `Option<Retained<T>>` |

### AppKit Types

| Old (cocoa::appkit) | New (objc2_app_kit) | Migration Notes |
|---------------------|---------------------|-----------------|
| `NSApp()` | `NSApplication::sharedApplication()` | Returns `Retained<NSApplication>` |
| `NSWindow::alloc(nil)` | `NSWindow::alloc()` | Use `define_class!` for custom init |
| `NSView` trait methods | `NSView` struct methods | Direct method calls |
| `NSScreen::screens(nil)` | `NSScreen::screens()` | Returns `Retained<NSArray<NSScreen>>` |
| `NSScreen::mainScreen(nil)` | `NSScreen::mainScreen()` | Returns `Option<Retained<NSScreen>>` |
| `NSScreen::frame(screen)` | `screen.frame()` | Method on instance |
| `NSColor::clearColor(nil)` | `NSColor::clearColor()` | Class method |
| `NSEvent::mouseLocation(nil)` | `NSEvent::mouseLocation()` | Class method |
| `NSEventMask::NSAnyEventMask` | `NSEventMask::Any` | Renamed constants |
| `NSEventType::NSLeftMouseDown` | `NSEventType::LeftMouseDown` | Renamed constants |

### Core Graphics

| Old (core-graphics) | New (objc2-core-graphics) | Migration Notes |
|---------------------|---------------------------|-----------------|
| `CGContext::create_bitmap_context()` | `CGBitmapContextCreate()` | Function, not method |
| `CGColorSpace::create_device_rgb()` | `CGColorSpaceCreateDeviceRGB()` | Function returns `Option<CFRetained<CGColorSpace>>` |
| `ctx.create_image()` | `CGBitmapContextCreateImage(ctx)` | Function, not method |
| `ctx.draw_image(rect, image)` | `CGContextDrawImage(ctx, rect, image)` | Function, not method |
| `CGContextRef::from_existing_context_ptr()` | Direct use of `CGContextRef` | Already a pointer type |
| `kCGImageAlphaPremultipliedFirst` | `CGImageAlphaInfo::PremultipliedFirst` | Enum variant |

### objc Macros

| Old (objc) | New (objc2) | Migration Notes |
|------------|-------------|-----------------|
| `msg_send![obj, method]` | `obj.method()` or `msg_send![obj, method]` | Prefer direct methods when available |
| `msg_send![obj, method: arg]` | `obj.method(arg)` or `msg_send![obj, method: arg]` | Same syntax, improved type safety |
| `class!(NSView)` | `class!(NSView)` | Same, but prefer `NSView::class()` |
| `sel!(drawRect:)` | `sel!(drawRect:)` | Same syntax |
| `ClassDecl::new()` | `define_class!` macro | Declarative, safer |
| `decl.add_ivar::<T>()` | `#[ivars = IvarStruct]` | Struct-based ivars |
| `decl.add_method()` | `#[unsafe(method(selector))]` | Attribute-based |

### Memory Management

| Old Pattern | New Pattern | Notes |
|-------------|-------------|-------|
| `id` (raw pointer) | `Retained<T>` or `&T` | Strong reference or borrowed |
| `nil` checks | `Option<Retained<T>>` | Nullable returns are `Option` |
| Manual retain/release | Automatic via `Retained<T>` | RAII semantics |
| `YES`/`NO` | `true`/`false` or `Bool::YES`/`Bool::NO` | Use Rust bools where possible |

## Custom NSView Subclass Migration

### Old Pattern (objc crate)

```rust
use objc::declare::ClassDecl;
use objc::runtime::{Class, Object, Sel, BOOL};
use objc::{class, msg_send, sel, sel_impl};

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
            // drawing code
        }

        decl.add_method(
            sel!(drawRect:),
            draw_rect as extern "C" fn(&Object, Sel, NSRect),
        );

        let cls = decl.register();
        OVERLAY_VIEW_CLASS = Some(cls);
        cls
    }
}
```

### New Pattern (objc2)

```rust
use std::cell::Cell;
use objc2::{define_class, msg_send, ClassType, DeclaredClass};
use objc2::rc::Retained;
use objc2::runtime::NSObject;
use objc2_app_kit::NSView;
use objc2_foundation::NSRect;

struct BarasOverlayViewIvars {
    pixel_data: Cell<*mut u8>,
    buffer_width: Cell<u32>,
    buffer_height: Cell<u32>,
}

define_class!(
    // Safety: NSView is a valid superclass
    #[unsafe(super(NSView))]
    #[name = "BarasOverlayView"]
    #[ivars = BarasOverlayViewIvars]
    struct BarasOverlayView;

    impl BarasOverlayView {
        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, dirty_rect: NSRect) {
            let ivars = self.ivars();
            let pixel_ptr = ivars.pixel_data.get();
            let width = ivars.buffer_width.get();
            let height = ivars.buffer_height.get();

            if pixel_ptr.is_null() || width == 0 || height == 0 {
                return;
            }

            // Drawing implementation using objc2-core-graphics
        }

        #[unsafe(method(isOpaque))]
        fn is_opaque(&self) -> bool {
            false
        }
    }
);

impl BarasOverlayView {
    fn new(frame: NSRect) -> Retained<Self> {
        let this = Self::alloc();
        let ivars = BarasOverlayViewIvars {
            pixel_data: Cell::new(std::ptr::null_mut()),
            buffer_width: Cell::new(0),
            buffer_height: Cell::new(0),
        };
        // Safety: Calling initWithFrame: on allocated NSView
        unsafe { msg_send![super(this.set_ivars(ivars)), initWithFrame: frame] }
    }

    fn set_pixel_data(&self, data: *mut u8, width: u32, height: u32) {
        let ivars = self.ivars();
        ivars.pixel_data.set(data);
        ivars.buffer_width.set(width);
        ivars.buffer_height.set(height);
    }
}
```

## Critical Migration Considerations

### 1. NSWindow Memory Management

When creating `NSWindow` outside a window controller, you MUST call:

```rust
// Old
window.setReleasedWhenClosed_(NO);

// New
unsafe { window.setReleasedWhenClosed(false) };
```

This is why `NSWindow` initialization methods are marked `unsafe` in objc2.

### 2. Application Initialization Order

AppKit requires proper initialization before most UI operations. The recommended pattern:

```rust
// Get shared application (initializes if needed)
let app = NSApplication::sharedApplication();

// Set activation policy
app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

// Now safe to create windows
```

### 3. Thread Safety

objc2 uses `MainThreadOnly` marker for types that must only be used on the main thread. Most AppKit types require this. For overlays running their own event loop, this is typically fine.

### 4. Getting CGContext from NSGraphicsContext

```rust
use objc2_app_kit::NSGraphicsContext;

// In drawRect:
if let Some(ns_ctx) = NSGraphicsContext::currentContext() {
    let cg_ctx = ns_ctx.CGContext(); // Returns Retained<CGContext>
    // Draw using CGContext functions
}
```

## What NOT to Use

### Deprecated Crates (Do NOT use)

| Crate | Status | Replacement |
|-------|--------|-------------|
| `cocoa` | DEPRECATED | `objc2-app-kit` + `objc2-foundation` |
| `cocoa-foundation` | DEPRECATED | `objc2-foundation` |
| `objc` | DEPRECATED | `objc2` |
| `core-foundation` | Soft-deprecated | `objc2-core-foundation` |
| `core-graphics` | Soft-deprecated | `objc2-core-graphics` |
| `io-surface` | DEPRECATED | `objc2-io-surface` |

### Problematic Patterns

| Pattern | Problem | Solution |
|---------|---------|----------|
| `static mut CLASS` | Unsafe global state | Use `define_class!` which handles registration |
| Raw `id` pointers everywhere | No type safety | Use `Retained<T>` and `&T` |
| Manual `msg_send!` for everything | Error-prone | Use generated method bindings |
| `YES`/`NO` constants | C-style | Use Rust `bool` where methods accept it |
| `nil` everywhere | Nullable unclear | Use `Option<T>` |

## Version Compatibility

| Requirement | Value |
|-------------|-------|
| Minimum Rust version | 1.71 |
| Xcode SDK used for bindings | 16.4 |
| macOS deployment target | 10.12+ (varies by feature) |

## Migration Effort Estimate

| Component | Complexity | Estimated Effort |
|-----------|------------|------------------|
| Import changes | Low | 1 hour |
| NSWindow/NSView setup | Medium | 2-4 hours |
| Custom NSView subclass | Medium-High | 4-6 hours |
| Event handling | Medium | 2-3 hours |
| CGContext drawing | Medium | 2-4 hours |
| Testing & debugging | Medium | 4-8 hours |
| **Total** | | **15-26 hours** |

## Sources

### HIGH Confidence (Official Documentation)
- [objc2 crate documentation](https://docs.rs/objc2/)
- [objc2-app-kit documentation](https://docs.rs/objc2-app-kit/)
- [objc2-foundation documentation](https://docs.rs/objc2-foundation/)
- [objc2-core-graphics documentation](https://docs.rs/objc2-core-graphics/)
- [define_class! macro documentation](https://docs.rs/objc2/latest/objc2/macro.define_class.html)
- [NSWindow documentation](https://docs.rs/objc2-app-kit/latest/objc2_app_kit/struct.NSWindow.html)
- [NSGraphicsContext documentation](https://docs.rs/objc2-app-kit/latest/objc2_app_kit/struct.NSGraphicsContext.html)

### MEDIUM Confidence (Official GitHub)
- [objc2 GitHub repository](https://github.com/madsmtm/objc2)
- [servo/core-foundation-rs deprecation discussion](https://github.com/servo/core-foundation-rs/issues/729)
- [cocoa-rs deprecation notice](https://github.com/servo/cocoa-rs)

### Crate Registry
- [objc2-app-kit on lib.rs](https://lib.rs/crates/objc2-app-kit)
- [objc2-core-graphics on lib.rs](https://lib.rs/crates/objc2-core-graphics)
- [objc2-app-kit features](https://lib.rs/crates/objc2-app-kit/features)
- [objc2-core-graphics features](https://lib.rs/crates/objc2-core-graphics/features)
