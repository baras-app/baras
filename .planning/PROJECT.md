# BARAS Tech Debt Cleanup

## What This Is

A technical debt reduction milestone for BARAS, a combat log parser for Star Wars: The Old Republic. This work eliminates production panics that freeze the UI, adds proper error handling and logging infrastructure, and cleans up excessive cloning patterns.

## Core Value

Users never see a frozen UI from a panic. Errors are caught, logged, and communicated gracefully.

## Requirements

### Validated

- ✓ Combat log parsing and encounter tracking — existing
- ✓ Real-time overlay rendering (Wayland, X11, Windows) — existing
- ✓ Historical data analysis via DataFusion/Parquet — existing
- ✓ Timers, alerts, and boss mechanic tracking — existing
- ✓ Effect tracking (HoTs, buffs, debuffs) — existing
- ✓ Parsely.io upload integration — existing

### Active

- [ ] Eliminate all `.unwrap()` calls in production code (tests excluded)
- [ ] Proper error propagation from Tauri backend to UI
- [ ] UI error feedback components (not frozen screens)
- [ ] Structured logging infrastructure (errors + debug)
- [ ] Migrate existing 382 `eprintln!` calls to unified logging
- [ ] Reduce unnecessary `.clone()` calls in hot paths

### Out of Scope

- New features — this is debt reduction only
- Performance optimization beyond clone cleanup — separate milestone
- Test coverage expansion — separate milestone
- Platform-specific overlay refactoring — separate milestone

## Context

**Current state (from codebase analysis):**
- 320+ `.unwrap()`/`.expect()` calls in production code
- 200+ unwraps in frontend JS interop (charts_panel.rs, data_explorer.rs, api.rs)
- 382 `eprintln!` calls with no structured logging
- Clone-heavy hot paths: phase.rs (35), timers/manager.rs (36), effects/tracker.rs (28)

**Key problem areas:**
- `core/src/signal_processor/` — phase.rs, counter.rs, processor.rs
- `app/src/components/` — charts_panel.rs, data_explorer.rs
- `app/src/api.rs` — JS interop
- `app/src-tauri/src/service/` — backend service layer

**Architecture:**
- Tauri 2.x backend with Dioxus 0.7.2 frontend
- When backend panics, IPC breaks, UI appears frozen
- No recovery path — users must reload

## Constraints

- **Rust edition**: 2024 — use modern error handling patterns
- **No new dependencies**: Prefer `thiserror` + `tracing` which align with ecosystem
- **Backwards compatible**: Config file format unchanged
- **No UI redesign**: Error display should integrate with existing UI patterns

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Remove ALL production unwraps | Clean policy, no exceptions for "can't fail" | — Pending |
| Use `tracing` for logging | Industry standard, structured, async-friendly | — Pending |
| Error type per crate | Avoid monolithic error enum | — Pending |

---
*Last updated: 2026-01-17 after initialization*
