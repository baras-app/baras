# v2026.11.13

## Personal Stats Overlay Rework

The personal stats overlay has been reworked to reduce text clipping and scaling issues. If this overlay is enabled, it's appearance will chance and you will have to edit it in the settings menu.

- Scaling has been changed to width-only. Lengthening the overlay will no longer increase the size of an individual row element
- Max columns for each element is now 3 instead of 4.
- The following optional items have been added:
  - Crit %
  - Healing % (effective and critical)
  - Operations timer
  - Healing taken
- Difficulty no longer displays when in open world
- Difficulty strings have been truncated
- All text is now left-aligned

## Keyboard Navigation Shortcuts

Several in-app keyboard shortcuts have been added to streamline app navigation. You can see the list of them in the application settings page.

## Data Explorer

### Death Recap Improvements

- Death recap display buttons now appears in the overview summary table next to player name
- Deaths now appear on the overview charts
- A new chart has been added to visualize player death status over time
- Added controls to navigate to the next/previous death within the death recap window
- Added the ability to filter the final events table for damage or healing only

### Other

- Reduced visual noise caused by chart interactivity options while hovering over graphs
- Improved chart tooltip formatting
- Combat log event type filter has been changed to a multi select drop down
- Added Modify Charges, Modify Threat, and Target Set as filterable combat log event types

## Encounter Definitions

Minor Cosmetic Timer Improvements:

- Huntmaster
- Olok
- Cartel Warlords
- Zorn and Toth
- Firebrand and Storm Caller
- Warlord Kephess
- IP-CPT

Significant changes and additions:

- Karagga's Palace
- Eternity Vault

## Bugfixes

- Threat metrics overlay now respects the 0 threat floor
- Overlays now respect the FPS settings properly
- Increased size of stack count in effects overlay bar mode
- Fixed various text clipping and truncation issues in overlay element titles
- Data explorer charts now respond properly to window resizing
