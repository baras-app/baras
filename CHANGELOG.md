# v2026.11.11

## General

- **Player threat totals are now floored at 0. Threat will no longer display as negative.**
- Added Overlay settings for toggling FPS for countdown overlays
- Light mode support improved
- Header bar reformatted
- General UI polishing

## Data Explorer

- New raid metrics chart added to overview page
- New Effects Uptime interactive page added
- Death Recap window changed to side panel instead of pop-up
- Ability name faction toggles moved to relevant pages
- Fixed bug causing timeline colors failing to appear when switching between encounters

## Encounter/Effects Editors

- Icons now visible in table
- Added basic filters and sorting to editing card
- Fixed issue preventing editor drawer from expanding
- Deprecated timer chains to and counter decremental and set value fields
- Updated formatting of all editor cards

## Boss Statistics

- Split metrics line graphs by role
- Added ability to hide pulls

## Raid Summaries

- Flashpoints now start at area entrance instead of first boss pull (requires log data to be rescanned)
- Include flashpoints is now a persistent toggle
- Add filter for successful timer runs
- Reformatted summary tables
- Added ability to hide entries
- All roles/disciplines played on a character are now visible

## Log Management

- Removed columns file name, status
- Reordered table columns
- Fixed issue causing deleted files to be marked as needing scanning

## Characters

- The difficulty filter on the lockouts tab now persists across sessions

# v2026.11.9

- Light Mode Added (beta)
- More UI Improvements

# v2026.11.7

## UI Overhaul

The UI has been significantly revamped with the intent to improve visual consistency and navigation.

Major changes:

- Auto Hide and Hide in conversations options have been moved to global options in the overlay settings menu
- Overlay settings menu has been reworked to make it easier to discover
- Effects editor sidebar added

## Character Roster and Lockout Tracker

A new tab called "Characters" contains a roster for tracking and managing your characters. Information such as server, class, and last login time is recorded automatically, other information can be manually added.

This page also serves to track lockouts across characters.

## PvP Stats

A new page has been added that displays summary statistics of PvP matches.

## Other

- Parsely upload links are now retained properly across application sessions

# v2026.11.6

## Log Management

Log file management has moved to its own page under the _Historical_ menu.

- Cleanup rules: delete logs under a size, archive logs older than N days, delete logs older than N days. A dry run shows exactly what a rule would touch before it runs.
- Retention by content: logs tagged NiM Ops, HM Ops, SM Ops, MM FP, VM FP, World Boss, PvP or Dummy can be marked as never-delete. Retained logs are archived instead once old enough.
- Archiving compresses a log into cold storage. Archived logs stay in the statistics, are still covered by scans, and can be restored from the table at any time.
- Favorites are never archived or deleted. Logs can carry tags and a one-line note, and the table searches by file, character, area, tag or note.
- The log that is open right now (live or historical) is pinned to the top of the table, highlighted, and protected from delete. An _Open log_ button clears the filters and jumps to it.
- _Scan missing_ parses only logs whose history is missing or out of date; _Rebuild all_ re-parses every log, archives included. New logs can be scanned automatically at startup.
- Parsely links survive a re-scan and travel with the file.

## Raid Summaries

A new view that shows one raid at a time: a visual timeline of pulls and the gaps between them, summary cards, and the pull list.

- The run time is always shown, from the timer start line (or the first pull) to the final kill, or to the last pull if the run was not cleared.
- Runs can be opened in the Data Explorer from the summary. Archived runs offer _Restore & open_.
- Runs can be tagged and annotated; tags also show on the pulls they cover.

## Boss Statistics

The History tab has been renamed to Boss Statistics with an easier-to-navigate view of your performance on each boss over time.

- Pulls from archived logs can be opened straight from the table: the log is restored, then the pull opens in the Data Explorer.

## Overlays

- DOT Tracker icons can now be right-aligned.
- Effects on the Effects and Cooldown overlays can optionally display their target (per effect, alongside the existing _Display Source_). Useful for taunts, guard and single-target buffs.

## Data Explorer

- Improved ability to select single instances of duplicate NPCs.
- Right-clicking a line in the combat log now offers to filter by that line's source or target, including an individual NPC instance.
- Suppressed the small 0–3 second gap between the start of the fight and the start of the first phase in the phase timeline.

## Bugfixes

- Linux users are no longer prompted for OCR permissions every time the application starts.
- Fixed the DOT Tracker mis-attributing targets when refreshing dots while multi-dotting. This primarily affected Scorch.
- Toggling ability names now works when the Cooldowns and Effects overlays are in bar mode.
- The "Saved" prompt only appears when an overlay has actually been moved in move mode.
- A battle rez cast after a brief log stall (for example a fall death) is now attributed to the local player, so death and combat state recover correctly.
- Phase, counter and victory triggers on _effect removed_ now match only the effect being removed, not the ability that removed it, matching the timer behavior.
- Damage and healing with no source (environmental) are now evaluated by triggers.

## Definitions

- Explosive Conflict: 16M ability and boss IDs added, Kephess and Minefield IDs corrected, pull-in notification restored.
- Updated definition formatting for EC, DF and DP.
- Added challenges for Countermeasure stacks in A&E and floor stacks in IP-CPT.
- Lady Dominique burn challenge now shows a per-second amount and more closely matches the Parsely challenge number.
- Propagator Core phase trigger corrected.
