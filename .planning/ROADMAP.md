# Roadmap: BARAS v1.2 macOS Support

## Overview

This milestone adds macOS platform support by fixing the overlay renderer implementation. The work progresses from fixing immediate compilation errors, through a full migration from deprecated `cocoa`/`objc` crates to the modern `objc2` ecosystem, to final cleanup of deprecated dependencies.

## Milestones

- v1.0 Tech Debt Cleanup (shipped 2026-01-18) - see `.planning/milestones/v1.0-ROADMAP.md`
- v1.1 UX Polish (shipped 2026-01-18) - see `.planning/milestones/v1.1-ROADMAP.md`
- **v1.2 macOS Support** - Phases 14-16 (in progress)

## Phases

- [x] **Phase 14: CGContext Fix** - Fix compilation errors to unblock macOS builds
- [ ] **Phase 15: objc2 Migration** - Migrate from deprecated cocoa/objc crates to objc2 ecosystem
- [ ] **Phase 16: Cleanup** - Remove deprecated dependencies after migration verified

## Phase Details

### Phase 14: CGContext Fix
**Goal**: macOS overlay builds successfully
**Depends on**: Nothing (independent fix)
**Requirements**: MAC-01
**Success Criteria** (what must be TRUE):
  1. `cargo build --target aarch64-apple-darwin -p overlay` compiles without errors
  2. CGContext bitmap rendering code uses correct type signatures
**Plans**: 1 plan

Plans:
- [x] 14-01-PLAN.md - Fix CGContext type mismatches and API calls

### Phase 15: objc2 Migration
**Goal**: Overlay uses modern, memory-safe Objective-C bindings
**Depends on**: Phase 14
**Requirements**: MAC-02, MAC-03, MAC-04, MAC-05
**Success Criteria** (what must be TRUE):
  1. All NSWindow/NSView/NSApplication code uses objc2-app-kit types
  2. BarasOverlayView uses define_class! macro with struct ivars
  3. Window creation includes setReleasedWhenClosed(false) for proper memory management
  4. All Objective-C objects use Retained<T> smart pointers (no raw id pointers)
**Plans**: 3 plans

Plans:
- [ ] 15-01-PLAN.md - Add objc2 dependencies and migrate msg_send! calls
- [ ] 15-02-PLAN.md - Migrate BarasOverlayView to define_class! macro
- [ ] 15-03-PLAN.md - Migrate window/app management to objc2-app-kit

### Phase 16: Cleanup
**Goal**: Clean dependency tree with no deprecated crates
**Depends on**: Phase 15
**Requirements**: MAC-06
**Success Criteria** (what must be TRUE):
  1. `cocoa` and `objc` crates removed from Cargo.toml
  2. macOS overlay still builds and functions correctly
  3. CI passes on macOS target
**Plans**: 1 plan

Plans:
- [ ] 16-01-PLAN.md - Remove deprecated cocoa/objc dependencies and verify build

## Progress

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 14. CGContext Fix | 1/1 | Complete | 2026-01-18 |
| 15. objc2 Migration | 0/3 | Planned | - |
| 16. Cleanup | 0/1 | Planned | - |

---
*Created: 2026-01-18*
*Milestone: v1.2 macOS Support*
