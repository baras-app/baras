---
phase: 02-core-error-types
plan: 02
subsystem: database
tags: [datafusion, parquet, arrow, thiserror, error-handling]

# Dependency graph
requires:
  - phase: 02-01
    provides: ParseError, ReaderError patterns from combat_log
provides:
  - QueryError enum for DataFusion operations
  - StorageError enum for parquet file operations
  - "#[from] conversions for library errors"
affects: [03-context-error-types, 04-error-propagation]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Context-rich error variants with path/query fields"
    - "#[from] for library error conversions (DataFusionError, ParquetError, ArrowError)"
    - "#[source] for error chaining with context"

key-files:
  created:
    - core/src/query/error.rs
    - core/src/storage/error.rs
  modified:
    - core/src/query/mod.rs
    - core/src/storage/mod.rs

key-decisions:
  - "Both Arrow and DataFusion errors have #[from] in QueryError (frequently occur together)"
  - "StorageError has generic Io and Parquet #[from] plus context-rich CreateFile/WriteParquet variants"
  - "BuildRecordBatch uses reason: String for flexibility in describing batch construction failures"

patterns-established:
  - "Data layer errors: generic #[from] for common cases, context-rich variants for actionable errors"

# Metrics
duration: 2min
completed: 2026-01-17
---

# Phase 02 Plan 02: Query and Storage Error Types Summary

**QueryError and StorageError enums with #[from] DataFusion/parquet conversions and context-rich variants for debugging**

## Performance

- **Duration:** 2 min
- **Started:** 2026-01-17T23:27:00Z
- **Completed:** 2026-01-17T23:29:27Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments

- QueryError enum covering DataFusion query operations with column/type/parquet context
- StorageError enum covering parquet file I/O with path context
- Both error types use #[from] for library error conversions enabling ? operator

## Task Commits

Each task was committed atomically:

1. **Task 1: Create query error types** - `2a34746` (feat)
2. **Task 2: Create storage error types** - `d03b7a6` (feat)

**Plan metadata:** (this commit) (docs: complete plan)

## Files Created/Modified

- `core/src/query/error.rs` - QueryError enum with DataFusion, Arrow, column errors
- `core/src/query/mod.rs` - Added `pub mod error` and `pub use error::QueryError`
- `core/src/storage/error.rs` - StorageError enum with parquet, IO, path errors
- `core/src/storage/mod.rs` - Added `pub mod error` and `pub use error::StorageError`

## Decisions Made

- **Dual #[from] in QueryError:** Both Arrow and DataFusion errors get automatic conversion since they frequently occur together in query operations
- **Context-rich variants:** RegisterParquet and SqlExecution include the path/query that failed for actionable debugging
- **Generic + specific:** StorageError has both generic `Io(#[from] std::io::Error)` for simple cases and `CreateFile`/`CreateDir` with path context for user-facing errors

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- QueryError and StorageError ready for migration in propagation phases
- Context module errors (02-03) should follow same patterns
- Phase 03 can reference these as examples for additional error types

---
*Phase: 02-core-error-types*
*Completed: 2026-01-17*
