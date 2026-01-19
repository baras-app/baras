# v2026.1.1900

## New Features

- macOS support (experimental)
- X11 support for Linux

## Under the hood

- Application error logs + major stability improvements
- Parsing time significantly reduced for larger files

## Improvements

- Session page polish with loading indicators and empty states
- Profile selector always visible with improved empty state
- Overlay settings live preview
- Overlay button tooltips
- Effect editor card-based UI with tooltips
- Alacrity/latency parameters moved to the session page

## Overlays

- Overlay customization previews are now visible immediately without saving
- Class icons can now be displayed on metrics overlays
- Effective damage visually shows boss/non-boss DPS splits
- Raw healing visually shows total/effective HPS splits
- Effective healing now shows shielding

## Bugfixes

- The application can now only have one instance open at at time
- Changing overlay profiles no longer changes overlay visibility
- Raid frames properly re-render after profiles are changed
- Timers now load on the first encounter when application is restarted within an area
- Overlays now display the latest combat encounter automatically when a file is opened
- Data explorer race conditions and formatting fixed
- Combat log scroll resets when new encounter selected

## Timers/Definitions

- Shelleigh is now counted to boss DPS in Huntmaster
- XR-53 digestive enzyme and Revan force bond no longer contribute to player DPS
- Corruptor Zero timer for first gravity field added
- Vorgath boss encounter is now properly detected on story mode
- Revanite Commanders now appear on boss healthbar
