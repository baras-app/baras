---
phase: 15-objc2-migration
plan: 02
subsystem: overlay
tags: [macos, objc2, define_class, nsview, appkit]

# Dependency graph
requires:
  - phase: 15-01
    provides: objc2 dependencies and msg_send! migration
provides:
  - BarasOverlayView defined via define_class! macro
  - Type-safe NSView subclass with Cell<T> interior mutability
  - Thread-safe main thread marker for AppKit
affects: [15-03-PLAN]

# Tech tracking
tech-stack:
  added: []
  patterns: [define_class! macro, Cell<T> interior mutability, Retained<T> field storage]

key-files:
  created: []
  modified:
    - overlay/src/platform/macos.rs

key-decisions:
  - "Use Cell<T> for interior mutability in ivars (objc2 methods take &self)"
  - "Implement unsafe Send/Sync for ivars (AppKit main thread requirement)"
  - "Use #[thread_kind = MainThreadOnly] for thread safety enforcement"
  - "Use Option<Retained<NSGraphicsContext>> from currentContext() instead of id/nil pattern"

patterns-established:
  - "define_class! with #[unsafe(super(T))] and #[thread_kind = MainThreadOnly] for NSView subclasses"
  - "Ivars struct with Cell<T> for each mutable field, Default derive for initialization"
  - "BarasOverlayView::new() uses set_ivars() pattern for initialization"
  - "Use &*self.view to dereference Retained<T> in msg_send! calls"

# Metrics
duration: 3min
completed: 2026-01-18
---

# Phase 15 Plan 02: define_class! Migration Summary

**BarasOverlayView now uses objc2 define_class! macro with Cell<T> ivars and MainThreadOnly thread safety**

## Performance

- **Duration:** 3 min
- **Started:** 2026-01-18T23:26:34Z
- **Completed:** 2026-01-18T23:29:15Z
- **Tasks:** 3
- **Files modified:** 1

## Accomplishments
- Replaced ClassDecl with objc2's define_class! macro for safer class declaration
- Added BarasOverlayViewIvars struct with Cell<T> for interior mutability
- Added #[thread_kind = MainThreadOnly] for AppKit thread safety
- Updated MacOSOverlay.view from `id` to `Retained<BarasOverlayView>`
- draw_rect now uses NSGraphicsContext::currentContext() returning Option<Retained<>>
- Removed static mut OVERLAY_VIEW_CLASS, get_overlay_view_class(), and extern "C" fn definitions

## Task Commits

All tasks committed as single atomic commit:

1. **Tasks 1-3: define_class! migration** - `ae33004` (feat)
   - BarasOverlayViewIvars struct with Cell<T> fields
   - define_class! macro with thread safety marker
   - MacOSOverlay updated to use Retained<BarasOverlayView>

## Files Modified
- `overlay/src/platform/macos.rs` - Complete define_class! migration

## Decisions Made
- **Cell<T> for ivars:** objc2 methods take `&self`, so interior mutability via Cell is required for mutable state
- **Send/Sync implementation:** Marked unsafe Send/Sync on ivars struct since we know AppKit views are main thread only
- **MainThreadOnly marker:** Enforces compile-time thread safety for AppKit operations
- **Option<Retained<>> pattern:** NSGraphicsContext::currentContext() returns proper Option type, eliminating id/nil checks

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

- **Cross-compilation limitation:** Cannot fully cross-compile to macOS from Linux due to zstd-sys native code dependencies. Verification done on Linux where macOS code is conditionally compiled out (same approach as Plan 15-01).

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- define_class! migration complete
- Ready for Plan 15-03 (cocoa crate removal)
- Remaining cocoa imports still needed for: NSWindow, NSScreen, NSColor, NSApp, NSEvent, etc.
- NSRect/NSPoint/NSSize now from objc2_foundation

---
*Phase: 15-objc2-migration*
*Completed: 2026-01-18*
