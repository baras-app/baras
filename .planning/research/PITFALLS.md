# Pitfalls Research

**Domain:** Adding Ability Queue Overlay to BARAS (Rust event-driven combat parser)
**Researched:** 2026-04-11
**Confidence:** HIGH — based on direct codebase inspection of all affected files

---

## Critical Pitfalls

### Pitfall 1: GCD Vec Not Cleared in `clear_combat_timers`

**What goes wrong:**
`clear_combat_timers` in `core/src/timers/signal_handlers.rs` (line 665) clears `active_timers`, `fired_alerts`, `boss_entity_ids`, and `combat_time_started`. The new `active_gcds: Vec<ActiveGcd>` field is not yet in the manager — if it is added without updating `clear_combat_timers`, GCD bars will persist across encounters and across the area-change that triggers `CombatEnded`.

**Why it happens:**
`clear_combat_timers` is a single function called from two places: `GameSignal::CombatEnded` (manager's `handle_signal`) and `on_encounter_end` (the `SignalHandler` trait impl). It is easy to add new state fields to the manager struct without auditing every clearance site.

**How to avoid:**
When `active_gcds` is added to `TimerManager`, update `clear_combat_timers` in the same commit. Treat that function as the authoritative teardown list — keep a comment listing every field that must be cleared.

**Warning signs:**
GCD bars visible after a wipe or zone transition. The overlay remains populated with stale synthetic entries while `active_timers` is empty.

**Phase to address:** Phase 1 (Core Data Model) — the field and its clearance must land together.

---

### Pitfall 2: Queued Entries Not Excluded from the `remaining <= 0.0 → continue` Guard

**What goes wrong:**
`build_timer_data_with_audio` (service/mod.rs line 3219) skips any timer with `remaining <= 0.0`. Queued entries (`is_queued = true`) have expired (`remaining_secs` is zero or negative) but must still be sent to the overlay. If the guard is applied uniformly, queued entries are silently dropped and never appear in the ability queue tier.

**Why it happens:**
The guard is correct for TimersA/B — a timer at zero should disappear. But the queued-hold design intentionally keeps entries alive past zero. Extending the routing `match` for `AbilityQueue` inside the existing loop without restructuring the guard means the guard fires before the routing branch is reached.

**How to avoid:**
Either (a) separate queued-entry collection before the `remaining <= 0.0` guard and only apply the guard to active-countdown entries, or (b) check `is_queued` inside the guard itself: `if remaining <= 0.0 && !timer.is_queued { continue; }`. Option (b) is a smaller diff but adds a branch that reads a new field; option (a) is more readable but requires splitting the loop.

**Warning signs:**
The ability queue overlay is empty when `queue_on_expire = true` timers expire. No queued tier appears despite the timer having fired.

**Phase to address:** Phase 2 (Service Layer).

---

### Pitfall 3: `build_timer_data_with_audio` Return Tuple Expansion Breaks Its Only Call Site

**What goes wrong:**
`build_timer_data_with_audio` currently returns `Option<(TimerData, TimerData, Vec<(String, u8, String)>, Vec<FiredAlert>)>`. There is exactly one call site (service/mod.rs line 2545):
```rust
if let Some((timers_a, timers_b, countdowns, alerts)) =
    build_timer_data_with_audio(&shared, icon_cache.as_ref()).await
```
Adding a third `TimerData` payload changes the return to a 5-tuple. The pattern-match at the call site must be updated in the same change or the code will not compile. Because the file is 3787 lines, a naive find-replace risk exists if the caller is misidentified.

**Why it happens:**
Large files make it easy to search for the function definition and change the signature without updating the only destructuring call, especially when using an IDE that does not fully expand macro-generated code. The `service/mod.rs` monolith increases this risk.

**How to avoid:**
Use a named return struct instead of growing the tuple:
```rust
struct TimerDataBundle {
    timers_a: TimerData,
    timers_b: TimerData,
    ability_queue: AbilityQueueData,
    countdowns: Vec<(String, u8, String)>,
    alerts: Vec<FiredAlert>,
}
```
This makes the call site `bundle.ability_queue` instead of a positional index, eliminates ordering ambiguity, and the compiler enforces completeness at the single call site. The struct can remain private to `service/mod.rs`.

**Warning signs:**
Compile error is the warning sign — this pitfall is caught at compile time, not runtime. The risk is forgetting to update the destructure entirely if the struct approach is not used.

**Phase to address:** Phase 2 (Service Layer) — use the struct approach from the start.

---

### Pitfall 4: AbilityQueue Overlay Not Flushed in `OverlayUpdate::CombatEnded` Router Handler

**What goes wrong:**
`router.rs` lines 416-454 handle `OverlayUpdate::CombatEnded` by collecting channels for BossHealth, TimersA, TimersB, and CombatTime and sending `Default::default()` payloads. There is also `OverlayUpdate::ClearAllData` (lines 456-553) which handles file-switch clears. Neither block contains an Ability Queue channel — because it does not yet exist. If the new overlay is added to `overlay/state.rs` but forgotten in the `CombatEnded` block of `router.rs`, the ability queue overlay will show stale entries from the previous encounter when a new combat begins. GCD bars from the last boss will remain pinned at the top.

**Why it happens:**
`CombatEnded` and `ClearAllData` are separate code blocks in a 580-line file. Adding a new overlay type typically means updating the `ClearAllData` block (it is the obvious "clear everything" path) while missing the more targeted `CombatEnded` block that only clears the overlays that reset between encounters.

**How to avoid:**
Search for both `CombatEnded` and `ClearAllData` in `router.rs` when adding any new overlay. Treat them as a pair that must both be updated. Add the ability queue channel to the `CombatEnded` collection block alongside TimersA/B:
```rust
if let Some(tx) = state.get_ability_queue_tx() {
    channels.push((tx.clone(), OverlayData::AbilityQueue(Default::default())));
}
```

**Warning signs:**
After a wipe or boss kill, the ability queue overlay still shows the previous encounter's GCD bar or queued entries. TimersA/B clear correctly but ability queue does not.

**Phase to address:** Phase 4 (App Wiring) — router changes live here, but the intent must be established in Phase 2 when `OverlayUpdate::CombatEnded` semantics are defined.

---

### Pitfall 5: `queue_remove_trigger` Evaluation Runs After `is_queued` Guard — Ordering Hazard

**What goes wrong:**
The manager's `process_expirations` runs on every tick. A queued entry (is_queued = true) sits in `active_timers` at zero remaining. If the same combat signal that should clear the queued entry (matching `queue_remove_trigger`) also happens to re-trigger the same timer definition, the removal and re-start may happen in the same tick. The ordering of "evaluate remove trigger" vs. "start new timer instance" within a single tick is not yet defined. If removal runs after the new instance is pushed, the new instance is also removed.

**Why it happens:**
The existing expiration pipeline in `process_expirations` clears the timer and chains to the next one atomically within the same tick. Adding a separate "check remove triggers for queued entries" pass creates a second removal pathway that can interact with chain-start logic. `expired_this_tick` is cleared at the top of `process_expirations`, so any queued-entry removal must use its own tracking vector or piggyback on the existing one carefully.

**How to avoid:**
Process `queue_remove_trigger` evaluation as a distinct, ordered phase — after normal expirations and chain-starts complete for the tick but before the batch vectors are committed. Explicitly document the execution order in code comments:
1. `process_expirations` — clears expired non-queued timers, starts chains
2. `process_queued_removals` — evaluates `queue_remove_trigger` matches, removes queued entries, then allows their timer definition to re-trigger normally on the next signal if applicable

Keep queued-entry removal out of `process_expirations` entirely to avoid mutual interaction.

**Warning signs:**
A queued entry that should clear when the ability fires again instead clears AND the new timer instance for that ability also disappears. Or conversely: the queued entry never clears because the new instance is detected first.

**Phase to address:** Phase 1 (Core Data Model — manager logic).

---

### Pitfall 6: `timer_overlay_active` AtomicBool Guards TimersA/B But Not Ability Queue Independently

**What goes wrong:**
The service loop's timer polling block (service/mod.rs line 2549) gates sending `TimersAUpdated` and `TimersBUpdated` behind `in_combat && timer_active`. The `timer_active` flag reads `shared.timer_overlay_active`, which is set by the overlay manager when TimersA or TimersB is running. There is no separate `ability_queue_overlay_active` flag yet. If the implementation reuses `timer_overlay_active` for the ability queue (easiest path), the ability queue overlay stops receiving updates when both TimersA and TimersB are closed, even if the ability queue overlay is still open. Conversely, if the ability queue polling is gated on a new flag that is never set in the manager, the overlay also never receives updates.

**Why it happens:**
The pattern of per-overlay `AtomicBool` flags in `SharedState` is not automatically extended when a new overlay is added. The service loop and the overlay manager must be co-updated. Because they are in different files (`service/mod.rs` vs `overlay/manager.rs`), it is easy to wire one without the other.

**How to avoid:**
Add `ability_queue_overlay_active: AtomicBool` to `SharedState` in the same commit as the overlay spawn/manager changes. Update the service loop to check this flag when deciding whether to send `AbilityQueueUpdated`. Update the manager to set/clear the flag on show/hide.

**Warning signs:**
Ability queue overlay window is open but never renders any data, or renders only on the first frame before going blank. Alternatively: ability queue data updates fire even when no overlay window is open, wasting CPU.

**Phase to address:** Phase 4 (App Wiring) — SharedState, manager, and service loop must be updated together.

---

### Pitfall 7: 3-Tier Sort Relies on `is_pinned`/`is_queued` Flags But `remaining_secs = 0` Is Ambiguous

**What goes wrong:**
`AbilityQueueEntry` uses two flags (`is_pinned` for tier 1 / GCD, `is_queued` for tier 2 / ready). Tier 3 (active countdown) is everything else. A rendering sort might attempt to use `remaining_secs == 0.0` as a proxy for "queued/ready" rather than the explicit flag, since the service layer sets remaining to 0 for queued entries. If the sort logic uses `remaining_secs` directly, entries that just expired in the same frame (real remaining near zero but not flagged as queued) may be incorrectly rendered in tier 2 instead of disappearing, and legitimately queued entries may sort incorrectly among tier 3 if `remaining_secs` comparison is applied across tiers.

**Why it happens:**
The overlay rendering code is "pure render" — it receives a `Vec<AbilityQueueEntry>` and must sort it. If the sort function is written as a comparison on `remaining_secs` only (analogous to how TimersA/B sort by remaining time), the three-tier boundary is lost. The flags are the only reliable tier discriminators.

**How to avoid:**
Sort in the overlay using an explicit tier key before any time-based comparison:
```rust
fn tier_key(e: &AbilityQueueEntry) -> u8 {
    if e.is_pinned { 0 } else if e.is_queued { 1 } else { 2 }
}
entries.sort_by(|a, b| {
    tier_key(a).cmp(&tier_key(b))
        .then(/* within tier: pinned→remaining, queued→priority, active→remaining */)
});
```
Never use `remaining_secs == 0.0` as a tier discriminator. The service layer must set `remaining_secs = 0.0` for queued entries but must also set `is_queued = true` as the canonical flag.

**Warning signs:**
Queued entries appear in tier 3 (sorted by time among active countdowns) rather than tier 2. Or: entries that expired in the same frame as a queued-hold flash briefly in tier 2 before disappearing on the next frame.

**Phase to address:** Phase 3 (Overlay) — enforce flag-based tier sort from the first render implementation.

---

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| Reuse `timer_overlay_active` for ability queue | No new AtomicBool, no SharedState change | Ability queue and TimersA/B are coupled — can't show one without the other activating the service poll | Never — overlays must be independently addressable |
| Extend the return tuple to 5 elements instead of using a named struct | Smaller diff | Positional destructuring in the caller breaks on every future change to the function | Never — use named struct from the start |
| Let `clear_combat_timers` only clear `active_timers` and trust the overlay to show stale data | Simpler manager | GCD bars and queued entries survive wipes and zone transitions | Never |
| Skip the `queue_remove_trigger` evaluation pass and only remove queued entries on combat end | Avoids ordering complexity | Queued entries that should clear when the ability fires again never do | Never for production; acceptable as a Phase 1 stub if documented |
| Alias `AbilityQueueOverlayConfig = TimerOverlayConfig` with a type alias | No duplicate config fields | Locks ability queue config to TimerOverlayConfig forever; future-specific fields require breaking the alias | Acceptable if the plan document explicitly commits to this and notes the limitation |

---

## Integration Gotchas

| Integration Point | Common Mistake | Correct Approach |
|-------------------|----------------|------------------|
| `build_timer_data_with_audio` caller | Updating only the function signature and forgetting the destructuring at line 2545 | Change signature and call site in the same commit; use a named struct to make mismatch a compile error |
| `OverlayUpdate::CombatEnded` in router.rs | Adding the channel to `ClearAllData` but not `CombatEnded` | Always grep for both blocks when adding a new overlay |
| `on_encounter_end` in `TimerManager` | Calling `clear_combat_timers` which now misses `active_gcds` | `active_gcds.clear()` must be added to `clear_combat_timers` alongside the existing clears |
| `SharedState` overlay active flags | Reusing `timer_overlay_active` for ability queue | Add a dedicated `ability_queue_overlay_active: AtomicBool` |
| `OverlayData` enum in `baras-overlay` | Adding `AbilityQueue(AbilityQueueData)` variant without `Default` impl | All `OverlayData` variants used in `ClearAllData` require `Default`; add `#[derive(Default)]` to `AbilityQueueData` |
| Dioxus toggle button in `app.rs` | Adding the toggle button without wiring the `ability_queue_overlay_active` emission back to the frontend | The frontend polls or listens for overlay status; the new flag must be included in `emit_overlay_status_changed()` or equivalent |

---

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| Calling `active_gcds()` iterator inside the service loop's hot timer poll path without pruning expired GCDs first | Old GCD entries pile up; iteration cost grows per combat | Prune expired GCDs in `tick()` before the service polls — same pattern as timer expiration pruning | Rarely noticeable but degrades on long fights with many GCDs |
| Cloning `AbilityQueueData` for the `OverlayUpdate` channel when GCD entries contain `Arc<(u32, u32, Vec<u8>)>` icon data | Cheap arc clone, not a real issue | Icon data is already `Arc`-wrapped; the clone is cheap. Not a trap. | Not applicable |
| Rebuilding the sorted `Vec<AbilityQueueEntry>` from scratch on every 16ms frame in the overlay | Unnecessary allocation on every frame | Sort is O(n log n) on a very small n (typically < 10 entries); this is acceptable. Not a trap. | Not applicable at this scale |
| Adding `active_gcds` as `Vec<ActiveGcd>` with unbounded growth if pruning is missed | Memory grows across long sessions | Prune in `tick()` unconditionally by comparing `expires_at` to `Instant::now()` | Would not be noticed until a very long session (hours) |

---

## "Looks Done But Isn't" Checklist

- [ ] **GCD cleanup:** `clear_combat_timers` clears `active_gcds` — verify by confirming the function body was updated alongside the field addition.
- [ ] **Queued entries in service:** The `remaining <= 0.0` guard in `build_timer_data_with_audio` has an explicit `is_queued` exception — verify the queued entry appears in the returned `AbilityQueueData.entries`.
- [ ] **Combat end flush:** The `CombatEnded` router handler sends `OverlayData::AbilityQueue(Default::default())` — verify by grepping router.rs for the pattern that matches the other overlays.
- [ ] **ClearAllData flush:** `ClearAllData` in router.rs also clears the ability queue channel — verify it is listed alongside TimersA/B.
- [ ] **Overlay active flag:** `ability_queue_overlay_active` is set on overlay show and cleared on hide — verify `OverlayManager` arms for `AbilityQueue` both set and clear the flag.
- [ ] **Service poll gate:** The service loop checks `ability_queue_overlay_active` before sending `AbilityQueueUpdated` when `in_combat` — verify the condition mirrors the `timer_active` guard for TimersA/B.
- [ ] **Frontend status emission:** `emit_overlay_status_changed()` or equivalent includes the ability queue active state so the toggle button reflects real status — verify by toggling the overlay and checking the frontend button state updates.
- [ ] **Tier sort:** The ability queue overlay render function uses `is_pinned`/`is_queued` flags as primary sort keys, not `remaining_secs` — verify the sort comparator.

---

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| GCD not cleared on combat end | LOW | Add `self.active_gcds.clear()` to `clear_combat_timers`; no data migration needed |
| Queued entries dropped at zero | LOW | Add `is_queued` exception to the guard; fix is a one-line change |
| Return tuple position mismatch | LOW (compile error) | Refactor to named struct; compiler points to all mismatches |
| Router missing ability queue flush | LOW | Add channel to both `CombatEnded` and `ClearAllData` blocks |
| Remove trigger / new instance ordering conflict | MEDIUM | Separate `process_queued_removals` into its own pass after `process_expirations`; requires careful testing against manager_tests.rs patterns |
| Overlay active flag reuse | MEDIUM | Add new `AtomicBool` to `SharedState`, update manager and service loop; requires touching 3 files |
| Tier sort using `remaining_secs` as discriminator | LOW | Fix the comparator function in `ability_queue.rs`; no manager changes needed |

---

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| GCD Vec not cleared in `clear_combat_timers` | Phase 1 (Core Data Model) | After combat ends in manual test, ability queue overlay shows no entries |
| Queued entries dropped at zero by guard | Phase 2 (Service Layer) | Timer with `queue_on_expire = true` appears in ability queue after expiry |
| Return tuple expansion breaks call site | Phase 2 (Service Layer) | Named struct compiles without positional destructuring |
| AbilityQueue not flushed in `CombatEnded` router | Phase 4 (App Wiring) | Wipe test: ability queue clears, TimersA/B also clear |
| `queue_remove_trigger` / new instance ordering | Phase 1 (Core Data Model) | Unit test: fire remove trigger signal in same tick as timer re-trigger; verify old queued entry clears and new active entry appears |
| `timer_overlay_active` reuse | Phase 4 (App Wiring) | Close TimersA and TimersB while ability queue is open; verify queue still updates |
| 3-tier sort using `remaining_secs` as tier key | Phase 3 (Overlay) | GCD bar stays at top when active countdowns have more remaining time than GCD |

---

## Sources

- Direct inspection of `core/src/timers/signal_handlers.rs` (lines 664-674): `clear_combat_timers` function body
- Direct inspection of `app/src-tauri/src/service/mod.rs` (lines 3162-3267): `build_timer_data_with_audio` signature and call site
- Direct inspection of `app/src-tauri/src/router.rs` (lines 416-553): `CombatEnded` and `ClearAllData` handler blocks
- Direct inspection of `app/src-tauri/src/state/mod.rs` (lines 143-160): per-overlay `AtomicBool` flags pattern
- Direct inspection of `core/src/timers/manager.rs` (lines 971-1053): `process_expirations` logic and `expired_this_tick` lifecycle
- `ability-queue-overlay-plan.md`: canonical feature plan with phase structure
- `.planning/codebase/CONCERNS.md`: known fragile areas (service run loop, overlay spawn lifecycle)

---
*Pitfalls research for: Ability Queue Overlay feature addition to BARAS*
*Researched: 2026-04-11*
