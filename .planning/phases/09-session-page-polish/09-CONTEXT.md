# Phase 9: Session Page Polish - Context

**Gathered:** 2026-01-18
**Status:** Ready for planning

<domain>
## Phase Boundary

Session page shows helpful states and relevant information without noise. This includes: fixing empty state messaging, improving historical session display, adding clear live/historical indicators, and providing direct Parsely upload access. Also requires fixing log watcher behavior for empty files.

</domain>

<decisions>
## Implementation Decisions

### Empty State Messaging
- No log file watched → **"No Active Session"** (not "Unknown Session 1")
- Log file detected, no character login yet → **"Waiting for character..."**
- Character logged in → Display **character name** (available from line 2 of log file)
- Combat stats section: Already present/handled — no changes needed

### Log Watcher Behavior
- **Bug fix required:** Currently watcher only reads file once and caches character
- Empty files don't trigger re-read, causing stale character display
- **Expected:** Re-read empty/short files on subsequent polls until character data found
- When new empty log file becomes latest → show "Waiting for character..." until character logs in

### Historical Session Display
- Header format: **Character | Started | Ended | Duration**
- Duration format: Short form (e.g., "47m" or "1h 23m")
- **Completely hide** Area, Class, and Discipline for historical sessions (cleaner, less noise)
- Subtle background tint to distinguish from live sessions

### Live vs Historical Indicator
- Placement: **Both** session header area AND top navigation bar
- **Live indicator:** Green arrow/play icon
- **Historical indicator:** Pause icon in yellow/amber color

### Parsely Upload Access
- Button location: Session header area
- Available for: **Both** live and historical sessions
- Click behavior: Immediate upload (no confirmation dialog)
- Feedback: Toast notification with result and link to uploaded log

### Claude's Discretion
- Exact tint color/opacity for historical sessions
- Specific icon designs (within the play/pause concept)
- Toast notification styling and duration
- Progress indicator during upload (if any before toast)

</decisions>

<specifics>
## Specific Ideas

- Character name is available from line 2 of log file at login event — use this to populate session display early
- SWTOR creates new empty log files for new sessions — watcher must handle this pattern

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 09-session-page-polish*
*Context gathered: 2026-01-18*
