---
phase: 06-logging-migration
plan: 01
subsystem: logging
tags: [tracing, rolling-file, file-logging, size-rotation]

# Dependency graph
requires:
  - phase: 01-logging-foundation
    provides: tracing and tracing-subscriber workspace dependencies
provides:
  - File-based logging to ~/.config/baras/baras.log
  - Size-based log rotation at 10 MB
  - DEBUG_LOGGING=1 environment variable for verbose output
  - WorkerGuard lifetime management pattern
affects: [06-logging-migration, debugging, production-support]

# Tech tracking
tech-stack:
  added:
    - rolling-file = "0.2"
    - tracing-appender = "0.2"
  patterns:
    - "Dual-output logging with registry().with(layer).with(layer).with(filter)"
    - "WorkerGuard held in run() scope for log flushing on shutdown"
    - "Graceful fallback to stdout-only on file creation failure"

key-files:
  created:
    - app/src-tauri/src/logging.rs
  modified:
    - Cargo.toml
    - app/src-tauri/Cargo.toml
    - app/src-tauri/src/lib.rs

key-decisions:
  - "Single shared filter for both file and stdout layers (simplifies type composition)"
  - "Log to config dir root (baras.log) not logs subdirectory per CONTEXT.md"
  - "eprintln for pre-init errors since tracing not yet available"

patterns-established:
  - "logging::init() returns Option<WorkerGuard> for graceful degradation"
  - "File layer uses .with_ansi(false) for clean log files"

# Metrics
duration: 4min
completed: 2026-01-18
---

# Phase 06 Plan 01: File-Based Logging Summary

**File logging with 10 MB size-based rotation using rolling-file crate, DEBUG_LOGGING=1 flag for verbose baras crate output**

## Performance

- **Duration:** 4 min
- **Started:** 2026-01-18T01:07:00Z
- **Completed:** 2026-01-18T01:11:29Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments
- Added rolling-file and tracing-appender workspace dependencies
- Created logging.rs module with dual-output (file + stdout) configuration
- Implemented 10 MB size-based log rotation with 1 backup file retention
- DEBUG_LOGGING=1 enables debug-level output for app_lib, baras_core, baras_overlay
- Graceful fallback to stdout-only logging if file operations fail
- WorkerGuard held for app lifetime ensuring buffered logs flush on shutdown

## Task Commits

Each task was committed atomically:

1. **Task 1: Add rolling-file dependency** - `c62f577` (chore)
2. **Task 2: Create logging module with file rotation** - `c06fb06` (feat)

## Files Created/Modified
- `Cargo.toml` - Added rolling-file and tracing-appender to workspace dependencies
- `app/src-tauri/Cargo.toml` - Added rolling-file and tracing-appender dependencies
- `app/src-tauri/src/logging.rs` - New module with init() and init_stdout_only()
- `app/src-tauri/src/lib.rs` - Replaced inline tracing init with logging::init()

## Decisions Made
- Single shared filter for both layers: Simplifies the tracing-subscriber type composition. Both file and stdout get the same filter directive rather than per-layer filters which caused complex type errors.
- Log file at config root: `~/.config/baras/baras.log` per CONTEXT.md spec (no logs subdirectory since we only keep one file)
- Use eprintln for pre-init errors: Can't use tracing macros before subscriber is initialized, so fallback path uses eprintln

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- File logging infrastructure complete
- Ready for eprintln! migration in subsequent plans (06-02, 06-03)
- Log file will be created at `~/.config/baras/baras.log` on first app run

---
*Phase: 06-logging-migration*
*Completed: 2026-01-18*
