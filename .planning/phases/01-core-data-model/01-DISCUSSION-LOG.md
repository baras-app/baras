# Phase 1: Core Data Model - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-04-11
**Phase:** 01-core-data-model
**Areas discussed:** ActiveGcd struct fields, GCD re-fire policy, queue_priority type & direction, queue_remove_trigger type

---

## ActiveGcd struct fields

| Option | Description | Selected |
|--------|-------------|----------|
| Timing only | `started_at` + `expires_at` only. No name, no color — overlay uses config accent color. | ✓ |
| Timing + definition_id | Add `definition_id: String` for overlay to correlate back to the timer definition. | |
| Timing + name + color | Cache `name` and `color` from definition for richer overlay display. | |

**User's choice:** Timing only
**Notes:** Overlay will use its configurable accent color; no label needed from the struct.

---

## GCD re-fire policy

| Option | Description | Selected |
|--------|-------------|----------|
| Replace | Drop existing ActiveGcd, start fresh on re-fire. GCD resets to full duration. | ✓ |
| Ignore if active | Discard new fire if GCD is running; original finishes undisturbed. | |

**User's choice:** Replace
**Notes:** Matches SWTOR GCD behavior. Also implies field type is `Option<ActiveGcd>` (single slot), not `Vec<ActiveGcd>`.

---

## queue_priority type & direction

| Option | Description | Selected |
|--------|-------------|----------|
| u8, lower = higher priority | 0 = first in list, 255 = last. | |
| u32, lower = higher priority | Wider range, same semantics. | |
| u8, higher = higher priority | 255 = first in list, 0 = last. | ✓ |

**User's choice:** u8, higher = higher priority (255 = top of tier 2, 0 = bottom)
**Notes:** Default via `#[serde(default)]` = 0.

---

## queue_remove_trigger type

| Option | Description | Selected |
|--------|-------------|----------|
| Option\<Trigger\> | Full DSL type, consistent with `cancel_trigger`. Stubbed no-op in v1. | ✓ |
| Option\<String\> | Minimal placeholder; would require breaking TOML format change in v2. | |

**User's choice:** Option\<Trigger\>
**Notes:** Use `#[serde(default, skip_serializing_if = "Option::is_none")]`.

---
