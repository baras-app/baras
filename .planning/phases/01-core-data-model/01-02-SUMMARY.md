---
phase: 01-core-data-model
plan: 02
subsystem: timers
tags: [behavioral, ability-queue, gcd, queued-hold, timers]
dependency_graph:
  requires: [01-01]
  provides: [active_gcd lifecycle, queued-hold logic, GCD pruning on tick, combat-end GCD clear]
  affects: [core/src/timers/manager.rs, core/src/timers/signal_handlers.rs]
tech_stack:
  added: []
  patterns: [Option single-slot replace policy, in-place mutation for queued-hold, GCD pruning in tick loop]
key_files:
  created: []
  modified:
    - core/src/timers/manager.rs
    - core/src/timers/signal_handlers.rs
decisions:
  - "queued-hold fires alerts/chains on the expiry-to-queued transition tick, then is_queued guard prevents double-processing on subsequent ticks"
  - "GCD pruning placed in tick() before process_expirations() using interpolated game time"
  - "Two failing tests (test_ability_cast_triggers_timer, test_anyof_condition_triggers_on_either) are pre-existing on main — interner cross-context issue unrelated to this plan"
metrics:
  duration: ~15min
  completed: 2026-04-12
  tasks_completed: 2
  files_modified: 2
---

# Phase 01 Plan 02: GCD Tracking and Queued-Hold Logic Summary

GCD lifecycle and queued-hold behavioral logic wired into TimerManager: `active_gcd` Option slot created on timer fire with replace policy, pruned on tick, and cleared on combat end; `queue_on_expire` timers fire alerts/chains exactly once at the expiry-to-queued transition then remain in `active_timers` indefinitely.

## Tasks Completed

| Task | Name | Commit | Files |
|------|------|--------|-------|
| 1 | Add active_gcd field and GCD/queued-hold logic to TimerManager | ea5fac8 | core/src/timers/manager.rs |
| 2 | Extend clear_combat_timers to clear active_gcd | 5c8f3a0 | core/src/timers/signal_handlers.rs |

## What Was Built

**Task 1 — `manager.rs`:**
- Added `pub(super) active_gcd: Option<super::ActiveGcd>` field to `TimerManager` struct
- Initialized `active_gcd: None` in `TimerManager::new()`
- Added `pub fn active_gcd() -> Option<&ActiveGcd>` public accessor for Phase 2 overlay rendering
- GCD creation in `start_timer()`: when `def.gcd_secs` is `Some`, sets `self.active_gcd = Some(ActiveGcd::new(timestamp, secs))` — replaces any existing GCD (single-slot replace policy per D-03)
- GCD creation also added in the `can_be_refreshed` branch of `start_timer()`
- GCD pruning in `tick()`: checks `gcd.has_expired(interp_time)` and sets `self.active_gcd = None`
- Queued-hold logic in `process_expirations()`:
  - Guard at top of loop: `if timer.is_queued { continue; }` — skips already-queued timers
  - New branch before removal: `if timer.queue_on_expire` — fires alerts and chains on transition tick, then sets `is_queued = true` and continues (timer stays in `active_timers` indefinitely)
  - Normal removal path unchanged for timers with neither `can_repeat()` nor `queue_on_expire`

**Task 2 — `signal_handlers.rs`:**
- Added `manager.active_gcd = None;` in `clear_combat_timers()` between `active_timers.clear()` and `fired_alerts.clear()`
- Ensures GCD state does not leak across combat encounters (per SC-4, D-02)

## Deviations from Plan

**Pre-execution: Cherry-picked Plan 01 commits into worktree**

Plan 02 depends on Plan 01 (`is_queued`, `queue_on_expire`, `ActiveGcd` types). The worktree branch (`worktree-agent-aa5a605d`) was forked before Plan 01's commits landed on `next-update`. Cherry-picked 4 commits from main repo:
- `3d133ce` feat(01-01): add AbilityQueue variant and queue fields to TimerDefinition
- `e5a653f` feat(01-01): add ActiveGcd struct, is_queued/queue fields to ActiveTimer, update call sites
- `a1c842a` fix(01-01): update test struct literals with new TimerDefinition queue fields
- `078c2dc` fix(01-01): add queue fields to TimerDefinition struct literal in dsl/definition.rs

This is a worktree isolation artifact, not a plan deviation.

## Known Stubs

None — all behavioral requirements for Phase 1 are implemented. `queue_remove_trigger` evaluation remains stubbed (no-op) per the decision in Plan 01 — this is intentional for v1.

## Pre-existing Test Failures (Out of Scope)

Two timer tests fail due to a pre-existing interner cross-context issue in `test_ability_cast_triggers_timer` and `test_anyof_condition_triggers_on_either`. Both fail identically on `main` before any Plan 01/02 changes. These are logged in deferred items and are out of scope for this plan.

- `timers::manager_tests::test_ability_cast_triggers_timer` — IStr key-out-of-bounds in interner
- `timers::manager_tests::test_anyof_condition_triggers_on_either` — same root cause

## Threat Flags

None — no new network endpoints, auth paths, file access, or trust boundary crossings introduced. The `active_gcd` field is a single-slot `Option<ActiveGcd>` bounded by a fixed-size struct. Matches plan threat model dispositions (T-01-03: accept, T-01-04: accept).

## Self-Check: PASSED

- core/src/timers/manager.rs: FOUND
- core/src/timers/signal_handlers.rs: FOUND
- Commit ea5fac8: FOUND
- Commit 5c8f3a0: FOUND
- active_gcd field: FOUND (line 67 manager.rs)
- active_gcd: None in new(): FOUND (line 153 manager.rs)
- ActiveGcd::new in start_timer(): FOUND (lines 794, 841 manager.rs)
- GCD pruning block in tick(): FOUND (lines 467-470 manager.rs)
- is_queued guard in process_expirations(): FOUND (line 1021 manager.rs)
- queue_on_expire branch: FOUND (line 1034 manager.rs)
- timer.is_queued = true after alert firing: FOUND (line 1061 manager.rs)
- pub fn active_gcd() accessor: FOUND (line 200 manager.rs)
- manager.active_gcd = None in signal_handlers.rs: FOUND (line 669)
- cargo check -p baras-core: PASSED
