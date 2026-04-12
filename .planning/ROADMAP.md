# Roadmap: BARAS — Ability Queue Overlay (v1.0)

## Overview

Five phases following the data-flow dependency chain: core data model first, then shared types and service layer, then the pure-render overlay, then the mechanical app wiring, and finally the timer editor UI reveal. Each phase compiles and is independently verifiable before the next begins. The 16 v1.0 requirements map cleanly to these natural delivery boundaries with no orphans.

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [ ] **Phase 1: Core Data Model** - Extend timer definitions and manager with GCD tracking and queued-hold logic
- [ ] **Phase 2: Service Layer** - Produce AbilityQueueData via a dedicated third data path from the timer manager
- [ ] **Phase 3: Overlay Renderer** - Pure-render AbilityQueueOverlay with three-tier sort
- [ ] **Phase 4: App Wiring** - End-to-end overlay lifecycle: spawn, toggle, config, routing, and combat-end flush
- [ ] **Phase 5: Timer Editor UI** - Conditional field reveal for ability-queue-specific timer settings

## Phase Details

### Phase 1: Core Data Model
**Goal**: TimerDefinition, ActiveTimer, and TimerManager are extended with all ability-queue fields and GCD/queued-hold logic, forming the foundation all downstream phases depend on
**Depends on**: Nothing (first phase)
**Requirements**: DATA-01, DATA-02, DATA-03, DATA-04, DATA-05
**Success Criteria** (what must be TRUE):
  1. `TimerDisplayTarget::AbilityQueue` variant exists and `cargo check -p baras-core` passes
  2. A timer with `gcd_secs` set causes an `ActiveGcd` entry to be pushed to `active_gcds` when the timer fires, and the entry is pruned when it expires
  3. A timer with `queue_on_expire = true` remains alive in manager state with `is_queued = true` after its duration elapses, rather than being removed
  4. `clear_combat_timers` clears `active_gcds` in the same pass as active timers (GCD vec not leaked across combats)
  5. `queue_remove_trigger` field exists on `TimerDefinition` with `#[serde(default)]` and evaluation is stubbed as a no-op (ordering pitfall deferred safely)
**Plans**: TBD

### Phase 2: Service Layer
**Goal**: The service produces a complete `AbilityQueueData` snapshot and delivers it via a dedicated channel path, with `build_timer_data_with_audio` returning a named `TimerDataBundle` struct
**Depends on**: Phase 1
**Requirements**: SVC-01, SVC-02, SVC-03
**Success Criteria** (what must be TRUE):
  1. `AbilityQueueData` and `AbilityQueueEntry` types exist in `baras-types` and `cargo check -p baras-types` passes
  2. `build_timer_data_with_audio` returns `TimerDataBundle` (named struct, not a positional tuple) and its single call site compiles without manual destructuring changes
  3. GCD entries, queued entries (with `is_queued = true`), and active countdown entries are each assembled into `AbilityQueueData` — queued entries bypass the `remaining <= 0.0` guard
  4. Service sends `OverlayUpdate::AbilityQueueUpdated` on every timer tick
  5. Service flushes ability queue data (sends an empty payload) in both the `CombatEnded` and `ClearAllData` router handler arms, alongside TimersA/B flush
**Plans**: TBD

### Phase 3: Overlay Renderer
**Goal**: A self-contained `AbilityQueueOverlay` renders entries in three visually distinct tiers using explicit flag-based sort, with no logic of its own
**Depends on**: Phase 2
**Requirements**: OVLY-01, OVLY-02, OVLY-03
**Success Criteria** (what must be TRUE):
  1. `overlay/src/overlays/ability_queue.rs` exists, registers in `mod.rs`, and `cargo check -p baras-overlay` passes
  2. Overlay renders GCD bars in tier 1 (pinned top), queued/ready entries in tier 2 (middle), and active countdowns in tier 3 (bottom) — tier assignment is driven by `is_pinned`/`is_queued` flags, never by `remaining_secs` as primary discriminator
  3. Each tier is visually distinct: GCD bar uses an accent color (configurable), queued entries render a static "READY" label rather than a countdown, active countdown entries render a standard progress bar
**Plans**: TBD
**UI hint**: yes

### Phase 4: App Wiring
**Goal**: The overlay is fully wired end-to-end: it can be spawned, toggled, configured with its own persisted config, routed to from the service, and the frontend exposes a toggle button
**Depends on**: Phase 3
**Requirements**: WIRE-01, WIRE-02, WIRE-03
**Success Criteria** (what must be TRUE):
  1. User can click a toggle button in the frontend UI to show and hide the Ability Queue overlay window (window appears and disappears)
  2. `AbilityQueueOverlayConfig` fields (position, opacity, bar height, font size, GCD bar color) persist across application restarts
  3. `ability_queue_overlay_active: AtomicBool` exists in `SharedState` and gates overlay update delivery independently from TimersA/B active flags
  4. After combat ends, the Ability Queue overlay window clears (no stale GCD or queued entries remain visible from the previous fight)
**Plans**: TBD
**UI hint**: yes

### Phase 5: Timer Editor UI
**Goal**: The timer editor conditionally reveals ability-queue-specific fields so users can configure the feature without editing TOML directly
**Depends on**: Phase 4
**Requirements**: UI-01, UI-02
**Success Criteria** (what must be TRUE):
  1. When a user selects `display_target = AbilityQueue` in the timer editor, the `gcd_secs`, `queue_on_expire` fields become visible; changing to any other target hides them
  2. The `queue_priority` and `queue_remove_trigger` inputs are only visible when `queue_on_expire` is enabled — toggling the checkbox shows/hides them reactively
**Plans**: TBD
**UI hint**: yes

## Progress

**Execution Order:**
Phases execute in numeric order: 1 → 2 → 3 → 4 → 5

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Core Data Model | 0/TBD | Not started | - |
| 2. Service Layer | 0/TBD | Not started | - |
| 3. Overlay Renderer | 0/TBD | Not started | - |
| 4. App Wiring | 0/TBD | Not started | - |
| 5. Timer Editor UI | 0/TBD | Not started | - |
