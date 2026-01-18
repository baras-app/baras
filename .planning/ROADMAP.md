# Roadmap: BARAS Tech Debt Cleanup

## Overview

This roadmap transforms BARAS from a panic-prone application into one with graceful error handling. The journey starts with logging infrastructure (so all subsequent work can use it), then builds error types, migrates error handling from core outward to frontend, completes logging migration, and finishes with clone cleanup in hot paths. Every phase delivers observable improvements in stability and debuggability.

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

- [x] **Phase 1: Logging Foundation** - Tracing infrastructure for all subsequent work ✓
- [x] **Phase 2: Core Error Types** - Custom error types per module ✓
- [x] **Phase 3: Core Error Handling** - Migrate unwrap/expect in core crate ✓
- [x] **Phase 4: Backend Error Handling** - Tauri commands return errors to frontend ✓
- [ ] **Phase 5: Frontend Error Handling** - UI displays errors gracefully
- [ ] **Phase 6: Logging Migration** - Replace all eprintln with tracing macros
- [ ] **Phase 7: Clone Cleanup** - Reduce unnecessary clones in hot paths

## Phase Details

### Phase 1: Logging Foundation
**Goal**: Establish tracing infrastructure so all error handling work can log appropriately
**Depends on**: Nothing (first phase)
**Requirements**: LOG-01
**Success Criteria** (what must be TRUE):
  1. Application starts with tracing subscriber active
  2. Debug and error messages appear in structured log output
  3. Log level is configurable (at minimum via compile-time feature or env var)
**Plans**: 2 plans

Plans:
- [x] 01-01-PLAN.md - Add tracing dependencies to workspace and crates ✓
- [x] 01-02-PLAN.md - Initialize tracing subscriber in binaries ✓

### Phase 2: Core Error Types
**Goal**: Define custom error types so error handling migration has proper types to use
**Depends on**: Phase 1
**Requirements**: ERR-03
**Success Criteria** (what must be TRUE):
  1. Each core module with fallible operations has a dedicated error type
  2. Error types use thiserror for derive macros
  3. Error types include meaningful context (not just "failed")
  4. Errors can be converted up the call chain (From impls where needed)
**Plans**: 3 plans

Plans:
- [x] 02-01-PLAN.md - Add thiserror dependency, create combat_log and dsl error types ✓
- [x] 02-02-PLAN.md - Create query and storage error types ✓
- [x] 02-03-PLAN.md - Create context and timers error types, convert PreferencesError ✓

### Phase 3: Core Error Handling
**Goal**: Core crate returns Results instead of panicking
**Depends on**: Phase 2
**Requirements**: ERR-01, ERR-02
**Success Criteria** (what must be TRUE):
  1. Zero .unwrap() calls remain in core/src (tests excluded)
  2. Zero .expect() calls remain in core/src (tests excluded)
  3. Functions that can fail return Result with appropriate error types
  4. Errors are logged at error level when caught
**Plans**: 3 plans

Plans:
- [x] 03-01-PLAN.md - Refactor simple helpers (effects, timers, storage, shielding) ✓
- [x] 03-02-PLAN.md - Convert signal processor invariants to defensive returns ✓
- [x] 03-03-PLAN.md - Convert public API expects (config, reader) ✓

### Phase 4: Backend Error Handling
**Goal**: Tauri backend returns errors to frontend instead of panicking
**Depends on**: Phase 3
**Requirements**: ERR-04, ERR-05
**Success Criteria** (what must be TRUE):
  1. Zero .unwrap() calls remain in app/src-tauri/src (tests excluded)
  2. All Tauri commands return Result<T, String> for frontend consumption
  3. Backend errors include user-friendly messages
  4. IPC never breaks due to backend panic (errors return gracefully)
**Plans**: 3 plans

Plans:
- [x] 04-01-PLAN.md — Mutex poison recovery in updater.rs (3 unwraps) ✓
- [x] 04-02-PLAN.md — Dev fallback paths in effects.rs + service/mod.rs (8 unwraps) ✓
- [x] 04-03-PLAN.md — Tray icon fallback in tray.rs (1 unwrap) ✓

### Phase 5: Frontend Error Handling
**Goal**: UI displays errors gracefully instead of freezing
**Depends on**: Phase 4
**Requirements**: ERR-06, ERR-07, ERR-08
**Success Criteria** (what must be TRUE):
  1. Zero .unwrap() calls remain in app/src (tests excluded)
  2. Failed backend operations display error feedback in UI
  3. Error states show actionable information (not blank/frozen screens)
  4. User can recover from errors without reloading application
**Plans**: 4 plans

Plans:
- [ ] 05-01-PLAN.md — Toast notification system (component + CSS + provider)
- [ ] 05-02-PLAN.md — JS interop helper for api.rs (~60 unwraps)
- [ ] 05-03-PLAN.md — ECharts JS interop cleanup (~95 unwraps in charts + data explorer)
- [ ] 05-04-PLAN.md — API Result conversion + toast wiring

### Phase 6: Logging Migration
**Goal**: All diagnostic output uses structured tracing instead of eprintln
**Depends on**: Phase 1
**Requirements**: LOG-02, LOG-03, LOG-04
**Success Criteria** (what must be TRUE):
  1. Zero eprintln! calls remain in production code
  2. Caught errors are logged at error level with context
  3. Diagnostic information uses debug/trace level appropriately
  4. Log output includes spans/context for traceability
**Plans**: TBD

Plans:
- [ ] 06-01: TBD

### Phase 7: Clone Cleanup
**Goal**: Hot paths use references instead of unnecessary clones
**Depends on**: Phase 3 (core must be stable first)
**Requirements**: CLN-01, CLN-02, CLN-03
**Success Criteria** (what must be TRUE):
  1. signal_processor/phase.rs clone count reduced by 50%+
  2. timers/manager.rs clone count reduced by 50%+
  3. effects/tracker.rs clone count reduced by 50%+
  4. No functional regressions (existing tests pass)
**Plans**: TBD

Plans:
- [ ] 07-01: TBD

## Progress

**Execution Order:**
Phases execute in numeric order: 1 -> 2 -> 3 -> 4 -> 5 -> 6 -> 7

Note: Phase 6 (Logging Migration) depends only on Phase 1, so could theoretically run in parallel with Phases 2-5. However, sequential execution is cleaner for this milestone.

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Logging Foundation | 2/2 | Complete ✓ | 2026-01-17 |
| 2. Core Error Types | 3/3 | Complete ✓ | 2026-01-17 |
| 3. Core Error Handling | 3/3 | Complete ✓ | 2026-01-17 |
| 4. Backend Error Handling | 3/3 | Complete ✓ | 2026-01-18 |
| 5. Frontend Error Handling | 0/4 | Planned | - |
| 6. Logging Migration | 0/TBD | Not started | - |
| 7. Clone Cleanup | 0/TBD | Not started | - |

---
*Roadmap created: 2026-01-17*
*Coverage: 15/15 v1 requirements mapped*
