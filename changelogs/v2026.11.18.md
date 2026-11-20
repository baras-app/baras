# v2026.11.18

## Performance Improvements

- 40-60% reduction in time to parse historical log files
- Improved UI responsiveness in analysis pane

## Overlay Customization

- New global overlay options to set:
  - background opacity
  - background color
  - border thickness
  - border color
  - degree of edge rounding
  - font (already present, but can now be set individually)

These settings can now be set once for all overlays. However, each individual window has the option to override the global settings with an overlay-specific option for these settings.

## Metric Value Templates

- Metric overlays can now be formatted using a template system. Users can use a preset format or customize what metrics are displayed and how they are displayed.

## UI

- Added table showing buffs/debuffs leading up to the player's death in the death recap
- Log management value boxes now update with the filters
- Added overview/landing page to raid summaries
- Boss statistics tables now contain difficulty column

## Bugfix

- Boss Statistics difficulty filter now works for users on French and German clients
- Effect trackers created using the ability cast trigger will now properly resolve the target of the ability cast
