# v2026.11.16

## Phase Awareness

BARAS now has the ability to detect a specific phase instance and account for common events during raids such as:

- swapping characters to save a lockout
- resetting a phase for a timer run after wiping on the first boss
- disconnecting mid-raid

What this means is that the **Raid Summaries** and the **Operations Timer** now account for all these events when determining the length of a raid. Seeing these changes requires the logs to be re-scanned.

## Raid Summaries

- Improved formatting and searchability of the page, now lands on a table view with operation selection in the sidebar
- Raids are no longer marked as cleared unless all mandatory bosses were killed within the run

## Log Management

- Added filter to display only logs that require scanning
- Calendar chart now distinguishes between archived and unarchived logs, updates in response to filters

## Characters

Improved search options in the lockouts menu. You can now toggle between searching for characters who are locked out versus free. Lockouts for specific bosses can also be selected in the filter options.

## Alacrity Presets

Users can now add preset alacrity values for specific disciplines, or a specific discipline/character combination.

## Improved Disconnection Handling

The application will now attempt to restore the session information upon the next combat encounter after a disconnect. What this means is that timers, challenges, and the HP overlay will no longer fail to display information after a disconnect without requiring user action.

## Timer Preview Mode

You can now preview timers, alerts, and their associated audio queues from the editor menu.

## Other

- Definitions: Huntmaster clear detection logic will no longer classify wipes to NiM Shelleigh as a kill
- Definitions: Improved detection of Apex Vanguard boss resetting
- Linux: Enabled XWayland fallback on Wayland DEs that don't support layer-shell (GNOME, etc)
- Analysis: death recap window no longer covers application navbar
- Analysis: Encounter select sidebar now displays the time elapsed between the pull and the area being entered, instead of the wall clock time
- Overlays: Auto-Hide while not live should now properly work with the DX12 SWTOR client
