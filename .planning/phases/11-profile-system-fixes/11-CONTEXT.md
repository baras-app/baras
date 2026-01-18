# Phase 11: Profile System Fixes - Context

**Gathered:** 2026-01-18
**Status:** Ready for planning

<domain>
## Phase Boundary

Profile switching works reliably without side effects. Visibility stays independent of profiles, raid frames re-render correctly after switch, and profile selector is always accessible. No new profile features — just fixing existing behavior.

</domain>

<decisions>
## Implementation Decisions

### Visibility Decoupling
- Visibility is NOT a profile attribute — it's global state
- Profiles should not have a visibility field at all
- Migration: Remove visibility from profile schema, reference global config value
- No visual feedback needed when visibility changes (the change is obvious)

### Switch Transition
- Instant swap — no animation or loading state
- Prompt if unsaved changes: offer Save / Discard / Cancel
- Show brief toast after switch: "Switched to [Profile Name]"

### Selector Placement
- Keep current placement in overlay settings
- Always visible even when no profiles exist
- Empty state: Show "Default" label + create button
- Selector only switches profiles — rename/delete handled in existing management area

### Invalid State Handling
- Warning toast if profile has missing/stale references: "Some settings couldn't be loaded"
- Continue with defaults rather than blocking

### Raid Frame Bug Fix
- Known bug: After profile switch, raid frames stop receiving updates (effects, new members)
- Workaround: Toggling frames off/on fixes it until next switch
- Root cause appears to be subscriptions/listeners not reconnecting after switch
- Fix must ensure subscriptions are properly refreshed on profile switch

### Claude's Discretion
- Exact toast duration and styling
- Technical approach to subscription refresh
- Migration implementation details for visibility removal

</decisions>

<specifics>
## Specific Ideas

- Raid frame bug: frames appear but don't update with new effects or new raid members after switch
- Fix pattern: whatever toggling off/on does, the switch should do automatically

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 11-profile-system-fixes*
*Context gathered: 2026-01-18*
