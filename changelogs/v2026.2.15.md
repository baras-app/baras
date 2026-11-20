# v2026.2.15

Significant quality of life improvements and updates to overlay rendering

## Quality of Life

- Old log files now show operation areas entered in the file explorer
- File selector can now filter by area name and day of week
- UI for encounter and effects editors made clearer
- Added Audio preview button in editor UI
- App now considers stale files (no updates for 15 minutes) as non-live sessions and indicates this on the session tab
- Effects editor shows a badge when a default effect has been modified

## Effects Tracker

- Effects can now be scoped to specific disciplines
- Op Healer: Medical probe and Kolto infusion only refresh Kolto probe at 2 stacks (delete any custom entry for Kolto probe)
- Op Healer: Kolto infusion only refreshes Kolto probe after cast complete
- Improve DoT tracker for Virulence sniper
- Effects should now fall off correctly for Madness shock and when entities die

## Timers

- Alert text can now be added to fire at the start/end of timers that aren't pure alerts
- Fixed issue where editing default timers did not hot-reload them within an area

## Overlays

- Timer text now renders with a full surrounding glow for better readability
- Class icons on metric overlays are now role-colored (blue/green/red) with dark shadow outline
- Class icons can now be displayed on raid frames (toggle in settings, off by default)
- SWTOR role glyphs are used on raid frames instead of procedural shapes
- Raid frames can now select any row/column combination up to 24 slots
- Fixed issue where raid frame slot count was not updating on profile switch

## Misc

- When triggering timers, phases, or counters, entities are now considered dead when they are logged at 0 HP
- Added APM column to data explorer overview table
- The death review now only filters for events where the dead player is the target
- Healing % total now includes shielding
- Inline bar formatting in the data explorer tabs improved
- Various timers added/ tweaked
