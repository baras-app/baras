# v2026.3.1

## General

- Timers/Phases/Counters and Effects have improved UI handling to discriminate between built-in, modified, and user created elements
- New option to hide disabled elements in editor tabs
- Notification now appears if overlays have been moved without being saved

## Data Explorer

- Removed donut charts from the data explorer
- Reformatted NPC health table to a more compact design
- Charts now properly resize when sidebars are collapsed/expanded or tab is set to fullscreen
- Removed flashing visual artifact from ability usage tab
- Effects on the charts tab are now consolidated into a single table

## Bugfixes

- Overlays positioned at the edge of the screen are now properly assigned to the active monitor when saving
- Text alerts and audio queues now properly sync with the timer's end time
- Fixed Starparse timer import setting display target to non-existent overlay
- The `Show at` field for encounter timers is now properly evaluated
- Fixed issue where effects were not scoped to source, causing parallel applications of the same effect from multiple sources to affect tracking of each other
- Fixed issue with Huntmaster success/wipe classification
- Prevent dead NPCs from being registered in the next encounter in specific edge cases
- Timer/Phase/Counter trigger chains are now evaluated recursively on each event
