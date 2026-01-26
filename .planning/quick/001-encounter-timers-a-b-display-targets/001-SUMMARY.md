---
phase: quick
plan: "001"
type: feature
subsystem: overlay-system
tags: [timers, overlay, dsl, frontend, backend]
dependency-graph:
  requires: []
  provides: [timer-display-target, timers-a-b-overlays]
  affects: [encounter-editor]
tech-stack:
  added: []
  patterns: [dual-overlay-routing]
key-files:
  created: []
  modified:
    - core/src/timers/definition.rs
    - core/src/timers/mod.rs
    - core/src/timers/active.rs
    - core/src/timers/manager.rs
    - core/src/dsl/definition.rs
    - types/src/lib.rs
    - overlay/src/overlays/mod.rs
    - overlay/src/overlays/timers.rs
    - app/src-tauri/src/overlay/types.rs
    - app/src-tauri/src/overlay/spawn.rs
    - app/src-tauri/src/overlay/manager.rs
    - app/src-tauri/src/overlay/state.rs
    - app/src-tauri/src/commands/overlay.rs
    - app/src-tauri/src/router.rs
    - app/src-tauri/src/service/mod.rs
    - app/src/types.rs
    - app/src/components/encounter_editor/timers.rs
    - app/src/components/settings_panel.rs
    - app/src/app.rs
decisions: []
metrics:
  duration: 12m
  completed: 2026-01-26
---

# Quick Task 001: Encounter Timers A/B Display Targets Summary

**One-liner:** Split timer overlay into TimersA/TimersB with display_target field for routing boss timers to separate overlays

## What Was Built

Added the ability to route boss encounter timers to one of two separate overlay windows (Timers A or Timers B) based on a `display_target` field on the timer definition. This allows users to organize timers visually - for example, putting defensive cooldown reminders on one overlay and mechanic timers on another.

## Implementation Details

### Core Changes
- Added `TimerDisplayTarget` enum with variants: `TimersA` (default), `TimersB`, `None`
- Added `display_target` field to `TimerDefinition` and `BossTimerDefinition` structs
- Added `display_target` field to `ActiveTimer` runtime struct
- Timers with `display_target: None` are not shown on any timer overlay (alerts only)

### Overlay Infrastructure
- Split `OverlayType::Timers` into `OverlayType::TimersA` and `OverlayType::TimersB`
- Split `OverlayData::Timers` into `OverlayData::TimersA` and `OverlayData::TimersB`
- Split `OverlayConfigUpdate::Timers` into `OverlayConfigUpdate::TimersA` and `OverlayConfigUpdate::TimersB`
- Added separate spawn functions: `create_timers_a_overlay` and `create_timers_b_overlay`
- Both overlays use the same `TimerOverlay` renderer with different namespaces

### Config Changes
- Renamed `timer_overlay` to `timers_a_overlay` with backward-compatible alias
- Added `timers_b_overlay` config
- Renamed `timer_opacity` to `timers_a_opacity` with backward-compatible alias
- Added `timers_b_opacity` config

### Service Layer
- Updated `build_timer_data_with_audio` to return separate `TimerData` for A and B
- Timer routing based on `ActiveTimer.display_target` field
- Added `TimersAUpdated` and `TimersBUpdated` overlay update events

### Frontend
- Added `TimerDisplayTarget` enum to frontend types
- Added display target dropdown to timer editor (for countdown timers only)
- Updated settings panel with separate Timers A and Timers B appearance sections
- Updated `OverlayType` enum to use `TimersA`/`TimersB`

## Backward Compatibility

- Existing timer definitions default to `display_target: TimersA`
- Existing config files use aliases (`timer_overlay` -> `timers_a_overlay`)
- The "timers" enabled key maps to TimersA
- Window namespace "baras-timers" preserved for TimersA

## Commits

| Hash | Message |
|------|---------|
| aa86412 | feat(quick-001): add TimerDisplayTarget enum and update TimerDefinition |
| 46993f5 | feat(quick-001): add TimersA/TimersB overlay types and infrastructure |
| ba07e0c | feat(quick-001): add TimerDisplayTarget to frontend and timer editor UI |
| 7098261 | feat(quick-001): update frontend OverlayType to use TimersA/TimersB |

## Deviations from Plan

None - plan executed exactly as written.

## Testing Notes

To verify:
1. Create a boss definition with multiple timers
2. Set some timers to `display_target: timers_a` and others to `display_target: timers_b`
3. Enable both Timers A and Timers B overlays
4. Trigger the boss encounter - timers should appear on their respective overlays
5. Set a timer to `display_target: none` - it should not appear on either overlay but alerts should still fire
