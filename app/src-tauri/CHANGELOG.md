# v2026.1.1400

## Effects and Icon Support

The effects system has been updated to display SWTOR icons on 3 different overlay types:

- Effects A/B
- DOT/Debuff Tracker
- Cooldown Tracker

- Effects are fully wired into the alert and audio system
- Removed redundant "group member" entity filters
- Added "AnyExceptLocal" and "CurrentTarget" entity filters
- Added parameters for adjusting effect duration accounting for alacrity and sever lag

## General

- File browser now shows up-to-date log file sizes
- Added toggle to hide small files from the file browser
- Ability icons now display in data explorer
- NPCs now display on encounter history table

## Encounter Timers

- Added Timers, Phases, and Challenges for:
  - Brontes
  - SnV (\*Styrak only has knockback timers)
- Add phases for:
  - Apex Vanguard
- Dxun 2 & 3 are now detected properly
- Removed "void_zone" timers from Firebrand and Stormcaller
- Fixed TFB Kephess alerts firing on non-local player

## Bugfixes

- Timers with no difficulty setting will no longer appear
- Rate calculation in Damage/Healing + taken data explorer tabs now recognizes selected phase time
- Trash encounters are no longer all classified as "Open World"
- Bosses now show in the correct area when the Bosses Only filter is applied

---
