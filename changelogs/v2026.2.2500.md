# v2026.2.2500

## Timers

**Creating timers is a lot of work and it's easy to make mistakes, if you see something off please
report it so we can update the defaults**

- Timer conditions system has been overhauled to support full boolean composition
- Optional icon field has been added to timers (ability/effect icons), defaults to none
- Apex Vanguard Mass Target Lock P4 timer no longer fires on combat start
- XR-53 timers have been overhauled by Keetsune. Several are disabled by default on his recommendation.
- Timers for all bosses in Gods have been updated. Thank you to Error for working on this.

## Encounter Classification

- Non-battle revives of the local player will now automatically end the combat encounter
- Encounters ended by exiting the Area are now classified as wipes unless there is a known special condition (e.g. TFB excluded)
- Healing actions are ignored when evaluating fallback encounter timeout
- Huntmaster wipe/clear classification improved
- Izax wipe/clear classification corrected

## Data Explorer

- Increased font size of ability usage tab
- Combat log IDs now show before the associated name
- Combat log _Filter_ text now persists when navigating across encounters

## Performance

- Cache data-explorer icons to reduce memory pressure from frequent lookups
