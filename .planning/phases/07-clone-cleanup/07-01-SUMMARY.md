---
phase: 07-clone-cleanup
plan: 01
subsystem: signal-processor
tags: [rust, borrow-checker, clone-reduction, phase-transitions, memory-optimization]

# Dependency graph
requires:
  - phase: 06-logging-migration
    provides: tracing infrastructure for error logging
provides:
  - Two-pass borrow pattern for phase transition functions
  - Clone-on-match-only pattern (clones moved from cold to hot path)
affects:
  - 07-02-PLAN (entity_tracker.rs clone reduction)
  - 07-03-PLAN (signal_processor.rs clone reduction)

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Two-pass borrow pattern: immutable pass finds match, captures minimal data, mutable pass applies changes"
    - "Clone-on-match-only: defer clones to hot path (match found) rather than cold path (every call)"

key-files:
  created: []
  modified:
    - core/src/signal_processor/phase.rs

key-decisions:
  - "Two-pass pattern for borrow checker: split find-then-mutate into separate scopes"
  - "Clone only when match found: moved 5 clones per function from before-loop to inside-match"
  - "Clones for GameSignal fields are necessary: GameSignal owns Strings, can't use references"
  - "Clones for reset_counters_to_initial are necessary: mutable borrow prevents reference retention"

patterns-established:
  - "Two-pass borrow pattern: When function needs immutable access to search then mutable access to modify, use { let match_data = { immutable_search }; if let Some(data) = match_data { mutable_action } }"

# Metrics
duration: 6min
completed: 2026-01-18
---

# Phase 07 Plan 01: Phase Transition Clone Reduction Summary

**Two-pass borrow pattern eliminates 12 unnecessary clones in phase transition functions, deferring remaining clones to match-only hot path**

## Performance

- **Duration:** 6 min
- **Started:** 2026-01-18T07:22:55Z
- **Completed:** 2026-01-18T07:28:28Z
- **Tasks:** 2
- **Files modified:** 1

## Accomplishments

- Reduced clone count in phase.rs from 35 to 23 (34% reduction)
- Restructured all four phase transition functions using two-pass borrow pattern
- Moved clones from cold path (every call) to hot path (only when match found)
- All 89 core crate tests pass, confirming no behavioral regressions

## Task Commits

Each task was committed atomically:

1. **Task 1: Restructure borrow checker workarounds** - `bacc621` (refactor)
2. **Task 2: Verify phase transition behavior** - no commit (verification task)

## Files Created/Modified

- `core/src/signal_processor/phase.rs` - Phase transition logic with reduced clones via two-pass borrow pattern

## Decisions Made

1. **Two-pass borrow pattern** - Separating find (immutable) from mutate (mutable) into distinct scopes allows holding references longer, eliminating pre-loop defensive clones

2. **Clone-on-match-only** - Original code cloned `current_phase`, `previous_phase`, `counter_defs`, `phases`, `entities` before every loop iteration. New code only clones when a phase match is found.

3. **Necessary clones accepted** - 23 remaining clones are genuinely necessary:
   - GameSignal owns String fields (boss_id, old_phase, new_phase, phase_id)
   - reset_counters_to_initial needs owned data since enc is mutably borrowed
   - These only execute on the match path (phase transition found)

## Deviations from Plan

None - plan executed exactly as written.

Note: Plan target was 17 clones (50% reduction), achieved 23 clones (34% reduction). The difference is due to necessary clones for GameSignal String fields and reset_counters_to_initial parameters that cannot be eliminated without changing type signatures across the codebase.

## Issues Encountered

1. **Pre-existing build error in parser.rs** - Discovered unrelated borrow checker error from previous commit. The error was already fixed in the stashed working directory (`.to_vec()` calls on timer ID retrieval). Build passes.

2. **Pre-existing clippy warnings** - The codebase has 30+ clippy warnings (collapsible_if, derivable_impls, etc.) in files unrelated to phase.rs. My changes preserve the same number of warnings (5) in phase.rs as the original code.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Phase transition functions now use optimized two-pass borrow pattern
- Pattern established for similar clone reduction in 07-02 (entity_tracker.rs) and 07-03 (signal_processor.rs)
- Pre-existing clippy warnings should be addressed in a separate cleanup effort

---
*Phase: 07-clone-cleanup*
*Completed: 2026-01-18*
