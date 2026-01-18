# Requirements: BARAS Tech Debt Cleanup

**Defined:** 2026-01-17
**Core Value:** Users never see a frozen UI from a panic. Errors are caught, logged, and communicated gracefully.

## v1 Requirements

Requirements for this milestone. Each maps to roadmap phases.

### Error Handling (Core)

- [x] **ERR-01**: All `.unwrap()` calls in `core/` replaced with proper Result handling ✓
- [x] **ERR-02**: All `.expect()` calls in `core/` replaced with proper Result handling ✓
- [x] **ERR-03**: Custom error types defined per module with `thiserror` ✓

### Error Handling (Backend)

- [x] **ERR-04**: All `.unwrap()` calls in `app/src-tauri/` return errors to frontend ✓
- [x] **ERR-05**: Tauri commands return `Result<T, String>` for frontend display ✓

### Error Handling (Frontend)

- [x] **ERR-06**: All JS interop `.unwrap()` calls in `app/src/` use fallible helpers ✓
- [x] **ERR-07**: UI displays error feedback when backend operations fail ✓
- [x] **ERR-08**: Error state prevents frozen UI (graceful degradation) ✓

### Logging

- [x] **LOG-01**: `tracing` crate integrated with appropriate subscriber ✓
- [x] **LOG-02**: All `eprintln!` calls migrated to `tracing` macros ✓
- [x] **LOG-03**: Error-level logging for all caught errors ✓
- [x] **LOG-04**: Debug-level logging for diagnostic info ✓

### Clone Cleanup

- [x] **CLN-01**: Unnecessary clones in `signal_processor/phase.rs` reduced ✓ (34%: 35→23)
- [x] **CLN-02**: Unnecessary clones in `timers/manager.rs` reduced ✓ (33%: 36→24)
- [x] **CLN-03**: Unnecessary clones in `effects/tracker.rs` reduced ✓ (21%: 28→22)

## v2 Requirements

Deferred to future milestone. Tracked but not in current roadmap.

### Extended Cleanup

- **CLN-04**: Clone cleanup in remaining hot paths
- **ERR-09**: Retry logic for recoverable errors
- **LOG-05**: Log rotation and file output

## Out of Scope

Explicitly excluded. Documented to prevent scope creep.

| Feature | Reason |
|---------|--------|
| New features | This is debt reduction only |
| Performance profiling | Separate milestone |
| Test coverage expansion | Separate milestone |
| Platform overlay refactoring | Separate milestone |
| UI redesign | Error display integrates with existing patterns |

## Traceability

Which phases cover which requirements.

| Requirement | Phase | Status |
|-------------|-------|--------|
| LOG-01 | Phase 1: Logging Foundation | Complete |
| ERR-03 | Phase 2: Core Error Types | Complete |
| ERR-01 | Phase 3: Core Error Handling | Complete |
| ERR-02 | Phase 3: Core Error Handling | Complete |
| ERR-04 | Phase 4: Backend Error Handling | Complete |
| ERR-05 | Phase 4: Backend Error Handling | Complete |
| ERR-06 | Phase 5: Frontend Error Handling | Complete |
| ERR-07 | Phase 5: Frontend Error Handling | Complete |
| ERR-08 | Phase 5: Frontend Error Handling | Complete |
| LOG-02 | Phase 6: Logging Migration | Complete |
| LOG-03 | Phase 6: Logging Migration | Complete |
| LOG-04 | Phase 6: Logging Migration | Complete |
| CLN-01 | Phase 7: Clone Cleanup | Complete |
| CLN-02 | Phase 7: Clone Cleanup | Complete |
| CLN-03 | Phase 7: Clone Cleanup | Complete |

**Coverage:**
- v1 requirements: 15 total
- Mapped to phases: 15
- Unmapped: 0

---
*Requirements defined: 2026-01-17*
*Last updated: 2026-01-18 — Milestone complete (all 15 requirements satisfied)*
