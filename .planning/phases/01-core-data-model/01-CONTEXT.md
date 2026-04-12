# Phase 1: Core Data Model - Context

**Gathered:** 2026-04-11
**Status:** Ready for planning

<domain>
## Phase Boundary

Extend `TimerDefinition`, `ActiveTimer`, and `TimerManager` in `baras-core` with all ability-queue fields and GCD/queued-hold logic. This is pure data model and manager logic — no overlay rendering, no service wiring. All downstream phases (2–5) depend on these types being stable.

</domain>

<decisions>
## Implementation Decisions

### ActiveGcd Struct
- **D-01:** `ActiveGcd` carries timing fields only: `started_at: NaiveDateTime` and `expires_at: NaiveDateTime`. No name, no color — the overlay will use its configurable accent color and needs no label from the struct.
- **D-02:** The manager field is `active_gcd: Option<ActiveGcd>` (single slot), NOT `active_gcds: Vec<ActiveGcd>`. The ROADMAP SC-2 uses Vec language but the replace policy and single-bar constraint make `Option` the correct shape. Note: the success criterion text says "active_gcds" but this decision overrides it.

### GCD Re-fire Policy
- **D-03:** When a timer fires while an `ActiveGcd` is already counting down, **replace** the existing entry and start fresh. The GCD resets to the full `gcd_secs` duration. This matches SWTOR GCD behavior where re-casting resets the lockout.

### queue_priority Field
- **D-04:** Type is `u8`. Sort direction: **higher value = higher priority** (255 = first in tier-2 list, 0 = last). Default via `#[serde(default)]` = 0.

### queue_remove_trigger Field
- **D-05:** Type is `Option<Trigger>` — reuses the existing `Trigger` DSL type, consistent with `cancel_trigger` on `TimerDefinition`. Evaluation is a **no-op stub** in v1 (always returns false / never fires). Serde: `#[serde(default, skip_serializing_if = "Option::is_none")]`.

### TimerDisplayTarget
- **D-06:** Add `AbilityQueue` variant to `TimerDisplayTarget` enum. No `#[default]` change — `TimersA` remains default for backward compatibility.

### ActiveTimer
- **D-07:** Add `is_queued: bool` field to `ActiveTimer`. When a timer with `queue_on_expire = true` reaches zero, the manager sets `is_queued = true` instead of removing it from `active_timers`. The timer remains in the map indefinitely until a `queue_remove_trigger` fires (stubbed) or combat ends.

### Claude's Discretion
- Where to define `ActiveGcd` — same file as `ActiveTimer` (`active.rs`) is preferred since the file is 337 lines and adding ~20 lines keeps it well under 500.
- `gcd_secs` field type on `TimerDefinition` — `Option<f32>` with `#[serde(default)]` (None = no GCD) is the natural choice.
- `queue_on_expire` field type — `bool` with `#[serde(default)]` (false = normal expiry behavior).

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Timer System (files being extended)
- `core/src/timers/definition.rs` — `TimerDefinition` and `TimerDisplayTarget` structs being extended
- `core/src/timers/active.rs` — `ActiveTimer` struct being extended; `ActiveGcd` goes here
- `core/src/timers/manager.rs` — `TimerManager` receiving `active_gcd` field and GCD/queued logic
- `core/src/timers/signal_handlers.rs` — `clear_combat_timers()` must be extended to clear `active_gcd`
- `core/src/timers/mod.rs` — re-exports to update with any new public types

### Project Constraints
- `core/src/serde_defaults.rs` — use existing default functions; do not add inline closures
- `.planning/ROADMAP.md` §Phase 1 — success criteria (SC-1 through SC-5) are the acceptance test; note D-02 overrides SC-2's "active_gcds" Vec language

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `TimerDisplayTarget` enum (`definition.rs:26`): Add `AbilityQueue` variant here
- `ActiveTimer::new()` (`active.rs:110`): Add `is_queued: bool` parameter or set post-construction
- `clear_combat_timers()` (`signal_handlers.rs:665`): Add `manager.active_gcd = None;` to this function
- `TimerKey` (`active.rs:321`): No changes needed — queued timers stay in the same map, keyed normally

### Established Patterns
- `#[serde(default)]` without explicit function when field type implements `Default` (e.g., `bool` → false, `u8` → 0)
- `#[serde(default, skip_serializing_if = "Option::is_none")]` for optional DSL fields (matches `cancel_trigger` pattern)
- `Option<Trigger>` for optional trigger conditions — same type as `cancel_trigger` on `TimerDefinition`
- Section delimiters (`─────`) used within structs to group related fields

### Integration Points
- `TimerManager::active_gcd` is a new field — all existing `active_timers` logic is unaffected
- Queued timers stay in `active_timers: HashMap<TimerKey, ActiveTimer>` — the map is not split
- Phase 2 will read `active_gcd` and queued entries from the manager to build `AbilityQueueData`

</code_context>

<specifics>
## Specific Ideas

- No specific references or "I want it like X" moments during discussion — open to standard approaches within the patterns above.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 01-core-data-model*
*Context gathered: 2026-04-11*
