# Architecture Research

**Domain:** Ability Queue Overlay — BARAS integration
**Researched:** 2026-04-11
**Confidence:** HIGH (all findings grounded in direct codebase reads)

## Standard Architecture

### System Overview

```
GameSignal (AbilityActivated, EffectApplied, ...)
    ↓
TimerManager::handle_signal()          [core/src/timers/]
    ├── start_timer() → AbilityQueue timer → push ActiveGcd to active_gcds vec
    ├── process_expirations() → queue_on_expire=true → set is_queued=true (keep alive)
    └── signal scan → queue_remove_trigger matches → drop queued entry

build_timer_data_with_audio()          [service/mod.rs]
    ├── active_timers() with display_target=AbilityQueue + remaining > 0  → active tier
    ├── active_timers() with is_queued=true                               → queued tier
    └── active_gcds()                                                      → GCD tier
    → returns (TimerData, TimerData, AbilityQueueData, countdowns, alerts)

overlay_tx.try_send(OverlayUpdate::AbilityQueueUpdated(data))

spawn_overlay_router()  [router.rs]
    → get_ability_queue_tx()
    → OverlayCommand::UpdateData(OverlayData::AbilityQueue(data))

AbilityQueueOverlay thread            [overlay/src/overlays/ability_queue.rs]
    → sort into three tiers by (is_pinned, is_queued, remaining_secs)
    → render — pure display, no logic
```

### Component Responsibilities

| Component | Responsibility | File |
|-----------|----------------|------|
| `TimerManager` | GCD tracking, queued-hold state, queue_remove_trigger evaluation | `core/src/timers/manager.rs` |
| `TimerDefinition` | Schema: new fields gcd_secs, queue_on_expire, queue_priority, queue_remove_trigger | `core/src/timers/definition.rs` |
| `ActiveTimer` | Runtime mirror of new definition fields + is_queued flag | `core/src/timers/active.rs` |
| `build_timer_data_with_audio` | Assembles AbilityQueueData from manager state | `app/src-tauri/src/service/mod.rs` |
| `spawn_overlay_router` | Routes AbilityQueueUpdated to overlay thread | `app/src-tauri/src/router.rs` |
| `AbilityQueueOverlay` | Pure render, three-tier sort | `overlay/src/overlays/ability_queue.rs` (new) |
| `AbilityQueueOverlayConfig` | Alias of TimerOverlayConfig | `types/src/lib.rs` |

## Integration Question Analysis

### (1) Where ActiveGcd structs live and how they are cleared

**Where they live:** `active_gcds: Vec<ActiveGcd>` added as a field directly on `TimerManager`. This is the only correct location because:

- `TimerManager` already owns `active_timers: HashMap<TimerKey, ActiveTimer>` and manages the entire timer lifecycle
- The `SignalHandler` trait is the single entry point for all game signals; GCD spawn logic must co-locate with timer start logic
- GCDs are synthetic (not `TimerDefinition` instances), so they need their own lightweight container

The `ActiveGcd` struct itself should live in `core/src/timers/manager.rs` as a `pub(super)` struct (internal to the `timers` module), consistent with how `FiredAlert` is defined in the same file:

```rust
pub(super) struct ActiveGcd {
    pub parent_id: String,
    pub label: String,
    pub expires_at: NaiveDateTime,
}
```

`expires_at` should be `NaiveDateTime` (game time), not `std::time::Instant` (wall clock), to stay consistent with how all other expiry in the manager is tracked. The existing `interpolated_game_time()` method is then usable for pruning.

**Clearing on combat end:** The `SignalHandler` trait provides `on_encounter_end()`. `TimerManager` already uses this hook to clear `combat_time_started` and boss state. Add `self.active_gcds.clear()` there. Additionally, prune expired GCDs on every call to the `tick()` path by comparing `interpolated_game_time()` against each `ActiveGcd::expires_at`.

Queued timers (`is_queued = true`) must also be cleared on combat end. The existing `on_encounter_end` hook calls something equivalent to clearing `active_timers` — extend it to scan `active_timers` and drop any entry with `is_queued = true` (they are semantically attached to the encounter).

### (2) How queue_remove_trigger interacts with the existing trigger evaluation system

**Key insight from reading the code:** The existing `cancel_timers_matching` / `cancel_timers_matching_with_source_target` family in `manager.rs` is exactly the right model. The `cancel_trigger` field on `TimerDefinition` already handles "remove this timer when this signal fires" by scanning `active_timers` on every signal.

`queue_remove_trigger` is the same concept applied specifically to *queued* entries. The implementation follows the cancel pattern:

1. On every signal that passes through `handle_signal`, after the existing cancel scan, add a second scan that checks `queue_remove_trigger` on any timer where `is_queued = true`.
2. When matched: remove the entry from `active_timers` (not just flip `is_queued = false`).

The scan can reuse the existing trigger-matching infrastructure (`Trigger::matches_ability`, `Trigger::matches_effect_applied`, etc.) — no new matching logic required. The `queue_remove_trigger` field is `Option<TimerTrigger>`, same type as `cancel_trigger`.

**Where to add the scan:** In `signal_handlers.rs`, each handler function (e.g., `handle_ability`, `handle_effect_applied`) already calls `cancel_timers_matching_with_source_target` at the end. Add a parallel call to a new `remove_queued_matching` helper using the same pattern. This keeps the concern co-located with existing cancel logic.

**Default behavior (no queue_remove_trigger set):** The queued entry persists until the timer's own trigger fires again (i.e., the same ability is cast again), which restarts the `ActiveTimer` and clears `is_queued` naturally via `start_timer()`. This matches the plan's intent without requiring a default trigger.

### (3) Safest way to add a third output to build_timer_data_with_audio

**Current signature:**
```rust
async fn build_timer_data_with_audio(
    shared: &Arc<SharedState>,
    icon_cache: Option<&Arc<baras_overlay::icons::IconCache>>,
) -> Option<(TimerData, TimerData, Vec<(String, u8, String)>, Vec<FiredAlert>)>
```

The return tuple already has four elements. Adding a fifth breaks all existing callers in one call site (`service/mod.rs` line ~2546). Since this is a private function with a single call site, the change is safe. The pattern used by the existing call site is a destructured `let`:

```rust
let (timer_a, timer_b, countdowns, alerts) = build_timer_data_with_audio(...).await?;
```

Extend to:
```rust
let (timer_a, timer_b, ability_queue, countdowns, alerts) =
    build_timer_data_with_audio(...).await?;
```

**The third data path within the function:** After the existing `for timer in timer_mgr.active_timers()` loop (which routes to `entries_a` / `entries_b`), add:

- A second pass over `active_timers()` collecting entries where `display_target == AbilityQueue`
  - Active countdown entries: `remaining > 0 && !timer.is_queued`
  - Queued entries: `timer.is_queued == true` (skip the `remaining <= 0 → continue` guard for these)
- A pass over `timer_mgr.active_gcds()` to produce `AbilityQueueEntry { is_pinned: true, ... }`

**Preserving existing behavior:** The `TimerDisplayTarget::AbilityQueue` arm in the existing routing match must be `AbilityQueue => {}` (no-op in the TimersA/B path), exactly like `None` is handled today. This ensures AbilityQueue timers are invisible to the existing overlays regardless of the new code path.

### (4) Correct routing pattern in router.rs for AbilityQueueUpdated

Reading `router.rs` lines 229–353, the established pattern for every overlay is:

```rust
OverlayUpdate::TimersAUpdated(timer_data) => {
    let timer_tx = {
        let state = match overlay_state.lock() {
            Ok(s) => s,
            Err(_) => return,
        };
        state.get_timers_a_tx().cloned()
    };
    if let Some(tx) = timer_tx {
        let _ = tx.send(OverlayCommand::UpdateData(OverlayData::TimersA(timer_data))).await;
    }
}
```

The ability queue arm is identical in shape:

```rust
OverlayUpdate::AbilityQueueUpdated(aq_data) => {
    let tx = {
        let state = match overlay_state.lock() {
            Ok(s) => s,
            Err(_) => return,
        };
        state.get_ability_queue_tx().cloned()
    };
    if let Some(tx) = tx {
        let _ = tx.send(OverlayCommand::UpdateData(OverlayData::AbilityQueue(aq_data))).await;
    }
}
```

**Flush on CombatEnded:** In the `OverlayUpdate::CombatEnded` handler (router.rs ~line 416), Timers A and Timers B are cleared by pushing `OverlayData::TimersA(Default::default())` and `OverlayData::TimersB(Default::default())`. Add the same for AbilityQueue:

```rust
if let Some(tx) = state.get_ability_queue_tx() {
    channels.push((tx.clone(), OverlayData::AbilityQueue(Default::default())));
}
```

**Flush on ClearAllData:** Same pattern in the `ClearAllData` arm (router.rs ~line 456). Both the Timers A/B and the new ability queue entries need clearing here.

**Add accessor to OverlayState:** Following the `get_timers_a_tx` / `get_timers_b_tx` pattern in `overlay/state.rs`:

```rust
pub fn get_ability_queue_tx(&self) -> Option<&Sender<OverlayCommand>> {
    self.get_tx(OverlayType::AbilityQueue)
}
```

### (5) Config wiring — can AbilityQueueOverlayConfig alias TimerOverlayConfig

Yes. The plan's intent is correct and the existing code confirms it. `effects_overlay: TimerOverlayConfig` is already a direct reuse of `TimerOverlayConfig` for a different overlay type (line 2111 in `types/src/lib.rs`). The same pattern applies:

```rust
// In OverlaySettings (types/src/lib.rs)
#[serde(default)]
pub ability_queue_overlay: TimerOverlayConfig,
#[serde(default = "default_opacity")]
pub ability_queue_opacity: u8,
```

No new struct needed. `TimerOverlayConfig` provides `default_bar_color`, `font_color`, `max_display`, `sort_by_remaining`, `font_scale`, and `dynamic_background` — all directly applicable to the ability queue overlay.

**Config wiring in core/src/context/config.rs:** The pattern is to import `TimerOverlayConfig` from `baras_types` (already imported at line 13) and add `ability_queue_overlay` and `ability_queue_opacity` fields to `OverlaySettings`. The `Default` impl addition follows the pattern on line 2184.

**OverlayType::config_key():** Add `OverlayType::AbilityQueue => "ability_queue"` in the `config_key()` match.

## Recommended File Change Order

Build order respects data flow dependencies (each layer can compile independently):

**Layer 1 — Core data model (no dependencies on app layer)**
1. `core/src/timers/definition.rs` — Add `AbilityQueue` variant to `TimerDisplayTarget`, add 4 fields to `TimerDefinition`
2. `core/src/timers/active.rs` — Mirror fields, add `is_queued: bool`
3. `core/src/timers/manager.rs` — Add `active_gcds: Vec<ActiveGcd>`, GCD spawn on start, queued-hold on expire, clear on combat end, `active_gcds()` accessor, `remove_queued_matching` helper

**Layer 2 — Shared types (depends on types crate only)**
4. `types/src/lib.rs` — `AbilityQueueData`, `AbilityQueueEntry` structs; add `ability_queue_overlay: TimerOverlayConfig` + `ability_queue_opacity: u8` to `OverlaySettings`

**Layer 3 — Overlay implementation (depends on core + types)**
5. `overlay/src/overlays/ability_queue.rs` — New file: `AbilityQueueOverlay`, render loop with three-tier sort
6. `overlay/src/overlays/mod.rs` — Register module, add `OverlayData::AbilityQueue(AbilityQueueData)` variant

**Layer 4 — App wiring (depends on all above)**
7. `app/src-tauri/src/overlay/types.rs` — Add `OverlayType::AbilityQueue`, namespace `"baras-ability-queue"`, config_key `"ability_queue"`, default position
8. `app/src-tauri/src/overlay/state.rs` — Add `get_ability_queue_tx()` convenience method
9. `app/src-tauri/src/overlay/spawn.rs` — `create_ability_queue_overlay()` mirroring `create_timers_a_overlay`
10. `app/src-tauri/src/overlay/manager.rs` — Handle `AbilityQueue` show/hide/toggle/config
11. `app/src-tauri/src/service/mod.rs` — Add `OverlayUpdate::AbilityQueueUpdated(AbilityQueueData)`; extend `build_timer_data_with_audio` signature and body; update call site destructure; add `AbilityQueue` arm to the `display_target` match
12. `core/src/context/config.rs` — Wire `ability_queue_overlay` and `ability_queue_opacity`
13. `app/src-tauri/src/router.rs` — Add `AbilityQueueUpdated` arm; add flush in `CombatEnded` and `ClearAllData` arms
14. `app/src-tauri/src/commands/overlay.rs` — `show/hide/toggle_ability_queue_overlay`
15. `app/src-tauri/src/commands/mod.rs` — Re-export
16. `app/src-tauri/src/lib.rs` — Register 3 new commands in `invoke_handler`
17. `app/src/api.rs` — API wrappers for the 3 commands
18. `app/src/app.rs` — Toggle button alongside Timers A/B

**Layer 5 — Timer editor UI**
19. Timer editor component — Reveal `gcd_secs`, `queue_on_expire`, `queue_priority`, `queue_remove_trigger` fields when `display_target = AbilityQueue`

## Threading Model

No new threading concerns. The pattern is identical to TimersA/TimersB:

- `TimerManager` is accessed behind `Mutex<TimerManager>` (same mutex used today for all timer ops). GCD state and queued state live in the manager and are mutated only within `handle_signal()` (sync, single-threaded signal dispatch path) and `build_timer_data_with_audio()` (async service path, locks the same mutex).
- `AbilityQueueOverlay` runs in a dedicated OS thread (same as every other overlay). It receives `OverlayData::AbilityQueue` via its `mpsc::Sender<OverlayCommand>` and only reads data, never writes back.
- No new `Arc`, `AtomicBool`, or `RwLock` needed — the overlay active flag follows the same `SharedState::overlay_settings.enabled` map pattern used by existing overlays.

## Architectural Patterns

### Pattern: Queued-Hold as Alive-Past-Zero Timer

**What:** When `queue_on_expire = true`, `process_expirations()` in `manager.rs` sets `timer.is_queued = true` instead of removing the entry. The `ActiveTimer` remains in `active_timers` with `remaining_secs = 0`, invisible to the TimersA/B render path (which already gates on `remaining > 0`), but visible to the AbilityQueue data path (which explicitly includes `is_queued` entries).

**Why this works:** The existing `process_expirations()` already handles the "remove or chain" decision. Adding a `queue_on_expire` branch is an extension of that decision tree, not a new code path.

### Pattern: Synthetic GCDs as Separate Vec

**What:** `active_gcds: Vec<ActiveGcd>` is a separate lightweight vec, not `TimerDefinition` instances. This avoids polluting `definitions`, `trigger_index`, and `active_timers` with synthetic entries that have no trigger, no cancel condition, and no chaining.

**Why not `ActiveTimer`:** `ActiveTimer` carries 20+ fields (audio config, countdown state, role filtering, alert config) that are meaningless for a synthetic GCD. A flat `{parent_id, label, expires_at}` struct is sufficient and keeps the hot path allocations minimal.

### Pattern: queue_remove_trigger Mirrors cancel_trigger

**What:** `queue_remove_trigger: Option<TimerTrigger>` is processed identically to `cancel_trigger`, but only scans entries where `is_queued = true`. This reuses all existing trigger matching infrastructure without new enums or handler trait methods.

## Anti-Patterns

### Anti-Pattern: GCD State in the Overlay Thread

**What:** Spawning GCD timers from the overlay's `update_data()` method using wall-clock time.

**Why wrong:** Overlays are pure render. State in an overlay thread is invisible to the signal pipeline — it cannot react to game events, cannot be cleared on combat end, and cannot feed back into audio or chaining logic. This breaks the invariant that overlays are pure render and violates the architecture documented in `PROJECT.md`.

**Instead:** All GCD lifecycle lives in `TimerManager`. Overlay receives a fully assembled `AbilityQueueData` snapshot.

### Anti-Pattern: Separate `AbilityQueueTimer` Type

**What:** Adding a new `AbilityQueueTimer` enum variant alongside `TimerDefinition`.

**Why wrong:** This duplicates the trigger system, TOML loader, preferences, definition fingerprinting, and all matching logic. The `display_target` field already cleanly separates routing from definition — there is no need for a new type.

**Instead:** Extend `TimerDefinition` with four new fields gated on `display_target = AbilityQueue`.

### Anti-Pattern: Mutating build_timer_data_with_audio Return Type Destructively

**What:** Changing the return type to a named struct instead of extending the tuple.

**Why unnecessary:** The function has one call site in a 3787-line file. Tuple extension is simpler and matches the existing style. A named struct would be warranted only if there were multiple callers.

**Instead:** Extend the tuple to five elements and update the single destructuring site.

## Integration Points

### Internal Boundaries

| Boundary | Communication | Notes |
|----------|---------------|-------|
| `TimerManager` → service | `active_timers()` + `active_gcds()` read under `Mutex` lock | Same lock path as today, no deadlock risk |
| service → router | `overlay_tx.try_send(OverlayUpdate::AbilityQueueUpdated(...))` | Capacity-256 channel, same as all other updates |
| router → overlay thread | `Sender<OverlayCommand>` stored in `OverlayState` | Lock-acquire-get_tx-clone-release-send pattern, consistent with all 15 existing overlays |
| `OverlaySettings` → overlay thread | `OverlayConfigUpdate` via `OverlayCommand::UpdateConfig` | Already handled by manager; new config field follows same path |
| WASM frontend → backend | Tauri IPC: `show/hide/toggle_ability_queue_overlay` commands | 3 new commands, identical to existing timer overlay commands |

## Sources

- Direct reads: `core/src/timers/manager.rs`, `active.rs`, `definition.rs`, `signal_handlers.rs`
- Direct reads: `app/src-tauri/src/service/mod.rs` (lines 1–440, 2540–2560, 3167–3267)
- Direct reads: `app/src-tauri/src/router.rs` (full file)
- Direct reads: `app/src-tauri/src/overlay/state.rs`, `types.rs`
- Direct reads: `types/src/lib.rs` (lines 1500–1545, 2095–2210)
- Direct reads: `.planning/PROJECT.md`, `ability-queue-overlay-plan.md`
- Direct reads: `.planning/codebase/ARCHITECTURE.md`, `STRUCTURE.md`

---
*Architecture research for: Ability Queue Overlay integration into BARAS*
*Researched: 2026-04-11*
