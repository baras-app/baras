# BARAS

## What This Is

BARAS is a combat log parser for the MMO Star Wars: The Old Republic. It is a feature-complete replacement for tools like StarParse and Orbs. Built in Rust with a Dioxus/WASM frontend and custom overlay rendering engine, it provides live combat overlays, historical data analysis, encounter tracking, and configurable timers and alerts — all with a focus on speed and minimal resource usage.

## Core Value

Real-time overlays that give players accurate, actionable combat data without performance overhead.

## Requirements

### Validated

<!-- Shipped and confirmed valuable. -->

- ✓ Fully customizable metric overlays (DPS/HPS/etc.)
- ✓ Raid frame effect tracker (HOT click-to-swap)
- ✓ Personal statistics view
- ✓ Boss health bar overlay
- ✓ Windows, Wayland, X11, and macOS overlay support
- ✓ Overlay profiles with save/load
- ✓ Data Explorer with 9 views (Overview, Charts, CombatLog, Rotation, Usage, Detailed breakdowns)
- ✓ Rotation analysis and death timeline reconstruction
- ✓ Log file directory management
- ✓ Timers and Alerts with audio (TimersA / TimersB overlays)
- ✓ Effects system with overlay
- ✓ Parsely upload integration
- ✓ Raid Challenges (damage/HPS per phase)
- ✓ Boss encounter notes (Markdown-formatted overlay)
- ✓ Encounter editor with triggers, phases, and counters
- ✓ 21 encounter definitions (operations, flashpoints, world bosses)
- ✓ Process monitoring for auto-hide when game not running

### Active

<!-- Current milestone v1.0: Ability Queue Overlay -->

- [ ] User can configure a timer with `display_target = AbilityQueue` to route it to the ability queue overlay
- [ ] User can configure a GCD duration on an ability-queue timer so a synthetic GCD bar appears when the ability fires
- [ ] User can configure a timer to hold as "queued/ready" at zero instead of disappearing
- [ ] User can configure queue priority and remove trigger on queued timers
- [ ] Service produces AbilityQueueData with GCD, queued, and active countdown entries
- [ ] User sees a dedicated Ability Queue overlay window that can be shown/hidden
- [ ] Overlay displays three visual tiers: GCD pinned (top), queued/ready entries (middle), active countdowns (bottom)
- [ ] User can toggle the Ability Queue overlay from the frontend UI
- [ ] Ability Queue overlay has its own config (position, opacity, bar height, font size)
- [ ] Timer editor reveals ability-queue-specific fields when AbilityQueue target is selected

### Out of Scope

<!-- Explicit boundaries. Includes reasoning to prevent re-adding. -->

- Ability queue logic in overlay — GCD/queued state lives in the timer manager; overlays are pure render
- Separate "ability queue" timer type — existing TimerDefinition is extended, not replaced
- Multiple GCD tracks — single GCD bar per ability fire is sufficient for v1

## Context

BARAS uses an event-driven pipeline: `LogParser → EventProcessor → CombatSignalHandler → CombatService → OverlayUpdate`. Overlays run in dedicated OS threads with platform-native windows. Timer state lives in `TimerManager` (core), which is the right place for GCD tracking and queued-hold logic.

The existing Timers A/B overlays (`overlay/src/overlays/timers_a.rs`, `timers_b.rs`) provide the rendering pattern to follow. The `build_timer_data_with_audio` function in `service/mod.rs` will need a third output path for ability queue data.

Key constraint: `AbilityQueueOverlayConfig` can reuse/alias `TimerOverlayConfig` fields — no need to define duplicate config structures.

## Constraints

- **Tech Stack**: Rust 2024 edition, Dioxus 0.7.2, Tauri 2, tiny-skia overlay renderer — no new runtime dependencies expected
- **Architecture**: Overlays must be pure render — no state logic in overlay thread; all GCD/queued state managed by `TimerManager`
- **Performance**: GCD tracking adds a lightweight `Vec<ActiveGcd>` to manager; must not block hot path
- **File Size**: Keep new files under 500 lines; `ability_queue.rs` overlay should be self-contained

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| GCD state lives in TimerManager, not overlay | Overlays are pure render; manager already owns timer lifecycle | — Pending |
| AbilityQueue as TimerDisplayTarget variant | Reuses timer infrastructure (triggers, effects, conditions) without new timer type | — Pending |
| Queued hold analogous to effects Ready State | Pattern already validated in effects tracker; manager sets `is_queued` flag instead of removing | — Pending |
| AbilityQueueOverlayConfig aliases TimerOverlayConfig | Avoids duplicating shared fields (bar height, font size, opacity) | — Pending |

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `/gsd-transition`):
1. Requirements invalidated? → Move to Out of Scope with reason
2. Requirements validated? → Move to Validated with phase reference
3. New requirements emerged? → Add to Active
4. Decisions to log? → Add to Key Decisions
5. "What This Is" still accurate? → Update if drifted

**After each milestone** (via `/gsd-complete-milestone`):
1. Full review of all sections
2. Core Value check — still the right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-04-11 — Milestone v1.0 Ability Queue Overlay started*
