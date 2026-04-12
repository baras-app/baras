# Stack Research

**Domain:** Ability Queue Overlay — BARAS milestone addition
**Researched:** 2026-04-11
**Confidence:** HIGH

## Summary

No new crates are required. The existing stack handles every technical requirement for the ability queue overlay. The verdict is: extend, don't add.

## Recommended Stack

### Core Technologies (existing — no changes)

| Technology | Version | Purpose | Why Sufficient |
|------------|---------|---------|----------------|
| tiny-skia | 0.11 | Software 2D rasterization for overlay frames | Progress bars and solid-color tier separators already render via `ProgressBar` widget; three visual tiers are just three sequential draw passes |
| cosmic-text | 0.16 | Text shaping and layout | `draw_text_styled` + `measure_text_styled` already used in notes overlay for inline styled spans — sufficient for "READY" label and countdown text |
| fontdb | 0.23 | Font database shared across overlays | `SHARED_FONT_DB: OnceLock` pattern means zero additional cost |
| `std::time::Instant` | stdlib | GCD expiry tracking | `ActiveGcd { expires_at: Instant }` — no external crate; Instant is monotonic and already used throughout TimerManager |
| tokio mpsc | 1.48 | Overlay update channel | `OverlayUpdate::AbilityQueueUpdated(AbilityQueueData)` routes through the existing 256-capacity overlay_tx channel — same as TimersA/B |

### Supporting Libraries (existing — no changes)

| Library | Version | Purpose | Integration Point |
|---------|---------|---------|-------------------|
| `Arc<(u32, u32, Vec<u8>)>` | stdlib | Icon data sharing | `AbilityQueueEntry.icon` follows exact same pattern as `TimerEntry.icon` — cheap clone across channel |
| hashbrown | 0.16.1 | Icon scale cache | `ScaledIconCache = HashMap<(u64, u32), Vec<u8>>` pattern from `timers.rs` reused verbatim |
| serde / serde_json | 1.0 | Config serialization | `TimerDisplayTarget::AbilityQueue` variant serializes with existing `#[serde(rename_all = "snake_case")]` — no config changes needed |
| confy | 2.0.0 | Config persistence | `AbilityQueueOverlayConfig` added to `AppConfig` follows same pattern as `timers_a_overlay` / `timers_b_overlay` |

### Development Tools (no changes)

The existing `cargo check -p app` and `cd app && cargo check --target wasm32-unknown-unknown` workflows cover all new code paths.

## What NOT to Add

| Avoid | Why | Pattern to Use Instead |
|-------|-----|------------------------|
| A new timer type / separate data structure | `TimerDefinition` extended with four optional fields is sufficient; a new type duplicates all trigger/condition/audio infrastructure | Add `AbilityQueue` to `TimerDisplayTarget` enum, add fields with `#[serde(default)]` |
| External sorting crate | Three-tier sort is a simple stable partition: pinned first, then queued by `queue_priority`, then active by `remaining_secs` | `entries.sort_by(...)` on `Vec<AbilityQueueEntry>` in the render method, same as `TimerOverlay` already does |
| A GCD timer as a `TimerDefinition` | GCDs are synthetic — they fire implicitly when an ability fires, not from a user-visible definition. Making them definitions creates config noise and UI confusion | `active_gcds: Vec<ActiveGcd>` on `TimerManager`, pruned on tick, never serialized |
| `parking_lot::Mutex` or new sync primitive | No new shared state between threads; `AbilityQueueData` is plain `Clone` sent over existing mpsc channel | Follow the existing `AtomicBool` + `mpsc::Sender` pattern |
| A new `TimerOverlayConfig` struct | `TimerOverlayConfig` already has `default_bar_color`, `font_color`, `max_display`, `font_scale`, `dynamic_background` — the exact fields needed | `AbilityQueueOverlayConfig` either type-aliases `TimerOverlayConfig` or wraps it with a few additional fields (`gcd_color: Color`) |

## Integration Points

### Data model changes (baras-core)

`core/src/timers/definition.rs` — add `AbilityQueue` to `TimerDisplayTarget`; add four `#[serde(default)]` fields to `TimerDefinition`.

`core/src/timers/active.rs` — mirror the four new fields onto `ActiveTimer`; add `is_queued: bool`.

`core/src/timers/manager.rs` — add `active_gcds: Vec<ActiveGcd>` (private struct, never serialized); update tick logic for GCD pruning and queued-hold.

### Service layer (app/src-tauri)

`service/mod.rs` — add `OverlayUpdate::AbilityQueueUpdated(AbilityQueueData)` variant; extend `build_timer_data_with_audio` to produce a third output. The function already returns a tuple — extend the tuple.

### Overlay layer (overlay crate)

`overlay/src/overlays/ability_queue.rs` — new file following `timers.rs` as the template. Render loop sorts the flat `Vec<AbilityQueueEntry>` into three visual tiers before drawing. No logic, pure render.

`overlay/src/overlays/mod.rs` — register module, add `OverlayData::AbilityQueue` and `OverlayConfigUpdate::AbilityQueue` variants.

### App wiring (12-step checklist from CLAUDE.local.md)

Follows the documented checklist exactly: types.rs → spawn.rs → state.rs → manager.rs → router.rs → config → commands → lib.rs → api.rs → app.rs. No novel patterns.

### Frontend (app-ui WASM)

Timer editor component reveals `gcd_secs`, `queue_on_expire`, `queue_priority`, `queue_remove_trigger` fields conditionally when `display_target == "ability_queue"`. Uses existing `use_signal` + conditional render pattern.

## Version Compatibility

All changes are purely additive to existing types. The `TimerDisplayTarget` enum uses `#[serde(rename_all = "snake_case")]` so the new `AbilityQueue` variant deserializes as `"ability_queue"` without any custom impl. Existing TOML timer files that omit `display_target` continue to default to `TimersA` via `#[default]`.

## Sources

- Direct codebase inspection: `overlay/src/overlays/timers.rs`, `overlay/src/overlays/mod.rs`, `core/src/timers/definition.rs`, `core/src/timers/active.rs`, `types/src/lib.rs` (TimerOverlayConfig at line 1505)
- `ability-queue-overlay-plan.md` — implementation plan specifying data structures and file change list
- `.planning/PROJECT.md` — confirmed constraint "no new runtime dependencies expected"
- `.planning/codebase/STACK.md` — verified current dependency versions

---
*Stack research for: BARAS Ability Queue Overlay milestone*
*Researched: 2026-04-11*
