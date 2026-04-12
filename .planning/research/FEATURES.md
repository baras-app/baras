# Feature Landscape: Ability Queue Overlay

**Domain:** MMO combat overlay — GCD tracking + ability queue visualization
**Researched:** 2026-04-11
**Project context:** BARAS (SWTOR combat parser). Subsequent milestone. Existing: Timers A/B overlays with countdown bars, effects tracker, boss health bar, trigger-based timer editor.

---

## Table Stakes

Features users expect from an ability queue overlay. Missing = product feels incomplete.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| GCD bar pinned at top | Every GCD tracker in every MMO shows a top-anchored bar for the global cooldown. ESO Combat Metronome, WoW WeakAuras GCD auras — all pin GCD at top or as a distinct first element. | Low | Synthetic entry; not a `TimerDefinition` instance. Spawned by manager on any `AbilityQueue` timer fire that has `gcd_secs` set. |
| GCD bar appears only when active, hides at zero | GCD bars in all tooling (ESO, WoW) animate in when an ability fires and disappear at zero — they do not persist. Users expect GCD to be ambient/invisible when no ability is being used. | Low | Manager prunes expired `ActiveGcd` entries on tick; overlay renders nothing when `active_gcds` is empty. |
| GCD countdown fills/drains as bar | A linear progress bar depleting from full to empty over the GCD window. Industry standard for all cast/GCD bars. | Low | Reuse existing bar-drawing code from `timers_a`/`timers_b`. `remaining_secs / total_secs` ratio. |
| Queued/ready entries stay visible at zero | "Hold" at zero instead of vanishing is the core differentiator of a queue view vs a plain cooldown list. Analogous to effects in "ready" state. Players need to see what's waiting to be used. | Low | Manager sets `is_queued = true` instead of removing. Entry renders as a static "READY" state bar at full width. |
| Active countdown bars sorted by remaining time | Standard in all timer overlays. Users scan shortest-remaining first. ESO Combat Metronome and WoW cooldown trackers sort ascending by remaining time. | Low | Sort in overlay render loop, not in manager. Manager delivers entries unsorted. |
| Visual distinction between three tiers | GCD, queued, and active countdowns must be visually distinct. Users need to parse tier at a glance. This is a table stake because mixing them creates immediate confusion. | Low | Color + layout position is sufficient. GCD: accent color (yellow/gold). Queued: desaturated/full-width static bar with "READY" label. Active: same as Timers A/B style. |
| Overlay show/hide toggle | All other overlays support this. Users expect the same entry point. | Low | Mirrors the 12-step checklist already defined in the implementation plan. |
| Overlay position + opacity config | Standard for all BARAS overlays — position, opacity, bar height, font size. | Low | `AbilityQueueOverlayConfig` can alias `TimerOverlayConfig` fields. |
| Timer editor reveals ability-queue fields when target = AbilityQueue | Without this, users cannot configure the new fields. Discovery requires the UI reveal pattern. | Medium | Conditional field display: `gcd_secs`, `queue_on_expire`, `queue_priority`, `queue_remove_trigger`. |
| Flush on combat end and area change | All overlays clear on combat end/area change. Leaving stale queued entries between pulls is a bug-class issue users immediately notice. | Low | Route flush alongside existing Timers A/B flush in router.rs. |

---

## Differentiators

Features that set this apart from basic countdown overlays. Not expected by default, but valued.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| Configurable GCD duration per timer | SWTOR has three GCD breakpoints (1.5s, 1.4s, 1.3s) based on Alacrity rating. Timers with `gcd_secs` let users set the correct value for their build. ESO Combat Metronome 2025 added per-ability GCD fine-tuning for exactly this reason. | Low | `gcd_secs: Option<f32>` on `TimerDefinition`. If absent, no GCD bar is spawned. |
| Configurable queue priority | Queued entries sort by `queue_priority` ascending (lower = higher priority = shown first). This lets users order "Corrosive Grenade is ready before Explosive Probe" semantically. No other open-source SWTOR tool exposes this. | Low | `queue_priority: Option<u32>`. Entries with None sort to bottom of queued tier. |
| Configurable remove trigger for queued entries | Default: queued entry clears when the same timer starts again (ability is used). Advanced: configurable `queue_remove_trigger` allows clearing on phase change, counter event, etc. Enables modeling "this ability is wasted after X event." | Medium | `queue_remove_trigger: Option<TimerTrigger>` evaluated in manager on each signal. |
| Ability icon display on entries | All major MMO overlays (Hekili, WeakAuras) show ability icons. Users learn ability queues faster with visual icon recognition than with text alone. | Medium | `icon: Option<Arc<(u32, u32, Vec<u8>)>>` already in the planned `AbilityQueueEntry` struct. Leverages existing `icon_ability_id` pattern from `TimerDefinition`. |
| Three-tier spatial layout (GCD / queued / active) | Hekili (WoW) popularized a horizontally-stacked queue showing "cast now" + "cast next" icons. BARAS implements a vertically-tiered equivalent: GCD at top signals "locked", queued in middle signals "waiting", active at bottom signals "incoming." The spatial separation eliminates need for color-only differentiation. | Low | Pure rendering concern — sort `entries` into three groups before draw pass. No manager changes needed. |

---

## Anti-Features

Features to explicitly NOT build in this milestone.

| Anti-Feature | Why Avoid | What to Do Instead |
|--------------|-----------|-------------------|
| GCD state logic in the overlay thread | Overlays are pure render. Putting GCD tracking in the overlay means duplicating state, complicating the data flow, and coupling render to timing logic. The existing pattern (manager owns all state, overlay gets a snapshot) is correct. | All GCD/queued logic stays in `TimerManager`. Overlay receives `AbilityQueueData` and renders it. |
| Separate "ability queue timer type" | Creates a parallel definition system that must be maintained alongside `TimerDefinition`. SWTOR timer triggers, conditions, phases, and context filters are already built and tested. | Extend `TimerDefinition` with new optional fields. Add `AbilityQueue` variant to `TimerDisplayTarget`. |
| Multiple simultaneous GCD tracks | SWTOR has one GCD shared by all on-GCD abilities. Multiple tracks would add UI complexity with no gameplay value. StarParse does not expose multi-GCD tracking; neither does any SWTOR-specific tooling found in research. | Single `active_gcds: Vec<ActiveGcd>` in manager. Multiple entries possible (if multiple abilities fire in quick succession before first GCD expires), but rendered as one visual bar using the most recently started GCD. |
| Predictive next-ability suggestions | Hekili (WoW) computes next-optimal-ability via SimulationCraft APLs. SWTOR rotations are deterministic enough that users configure their own queue manually via timer definitions. Building a rotation solver is a separate product entirely. | Let users define their queue via `AbilityQueue` display target timers. The overlay shows what they configure, not what an algorithm computes. |
| Latency-aware GCD window adjustment | ESO Combat Metronome shows live ping to allow latency-aware queuing. BARAS reads from combat log — log timestamps already reflect server-acknowledged events, so latency is baked in. Displaying ping adds no actionable information. | Use combat log timestamps as-is. GCD bar timing derived from log events, not wall-clock inference. |
| Per-instance queued state (multi-target) | `queue_on_expire` with `per_target = true` would require a separate queued slot per target entity. Reasoning: GCD is global, queued state represents "this ability is ready to use next" — which is inherently a single-player decision, not per-target. | If a timer is `per_target`, its `queue_on_expire` behavior applies to the first-to-expire instance only, or is ignored. Document this limitation. |

---

## Feature Dependencies

```
AbilityQueue TimerDisplayTarget variant
  → gcd_secs / queue_on_expire / queue_priority / queue_remove_trigger fields on TimerDefinition
    → Mirrored onto ActiveTimer
      → Manager: ActiveGcd tracking + is_queued flag
        → Service: build_timer_data_with_audio third output path
          → AbilityQueueData / AbilityQueueEntry types (types crate)
            → ability_queue.rs overlay (new file)
              → Full 12-step app wiring (types, spawn, state, manager, router, commands, frontend)
                → Timer editor UI conditional field reveal
```

Dependency from existing infrastructure:

```
TimerManager (exists) → extended with new fields and GCD vec
Timers A/B rendering pattern (exists) → copied and adapted for three-tier render
icon_ability_id pattern (exists) → reused for ability icons in queue entries
cancel_trigger pattern (exists) → reused concept for queue_remove_trigger
Effects "ready state" pattern (exists) → direct model for is_queued hold behavior
```

---

## MVP Recommendation

Build in this order:

1. **Data model** (`TimerDefinition` / `ActiveTimer` / `TimerManager`) — all other phases depend on this. Zero risk of invalidation.
2. **Service layer** (`AbilityQueueData`, `OverlayUpdate::AbilityQueueUpdated`) — wire data path before rendering anything.
3. **Overlay** (`ability_queue.rs`) — pure render, can be built and iterated without UI changes.
4. **App wiring** (12 steps) — mechanical but load-bearing for end-to-end function.
5. **Timer editor UI** — last because it's additive and doesn't block other phases.

Defer to post-milestone:
- Ability icon display (add later without architectural change — `icon` field is already in the struct)
- `queue_remove_trigger` support (add after basic queued-hold is validated)

---

## SWTOR GCD Edge Cases

These must be designed for, not discovered in QA.

### Variable GCD Duration (Alacrity Breakpoints)

SWTOR GCD is NOT always 1.5 seconds. Confirmed breakpoints:
- 0–702 Alacrity rating: 1.5s GCD
- 703–1859 Alacrity: 1.4s GCD
- 1860+ Alacrity: 1.3s GCD
- ~3000+ Alacrity (theoretical): 1.2s GCD (unattainable with current stat budgets)

Alacrity rounds GCD up to nearest 0.1s. Values between breakpoints are wasteful — players target specific thresholds.

**Design implication:** `gcd_secs` on `TimerDefinition` is a user-configured value, not auto-detected. Users set the correct value for their build. This is correct — BARAS cannot detect Alacrity rating from combat log events. The field is `Option<f32>` with no default, so users who don't configure it get no GCD bar (safe fallback).

### Off-GCD Abilities

Many SWTOR abilities do NOT trigger the GCD: taunts, guards, most defensive cooldowns (Saber Reflect, many class defensives), interrupts, stun breaks. If a timer has `display_target = AbilityQueue` but its trigger is an off-GCD ability, the user setting `gcd_secs` on it is a misconfiguration — not a crash scenario, just a cosmetically wrong GCD bar.

**Design implication:** No validation needed. Document in user-facing timer editor that `gcd_secs` is for on-GCD abilities. A spurious short GCD bar causes no downstream damage.

### Channel/Cast Interruption

If a channeled ability is interrupted (by player or by game mechanics), BARAS emits the event that started the timer, but not necessarily a cancel event unless a `cancel_trigger` is configured. GCD still fires server-side on channel start.

**Design implication:** `ActiveGcd` expires naturally by wall clock. There is no "cancel the GCD bar" mechanic needed. Channel interruption does not affect GCD tracking because GCD already started on activation. This is consistent with how SWTOR server handles it.

### Off-GCD During Channel (Channel Cancellation Edge Case)

In SWTOR, using an off-GCD ability during a channel cancels the channel. The off-GCD ability fires but the channel stops early. If the channeling timer had `queue_on_expire = true`, it would not reach natural expiry and thus would not enter queued state.

**Design implication:** If users want the channel to queue even when cancelled early, they need a `cancel_trigger` on the timer set to clear the active instance AND a separate mechanism for queuing. This is an advanced configuration edge case, not an MVP concern. Document as known limitation.

### Rapid Re-trigger Before GCD Expires

If an ability fires again (e.g., via proc reset) before the previous GCD finishes, the manager pushes a new `ActiveGcd`. The overlay should display one GCD bar — the newest one. The previous one expires naturally.

**Design implication:** Render only the GCD with the latest `expires_at` (highest remaining time), or render all and let visual overlap show the most recent. Simplest correct behavior: render only the most-recently-pushed `ActiveGcd` entry. The `parent_id` field enables this deduplication by label/parent if desired.

### Multiple AbilityQueue Timers Firing Simultaneously

If two `AbilityQueue` timers with `gcd_secs` set trigger in the same game tick (e.g., a proc and a cast in the same log line), two `ActiveGcd` entries enter the vec. This is benign — the manager prunes them on the next tick and both are short-lived.

**Design implication:** Render only the highest-remaining-time GCD bar, or cap `active_gcds` at 1 entry by overwriting on new trigger. Capping is simpler and matches user expectation (one GCD at a time).

---

## Sources

- ESO Combat Metronome GCD Tracker (2025 updates): [esoui.com](https://www.esoui.com/downloads/info2373-CombatMetronomeGCDTracker.html)
- WoW WeakAuras GCD tracking patterns: [wago.io GCD tracker](https://wago.io/HJdVCNG6W), [wago.io Spell Queue Indicator](https://wago.io/rkeEvWUyM)
- Hekili rotation addon icon queue pattern: [CurseForge](https://www.curseforge.com/wow/addons/hekili), [Arcane Intellect guide](https://arcaneintellect.com/world-of-warcraft-wow-addon-hekili/)
- SWTOR GCD breakpoints and Alacrity: [rambol.net](https://www.rambol.net/home/2017/10/24/alacrity-and-the-global-cooldown)
- SWTOR off-GCD abilities and mechanics: [SWTOR forums GCD thread](https://forums.swtor.com/topic/910575-a-question-about-gcd/)
- SWTOR ability activation queue mechanics: [SWTOR forums](https://forums.swtor.com/topic/294629-ability-activation-queue-how-it-works/)
- WoW Cooldown Manager UI (comparison reference): [Wowhead guide](https://www.wowhead.com/guide/ui/cooldown-manager-setup)
