# Ability Queue Overlay — Implementation Plan

## Overview

A new dedicated overlay for modeling boss ability sequencing in SWTOR. Tracks boss abilities,
GCD state, and queued/ready abilities. Completely independent from the existing Timers overlays.

Timers with `display_target = AbilityQueue` are routed to this overlay. Selecting that target
in the timer editor reveals ability-queue-specific fields.

---

## Visual Tiers (top → bottom)

| Tier | Content | Sort |
|------|---------|------|
| 1 | GCD bars (synthetic, pinned) | Remaining time |
| 2 | Queued/ready entries (`is_queued = true`) | `queue_priority` ascending |
| 3 | Active cooldown countdown entries | Remaining time |

**Visual distinction:**
- GCD bar: distinct accent color (e.g. yellow), always top
- Queued entry: full-width static bar, no countdown text (or "READY" label), distinct color
- Active entry: standard countdown bar (same as Timers A/B)

---

## Phase 1: Core Data Model

### `core/src/timers/definition.rs`

Add `AbilityQueue` variant to `TimerDisplayTarget`:

```rust
pub enum TimerDisplayTarget {
    #[default]
    TimersA,
    TimersB,
    AbilityQueue,  // new
    None,
}
```

Add fields to `TimerDefinition` (only meaningful when `display_target = AbilityQueue`):

```rust
/// GCD duration to pin at top when this ability fires (seconds)
pub gcd_secs: Option<f32>,
/// Hold entry as "queued/ready" at zero instead of removing
pub queue_on_expire: bool,
/// Sort order in queued tier (lower = higher priority, shown first)
pub queue_priority: Option<u32>,
/// What clears the queued entry (default: when this timer starts again)
pub queue_remove_trigger: Option<TimerTrigger>,
```

### `core/src/timers/active.rs`

Mirror new definition fields onto `ActiveTimer`:
- `gcd_secs: Option<f32>`
- `queue_on_expire: bool`
- `queue_priority: Option<u32>`
- `queue_remove_trigger: Option<TimerTrigger>`
- `is_queued: bool` — set by manager when expired with `queue_on_expire = true`

### `core/src/timers/manager.rs`

Add `active_gcds: Vec<ActiveGcd>` — lightweight struct:

```rust
struct ActiveGcd {
    parent_id: String,   // which ability triggered this
    label: String,       // display name (e.g. "GCD")
    expires_at: Instant,
}
```

Logic changes:
- On `AbilityQueue` timer start with `gcd_secs` set → push `ActiveGcd`
- On tick: prune expired GCDs
- On `AbilityQueue` timer expiry with `queue_on_expire = true` → set `is_queued = true` instead of removing
- Watch `queue_remove_trigger` events on queued entries → remove when matched
- Expose `active_gcds()` iterator for service layer

---

## Phase 2: Service Layer

### `app/src-tauri/src/service/mod.rs`

Add `OverlayUpdate::AbilityQueueUpdated(AbilityQueueData)` variant.

Extend `build_timer_data_with_audio` to produce and return a third data payload:

```rust
// Returns (TimersA, TimersB, AbilityQueue, countdowns, alerts)
```

Build logic for ability queue entries:
- **GCD entries**: from `timer_mgr.active_gcds()` → `AbilityQueueEntry { is_pinned: true, ... }`
- **Active countdown entries**: `AbilityQueue` display target timers with `remaining > 0`
- **Queued entries**: `AbilityQueue` timers with `is_queued = true` — skip the `remaining <= 0 → continue` guard for these

Send via `overlay_tx.try_send(OverlayUpdate::AbilityQueueUpdated(...))`.

Also update combat-end and area-change clear blocks (alongside Timers A/B flushes) to flush ability queue overlay.

---

## Phase 3: Overlay

### `overlay/src/overlays/ability_queue.rs` (new file)

```rust
pub struct AbilityQueueEntry {
    pub name: String,
    pub remaining_secs: f32,
    pub total_secs: f32,
    pub color: [u8; 4],
    pub is_pinned: bool,              // GCD synthetic — tier 1
    pub is_queued: bool,              // ready-to-fire — tier 2
    pub queue_priority: Option<u32>,
    pub icon: Option<Arc<(u32, u32, Vec<u8>)>>,
}

pub struct AbilityQueueData {
    pub entries: Vec<AbilityQueueEntry>,
}
```

Render loop sorts into three tiers before drawing. No logic — pure render.

### `overlay/src/overlays/mod.rs`

- `pub mod ability_queue`
- `pub use ability_queue::{AbilityQueueData, AbilityQueueEntry, AbilityQueueOverlay}`
- Add `OverlayData::AbilityQueue(AbilityQueueData)` variant

---

## Phase 4: App Wiring (12-step checklist)

| Step | File | Change |
|------|------|--------|
| Type | `app/src-tauri/src/overlay/types.rs` | `OverlayType::AbilityQueue`, label/namespace/default-size |
| Spawn | `app/src-tauri/src/overlay/spawn.rs` | `create_ability_queue_overlay()` mirroring `create_timers_a_overlay` |
| State | `app/src-tauri/src/overlay/state.rs` | `get_ability_queue_tx()` |
| Manager | `app/src-tauri/src/overlay/manager.rs` | Handle `AbilityQueue` show/hide/toggle, config update |
| Router | `app/src-tauri/src/router.rs` | Route `AbilityQueueUpdated`, flush on combat end/area change |
| Config | `types/src/lib.rs` | `AbilityQueueOverlayConfig`, add to `AppConfig` |
| Config | `core/src/context/config.rs` | Wire `ability_queue_overlay` and `ability_queue_opacity` |
| Commands | `app/src-tauri/src/commands/overlay.rs` | `show/hide/toggle_ability_queue_overlay` |
| Commands | `app/src-tauri/src/commands/mod.rs` | Re-export |
| Handler | `app/src-tauri/src/lib.rs` | Register in `invoke_handler` |
| Frontend API | `app/src/api.rs` | Wrappers for show/hide/toggle commands |
| Frontend UI | `app/src/app.rs` | Toggle button alongside Timers A/B |

---

## Phase 5: Timer Editor UI

When `display_target = AbilityQueue` is selected, reveal additional fields:

- `gcd_secs` number field — "GCD duration (seconds)"
- `queue_on_expire` checkbox — "Hold as queued when ready"
  - Reveals: `queue_priority` number field — "Priority (lower fires first)"
  - Reveals: `queue_remove_trigger` selector — "Remove trigger"

No changes to Timers A/B timer editor path.

---

## File Change Summary

| File | Change type |
|------|-------------|
| `core/src/timers/definition.rs` | Extend enum + struct |
| `core/src/timers/active.rs` | Mirror new fields, `is_queued` flag |
| `core/src/timers/manager.rs` | GCD tracking, queued-hold logic |
| `app/src-tauri/src/service/mod.rs` | Third data path, new `OverlayUpdate` variant |
| `overlay/src/overlays/ability_queue.rs` | **New file** |
| `overlay/src/overlays/mod.rs` | Register new overlay |
| `types/src/lib.rs` | `AbilityQueueOverlayConfig`, extend `AppConfig` |
| `core/src/context/config.rs` | Config fields |
| `app/src-tauri/src/overlay/types.rs` | New `OverlayType` variant |
| `app/src-tauri/src/overlay/spawn.rs` | Spawn function |
| `app/src-tauri/src/overlay/state.rs` | Channel accessor |
| `app/src-tauri/src/overlay/manager.rs` | Show/hide/toggle/config |
| `app/src-tauri/src/router.rs` | Routing + flush |
| `app/src-tauri/src/commands/overlay.rs` | 3 new commands |
| `app/src-tauri/src/commands/mod.rs` | Re-export |
| `app/src-tauri/src/lib.rs` | Register commands |
| `app/src/api.rs` | API wrappers |
| `app/src/app.rs` | Toggle button |
| Timer editor component | UI reveal for new fields |

---

## Key Design Decisions

- **GCD state lives in the manager** — overlays are pure render, no logic
- **Queued hold is analogous to effects Ready State** — manager keeps entry alive past zero, sets `is_queued` flag
- **Synthetic GCD entries** are not `TimerDefinition` instances — they're spawned implicitly by the manager on ability fire and tracked in a separate `active_gcds` vec
- **Show/hide is the only toggle** — no global flag, just enable/disable the overlay
- **`AbilityQueueOverlayConfig`** can alias/reuse `TimerOverlayConfig` fields (bar height, font size, opacity, etc.)
