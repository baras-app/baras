# v2026.4.2

## Windows Monitor Identifier Update

The configuration format for identifying the display monitor on Windows has changed. The old format was not stable across all hardware setups and causing overlays to appear on the incorrect monitor for many users.

This is a breaking change that will require users to resave their overlay profiles to update them to the new IDs. Profiles with the old monitor identifier will display overlays on the primary monitor by default.

## Features

- Added ability to display HP markers conditionally

## Timers & Definitions

- Updated Gods timers to use Keetsune's files (for real this time)
- Updated DP timers
- Added HP markers for EC Z&T and Tanks
- Updated RR timers to all enabled by default
- Added Colossal Monolith Timers

## Bugfixes

- Stack counts no longer display on the DoT tracker
- Filtering the combat log for player deaths now displays up to +1.5 seconds after death
- Effect removal triggers now process even when there's no source
- Other minor fixes to trigger detection logic
