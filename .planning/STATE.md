# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-04-11)

**Core value:** Real-time overlays that give players accurate, actionable combat data without performance overhead.
**Current focus:** Phase 1 — Core Data Model

## Current Position

Phase: 1 of 5 (Core Data Model)
Plan: — (not yet planned)
Status: Ready to plan
Last activity: 2026-04-11 — Roadmap created, all 16 requirements mapped

Progress: [░░░░░░░░░░] 0%

## Performance Metrics

**Velocity:**
- Total plans completed: 0
- Average duration: —
- Total execution time: —

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| - | - | - | - |

*Updated after each plan completion*

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

- GCD state lives in TimerManager, not overlay — overlays are pure render
- `AbilityQueue` is a new `TimerDisplayTarget` variant, not a new timer type — reuses trigger infrastructure
- Queued hold modeled as alive-past-zero `ActiveTimer` with `is_queued` flag — analogous to effects Ready State
- `AbilityQueueOverlayConfig` aliases `TimerOverlayConfig` fields — avoids duplicate config structs
- `queue_remove_trigger` evaluation stubbed as no-op in Phase 1 — avoids ordering pitfall, can be completed post-milestone

### Pending Todos

None yet.

### Blockers/Concerns

None yet.

## Session Continuity

Last session: 2026-04-11
Stopped at: Roadmap created — ready to plan Phase 1
Resume file: None
