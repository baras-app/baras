# Requirements: BARAS — Ability Queue Overlay

**Defined:** 2026-04-11
**Core Value:** Real-time overlays that give players accurate, actionable combat data without performance overhead.

## v1.0 Requirements

### Data Model

- [ ] **DATA-01**: User can configure a timer with `display_target = AbilityQueue` to route it to the ability queue overlay
- [ ] **DATA-02**: User can configure `gcd_secs` on an ability-queue timer so a synthetic GCD bar appears and counts down when the ability fires
- [ ] **DATA-03**: User can configure `queue_on_expire` so a timer holds at zero as "queued/ready" instead of disappearing
- [ ] **DATA-04**: User can configure `queue_priority` to control the sort order of queued entries in tier 2
- [ ] **DATA-05**: User can configure a `queue_remove_trigger` to control when a queued entry clears

### Service Layer

- [ ] **SVC-01**: Service produces `AbilityQueueData` with GCD, queued, and active countdown entries via a dedicated third data path
- [ ] **SVC-02**: Service flushes ability queue data on combat end and area change alongside TimersA/B
- [ ] **SVC-03**: `build_timer_data_with_audio` returns a named `TimerDataBundle` struct instead of a positional tuple

### Overlay Rendering

- [ ] **OVLY-01**: User sees a dedicated Ability Queue overlay window that can be shown and hidden
- [ ] **OVLY-02**: Overlay renders entries in three visual tiers: GCD bars pinned at top, queued/ready in middle, active countdowns at bottom
- [ ] **OVLY-03**: Overlay visually distinguishes the three tiers with distinct colors/styles

### App Wiring

- [ ] **WIRE-01**: User can toggle the Ability Queue overlay on/off from the frontend UI
- [ ] **WIRE-02**: Overlay has its own config (position, opacity, bar height, font size, GCD bar color) persisted across sessions
- [ ] **WIRE-03**: Overlay active state is gated by a dedicated `ability_queue_overlay_active` `AtomicBool` in `SharedState`

### Timer Editor UI

- [ ] **UI-01**: Timer editor reveals ability-queue-specific fields when `display_target = AbilityQueue` is selected
- [ ] **UI-02**: `queue_priority` and `queue_remove_trigger` inputs are only visible when `queue_on_expire` is enabled

## Future Requirements

### v2 Candidates

- **DATA-F01**: Auto-detect GCD duration from Alacrity rating if SWTOR ever exposes it in the combat log
- **DATA-F02**: Support `queue_on_expire + per_target` interaction (currently undefined/unsupported)
- **DATA-F03**: Multiple GCD tracks (currently capped at 1 active GCD bar per ability)
- **OVLY-F01**: Icon display for queued/GCD entries (infrastructure exists in `AbilityQueueEntry`)

## Out of Scope

| Feature | Reason |
|---------|--------|
| GCD auto-detection from log | Alacrity rating is not exposed in SWTOR combat logs; user must configure `gcd_secs` per timer |
| Logic in overlay thread | Overlays are pure render; all GCD/queued state managed by `TimerManager` |
| Separate timer type for ability queue | Extends existing `TimerDefinition`; no new type needed |
| `queue_on_expire` + `per_target` combined | Interaction is undefined; document as unsupported in v1 |
| Multiple simultaneous GCD bars | Single GCD bar per ability fire; capped at 1 active `ActiveGcd` entry |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| DATA-01 | Phase 1 | Pending |
| DATA-02 | Phase 1 | Pending |
| DATA-03 | Phase 1 | Pending |
| DATA-04 | Phase 1 | Pending |
| DATA-05 | Phase 1 | Pending |
| SVC-01 | Phase 2 | Pending |
| SVC-02 | Phase 2 | Pending |
| SVC-03 | Phase 2 | Pending |
| OVLY-01 | Phase 3 | Pending |
| OVLY-02 | Phase 3 | Pending |
| OVLY-03 | Phase 3 | Pending |
| WIRE-01 | Phase 4 | Pending |
| WIRE-02 | Phase 4 | Pending |
| WIRE-03 | Phase 4 | Pending |
| UI-01 | Phase 5 | Pending |
| UI-02 | Phase 5 | Pending |

**Coverage:**
- v1.0 requirements: 16 total
- Mapped to phases: 16
- Unmapped: 0 ✓

---
*Requirements defined: 2026-04-11*
*Last updated: 2026-04-11 — traceability filled after roadmap creation*
