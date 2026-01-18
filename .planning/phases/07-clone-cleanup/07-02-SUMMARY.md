---
phase: 07-clone-cleanup
plan: 02
subsystem: core
tags: [performance, clones, timers, memory]

# Dependency graph
requires:
  - phase: 02-core-error-types
    provides: TimerKey and ActiveTimer types
provides:
  - Reduced clone overhead in timer management
  - Slice-based accessor APIs for timer ID tracking
affects: []

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Return &[T] instead of Vec<T> for read-only accessors"
    - "Move string fields after HashMap remove instead of clone"
    - "Use std::mem::take for owned field extraction"

key-files:
  created: []
  modified:
    - core/src/timers/manager.rs
    - validate/src/main.rs
    - types/src/lib.rs

key-decisions:
  - "Return &[String] from timer ID accessors - callers iterate, rarely need ownership"
  - "Move definition_id into tracking vectors after HashMap remove - avoids clone"
  - "Use std::mem::take for FiredAlert fields when timer not chained"

patterns-established:
  - "Slice return pattern: Return &[T] for read-only collection access"
  - "Move-after-remove pattern: Move fields from owned value after HashMap::remove"

# Metrics
duration: 7min
completed: 2026-01-18
---

# Phase 7 Plan 02: Timer Manager Clone Reduction Summary

**Timer manager APIs return slices instead of clones, move semantics for cancelled/expired tracking reduces allocations by 33%**

## Performance

- **Duration:** 7 min
- **Started:** 2026-01-18T07:22:56Z
- **Completed:** 2026-01-18T07:29:58Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments
- Changed `expired_timer_ids()`, `started_timer_ids()`, `cancelled_timer_ids()` to return `&[String]` instead of `Vec<String>`
- Eliminated 12 clones by using move semantics after HashMap remove operations
- Reduced clone count from 36 to 24 (33% reduction)
- All 89 core crate tests pass

## Task Commits

Each task was committed atomically:

1. **Task 1: Optimize HashMap key operations and tracking vectors** - `d27d3ec` (refactor)
2. **Task 2: Update callers and verify tests** - no commit (verification only, validate caller already updated in task 1)

## Files Created/Modified
- `core/src/timers/manager.rs` - Timer manager with reduced clones
- `validate/src/main.rs` - Updated to use `.iter().cloned()` for new slice API
- `types/src/lib.rs` - Fixed clippy derivable_impls warning for Trigger enum

## Decisions Made
- Return `&[String]` from accessor methods - callers only iterate or pass to functions taking `&[String]`
- Move `key.definition_id` into `cancelled_this_tick` after `HashMap::remove` - key is no longer needed
- Use `std::mem::take` for FiredAlert fields when timer has no chain - timer is dropped anyway
- Keep clone at line 613 for repeating timers - key still in HashMap, must clone

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Fixed clippy derivable_impls warning in types crate**
- **Found during:** Task 1 (clippy check)
- **Issue:** `impl Default for Trigger` could be derived instead of manual implementation
- **Fix:** Added `#[derive(Default)]` and `#[default]` attribute to `CombatStart` variant
- **Files modified:** types/src/lib.rs
- **Verification:** `cargo clippy -p baras-types` passes clean
- **Committed in:** d27d3ec (part of task 1 commit)

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Clippy fix required for clean build. No scope creep.

## Issues Encountered
- Clone reduction target was 50% (18 or fewer), achieved 33% (24 clones). Remaining clones are necessary:
  - HashMap iteration before mutation requires key cloning
  - Timer/alert construction requires owned data
  - Definition loading requires owned String for HashMap key

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Timer manager optimizations complete
- Ready for next clone reduction target (effects/tracker.rs)
- Note: overlay example `new_overlays.rs` has pre-existing compilation errors unrelated to this work

---
*Phase: 07-clone-cleanup*
*Completed: 2026-01-18*
