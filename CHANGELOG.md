# v2026.4.15

## Overlays

- Added ability to display tracked effects on multiple target overlays
- Effects can now be displayed on the Boss HP Overlay
- Effects A/B and Cooldown overlays now have a `Bar Mode` option to display them in a traditional bar format
- Metrics overlays now have the option to display discipline icons
- Metrics overlays now have an option to color individual bars colored based on the player's class
- Added experimental Ability Queue overlay for tracking upcoming ability casts of bosses non-static rotations
- The local player's entry in metrics overlays is now bolded

## Bugfixes

- Role based profile switching should now behave properly
- Fixed issue causing DoTs failing to refresh in tracker when recast close to expiration time
- Changes to the global audio volume setting should now properly update without requiring the application to be restarted
- Updated parser to prevent panic from occurring in rare cases where SWTOR writes invalid log output
- Trailing metric events will now accumulate in challenges

## Other

- Added optional mitigation field for `DamageTaken` trigger type
- Added `Threat Modified` encounter trigger type
- Minor tweaks to fight definitions
- Users can now resize the width of columns in the combat log by dragging the headers to the left or right
- The challenge system can now track player interrupts on specific abilities
