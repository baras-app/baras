# Project Research Summary

**Project:** BARAS — Ability Queue Overlay
**Domain:** MMO combat overlay — GCD tracking + ability queue visualization
**Researched:** 2026-04-11
**Confidence:** HIGH

## Executive Summary

The Ability Queue Overlay is a pure extension of the existing BARAS timer infrastructure, not a new subsystem. All four research areas converge on the same conclusion: extend `TimerDefinition` with four optional fields, extend `TimerManager` with a lightweight GCD vec, add a new `AbilityQueueOverlay` render-only component, and wire it through the established 12-step overlay checklist. No new crates, no new architectural patterns, no new threading models are needed. Every pattern already exists in the codebase and is directly reusable.

The recommended approach is to build in strict dependency order: data model first (core/timers), then shared types, then the overlay renderer, then app wiring, and finally the timer editor UI reveal. This order ensures every layer compiles independently and pitfalls are addressed at their prevention phase rather than discovered during integration. The core design decision — modeling queued-hold as an alive-past-zero `ActiveTimer` with an `is_queued` flag rather than a separate type — is the correct one and is directly supported by the existing `process_expirations` decision tree.

The primary risk is omission, not complexity. Seven concrete pitfalls are identified, all arising from the same root cause: adding a new overlay type requires updating multiple files that must stay in sync (manager clearance functions, service layer guards, router flush handlers, SharedState atomic flags, frontend status emission). Each pitfall has a one-commit prevention strategy, and all are caught either at compile time or by straightforward manual testing. The implementation is well-understood and low-risk as long as the "Looks Done But Isn't" checklist from PITFALLS.md is treated as mandatory gate criteria.

## Key Findings

### Recommended Stack

No new dependencies. The existing stack covers all technical requirements. `tiny-skia` and `cosmic-text` handle rendering, `tokio mpsc` routes updates through the existing 256-capacity channel, `NaiveDateTime` (game time) handles GCD expiry tracking consistent with the rest of the manager, and `TimerOverlayConfig` from `baras-types` is directly reusable as `AbilityQueueOverlayConfig` — the same reuse pattern already used by `effects_overlay`.

**Core technologies (all existing):**
- `tiny-skia` 0.11: three-tier bar rendering — existing `ProgressBar` widget reused verbatim
- `cosmic-text` 0.16 / `SHARED_FONT_DB`: "READY" labels and countdown text — zero additional cost
- `tokio mpsc` (capacity 256): `OverlayUpdate::AbilityQueueUpdated` routes through existing overlay channel
- `Arc<(u32, u32, Vec<u8>)>`: icon sharing — exact same pattern as `TimerEntry.icon`
- `TimerOverlayConfig`: config struct alias — same reuse as `effects_overlay` field in `OverlaySettings`

### Expected Features

**Must have (table stakes):**
- GCD bar pinned at top, visible only while active — users expect this from every GCD tracker
- Queued/ready entries hold at zero instead of disappearing — core differentiator of a queue view
- Active countdown bars sorted by remaining time — standard in all timer overlays
- Visual three-tier distinction (GCD / queued / active) — required for at-a-glance tier parsing
- Flush on combat end and area change — missing this is a bug-class issue
- Timer editor reveals ability-queue fields when `display_target = AbilityQueue`

**Should have (differentiators):**
- Configurable GCD duration (`gcd_secs`) — SWTOR has three Alacrity breakpoints (1.5s / 1.4s / 1.3s); no other SWTOR tool exposes this per-timer
- Configurable queue priority — determines display order of ready abilities; unique to BARAS among SWTOR tooling
- Ability icon display — all major MMO overlays show icons; struct already has the `icon` field planned

**Defer to post-milestone:**
- `queue_remove_trigger` advanced configuration — add after basic queued-hold is validated
- Ability icons — field is already planned in the struct; rendering can be added without architectural change

**Explicit anti-features (do not build):**
- Predictive next-ability suggestions (rotation solver — separate product)
- GCD state logic in the overlay thread (violates pure-render invariant)
- Separate `AbilityQueueTimer` type (duplicates trigger infrastructure)
- Latency-aware GCD window adjustment (log timestamps already reflect server-acknowledged events)

### Architecture Approach

The ability queue overlay follows the identical pattern to TimersA/B: `TimerManager` owns all state, `build_timer_data_with_audio` assembles a snapshot, the snapshot travels via `OverlayUpdate::AbilityQueueUpdated` through the router to a dedicated OS thread, and `AbilityQueueOverlay` is a pure render component with no state of its own. The only architectural novelty is the three-tier sort (GCD pinned / queued hold / active countdown) performed entirely in the overlay render loop using explicit `is_pinned`/`is_queued` flags — never inferred from `remaining_secs`.

**Major components:**
1. `TimerManager` (extended) — GCD spawn on ability activation, queued-hold on expire, `active_gcds: Vec<ActiveGcd>`, `remove_queued_matching` helper, clearance on combat end
2. `build_timer_data_with_audio` (extended) — third output path collecting `AbilityQueue` display-target timers + GCD entries; return wrapped in named `TimerDataBundle` struct to eliminate positional tuple fragility
3. `AbilityQueueOverlay` (new file) — pure render, three-tier sort by `(tier_key, priority/remaining)`, no logic
4. App wiring (19 files across 5 layers) — mechanical 12-step checklist; no novel patterns

### Critical Pitfalls

1. **GCD vec not cleared in `clear_combat_timers`** — add `self.active_gcds.clear()` in the same commit as the field addition; treat `clear_combat_timers` as the authoritative teardown list
2. **Queued entries silently dropped by `remaining <= 0.0` guard in service** — add `&& !timer.is_queued` to the guard; one-line fix that must land in Phase 2
3. **`build_timer_data_with_audio` return tuple expansion breaks its only call site** — use a named `TimerDataBundle` struct from the start; compiler enforces completeness
4. **AbilityQueue overlay not flushed in `OverlayUpdate::CombatEnded` router handler** — always update both `CombatEnded` and `ClearAllData` arms when adding a new overlay; treat them as a mandatory pair
5. **`timer_overlay_active` reuse couples ability queue to TimersA/B** — add a dedicated `ability_queue_overlay_active: AtomicBool` to `SharedState`; update manager and service loop together
6. **`queue_remove_trigger` evaluation ordering hazard with same-tick re-trigger** — implement as a separate `process_queued_removals` pass that runs after `process_expirations` completes
7. **Three-tier sort using `remaining_secs` as tier discriminator** — use explicit `is_pinned`/`is_queued` flags as primary sort keys; `remaining_secs` is only a secondary sort within a tier

## Implications for Roadmap

Based on research, the natural phase structure follows the data-flow dependency chain. Each phase is independently compilable and delivers a verifiable artifact.

### Phase 1: Core Data Model
**Rationale:** All downstream phases depend on the schema. Zero risk of invalidation since this is pure Rust type extension with no UI surface. Both critical manager pitfalls (GCD vec clearance, `queue_remove_trigger` ordering) must be addressed here.
**Delivers:** Extended `TimerDefinition`, `ActiveTimer`, `TimerManager` with GCD vec + queued-hold logic
**Addresses:** GCD pinned bar, queued-hold at zero, configurable GCD duration, configurable queue priority
**Avoids:** Pitfall 1 (GCD not cleared), Pitfall 6 (`queue_remove_trigger` ordering)
**Files:** `core/src/timers/definition.rs`, `core/src/timers/active.rs`, `core/src/timers/manager.rs`

### Phase 2: Shared Types and Service Layer
**Rationale:** Shared types must exist before the overlay can be compiled; the service layer closes the data path from manager to channel. The named `TimerDataBundle` struct must be established here to prevent the call-site pitfall.
**Delivers:** `AbilityQueueData`, `AbilityQueueEntry` in `baras-types`; extended `build_timer_data_with_audio` with third output; `OverlayUpdate::AbilityQueueUpdated`
**Implements:** Data assembly component; eliminates positional tuple fragility
**Avoids:** Pitfall 2 (queued entries dropped at zero), Pitfall 3 (return tuple expansion)
**Files:** `types/src/lib.rs`, `app/src-tauri/src/service/mod.rs`

### Phase 3: Overlay Renderer
**Rationale:** Pure render with no external dependencies once types exist. Can be iterated in isolation before app wiring completes. Three-tier sort logic must be established with flag-based discriminators from the first implementation.
**Delivers:** `ability_queue.rs` — functional overlay with GCD / queued / active tier rendering
**Implements:** `AbilityQueueOverlay` render component, three-tier sort
**Avoids:** Pitfall 7 (tier sort using `remaining_secs` as discriminator)
**Files:** `overlay/src/overlays/ability_queue.rs` (new), `overlay/src/overlays/mod.rs`

### Phase 4: App Wiring
**Rationale:** Mechanical but load-bearing. All 12 checklist steps plus the three overlay-lifecycle pitfalls land here. SharedState, manager, and service loop changes must be committed together.
**Delivers:** End-to-end functional overlay: spawn, toggle, config, router routing, combat-end flush, frontend toggle button
**Addresses:** Overlay show/hide toggle, overlay position and opacity config, flush on combat end
**Avoids:** Pitfall 4 (router CombatEnded missing flush), Pitfall 5 (`timer_overlay_active` reuse)
**Files:** 14 files across `overlay/`, `app/src-tauri/src/`, `core/src/context/config.rs`, `app/src/`

### Phase 5: Timer Editor UI
**Rationale:** Purely additive; does not block any other phase. Can be developed and iterated after the backend is fully functional. Last because it has no blocking dependencies and can be validated against a live overlay.
**Delivers:** Conditional field reveal in timer editor for `gcd_secs`, `queue_on_expire`, `queue_priority`, `queue_remove_trigger` when `display_target = AbilityQueue`
**Files:** Timer editor component in `app/src/components/`

### Phase Ordering Rationale

- Phases 1 → 2 → 3 follow the signal pipeline direction: manager state → service snapshot → overlay render. Each compiles independently.
- Phase 4 is deliberately last among backend phases because it touches the most files and has the most omission-style pitfalls — by this point the data model and renderer are validated.
- Phase 5 is safely decoupled: the overlay functions without the UI reveal (users can configure directly in TOML); the UI is a quality-of-life addition.
- The named `TimerDataBundle` struct decision in Phase 2 reduces risk across the entire 19-file change surface by catching mismatches at compile time rather than runtime.

### Research Flags

Phases with well-documented patterns (skip deeper research):
- **Phase 1:** `TimerDefinition` / `ActiveTimer` extension is a mechanical field addition; `process_expirations` logic is well-understood from direct codebase inspection
- **Phase 3:** Overlay rendering follows `timers.rs` template exactly; three-tier sort is a simple stable partition
- **Phase 4:** 12-step checklist from `CLAUDE.local.md` covers all wiring steps; router patterns are documented in ARCHITECTURE.md with exact code examples

Phases that may benefit from a focused implementation check:
- **Phase 2 (service layer):** `build_timer_data_with_audio` is inside a 3787-line file; the named struct refactor should be scoped carefully to avoid unintended changes to the existing TimersA/B paths

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | All findings based on direct codebase inspection; no new dependencies required |
| Features | HIGH | SWTOR GCD mechanics verified via forum/community sources; feature scope grounded in existing tooling patterns |
| Architecture | HIGH | All integration points verified by reading actual source files at specific line numbers; exact code patterns provided |
| Pitfalls | HIGH | All 7 pitfalls identified from direct inspection of affected code paths; each has a verified prevention strategy |

**Overall confidence:** HIGH

### Gaps to Address

- **`queue_remove_trigger` in MVP:** Research recommends deferring evaluation logic to post-milestone. Phase 1 should add the field with `#[serde(default)]` but stub the evaluation pass as a no-op with a `// TODO: queue_remove_trigger evaluation` comment. This avoids the ordering pitfall entirely in the initial release.
- **GCD bar color config:** `TimerOverlayConfig` provides `default_bar_color` and `font_color` but a separate `gcd_bar_color` field is needed to allow independent GCD bar coloring. This is a small addition to `AbilityQueueOverlayConfig` that should be decided during Phase 2 types work.
- **`queue_on_expire` + `per_target` interaction:** Queued state applies to first-to-expire instance only for per-target timers. This is a known limitation that needs a tooltip note in the timer editor (Phase 5); no code change required.

## Sources

### Primary (HIGH confidence — direct codebase inspection)
- `core/src/timers/manager.rs`, `active.rs`, `definition.rs`, `signal_handlers.rs` — timer lifecycle
- `app/src-tauri/src/service/mod.rs` (lines 3162–3267, 2540–2560) — `build_timer_data_with_audio` signature and call site
- `app/src-tauri/src/router.rs` (full file) — `CombatEnded` and `ClearAllData` handler blocks
- `app/src-tauri/src/state/mod.rs` (lines 143–160) — per-overlay `AtomicBool` pattern
- `types/src/lib.rs` (lines 1500–1545, 2095–2210) — `TimerOverlayConfig`, `OverlaySettings`
- `overlay/src/overlays/timers.rs`, `mod.rs` — rendering template
- `.planning/PROJECT.md`, `ability-queue-overlay-plan.md` — implementation intent

### Secondary (MEDIUM confidence — external tooling comparison)
- ESO Combat Metronome GCD Tracker (2025): GCD bar UX patterns
- WoW WeakAuras GCD tracking / Spell Queue Indicator (wago.io): queue visualization patterns
- Hekili rotation addon (CurseForge): three-tier queue icon layout inspiration

### Tertiary (LOW confidence — game mechanics documentation)
- SWTOR Alacrity breakpoints: rambol.net — GCD values at 1.5s/1.4s/1.3s thresholds
- SWTOR GCD and off-GCD mechanics: SWTOR community forums

---
*Research completed: 2026-04-11*
*Ready for roadmap: yes*
