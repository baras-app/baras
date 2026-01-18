# Architecture Research: cocoa to objc2 Migration

**Project:** BARAS Overlay macOS Platform
**Researched:** 2026-01-18
**Confidence:** MEDIUM (objc2 API verified via docs.rs, specific NSView+drawRect pattern requires validation)

## Executive Summary

The migration from the deprecated `cocoa` crate to the `objc2` ecosystem is a well-supported path. The `cocoa` crate's appkit module explicitly states "use the objc2-app-kit crate instead." The objc2 ecosystem provides modern, safer Rust bindings with better type safety and memory management semantics.

The current CGContext errors are unrelated to the cocoa/objc2 migration - they stem from incorrect usage of the existing `core-graphics` crate API. These can be fixed independently.

## Migration Strategy

### Recommended Order of Changes

**Phase 1: Fix CGContext Errors (Independent)**
Fix the immediate compilation errors without changing the cocoa/objc imports. This unblocks builds.

**Phase 2: Update Dependencies**
Replace deprecated crates with objc2 equivalents in Cargo.toml.

**Phase 3: Migrate msg_send! Calls**
Update message sending syntax from old objc to objc2 style.

**Phase 4: Migrate Custom NSView Subclass**
Convert ClassDecl-based view to define_class! macro.

**Phase 5: Update Window/App Management**
Migrate NSWindow, NSApplication, NSScreen usage.

### Why This Order

1. CGContext fix is independent - enables immediate builds
2. Dependencies must change before code migration
3. Simple msg_send! changes are mechanical and low-risk
4. Custom NSView is the most complex, benefits from earlier migrations as practice
5. Window management changes are straightforward once patterns are established

## Dependency Analysis

### Current Dependencies (overlay/Cargo.toml)

```toml
[target.'cfg(target_os = "macos")'.dependencies]
cocoa = "0.26"           # DEPRECATED - migrate to objc2-app-kit
core-graphics = "0.24"   # Keep (or migrate to objc2-core-graphics)
objc = "0.2"             # DEPRECATED - migrate to objc2
```

### Target Dependencies

```toml
[target.'cfg(target_os = "macos")'.dependencies]
objc2 = "0.6"
objc2-foundation = { version = "0.3", features = ["NSArray", "NSString", "NSDate"] }
objc2-app-kit = { version = "0.3", features = [
    "NSApplication",
    "NSColor",
    "NSEvent",
    "NSGraphicsContext",
    "NSResponder",
    "NSScreen",
    "NSView",
    "NSWindow",
] }
# Option A: Keep core-graphics (simpler migration)
core-graphics = "0.24"
# Option B: Full objc2 ecosystem (more consistent, but more work)
# objc2-core-graphics = "0.3"
```

### Dependency Graph

```
cocoa::appkit::* ─────────────────► objc2-app-kit
    NSApp, NSApplication              NSApplication
    NSWindow, NSScreen                NSWindow, NSScreen
    NSView, NSColor                   NSView, NSColor
    NSEvent, NSEventMask              NSEvent
    NSWindowStyleMask                 NSWindowStyleMask
    NSBackingStoreBuffered            NSBackingStoreType

cocoa::base::* ───────────────────► objc2 + objc2-foundation
    id, nil                           Retained<T>, Option<Retained<T>>
    YES, NO                           bool (automatic conversion)

cocoa::foundation::* ─────────────► objc2-foundation
    NSRect, NSPoint, NSSize           NSRect, NSPoint, NSSize
    NSString, NSArray, NSDate         NSString, NSArray, NSDate

objc::declare::ClassDecl ─────────► objc2::define_class! macro
objc::runtime::{Class, Object, Sel} ► objc2::{AnyClass, AnyObject, Sel}
objc::{msg_send, sel, class} ─────► objc2::{msg_send, sel, class}

core-graphics ────────────────────► core-graphics (keep) OR objc2-core-graphics
    CGContext                         CGContext / CGContextRef
    CGColorSpace                      CGColorSpace / CGColorSpaceRef
    CGImage                           CGImage / CGImageRef
```

## Suggested Phases

### Phase 1: CGContext Fix (1-2 hours)

**Goal:** Fix compilation errors, unblock builds

**Changes:**
1. Fix `create_bitmap_context` parameter type
2. Remove Option unwrapping (function returns CGContext directly)
3. Fix `from_existing_context_ptr` to use correct API

**Files:** `overlay/src/platform/macos.rs` lines 109-131

**Risk:** LOW - isolated fix, well-documented API

### Phase 2: Dependency Update (30 minutes)

**Goal:** Add objc2 crates alongside existing (for gradual migration)

**Changes:**
1. Add objc2, objc2-foundation, objc2-app-kit to Cargo.toml
2. Keep cocoa and objc temporarily for parallel operation
3. Test that both sets of dependencies compile

**Files:** `overlay/Cargo.toml`

**Risk:** LOW - additive change only

### Phase 3: msg_send! Migration (2-3 hours)

**Goal:** Update all msg_send! calls to objc2 syntax

**Changes:**
1. Update imports: `use objc2::{msg_send, sel, class};`
2. Add commas between arguments in msg_send! calls
3. Update return type annotations (bool instead of BOOL)
4. Replace `id` with proper `Retained<T>` or `*const NSObject`

**Key Syntax Changes:**
```rust
// OLD (objc crate)
let obj: id = msg_send![class!(NSView), alloc];
let obj: id = msg_send![obj, initWithFrame: rect];
let _: () = msg_send![self.view, setNeedsDisplay: YES];

// NEW (objc2 crate)
let obj = NSView::alloc();
let obj: Retained<NSView> = unsafe { msg_send![obj, initWithFrame: rect] };
let _: () = unsafe { msg_send![&*self.view, setNeedsDisplay: true] };
```

**Files:** `overlay/src/platform/macos.rs` - approximately 25 msg_send! calls

**Risk:** MEDIUM - mechanical but many changes, easy to miss one

### Phase 4: Custom NSView Migration (3-4 hours)

**Goal:** Convert ClassDecl-based BarasOverlayView to define_class! macro

**Changes:**
1. Replace static mut OVERLAY_VIEW_CLASS pattern
2. Define ivars struct with proper interior mutability
3. Implement drawRect using define_class! method syntax
4. Update isOpaque override

**Current Pattern (ClassDecl):**
```rust
static mut OVERLAY_VIEW_CLASS: Option<&'static Class> = None;

fn get_overlay_view_class() -> &'static Class {
    let superclass = class!(NSView);
    let mut decl = ClassDecl::new("BarasOverlayView", superclass).unwrap();
    decl.add_ivar::<*mut c_void>("pixelData");
    decl.add_ivar::<u32>("bufferWidth");
    decl.add_ivar::<u32>("bufferHeight");

    extern "C" fn draw_rect(this: &Object, _sel: Sel, _dirty_rect: NSRect) { ... }
    decl.add_method(sel!(drawRect:), draw_rect as extern "C" fn(...));

    decl.register()
}
```

**Target Pattern (define_class!):**
```rust
use std::cell::Cell;
use objc2::{define_class, msg_send, MainThreadOnly, ClassType};
use objc2_app_kit::NSView;
use objc2_foundation::NSRect;

struct BarasOverlayViewIvars {
    pixel_data: Cell<*mut c_void>,
    buffer_width: Cell<u32>,
    buffer_height: Cell<u32>,
}

define_class!(
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "BarasOverlayView"]
    #[ivars = BarasOverlayViewIvars]
    pub struct BarasOverlayView;

    unsafe impl BarasOverlayView {
        #[method(drawRect:)]
        fn draw_rect(&self, dirty_rect: NSRect) {
            let pixel_ptr = self.ivars().pixel_data.get();
            let width = self.ivars().buffer_width.get();
            let height = self.ivars().buffer_height.get();
            // ... drawing code ...
        }

        #[method(isOpaque)]
        fn is_opaque(&self) -> bool {
            false
        }
    }
);
```

**Files:** `overlay/src/platform/macos.rs` lines 80-164

**Risk:** HIGH - most complex change, requires careful testing

### Phase 5: Window/App Management (2-3 hours)

**Goal:** Replace cocoa NSWindow/NSApplication with objc2-app-kit

**Changes:**
1. Replace NSApp() with NSApplication::sharedApplication()
2. Update NSWindow creation to use objc2-app-kit methods
3. Replace NSScreen calls with objc2-app-kit equivalents
4. Update NSColor usage

**Current Pattern:**
```rust
use cocoa::appkit::{NSApp, NSApplication, NSWindow, NSScreen, NSColor};

let app = NSApp();
app.setActivationPolicy_(NSApplicationActivationPolicyAccessory);

let window = NSWindow::alloc(nil).initWithContentRect_styleMask_backing_defer_(...);
window.setLevel_(cocoa::appkit::NSMainMenuWindowLevel as i64 + 1);
```

**Target Pattern:**
```rust
use objc2_app_kit::{NSApplication, NSWindow, NSScreen, NSColor, NSWindowLevel};

let app = NSApplication::sharedApplication(MainThreadMarker::new().unwrap());
app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

let window = unsafe {
    NSWindow::initWithContentRect_styleMask_backing_defer(
        NSWindow::alloc(),
        rect,
        style_mask,
        NSBackingStoreType::Buffered,
        false,
    )
};
window.setLevel(NSWindowLevel(/* appropriate value */));
```

**Files:** `overlay/src/platform/macos.rs` lines 232-324, 346-410

**Risk:** MEDIUM - straightforward but many call sites

### Phase 6: Cleanup (1 hour)

**Goal:** Remove deprecated dependencies

**Changes:**
1. Remove `cocoa` from Cargo.toml
2. Remove `objc` from Cargo.toml
3. Clean up any remaining compatibility imports
4. Final testing

**Files:** `overlay/Cargo.toml`, `overlay/src/platform/macos.rs`

**Risk:** LOW - verification step

## CGContext Fix (Immediate)

The three errors at lines 110, 119, and 128 are caused by API misuse of `core-graphics`, not the cocoa/objc2 migration.

### Error 1 (Line 110): Parameter Type

**Error:** `CGContext::create_bitmap_context expects *mut c_void, got *mut u8`

**Current Code:**
```rust
let ctx = CGContext::create_bitmap_context(
    Some(pixel_ptr as *mut u8),  // ERROR: wrong type
    ...
);
```

**Fix:**
```rust
let ctx = CGContext::create_bitmap_context(
    Some(pixel_ptr),  // pixel_ptr is already *mut c_void
    ...
);
```

The `pixel_ptr` is already `*mut c_void` from the ivar, so no cast is needed. If it were `*mut u8`, cast it to `*mut c_void`:
```rust
Some(pixel_ptr as *mut c_void)
```

### Error 2 (Line 119): Return Type

**Error:** `CGContext::create_bitmap_context returns CGContext, not Option<CGContext>`

**Current Code:**
```rust
if let Some(ctx) = ctx {  // ERROR: ctx is CGContext, not Option
    ...
}
```

**Fix:**
The `create_bitmap_context` function returns `CGContext` directly (with internal assertion that it's valid). Remove the Option unwrapping:

```rust
let ctx = CGContext::create_bitmap_context(
    Some(pixel_ptr),
    width as usize,
    height as usize,
    8,
    (width * 4) as usize,
    &color_space,
    kCGImageAlphaPremultipliedFirst,
);

// Use ctx directly - no Option unwrapping
let image = ctx.create_image();
if let Some(image) = image {
    // ... drawing code ...
}
```

### Error 3 (Line 128): Non-existent Method

**Error:** `CGContextRef::from_existing_context_ptr doesn't exist`

**Analysis:** The current code tries to wrap a raw `CGContext` pointer obtained from `NSGraphicsContext`. The correct approach depends on what you need:

**Current Code:**
```rust
let cg_ctx: *mut c_void = msg_send![ns_ctx, CGContext];
let cg_ctx = core_graphics::context::CGContextRef::from_existing_context_ptr(cg_ctx);
cg_ctx.draw_image(...);
```

**Fix Option A: Use from_existing_context_ptr (correct method name)**

The method exists but may be named differently or require a different import. Check the actual API:

```rust
use core_graphics::context::CGContext;

let cg_ctx_ptr: *mut core_graphics::sys::CGContext = msg_send![ns_ctx, CGContext];
if !cg_ctx_ptr.is_null() {
    let cg_ctx = unsafe { CGContext::from_existing_context_ptr(cg_ctx_ptr) };
    cg_ctx.draw_image(rect, &image);
}
```

**Fix Option B: Use CGContextDrawImage directly via FFI**

If the wrapper doesn't work, call CoreGraphics directly:

```rust
use core_graphics::sys::{CGContextRef, CGContextDrawImage};

let cg_ctx: CGContextRef = msg_send![ns_ctx, CGContext];
if !cg_ctx.is_null() {
    let cg_rect = core_graphics::geometry::CGRect::new(
        &core_graphics::geometry::CGPoint::new(0.0, 0.0),
        &core_graphics::geometry::CGSize::new(bounds.size.width, bounds.size.height),
    );
    unsafe {
        CGContextDrawImage(cg_ctx, cg_rect, image.as_ptr());
    }
}
```

### Complete Fixed draw_rect Function

```rust
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
        // Note: create_bitmap_context returns CGContext directly, not Option
        let ctx = CGContext::create_bitmap_context(
            Some(pixel_ptr),  // Already *mut c_void, no cast needed
            width as usize,
            height as usize,
            8,
            (width * 4) as usize,
            &color_space,
            kCGImageAlphaPremultipliedFirst,
        );

        // create_image returns Option<CGImage>
        if let Some(image) = ctx.create_image() {
            // Get current graphics context
            let ns_ctx: id = msg_send![class!(NSGraphicsContext), currentContext];
            if ns_ctx != nil {
                let cg_ctx_ptr: *mut c_void = msg_send![ns_ctx, CGContext];

                if !cg_ctx_ptr.is_null() {
                    // Cast to the correct type for core-graphics
                    let cg_ctx = CGContext::from_existing_context_ptr(
                        cg_ctx_ptr as *mut core_graphics::sys::CGContext
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
```

## Confidence Assessment

| Area | Level | Reason |
|------|-------|--------|
| CGContext Fix | HIGH | API verified from core-graphics source on GitHub |
| objc2 msg_send! syntax | HIGH | Verified via docs.rs documentation |
| define_class! pattern | MEDIUM | Documented but no NSView+drawRect example found |
| Feature flags | MEDIUM | Feature system documented, specific flags need verification |
| NSWindow migration | MEDIUM | Types exist, specific API differences need testing |

## Open Questions

1. **NSWindowLevel values**: The old cocoa crate uses `NSMainMenuWindowLevel as i64 + 1`. Need to verify equivalent constant in objc2-app-kit.

2. **MainThreadMarker requirement**: objc2-app-kit requires `MainThreadMarker` for many operations. Need to verify how this integrates with the existing event loop.

3. **define_class! with NSRect parameter**: The drawRect method receives NSRect. Need to verify objc2-foundation's NSRect is compatible.

4. **core-graphics vs objc2-core-graphics**: Decision needed on whether to migrate core-graphics too, or keep it separate.

## Sources

- [objc2-app-kit crates.io](https://crates.io/crates/objc2-app-kit) - Current version and features
- [objc2 GitHub](https://github.com/madsmtm/objc2) - Main repository
- [define_class! macro docs](https://docs.rs/objc2/latest/objc2/macro.define_class.html) - Macro syntax
- [msg_send! macro docs](https://docs.rs/objc2/latest/objc2/macro.msg_send.html) - Message sending syntax
- [NSWindow docs](https://docs.rs/objc2-app-kit/latest/objc2_app_kit/struct.NSWindow.html) - NSWindow API
- [core-graphics context.rs source](https://github.com/servo/core-foundation-rs/blob/main/core-graphics/src/context.rs) - CGContext API
- [cocoa crate deprecation](https://docs.rs/cocoa/latest/cocoa/appkit/index.html) - Deprecation notice
