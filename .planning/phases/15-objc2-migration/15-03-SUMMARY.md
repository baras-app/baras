---
phase: 15-objc2-migration
plan: 03
subsystem: overlay
tags: [macos, objc2, objc2-app-kit, nswindow, nsapplication, memory-management]

# Dependency graph
requires:
  - phase: 15-02
    provides: define_class! migration for BarasOverlayView
provides:
  - Full objc2-app-kit window management (no cocoa crate usage)
  - Retained<NSWindow> smart pointer for memory safety
  - NSApplication::sharedApplication() for app access
  - setReleasedWhenClosed(false) for correct memory management
affects: [16-PLAN]

# Tech tracking
tech-stack:
  added: []
  patterns: [Retained<T> struct fields, Option return types, std::ptr::eq for identity comparison]

key-files:
  created: []
  modified:
    - overlay/src/platform/macos.rs

key-decisions:
  - "Use Retained<NSWindow> instead of raw id for memory-safe window ownership"
  - "Window level 25 (NSMainMenuWindowLevel + 1) for above-most-windows behavior"
  - "NSString::from_str() for run loop mode instead of NSString::alloc().init_str()"
  - "std::ptr::eq for window identity comparison in event handling"

patterns-established:
  - "NSApplication::sharedApplication() instead of NSApp()"
  - "NSScreen::mainScreen() returns Option<Retained<NSScreen>>"
  - "event.r#type() for event type (type is reserved keyword)"
  - "event.window() returns Option<Retained<NSWindow>>"
  - "NSEventMask::Any instead of NSAnyEventMask"
  - "NSEventType::LeftMouseDown instead of NSLeftMouseDown"
  - "setFrame_display(rect, true) instead of setFrame_display_(rect, YES)"

# Metrics
duration: 4min
completed: 2026-01-18
---

# Phase 15 Plan 03: cocoa Crate Removal Summary

**Complete objc2-app-kit migration - all cocoa and objc crate imports removed from macos.rs**

## Performance

- **Duration:** 4 min
- **Started:** 2026-01-18
- **Completed:** 2026-01-18
- **Tasks:** 5
- **Files modified:** 1

## Accomplishments
- Removed all cocoa::appkit imports (NSApp, NSApplication, NSWindow, etc.)
- Removed all cocoa::base imports (id, nil, YES, NO)
- Removed all cocoa::foundation imports (NSArray, NSDate, NSString)
- Replaced raw `id` pointer with `Retained<NSWindow>` for type safety
- Added `setReleasedWhenClosed(false)` for MAC-04 memory management
- Migrated `new()` constructor to objc2-app-kit initialization pattern
- Migrated simple methods (set_position, set_size, set_click_through, commit, Drop)
- Migrated complex `poll_events()` with proper Option handling
- Migrated `get_all_monitors()` to use NSScreen::screens() iterator

## Task Commits

| Task | Name | Commit | Type |
|------|------|--------|------|
| 1 | Replace cocoa imports with objc2-app-kit | b753a16 | refactor |
| 2 | Update MacOSOverlay struct to use Retained types | 315a5e9 | refactor |
| 3 | Migrate MacOSOverlay::new() to objc2-app-kit | 26db5ef | feat |
| 4 | Migrate simple methods | ce3a0cd | refactor |
| 5 | Migrate poll_events() to objc2-app-kit | ec07192 | feat |

## Files Modified
- `overlay/src/platform/macos.rs` - Complete objc2-app-kit migration

## Key API Migrations

| Old (cocoa) | New (objc2-app-kit) |
|-------------|---------------------|
| `NSApp()` | `NSApplication::sharedApplication()` |
| `NSScreen::mainScreen(nil)` | `NSScreen::mainScreen()` -> `Option` |
| `NSScreen::screens(nil)` | `NSScreen::screens()` -> `Retained<NSArray>` |
| `NSWindow::alloc(nil).initWith...` | `NSWindow::initWithContentRect_styleMask_backing_defer()` |
| `NSWindowStyleMask::NSBorderlessWindowMask` | `NSWindowStyleMask::Borderless` |
| `NSBackingStoreBuffered` | `NSBackingStoreType::Buffered` |
| `window.setLevel_(i64)` | `window.setLevel(NSWindowLevel(25))` |
| `window.setOpaque_(NO)` | `window.setOpaque(false)` |
| `window.setContentView_(view)` | `window.setContentView(Some(&view))` |
| `event.eventType()` | `event.r#type()` |
| `NSEventMask::NSAnyEventMask` | `NSEventMask::Any` |
| `NSEventType::NSLeftMouseDown` | `NSEventType::LeftMouseDown` |
| `msg_send![app, sendEvent: event]` | `app.sendEvent(&event)` |

## Decisions Made
- **Retained<NSWindow> for struct field:** Ensures automatic memory management via Rust ownership
- **setReleasedWhenClosed(false):** Critical for correct memory management without window controller (MAC-04)
- **Window level 25:** Explicit constant rather than importing NSMainMenuWindowLevel
- **std::ptr::eq for window comparison:** Safe identity comparison for Retained<T> types

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

- **Cross-compilation limitation:** Cannot fully cross-compile to macOS from Linux due to zstd-sys native code dependencies. Verification done on Linux where macOS code is conditionally compiled out (same approach as Plans 15-01 and 15-02).

## Verification Results

| Check | Status |
|-------|--------|
| No cocoa:: imports | PASS |
| No objc:: imports | PASS |
| No raw id type (except CGContext bridge) | PASS |
| Retained<NSWindow> for window field | PASS |
| setReleasedWhenClosed(false) present | PASS |
| NSEventType:: prefix (no NS on variants) | PASS |
| NSApplication::sharedApplication() used | PASS |
| cargo check passes (Linux host) | PASS |

## Requirements Satisfied

- **MAC-02:** objc2-app-kit for all AppKit types
- **MAC-04:** setReleasedWhenClosed(false) for memory management
- **MAC-05:** All Objective-C objects use Retained<T> smart pointers

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- objc2 migration complete for macos.rs
- Ready for Phase 16 (Dependency Cleanup)
- cocoa and objc crates can now be removed from Cargo.toml

---
*Phase: 15-objc2-migration*
*Completed: 2026-01-18*
