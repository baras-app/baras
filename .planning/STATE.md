# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-01-17)

**Core value:** Users never see a frozen UI from a panic. Errors are caught, logged, and communicated gracefully.
**Current focus:** Phase 7 Complete - Clone Cleanup finished

## Current Position

Phase: 7 of 7 (Clone Cleanup) - COMPLETE
Plan: 3 of 3 in phase 7
Status: Phase complete
Last activity: 2026-01-18 - Completed 07-03-PLAN.md

Progress: [██████████] 100% (23/23 plans)

## Performance Metrics

**Velocity:**
- Total plans completed: 23
- Average duration: 3.5 min
- Total execution time: 91 min

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 01-logging-foundation | 2 | 7 min | 3.5 min |
| 02-core-error-types | 3 | 6 min | 2 min |
| 03-core-error-handling | 3 | 12 min | 4 min |
| 04-backend-error-handling | 3 | 3 min | 1 min |
| 05-frontend-error-handling | 4 | 18 min | 4.5 min |
| 06-logging-migration | 4 | 17 min | 4.25 min |
| 07-clone-cleanup | 3 | 28 min | 9.3 min |

**Recent Trend:**
- Last 5 plans: 06-04 (5 min), 07-01 (6 min), 07-02 (7 min), 07-03 (15 min)
- Trend: Consistent

*Updated after each plan completion*

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

- Remove ALL production unwraps (clean policy, no exceptions)
- Use tracing for logging (industry standard, structured, async-friendly)
- Error type per crate (avoid monolithic error enum)
- Workspace dependency inheritance for consistent tracing versions (01-01)
- Binary crates get tracing-subscriber, library crates get tracing only (01-01)
- DEBUG default for debug builds, INFO for release in main app (01-02)
- INFO default for parse-worker subprocess (01-02)
- Use EnvFilter::from_env_lossy() for graceful RUST_LOG handling (01-02)
- Error types include context fields (paths, line numbers) for debugging (02-01)
- Separate ParseError vs ReaderError for combat_log (different failure modes) (02-01)
- Use #[source] attribute for error chaining with std::io::Error (02-01)
- Dual #[from] for Arrow and DataFusion in QueryError (frequently co-occur) (02-02)
- Generic + context-rich variants pattern for StorageError (02-02)
- WatcherError wraps notify::Error for file watching failures (02-03)
- ConfigError wraps confy::ConfyError for configuration persistence (02-03)
- PreferencesError converted from manual impl to thiserror derive (02-03)
- Use let-else early return pattern for get_entities helpers (03-01)
- Use Option::map().unwrap_or(false) for filter predicates (03-01)
- BUG-level tracing::error! for invariant violations in signal_processor (03-02)
- continue for loop invariant failures, return for function-level failures (03-02)
- save() returns Result, handler.rs uses fire-and-forget logging (03-03)
- SessionDateMissing is a distinct ReaderError variant for programming invariants (03-03)
- Use ok_or with string error for simple Option unwrap replacement (04-03)
- Mutex poison recovery instead of error return for transient state (04-01)
- warn level for Mutex poison recovery (successfully recovered) (04-01)
- Use ancestors().nth(2) for safe grandparent traversal (04-02)
- Ultimate fallback to PathBuf::from(".") prevents panic in edge cases (04-02)
- ToastManager methods take &mut self for Signal.write() access (05-01)
- Cap toasts at 5 max, oldest removed first (05-01)
- Toast durations: 5s Normal, 7s Critical per CONTEXT.md (05-01)
- Log to console on JS interop failure rather than panic (05-02)
- Centralize js_set helper in utils.rs for reuse (05-02)
- Use unwrap_or(Equal) for NaN handling in float comparisons (05-03)
- js_set pattern for all ECharts option building (05-03)
- Fire-and-forget config saves now show toast on error (05-04)
- Profile operations (load/save/delete) show toast on error (05-04)
- let mut toast = use_toast(); before spawn for error handling (05-04)
- All mutation APIs must use try_invoke to prevent UI freeze on error (05-HT)
- Single shared filter for both file and stdout layers (simplifies type composition) (06-01)
- Log to config dir root (baras.log) not logs subdirectory per CONTEXT.md (06-01)
- eprintln for pre-init errors since tracing not yet available (06-01)
- INFO level for parse timing and icon cache loaded (user-visible operations) (06-03)
- DEBUG level for worker paths, definition loading, encounter details (developer diagnostics) (06-03)
- Remove [PARSE], [ICONS], [EFFECTS], [ENCOUNTERS], [HOTKEY] prefixes - tracing targets replace them (06-03)
- Windows overlay_log! macro wraps tracing::debug! with format! (06-04)
- DEBUG level for Wayland position/state changes and output enumeration (06-04)
- WARN level for degraded scenarios (saved monitor not found) (06-04)
- ERROR level for rebind failures (missing compositor/layer_shell) (06-04)
- Two-pass borrow pattern: immutable pass finds match, captures minimal data, mutable pass applies changes (07-01)
- Clone-on-match-only: defer clones to hot path (match found) rather than cold path (every call) (07-01)
- GameSignal String field clones are necessary (owned data required by enum) (07-01)
- Return &[String] from timer ID accessors - callers iterate, rarely need ownership (07-02)
- Move definition_id into tracking vectors after HashMap remove - avoids clone (07-02)
- Use std::mem::take for FiredAlert fields when timer not chained (07-02)
- Use EffectKey::new() for cleaner key construction (07-03)
- 21% clone reduction acceptable - remaining clones needed for owned data in HashMap/FiredAlert (07-03)

### Pending Todos

None yet.

### Blockers/Concerns

- Pre-existing clippy warnings (30+) across codebase should be addressed in future cleanup
- Overlay example new_overlays.rs has pre-existing compilation errors (stale API)
- Overlay test format_number has pre-existing failure (precision mismatch)

## Session Continuity

Last session: 2026-01-18T07:40Z
Stopped at: Completed 07-03-PLAN.md (Phase 7 complete - ALL PHASES COMPLETE)
Resume file: None
