# v2026.8.29

## Effects Tracking

- Added Effects C and Cooldowns B overlays
- New option **Show Inactive** shows a greyed-out version of the icon/bar in a stable location on the screen that will fill in when the effect is Inactive. (pair with a 0 duration to track persistent effects such as Guard)
- Show countdown/stacks and other display options can now be toggled individually per effect instead of per overlay
- Kolto Shells, Kotlo Probe and Mirror abilities now show stack count at 1
- Raid frames now has an option to show a colored border that remains as the duration counts down
- Effects max icon/bar size increased

## Boss HP Bar

- HP bar formatting updated to be more readable.
- Additional scaling options and toggles have been added

## General

- Countdown bars for timers and effects now end at the rightmost edge of the icon instead of the icon obscuring visibility or the bar. Entries without icons are given a place-holder diamond glyph.

## Bugfixes

- Fixed improper data filtering when double clicking the timeline element to select a phase
- File selector area badges should now display properly for DE/FR localizations
- The metrics overlay "max entries" option now selects the top N entries across both teams in PvP zones, instead of the top N on your team
- Opening files where the final fight ended in a logout/disconnect will no longer trigger live parsing in historical mode
- DOT tracker bar mode will now use the display text field, if it is present
