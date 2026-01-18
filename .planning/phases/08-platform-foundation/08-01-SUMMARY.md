---
phase: 08-platform-foundation
plan: 01
status: complete
completed: 2026-01-18
---

# Plan 08-01 Summary: Single Instance + Hotkey Warnings

## What Was Built

Implemented single instance enforcement for the BARAS application and enhanced hotkey limitation warnings for Linux/Wayland users.

## Deliverables

| Task | Files Modified | Commit |
|------|----------------|--------|
| Add single instance plugin | Cargo.toml, lib.rs | `1158dcd` |
| Enhance hotkey limitation warning | app.rs | `d1b58f9` |

## Key Implementation Details

1. **Single Instance Enforcement**
   - Added `tauri-plugin-single-instance = "2"` dependency
   - Registered plugin as FIRST plugin in builder chain
   - On duplicate launch: shows window, unminimizes, and focuses existing instance

2. **Hotkey Limitation Warning**
   - Added prominent warning with exclamation icon in hotkey settings
   - Clear text: "Global hotkeys are Windows-only. Linux and Wayland do not support global hotkeys due to security restrictions."

## Deviations

None.

## Notes

The single instance plugin callback pattern matches existing tray.rs conventions for window management.
