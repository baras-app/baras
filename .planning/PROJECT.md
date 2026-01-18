# BARAS

## What This Is

A combat log parser for Star Wars: The Old Republic with real-time overlays, historical analytics, and raid tools. Now with robust error handling — the UI never freezes from backend errors.

## Core Value

Fast, reliable combat analysis that doesn't crash when something unexpected happens.

## Current Milestone: v1.1 UX Polish

**Goal:** Eliminate confusion and friction in the UI — empty states, unclear controls, buried features, and workflow inconsistencies.

**Target features:**
- Helpful waiting states (replace "Unknown Session" with guidance)
- Overlay hydration with last encounter data on startup
- Redesigned live/historical indicator with explicit status and "Resume Live" action
- Profile system fixes (visibility toggle decoupling, raid frame re-render)
- Overlay customization improvements (fixed save header, live preview, descriptions)
- Effects/Encounter builder clarity (tooltips, visual hierarchy, drag-drop reordering)
- Historical session display cleanup (end time, duration, hide noise)
- Single instance enforcement
- Platform fixes (SVG font fallback, hotkey clarity)

## Previous State (v1.0 shipped)

**Shipped 2026-01-18:**
- Panic-free production code (core, backend, frontend)
- Structured logging via `tracing` with file rotation
- Toast notification system for error feedback
- ~30% clone reduction in hot paths

**Codebase:**
- 69,752 LOC Rust
- Tech stack: Tauri 2.x, Dioxus 0.7.2, tiny-skia overlay rendering
- Platforms: Windows, Linux (Wayland/X11)

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

### Active

- [ ] Empty/waiting states show helpful guidance instead of error-like displays
- [ ] Overlays hydrate with last encounter data on startup
- [ ] Live/Historical mode clearly indicated with explicit status and Resume Live action
- [ ] Profile switching decoupled from visibility toggle
- [ ] Raid frames re-render correctly on profile switch
- [ ] Move mode never persists across app restart
- [ ] Overlay customization has fixed save header and live preview
- [ ] Effects/Encounter builder has tooltips and better visual hierarchy
- [ ] Drag-drop reordering for stats lists
- [ ] Historical session shows end time and duration, hides Area/Class/Discipline
- [ ] Single instance enforcement via Tauri plugin
- [ ] Parsely upload accessible from session page
- [ ] SVG font fallback for Windows
- [ ] Hotkey platform limitations clearly explained

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
*Last updated: 2026-01-18 after starting v1.1 milestone*
