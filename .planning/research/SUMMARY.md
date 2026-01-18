# Research Summary: macOS objc2 Migration

**Project:** BARAS Overlay Renderer - macOS Platform
**Domain:** macOS Objective-C bindings migration for overlay rendering
**Researched:** 2026-01-18
**Confidence:** HIGH

## Executive Summary

The migration from the deprecated `cocoa` crate (v0.26) to the `objc2` ecosystem is a well-documented, necessary path forward. The Servo project has officially deprecated `cocoa` in favor of `objc2-app-kit`, and the objc2 ecosystem provides modern, type-safe bindings automatically generated from Apple SDK headers (Xcode 16.4). The core overlay functionality (borderless windows, transparency, click-through, custom NSView with bitmap rendering) translates directly to objc2 equivalents with improved memory safety via `Retained<T>` smart pointers.

The recommended approach is a phased migration: first fix the immediate CGContext compilation errors (which are independent of the cocoa/objc2 migration), then progressively migrate each subsystem. The custom `BarasOverlayView` NSView subclass is the most complex component, requiring conversion from `ClassDecl` to the `define_class!` macro with struct-based ivars and interior mutability patterns.

Key risks center on memory management differences (`Retained<T>` vs raw `id` pointers), `msg_send!` syntax changes requiring review of all 25+ call sites, and the `define_class!` macro's stricter safety requirements. These are mitigated by the phased approach allowing incremental verification. The existing premultiplied-alpha BGRA pixel conversion is correct and should be preserved.

## Key Findings

### Recommended Stack

The objc2 ecosystem provides a complete replacement for deprecated crates with better type safety and automatic memory management.

**Core technologies:**
- `objc2` (0.6.3): Core runtime bindings, `define_class!` macro, `msg_send!` - replaces `objc` crate
- `objc2-foundation` (0.3.2): NSRect, NSPoint, NSSize, NSString, NSArray - replaces `cocoa::foundation`
- `objc2-app-kit` (0.3.2): NSWindow, NSView, NSScreen, NSApplication, NSEvent - replaces `cocoa::appkit`
- `objc2-core-graphics` (0.3.2): CGContext, CGColorSpace, CGImage - optional, can keep existing `core-graphics` crate
- `block2` (0.6.1): Objective-C blocks for callbacks if needed

**Key decision:** Keep `core-graphics` (0.24) for CGContext operations alongside objc2-app-kit. This reduces migration scope while gaining the AppKit improvements.

### Expected Features

All existing macOS overlay features have direct objc2 equivalents:

**Must have (table stakes):**
- Borderless transparent window (`NSWindowStyleMask::Borderless`, `setOpaque(false)`, `clearColor`)
- Click-through mode (`setIgnoresMouseEvents(true/false)`)
- Window level control for overlay (`CGWindowLevelForKey(OverlayWindow)`)
- Multi-space visibility (`NSWindowCollectionBehavior::CanJoinAllSpaces`)
- Custom NSView with bitmap rendering (`define_class!` with `drawRect:`)
- Multi-monitor support (`NSScreen::screens()`, coordinate conversion)

**Should have (competitive):**
- Override `isFlipped` to simplify coordinate handling (currently done manually)
- Use overlay window level (102) instead of mainMenuWindowLevel+1 (25) for better stacking

**Defer (v2+):**
- Full migration to `objc2-core-graphics` (can keep `core-graphics` for now)
- Screen change notifications for hot-plug monitor support

### Architecture Approach

The migration follows a dependency-ordered approach where independent fixes come first, then crate dependencies update, followed by progressive code migration from simple (msg_send!) to complex (define_class!).

**Major components:**
1. **Cargo.toml dependencies** - Add objc2 crates, keep core-graphics, remove deprecated crates last
2. **msg_send! call sites** - Mechanical syntax updates with type annotation improvements
3. **BarasOverlayView class** - Convert ClassDecl to define_class! with struct ivars
4. **Window/App management** - Replace NSApp(), NSWindow::alloc() with objc2-app-kit methods
5. **Event loop** - Update event polling with objc2 types

### Critical Pitfalls

1. **msg_send! syntax changes** - Old syntax `msg_send![obj, selector: arg1: arg2]` becomes `msg_send![obj, selector: arg1, arg2]` with commas. Every call must be reviewed. Detection: compile errors mentioning `Encode` trait.

2. **Retained<T> vs raw id pointers** - `Retained<T>` provides RAII semantics; mixing with raw pointers causes double-free or use-after-free. Use `Retained::from_raw()` only for +1 methods (init/alloc/new/copy), use `Retained::retain_autoreleased()` for others.

3. **define_class! strict requirements** - Must use `#[unsafe(super(NSView))]`, struct-based ivars with `Cell<T>` for interior mutability, `#[thread_kind = MainThreadOnly]` for AppKit types, and proper init via `set_ivars()`.

4. **NSWindow memory management** - When creating NSWindow outside a window controller, MUST call `setReleasedWhenClosed(false)` immediately after creation to prevent use-after-free. This is why init methods are unsafe in objc2.

5. **Premultiplied alpha required** - Core Graphics only accepts premultiplied alpha for RGBA contexts. Current BGRA conversion code is correct; preserve it exactly.

## Implications for Roadmap

Based on research, suggested phase structure:

### Phase 1: CGContext Fix
**Rationale:** Independent of objc2 migration, unblocks builds immediately
**Delivers:** Working compilation on macOS
**Addresses:** Current compilation errors at lines 110, 119, 128
**Avoids:** Blocking all macOS development

### Phase 2: Dependency Update
**Rationale:** Must update Cargo.toml before code migration
**Delivers:** objc2 crates available alongside existing (parallel operation)
**Uses:** objc2 (0.6), objc2-foundation (0.3), objc2-app-kit (0.3)
**Implements:** Foundation for subsequent phases

### Phase 3: msg_send! Migration
**Rationale:** Mechanical, low-risk changes that establish patterns
**Delivers:** All message sends using objc2 syntax
**Avoids:** Encode trait errors, type mismatches

### Phase 4: Custom NSView Migration
**Rationale:** Most complex component, benefits from earlier phases as practice
**Delivers:** BarasOverlayView using define_class! macro
**Implements:** drawRect:, isOpaque, proper ivars with Cell<T>
**Avoids:** Class registration failures, uninitialized ivars

### Phase 5: Window/App Management
**Rationale:** Straightforward once patterns established from Phase 3-4
**Delivers:** Full objc2-app-kit usage for NSWindow, NSApplication, NSScreen
**Avoids:** Memory management issues with Retained<T>

### Phase 6: Cleanup
**Rationale:** Remove deprecated dependencies only after full migration verified
**Delivers:** Clean dependency tree, no deprecated crates
**Uses:** Only objc2 ecosystem + core-graphics

### Phase Ordering Rationale

- Phase 1 is independent and unblocks development immediately
- Phase 2 must precede code changes (cannot use APIs without crates)
- Phase 3 before Phase 4 because msg_send! patterns apply to define_class! methods
- Phase 4 is highest risk; having Phases 2-3 done provides fallback context
- Phase 5 uses patterns established in 3-4
- Phase 6 is verification; removing old crates too early breaks incremental testing

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 4 (Custom NSView):** Most complex transformation. No existing NSView+drawRect objc2 example found. Pattern documented but needs validation.

Phases with standard patterns (skip research-phase):
- **Phase 1 (CGContext Fix):** core-graphics API well-documented
- **Phase 2 (Dependencies):** Standard Cargo.toml changes
- **Phase 3 (msg_send!):** Mechanical transformation, fully documented
- **Phase 5 (Window/App):** Direct API mapping documented in FEATURES.md
- **Phase 6 (Cleanup):** Verification only

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | Official crate docs, deprecation notices explicit |
| Features | HIGH | Verified against Apple docs and objc2 API docs |
| Architecture | MEDIUM | Phase order logical, define_class! needs validation |
| Pitfalls | MEDIUM | Based on docs and ecosystem issues; runtime testing needed |

**Overall confidence:** HIGH

### Gaps to Address

- **define_class! with NSRect parameter:** Verify objc2-foundation NSRect works in drawRect: method signature
- **MainThreadMarker integration:** Need to verify how MainThreadMarker works with existing event loop that runs on main thread
- **NSWindowLevel constants:** Verify objc2-app-kit has equivalent to old `NSMainMenuWindowLevel`
- **core-graphics CGContext::from_existing_context_ptr:** Current API usage may need adjustment; verify exact method signature

## Sources

### Primary (HIGH confidence)
- [objc2 crate documentation](https://docs.rs/objc2/) - define_class!, msg_send!, Retained<T>
- [objc2-app-kit documentation](https://docs.rs/objc2-app-kit/) - NSWindow, NSView, NSApplication
- [objc2-foundation documentation](https://docs.rs/objc2-foundation/) - NSRect, NSString, geometry types
- [cocoa crate deprecation notice](https://docs.rs/cocoa/latest/cocoa/appkit/index.html) - "use objc2-app-kit instead"
- [Apple Developer Documentation](https://developer.apple.com/documentation/appkit/) - NSWindow, NSView, CGContext

### Secondary (MEDIUM confidence)
- [objc2 GitHub repository](https://github.com/madsmtm/objc2) - Migration patterns, issue discussions
- [servo/core-foundation-rs deprecation](https://github.com/servo/core-foundation-rs/issues/729) - Ecosystem direction
- [Tauri/wry migration issue #1239](https://github.com/tauri-apps/wry/issues/1239) - Real-world migration experience

### Tertiary (LOW confidence)
- Community blog posts on macOS coordinate systems - Needs verification against Apple docs
- macOS Sonoma click-through bug discussions - May or may not affect BARAS use case

---
*Research completed: 2026-01-18*
*Ready for roadmap: yes*
