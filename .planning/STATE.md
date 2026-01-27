# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-01-18)

**Core value:** Fast, reliable combat analysis that doesn't crash when something unexpected happens.
**Current focus:** v1.2 macOS Support - Phase 15 Complete (objc2 Migration)

## Current Position

Milestone: v1.2 macOS Support
Phase: 15 of 16 (objc2 Migration)
Plan: 03 of 03
Status: Phase complete
Last activity: 2026-01-26 - Completed quick task 002: Encounter section overlay UI

Progress: [#########################.....] 37/38 plans (v1.0 + v1.1 complete, v1.2 in progress)

## Performance Metrics

**v1.0 Tech Debt Cleanup:**
- Total plans completed: 23
- Average duration: 3.5 min
- Total execution time: 91 min
- Commits: 87
- Files modified: 124

**v1.1 UX Polish:**
- Plans completed: 10 (Phase 8: 2, Phase 9: 2, Phase 11: 1, Phase 12: 3, Phase 13: 2)
- Phase 10 deferred to v1.3

**v1.2 macOS Support:**
- Plans completed: 4/5 (Phase 14: 1, Phase 15: 3)
- Phases: 14, 15, 16
- Phase 15 Plan 01: 2 min
- Phase 15 Plan 02: 3 min
- Phase 15 Plan 03: 4 min

## Accumulated Context

### Decisions

See `.planning/milestones/v1.0-ROADMAP.md` for v1.0 decisions.
See `.planning/milestones/v1.1-ROADMAP.md` for v1.1 decisions.

Recent:
- Keep `core-graphics` crate for CGContext (no objc2-core-graphics migration needed)
- Single file scope: all work in `overlay/src/platform/macos.rs`
- CGContext::create_bitmap_context returns CGContext directly (not Option)
- CGContext::from_existing_context_ptr requires sys::CGContext pointer type
- Use default-features = false with explicit feature flags for objc2 crates
- msg_send! uses comma-separated arguments with Rust bool (true/false)
- Use Cell<T> for interior mutability in define_class! ivars
- Add #[thread_kind = MainThreadOnly] for AppKit thread safety
- Use &*self.view pattern to dereference Retained<T> in msg_send! calls
- Use Retained<NSWindow> for type-safe window ownership
- setReleasedWhenClosed(false) required for correct memory management (MAC-04)
- Window level 25 for above-most-windows behavior
- std::ptr::eq for window identity comparison in event handling

### Pending Todos

- Phase 10 (Navigation Redesign) deferred to v1.3 - NAV-01, NAV-02, NAV-03

### Blockers/Concerns

- Pre-existing clippy warnings (30+) across codebase should be addressed in future cleanup
- Overlay example new_overlays.rs has pre-existing compilation errors (stale API)
- Overlay test format_number has pre-existing failure (precision mismatch)
- [RESOLVED] define_class! with NSRect parameter validated - compiles successfully
- [RESOLVED] cocoa crate removal complete - macos.rs now uses objc2-app-kit exclusively

### Quick Tasks Completed

| # | Description | Date | Commit | Directory |
|---|-------------|------|--------|-----------|
| 001 | Encounter timers A/B display targets | 2026-01-26 | e18a0d9 | [001-encounter-timers-a-b-display-targets](./quick/001-encounter-timers-a-b-display-targets/) |
| 002 | Encounter section overlay UI | 2026-01-26 | 59b1c72 | [002-encounter-section-overlay-ui](./quick/002-encounter-section-overlay-ui/) |

## Session Continuity

Last session: 2026-01-26
Stopped at: Completed quick task 002: Encounter section overlay UI
Resume file: None
Next: Phase 16 - Dependency Cleanup
