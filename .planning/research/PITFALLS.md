# Pitfalls Research: cocoa to objc2 Migration

**Domain:** macOS Rust FFI for overlay rendering
**Researched:** 2026-01-18
**Confidence:** MEDIUM (based on official docs, GitHub issues, and ecosystem migration experiences)

This document catalogs common mistakes and gotchas when migrating macOS Rust code from the deprecated `cocoa` crate to the `objc2` ecosystem, specifically for overlay rendering with custom NSView subclasses and CGContext bitmap operations.

---

## Critical Pitfalls

Mistakes that cause crashes, undefined behavior, or require significant rework.

### Pitfall 1: msg_send! Syntax and Type Safety Changes

**What goes wrong:** The `objc2` `msg_send!` macro has different syntax and stricter type requirements than the old `objc` crate. Code that compiled with `objc` may fail or have undefined behavior with `objc2`.

**Why it happens:**
- Old syntax: `msg_send![obj, selector: arg1: arg2]` (colons separate selector parts)
- New syntax: `msg_send![obj, selector: arg1, arg2]` (commas between arguments, deprecated to elide)
- `objc2` requires all types to implement `Encode` trait
- Return types must match exactly; `Retained<T>` vs `Option<Retained<T>>` matters

**Consequences:**
- Compile errors if types don't implement `Encode`
- Runtime panics if `Retained<T>` is used but method returns NULL
- Undefined behavior if argument/return types don't match Objective-C expectations

**Prevention:**
```rust
// OLD (objc crate) - does NOT work with objc2
let result: id = msg_send![window, setFrame:rect display:YES];

// NEW (objc2) - correct syntax with commas
let result: () = msg_send![window, setFrame: rect, display: true];

// For nullable returns, use Option
let view: Option<Retained<NSView>> = msg_send![window, contentView];
```

**Detection:** Compile errors with "trait `Encode` is not implemented" or runtime panics with "unexpected NULL" messages.

**Migration impact:** Every `msg_send!` call must be reviewed and potentially rewritten.

---

### Pitfall 2: Retained<T> vs Raw id Pointers

**What goes wrong:** The old `cocoa` crate used raw `id` pointers everywhere. `objc2` uses `Retained<T>` for automatic reference counting. Mixing these incorrectly causes memory leaks or use-after-free.

**Why it happens:**
- `id` in cocoa is just `*mut Object` with no ownership semantics
- `Retained<T>` automatically retains on creation and releases on drop
- `Retained::from_raw()` assumes +1 retain count (caller owns the reference)
- `Retained::retain()` adds +1 retain count to an unowned reference

**Consequences:**
- Double-free crashes if using `from_raw()` on autoreleased objects
- Memory leaks if forgetting to reconstruct `Retained` after `into_raw()`
- Use-after-free if holding raw pointers past autorelease pool drain

**Prevention:**
```rust
// Creating from +1 methods (init, alloc, new, copy)
let window: Retained<NSWindow> = unsafe {
    Retained::from_raw(msg_send![NSWindow::alloc(), initWithContentRect: ...])
}.expect("initWithContentRect returned NULL");

// Creating from other methods (autoreleased)
let screen: Retained<NSScreen> = unsafe {
    Retained::retain_autoreleased(msg_send![class!(NSScreen), mainScreen])
}.expect("mainScreen returned NULL");

// WRONG - will double-free
let bad: Retained<NSWindow> = unsafe {
    Retained::from_raw(msg_send![app, mainWindow]) // mainWindow is NOT +1
};
```

**Detection:** Crashes with "EXC_BAD_ACCESS" or "malloc: double free" errors. Memory growth over time indicates leaks.

**Migration impact:** All object creation and storage patterns must be audited.

---

### Pitfall 3: define_class! Macro Safety Requirements

**What goes wrong:** The `define_class!` macro replaces manual `ClassDecl` usage but has strict safety requirements. Missing or incorrect attributes cause runtime failures.

**Why it happens:**
- Must specify superclass with `#[unsafe(super(NSView))]`
- Must justify why subclassing is safe (superclass invariants)
- Instance variables must use interior mutability (`Cell`, `RefCell`, etc.)
- Cannot use `&mut self` in methods
- Thread safety must be explicitly declared

**Current code pattern (ClassDecl):**
```rust
// OLD approach
let superclass = class!(NSView);
let mut decl = ClassDecl::new("BarasOverlayView", superclass).unwrap();
decl.add_ivar::<*mut c_void>("pixelData");
decl.add_method(sel!(drawRect:), draw_rect as extern "C" fn(...));
```

**New pattern (define_class!):**
```rust
use std::cell::Cell;
use objc2::define_class;
use objc2::rc::{Allocated, Retained};
use objc2_app_kit::NSView;
use objc2_foundation::NSRect;

#[derive(Default)]
struct OverlayViewIvars {
    pixel_data: Cell<*mut u8>,
    buffer_width: Cell<u32>,
    buffer_height: Cell<u32>,
}

define_class!(
    // SAFETY: NSView permits subclassing for custom drawing.
    // We do not implement Drop.
    #[unsafe(super(NSView))]
    #[name = "BarasOverlayView"]
    #[ivars = OverlayViewIvars]
    pub struct BarasOverlayView;

    impl BarasOverlayView {
        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, dirty_rect: NSRect) {
            // Drawing implementation
        }

        #[unsafe(method(isOpaque))]
        fn is_opaque(&self) -> bool {
            false
        }
    }
);
```

**Consequences:**
- Panic at class registration if class name already exists
- Undefined behavior if method types don't match Objective-C expectations
- Runtime crashes if ivars accessed without proper synchronization

**Prevention:**
- Use `Cell<T>` or `RefCell<T>` for all mutable ivars
- Never use `&mut self` in method implementations
- Add safety comments explaining why superclass permits subclassing
- Use unique class names (consider including module path)

**Detection:** Panics with "class already exists" or crashes in method dispatch.

**Migration impact:** Complete rewrite of custom class declarations.

---

### Pitfall 4: Id<T> is Deprecated, Use Retained<T>

**What goes wrong:** Code using `Id<T>` from earlier objc2 versions will need updating. `Id` is a type alias for `Retained` that will be removed in v0.6.0.

**Why it happens:**
- `Id<T>` was the original name in objc2
- Renamed to `Retained<T>` for clarity
- Currently `pub type Id<T> = Retained<T>;`

**Consequences:**
- Deprecation warnings now, compile errors in future versions
- Confusion when reading mixed-era documentation

**Prevention:**
```rust
// Use Retained everywhere
use objc2::rc::Retained;

// NOT this (deprecated)
use objc2::rc::Id;
```

**Detection:** Deprecation warnings during compilation.

**Migration impact:** Search and replace `Id<` with `Retained<`.

---

## CGContext Pitfalls

Common mistakes with bitmap contexts for rendering.

### Pitfall 5: Premultiplied Alpha Requirement

**What goes wrong:** CGContext with RGB color space and transparency requires premultiplied alpha. Using non-premultiplied or wrong byte order causes incorrect rendering or context creation failure.

**Why it happens:**
- Core Graphics only accepts `kCGImageAlphaPremultipliedFirst` or `kCGImageAlphaPremultipliedLast` for RGBA contexts
- `kCGImageAlphaLast` (non-premultiplied) is rejected
- Byte order matters: `kCGImageAlphaPremultipliedFirst` = BGRA, `kCGImageAlphaPremultipliedLast` = RGBA

**Current code correctly handles this:**
```rust
// BARAS does premultiplication manually in commit()
for (i, chunk) in self.pixel_data.chunks(4).enumerate() {
    let a = chunk[3] as u32;
    self.bgra_buffer[offset] = ((chunk[2] as u32 * a) / 255) as u8;     // B
    self.bgra_buffer[offset + 1] = ((chunk[1] as u32 * a) / 255) as u8; // G
    self.bgra_buffer[offset + 2] = ((chunk[0] as u32 * a) / 255) as u8; // R
    self.bgra_buffer[offset + 3] = chunk[3];                            // A
}
```

**Consequences:**
- `CGContext::create_bitmap_context()` returns `None`
- Colors appear washed out or transparency doesn't work
- Blending artifacts in semi-transparent areas

**Prevention:**
- Always use premultiplied alpha info constants
- Match byte order to the alpha info flag
- Premultiply RGB channels: `channel = (channel * alpha) / 255`

**Detection:** `create_bitmap_context()` returning `None`, visual artifacts in transparent areas.

**Migration impact:** The current code handles this correctly; ensure the pattern is preserved.

---

### Pitfall 6: core-graphics vs objc2-core-graphics API Differences

**What goes wrong:** The `core-graphics` crate (used currently) has different APIs than `objc2-core-graphics`. Method names and calling conventions differ.

**Why it happens:**
- `core-graphics` uses wrapper types with Rust-style methods
- `objc2-core-graphics` uses raw C function bindings
- Some functions are renamed (e.g., `CGContextDrawLayerAtPoint` -> `CGContext::draw_layer_at_point`)

**Current code:**
```rust
use core_graphics::context::CGContext;

let ctx = CGContext::create_bitmap_context(...);
let image = ctx.create_image();
cg_ctx.draw_image(rect, &image);
```

**objc2-core-graphics approach:**
```rust
use objc2_core_graphics::{CGBitmapContextCreate, CGBitmapContextCreateImage, CGContextDrawImage};

// Functions are unsafe and take raw pointers
let ctx = unsafe { CGBitmapContextCreate(...) };
let image = unsafe { CGBitmapContextCreateImage(ctx) };
unsafe { CGContextDrawImage(ctx, rect, image) };
```

**Consequences:**
- Compile errors when switching crates
- Need to handle raw pointers and null checks manually

**Prevention:**
- Consider keeping `core-graphics` crate alongside `objc2-app-kit`
- They can coexist; `core-graphics` provides ergonomic wrappers
- If switching, wrap unsafe calls in safe Rust functions

**Detection:** Compile errors with "cannot find function" or type mismatches.

**Migration impact:** May keep `core-graphics` for CGContext operations while using `objc2-app-kit` for AppKit.

---

### Pitfall 7: CGContextRef Lifetime and Ownership

**What goes wrong:** Getting CGContext from NSGraphicsContext requires careful lifetime management. The context is borrowed, not owned.

**Current code:**
```rust
let ns_ctx: id = msg_send![class!(NSGraphicsContext), currentContext];
let cg_ctx: *mut c_void = msg_send![ns_ctx, CGContext];
let cg_ctx = CGContextRef::from_existing_context_ptr(cg_ctx);
```

**Consequences:**
- Use-after-free if NSGraphicsContext is deallocated
- Crashes if context is used outside `drawRect:`

**Prevention:**
- Only use the CGContext within `drawRect:` scope
- Don't store the context reference
- Let it go out of scope before `drawRect:` returns

**Detection:** Crashes with "EXC_BAD_ACCESS" when drawing.

**Migration impact:** Pattern remains the same, but use `objc2` types.

---

## Custom View Pitfalls

Issues specific to NSView subclasses with objc2.

### Pitfall 8: isFlipped Coordinate System Override

**What goes wrong:** macOS uses bottom-left origin by default. If not overriding `isFlipped`, coordinate conversion is needed everywhere. Forgetting to flip causes upside-down rendering.

**Why it happens:**
- NSView default: origin at bottom-left, Y increases upward
- Most graphics systems: origin at top-left, Y increases downward
- Subviews don't inherit parent's flipped state

**Current code manually converts:**
```rust
fn convert_y(&self, y: i32, height: u32) -> f64 {
    self.main_screen_height - y as f64 - height as f64
}
```

**Better approach - override isFlipped:**
```rust
define_class!(
    #[unsafe(super(NSView))]
    pub struct BarasOverlayView;

    impl BarasOverlayView {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true  // Use top-left origin
        }
    }
);
```

**Consequences:**
- Content appears upside-down
- Mouse coordinates don't match visual positions
- Drag operations move in wrong direction

**Prevention:**
- Override `isFlipped` to return `true` for top-left origin
- This automatically adjusts CTM before `drawRect:` is called
- Eliminates need for manual Y-coordinate flipping in draw code

**Detection:** Visual inspection shows upside-down content; mouse coordinates are inverted.

**Migration impact:** Can simplify code by using flipped coordinates; requires testing all coordinate-dependent code.

---

### Pitfall 9: Thread Safety and MainThreadOnly

**What goes wrong:** AppKit operations must run on the main thread. Custom classes not marked `MainThreadOnly` may be called from wrong threads.

**Why it happens:**
- objc2 defaults to allowing any thread
- AppKit views must only be accessed from main thread
- Callbacks from system may come on different threads

**Prevention:**
```rust
define_class!(
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]  // Enforces main thread
    #[ivars = OverlayViewIvars]
    pub struct BarasOverlayView;
);
```

**Consequences:**
- Crashes or data corruption from concurrent access
- Visual glitches from race conditions
- Undefined behavior in Objective-C runtime

**Detection:** Crashes with "UI API called on a background thread" or inconsistent visual state.

**Migration impact:** Add `#[thread_kind = MainThreadOnly]` to all view classes.

---

### Pitfall 10: Instance Variable Initialization in init

**What goes wrong:** objc2 ivars must be properly initialized in the designated initializer. Using inherited initializers without overriding causes uninitialized ivars.

**Why it happens:**
- `define_class!` ivars require initialization via `set_ivars()`
- Calling superclass init without setting ivars leaves them uninitialized
- Default trait implementation only works if you override init

**Prevention:**
```rust
define_class!(
    #[unsafe(super(NSView))]
    #[ivars = OverlayViewIvars]
    pub struct BarasOverlayView;

    impl BarasOverlayView {
        #[unsafe(method_id(initWithFrame:))]
        fn init_with_frame(this: Allocated<Self>, frame: NSRect) -> Retained<Self> {
            let this = this.set_ivars(OverlayViewIvars::default());
            unsafe { msg_send![super(this), initWithFrame: frame] }
        }
    }
);
```

**Consequences:**
- Crashes when accessing uninitialized ivars
- Undefined behavior from reading garbage data

**Detection:** Crashes on first ivar access; garbage values in debug output.

**Migration impact:** Must implement init method that calls `set_ivars()`.

---

## Memory Management Pitfalls

Retain/release gotchas when bridging Objective-C and Rust.

### Pitfall 11: Autorelease Pool Boundaries

**What goes wrong:** Objects returned from Objective-C methods may be autoreleased. Using them after pool drains causes use-after-free.

**Why it happens:**
- Objective-C methods often return autoreleased objects
- Pool drains at end of event loop iteration
- `Retained::retain_autoreleased()` should be used to extend lifetime

**Prevention:**
```rust
// CORRECT: Retain immediately
let screen: Retained<NSScreen> = unsafe {
    Retained::retain_autoreleased(msg_send![class!(NSScreen), mainScreen])
}.unwrap();

// WRONG: Raw pointer may become invalid
let screen: *mut NSScreen = unsafe { msg_send![class!(NSScreen), mainScreen] };
// ... later use of screen may crash
```

**Consequences:**
- Intermittent crashes depending on autorelease pool timing
- Hard to debug; works sometimes, crashes sometimes

**Detection:** Sporadic crashes with "EXC_BAD_ACCESS" on seemingly valid objects.

**Migration impact:** All raw pointer storage must be converted to `Retained<T>`.

---

### Pitfall 12: Drop Implementation Conflicts

**What goes wrong:** Implementing Rust's `Drop` trait on objc2 classes can conflict with Objective-C's `dealloc` mechanism.

**Why it happens:**
- Objective-C uses `dealloc` for cleanup
- Rust's `Drop` runs at different times
- Can cause double-free or resource leaks

**Prevention:**
- Override `dealloc` in Objective-C class instead of implementing `Drop`
- If using `Drop`, don't call overridden methods or retain objects
- Document in `define_class!` safety comment that `Drop` is not implemented

```rust
define_class!(
    // SAFETY: NSView permits subclassing.
    // We do NOT implement Drop - cleanup happens via Objective-C dealloc.
    #[unsafe(super(NSView))]
    pub struct BarasOverlayView;

    impl BarasOverlayView {
        #[unsafe(method(dealloc))]
        fn dealloc(&self) {
            // Cleanup code here
            unsafe { msg_send![super(self), dealloc] }
        }
    }
);
```

**Consequences:**
- Double-free crashes
- Resource leaks
- Accessing deallocated memory

**Detection:** Crashes during object destruction; memory leaks in Instruments.

**Migration impact:** Review any cleanup code in current `Drop` implementations.

---

## Coordinate System Pitfalls

macOS coordinate quirks affecting overlay positioning.

### Pitfall 13: Screen vs Window vs View Coordinates

**What goes wrong:** macOS has multiple coordinate systems that must be converted between. Each has different origins and scales.

**Coordinate systems:**
1. **Screen coordinates:** Bottom-left of primary screen is (0,0), Y up
2. **Window coordinates:** Bottom-left of window is (0,0), Y up
3. **View coordinates:** Depends on `isFlipped`; default is bottom-left
4. **Event coordinates:** Window coordinates, but Y is 1-based (not 0-based!)

**Current code handles screen-to-window:**
```rust
fn convert_y(&self, y: i32, height: u32) -> f64 {
    self.main_screen_height - y as f64 - height as f64
}
```

**Pitfall with event coordinates:**
```rust
// Event Y coordinates are 1-based!
let loc = event.locationInWindow();
// A click at bottom-left corner returns (0, 1), not (0, 0)
```

**Consequences:**
- Windows appear at wrong positions
- Mouse clicks register at wrong locations
- Drag operations behave incorrectly

**Prevention:**
- Document which coordinate system each variable uses
- Use NSView conversion methods when possible
- Account for 1-based event Y coordinates

**Detection:** Visual inspection of window positions; mouse interaction misalignment.

**Migration impact:** Coordinate handling code needs careful review during migration.

---

### Pitfall 14: Multi-Monitor Coordinate Handling

**What goes wrong:** Screen coordinates span all monitors. Secondary monitors may have negative coordinates or be above/below primary.

**Current code:**
```rust
let main_frame = NSScreen::frame(main_screen);
// Y conversion assumes main screen height
let y = main_frame.size.height - frame.origin.y - frame.size.height;
```

**Consequences:**
- Windows on secondary monitors positioned incorrectly
- Clamping logic fails for monitors with negative coordinates

**Prevention:**
- Use `NSScreen::screens()` to enumerate all monitors
- Calculate virtual screen bounds encompassing all monitors
- Test with monitors in various arrangements (above, below, left, right)

**Detection:** Test with multi-monitor setups; windows jump to wrong positions.

**Migration impact:** Monitor enumeration code needs verification with objc2.

---

## Phase-Specific Warnings

| Phase Topic | Likely Pitfall | Mitigation |
|-------------|---------------|------------|
| Basic compilation | msg_send! syntax | Review every msg_send! call for comma syntax |
| Object creation | Retained vs raw pointers | Audit all object storage for proper ownership |
| Custom NSView | define_class! attributes | Follow template with all required attributes |
| Drawing | CGContext API differences | May keep core-graphics alongside objc2-app-kit |
| Coordinate handling | Multiple coordinate systems | Document coordinate system for each variable |
| Event handling | 1-based Y coordinates | Account for in mouse position calculations |
| Memory management | Autorelease timing | Convert all raw storage to Retained<T> |
| Thread safety | MainThreadOnly | Add thread_kind attribute to view classes |

---

## Migration Checklist

Before starting:
- [ ] Read objc2 documentation on `define_class!` macro
- [ ] Understand `Retained<T>` ownership semantics
- [ ] Review all `msg_send!` calls for syntax changes

During migration:
- [ ] Convert `ClassDecl` to `define_class!`
- [ ] Replace `id` with `Retained<T>` where appropriate
- [ ] Update `msg_send!` to use comma syntax
- [ ] Add `#[thread_kind = MainThreadOnly]` to view classes
- [ ] Implement proper init with `set_ivars()`
- [ ] Consider overriding `isFlipped` to simplify coordinate handling

Testing:
- [ ] Test window creation and positioning
- [ ] Test drawing with transparency
- [ ] Test mouse interaction and dragging
- [ ] Test on multi-monitor setups
- [ ] Run under AddressSanitizer to catch memory issues

---

## Sources

### Official Documentation
- [objc2 crate documentation](https://docs.rs/objc2/)
- [define_class! macro](https://docs.rs/objc2/latest/objc2/macro.define_class.html)
- [msg_send! macro](https://docs.rs/objc2/latest/objc2/macro.msg_send.html)
- [Retained<T> documentation](https://docs.rs/objc2/latest/objc2/rc/struct.Retained.html)
- [objc2-app-kit NSView](https://docs.rs/objc2-app-kit/latest/objc2_app_kit/struct.NSView.html)
- [objc2-core-graphics](https://docs.rs/objc2-core-graphics)

### GitHub Resources
- [objc2 repository](https://github.com/madsmtm/objc2)
- [Migration tracking issue #174](https://github.com/madsmtm/objc2/issues/174)
- [Tauri/wry migration issue #1239](https://github.com/tauri-apps/wry/issues/1239)
- [core-foundation-rs discussion #729](https://github.com/servo/core-foundation-rs/issues/729)

### Apple Documentation
- [Coordinate Systems and Transforms](https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/CocoaDrawingGuide/Transforms/Transforms.html)
- [Creating a Custom View](https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/CocoaViewsGuide/SubclassingNSView/SubclassingNSView.html)
- [CGImageAlphaInfo](https://developer.apple.com/documentation/coregraphics/cgimagealphainfo)
