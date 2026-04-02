# v2026.3.23001 Hotfix

- Huntmaster clears will now properly detect encounter end

# v2026.3.23

- Operations timer has been overhauled and now works with flashpoints and PvP areas.
- Added parsely upload option to tag all guild members
- Overhauled the entity shield object to be more robust and support multiple group size and difficulty configurations
- Boss timers can now have visibility toggled via a show/hide all
- Challenges, phases, shields, and HP markers can now be difficulty-scoped

## Definitions

- Added definition files for additional flashpoints
- Vorgath now has phases for each demo droid
- Replaced all God's timers with Keetsune's set of timers
- Updated TFB (boss) to use correct IDs

## Overlays

- Added icon support for alerts overlay
- Added visual previews in move mode for boss HP, timers, and alerts overlays

## Bugfixes

- Boss definitions now properly load for second boss when fighting two bosses back to back in different areas
- Fixed race condition causing overlays to render incorrectly when auto-unhide and profile swaps triggered simultaneously
- Fixed issue with data explorer encounter time showing incorrect final value
- Operations timer now respects the final boss flag
- Overlays should now no longer flash on/off when autohide is enabled and game starts
- Fixed issue causing Huntmaster victory trigger to fail to fire in specific edge cases
- Challenges overlay will now clear on the next encounter
- Fixed issue causing effects audio to fail to fire when refreshing effects with specific triggers
- Uploading encounter data for training dummies will now capture all logs up to 5 seconds after the encounter is recorded as over.
