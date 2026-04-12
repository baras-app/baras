---
phase: 01-core-data-model
plan: 01
subsystem: timers
tags: [data-model, ability-queue, timers, structs]
dependency_graph:
  requires: []
  provides: [AbilityQueue variant, gcd_secs, queue_on_expire, queue_priority, queue_remove_trigger, ActiveGcd, is_queued]
  affects: [core/src/timers/definition.rs, core/src/timers/active.rs, core/src/timers/mod.rs, core/src/timers/manager.rs]
tech_stack:
  added: []
  patterns: [serde default fields, Option<T> skip_serializing_if, struct field caching from definition]
key_files:
  created: []
  modified:
    - core/src/timers/definition.rs
    - core/src/timers/active.rs
    - core/src/timers/mod.rs
    - core/src/timers/manager.rs
decisions:
  - "gcd_secs uses Option<f32> with skip_serializing_if to avoid cluttering TOML output for non-queue timers"
  - "queue_remove_trigger evaluation deferred (stubbed) per plan — field added to type only"
  - "ActiveGcd placed after ActiveTimer impl block, before TimerKey, consistent with module layout convention"
metrics:
  duration: ~5min
  completed: 2026-04-12
  tasks_completed: 2
  files_modified: 4
---

# Phase 01 Plan 01: Timer Data Model for Ability Queue Summary

Pure structural additions establishing the data model for the Ability Queue overlay feature: `AbilityQueue` display target variant, four new `TimerDefinition` fields, `ActiveGcd` struct, and three new `ActiveTimer` fields — all with updated call sites. Crate compiles cleanly with no behavioral changes.

## Tasks Completed

| Task | Name | Commit | Files |
|------|------|--------|-------|
| 1 | Add AbilityQueue variant and queue fields to TimerDefinition | 3d133ce | core/src/timers/definition.rs |
| 2 | Add ActiveGcd struct, is_queued/queue fields to ActiveTimer, update call sites | e5a653f | core/src/timers/active.rs, mod.rs, manager.rs |

## What Was Built

**Task 1 — `definition.rs`:**
- Added `AbilityQueue` variant to `TimerDisplayTarget` enum (serializes as `"ability_queue"` via `#[serde(rename_all = "snake_case")]`)
- Added four fields to `TimerDefinition` under a new `// ─── Ability Queue` section:
  - `gcd_secs: Option<f32>` — drives GCD bar creation in Plan 02
  - `queue_on_expire: bool` — hold-at-zero flag
  - `queue_priority: u8` — tier 2 sort order
  - `queue_remove_trigger: Option<Trigger>` — removal condition (evaluation stubbed in v1)

**Task 2 — `active.rs`, `mod.rs`, `manager.rs`:**
- Added `ActiveGcd` struct with `started_at`/`expires_at` fields and `has_expired`, `remaining_secs`, `fill_percent` helpers
- Added `is_queued: bool`, `queue_on_expire: bool`, `queue_priority: u8` fields to `ActiveTimer`
- Extended `ActiveTimer::new()` with `queue_on_expire` and `queue_priority` parameters; `is_queued` initializes to `false`
- Re-exported `ActiveGcd` from `timers/mod.rs`
- Updated the single `ActiveTimer::new()` call site in `manager.rs` to pass `def.queue_on_expire` and `def.queue_priority`

## Deviations from Plan

None — plan executed exactly as written.

## Known Stubs

- `queue_remove_trigger` field exists on `TimerDefinition` but evaluation is a no-op in v1. Plan 02 behavioral logic will wire it up.

## Threat Flags

None — no new network endpoints, auth paths, or trust boundary crossings introduced. New fields are primitive types with `#[serde(default)]` on local TOML user config. Matches the threat model disposition (T-01-01, T-01-02: accept).

## Self-Check: PASSED

- definition.rs: FOUND
- active.rs: FOUND
- mod.rs: FOUND
- manager.rs: FOUND
- Commit 3d133ce: FOUND
- Commit e5a653f: FOUND
