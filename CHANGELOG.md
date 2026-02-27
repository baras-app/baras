# v2026.2.2600

**Operations Timer** a new timer feature has been added that starts a time countdown from either the first boss pull or when the user triggers it.
This can be a great help when trying to track timer runs! Note: as of now, it must be manually stopped once it starts running.

The last window size and position should now be remembered upon re-opening the application.

## UI

- The session page has been removed; relevant information has been rolled into a persistent header panel
- Encounter-specific parsely upload, challenges, and enemy lists are now displayed in the data explorer
- Discipline specific icons are now used in the UI instead of the class icons
- Effects tab renamed to Effects Editor
- Time range selection clear button is now red

## Log Management

- Log indexing happens in a background process to prevent hanging when large amounts of log files are present

## Timers

- Fixed bug where compound cancel trigger were being evaluated via invalid codepath
- Definitions now hot reload in-game when changes are made to phases or variable counters
- Unified trigger handling logic across all objects
- Added `Timer Time Remaining` condition type
- Added `Timer Canceled` trigger type

## Effects

- Fixed issue causing medical probe to add a kolto probe stack
- Fixed issue where other modified charges events of other players were being attributed to the local player if
  same ability was cast on the same target
- Fixed issue with Kolto Probes being refreshed erroneously if ability cast shortly after they expired
- Fixed issue with Kolto Shells / Trauma Probes double registering if the "Other's" effects were activated

## Misc

- Removed "Imperfect Construct" from XR-53 boss definition fallback list
- Process monitor for `swtor.exe` now runs and is used to ensure prior session indicator stays in-sync
