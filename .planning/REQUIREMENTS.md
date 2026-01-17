# Requirements: BARAS Tech Debt Cleanup

**Defined:** 2026-01-17
**Core Value:** Users never see a frozen UI from a panic. Errors are caught, logged, and communicated gracefully.

## v1 Requirements

Requirements for this milestone. Each maps to roadmap phases.

### Error Handling (Core)

- [ ] **ERR-01**: All `.unwrap()` calls in `core/` replaced with proper Result handling
- [ ] **ERR-02**: All `.expect()` calls in `core/` replaced with proper Result handling
- [ ] **ERR-03**: Custom error types defined per module with `thiserror`

### Error Handling (Backend)

- [ ] **ERR-04**: All `.unwrap()` calls in `app/src-tauri/` return errors to frontend
- [ ] **ERR-05**: Tauri commands return `Result<T, String>` for frontend display

### Error Handling (Frontend)

- [ ] **ERR-06**: All JS interop `.unwrap()` calls in `app/src/` use fallible helpers
- [ ] **ERR-07**: UI displays error feedback when backend operations fail
- [ ] **ERR-08**: Error state prevents frozen UI (graceful degradation)

### Logging

- [ ] **LOG-01**: `tracing` crate integrated with appropriate subscriber
- [ ] **LOG-02**: All `eprintln!` calls migrated to `tracing` macros
- [ ] **LOG-03**: Error-level logging for all caught errors
- [ ] **LOG-04**: Debug-level logging for diagnostic info

### Clone Cleanup

- [ ] **CLN-01**: Unnecessary clones in `signal_processor/phase.rs` reduced
- [ ] **CLN-02**: Unnecessary clones in `timers/manager.rs` reduced
- [ ] **CLN-03**: Unnecessary clones in `effects/tracker.rs` reduced

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
| LOG-01 | Phase 1: Logging Foundation | Pending |
| ERR-03 | Phase 2: Core Error Types | Pending |
| ERR-01 | Phase 3: Core Error Handling | Pending |
| ERR-02 | Phase 3: Core Error Handling | Pending |
| ERR-04 | Phase 4: Backend Error Handling | Pending |
| ERR-05 | Phase 4: Backend Error Handling | Pending |
| ERR-06 | Phase 5: Frontend Error Handling | Pending |
| ERR-07 | Phase 5: Frontend Error Handling | Pending |
| ERR-08 | Phase 5: Frontend Error Handling | Pending |
| LOG-02 | Phase 6: Logging Migration | Pending |
| LOG-03 | Phase 6: Logging Migration | Pending |
| LOG-04 | Phase 6: Logging Migration | Pending |
| CLN-01 | Phase 7: Clone Cleanup | Pending |
| CLN-02 | Phase 7: Clone Cleanup | Pending |
| CLN-03 | Phase 7: Clone Cleanup | Pending |

**Coverage:**
- v1 requirements: 15 total
- Mapped to phases: 15
- Unmapped: 0

---
*Requirements defined: 2026-01-17*
*Last updated: 2026-01-17 — Traceability updated after roadmap creation*
