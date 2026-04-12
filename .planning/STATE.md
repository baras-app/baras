# STATE.md

## Current Position

Phase: Not started (defining requirements)
Plan: —
Status: Defining requirements
Last activity: 2026-04-11 — Milestone v1.0 started

## Project Reference

See: .planning/PROJECT.md (updated 2026-04-11)

**Core value:** Real-time overlays that give players accurate, actionable combat data without performance overhead.
**Current focus:** Ability Queue Overlay (v1.0)

## Accumulated Context

- Implementation plan captured in `ability-queue-overlay-plan.md` (project root) — detailed file-level breakdown for all 5 phases
- Existing timer overlays (timers_a.rs, timers_b.rs) are the rendering pattern to follow
- `build_timer_data_with_audio` in `service/mod.rs` needs a third output path for ability queue data
- GCD state and queued-hold logic belong in `TimerManager`, not the overlay
