# v2026.6.22

## Overlay Formatting

- **Gradients** - Gradient bars have been added and enabled by default on metrics overlays. This option can be toggled on/off in the overlay customization menu.
  It is also present on bar mode effects overlays, timers, and boss HP, but not as a default.
- **Bar Spacing** - Spacing between bars on metrics overlays can now be adjusted in the overlay customization menu.
- **Overlay Font Selection** - Users can now select any of their available system fonts as the font face for the overlays.

Enjoy customizing.

## Other

- Refresh on immune toggle option added for effects tracking
- Shielding and SPS columns have been condensed into to the HPS/EHPS columns in the data explorer. Blue-text coloring is used to distinguish between shielding from healing.
- Various additions to HP markers, alert text rewording in encounter timers
- Added entry for tracking group revive cooldowns
- Log file search now supports compound terms using the comma (e.g. `Player Name, Dxun` will find all Dxuns for `Player Name`)

## Bugfixes

- Fixed issue causing duration modifiers that add time to improperly render total duration in the overlay
- Removed Sustaining Aura/Fueled Corruption from tracked effects
- Challenges overlay will now display class/discipline icons when parsed from a historical file
