# BARAS

## What This Is

A combat log parser for Star Wars: The Old Republic with real-time overlays, historical analytics, and raid tools. Now with robust error handling — the UI never freezes from backend errors.

## Core Value

Fast, reliable combat analysis that doesn't crash when something unexpected happens.

## Current State (v1.1 shipped)

**Shipped 2026-01-18:**
- Polished UX with helpful empty states and waiting messages
- Overlay improvements: move mode reset, fixed save button, live preview, tooltips
- Profile system fixes: raid frame re-render, always-visible selector
- Editor polish: form tooltips, card sections, empty state guidance
- Session page: end time/duration for historical, Parsely upload button
- Platform fixes: single instance, Windows font, hotkey warnings

**Codebase:**
- ~46,000 LOC Rust
- Tech stack: Tauri 2.x, Dioxus 0.7.2, tiny-skia overlay rendering
- Platforms: Windows, Linux (Wayland/X11)

## Previous Milestones

**v1.0 Tech Debt Cleanup (2026-01-18):**
- Panic-free production code (core, backend, frontend)
- Structured logging via `tracing` with file rotation
- Toast notification system for error feedback
- ~30% clone reduction in hot paths

## Requirements

### Validated

- ✓ Combat log parsing and encounter tracking — existing
- ✓ Real-time overlay rendering (Wayland, X11, Windows) — existing
- ✓ Historical data analysis via DataFusion/Parquet — existing
- ✓ Timers, alerts, and boss mechanic tracking — existing
- ✓ Effect tracking (HoTs, buffs, debuffs) — existing
- ✓ Parsely.io upload integration — existing
- ✓ Panic-free production code — v1.0
- ✓ Structured logging infrastructure — v1.0
- ✓ UI error feedback (toast notifications) — v1.0
- ✓ Empty/waiting states show helpful guidance — v1.1
- ✓ Overlays hydrate with last encounter data on startup — v1.1
- ✓ Profile switching decoupled from visibility toggle — v1.1
- ✓ Raid frames re-render correctly on profile switch — v1.1
- ✓ Move mode resets on app startup — v1.1
- ✓ Overlay customization: fixed save header, live preview, tooltips — v1.1
- ✓ Effects/Encounter builder: tooltips, card sections, empty state guidance — v1.1
- ✓ Historical session: end time, duration, cleaner display — v1.1
- ✓ Single instance enforcement — v1.1
- ✓ Parsely upload on session page — v1.1
- ✓ Windows font rendering fixed — v1.1
- ✓ Hotkey platform limitations explained — v1.1

### Active

- [ ] Live/Historical mode indicator with explicit status and Resume Live action (NAV-01, NAV-02, NAV-03)
- [ ] Drag-drop reordering for stats lists (EDIT-05)

### Out of Scope

- MacOS support — platform complexity, low demand
- Mobile app — desktop focus

## Constraints

- **Rust edition**: 2024
- **Backwards compatible**: Config file format unchanged
- **No new heavy dependencies**: Prefer ecosystem-standard crates

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Remove ALL production unwraps | Clean policy, no exceptions | ✓ Good |
| Use `tracing` for logging | Industry standard, structured, async-friendly | ✓ Good |
| Error type per crate | Avoid monolithic error enum | ✓ Good |
| Toast notifications for errors | Non-blocking, auto-dismiss, stacking | ✓ Good |
| Two-pass borrow pattern | Avoid clone-before-mutate in hot paths | ✓ Good |
| Accept ~30% clone reduction | 50% target was optimistic given Rust ownership | ✓ Acceptable |

## Known Issues

- Pre-existing clippy warnings (30+) across codebase — cleanup milestone candidate
- Overlay example `new_overlays.rs` has stale API references
- Overlay test `format_number` has precision mismatch

---
*Last updated: 2026-01-18 after v1.1 milestone*
