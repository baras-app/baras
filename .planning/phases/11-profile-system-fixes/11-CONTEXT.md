# Phase 11: Profile System Fixes - Context

**Gathered:** 2026-01-18
**Status:** Ready for planning

<domain>
## Phase Boundary

Profile switching works reliably without side effects. Raid frames re-render correctly after switch or settings save, and profile selector is always accessible. No new profile features — just fixing existing behavior.

**Note:** Visibility decoupling (PROF-01) already implemented — remove from scope.

</domain>

<decisions>
## Implementation Decisions

### Raid Frames Re-render (Primary Focus)
- **Bug symptoms:** Frames disappear on profile switch AND on settings save
- **Effects/HOT tracking also breaks** after these actions
- **Recovery workaround:** Toggling visibility off/on brings frames back
- **Root cause:** Render state not being refreshed properly — data likely still present

**Expected behavior:**
- Profile switch: settings apply instantly, current raid group stays visible
- Settings save: identical behavior to profile switch
- No frame disappearance, no visibility toggle workaround needed
- Effects/HOT tracking continues working seamlessly

### Selector Placement (PROF-03)
- Keep current placement in overlay settings
- Always visible even when no profiles exist
- Empty state: Show "Default" label + create button
- Selector only switches profiles — rename/delete handled in existing management area

### Claude's Discretion
- Technical approach to triggering the re-render fix
- Whether to batch multiple setting changes before redraw
- Internal state management patterns

</decisions>

<specifics>
## Specific Ideas

- "Instant switch, preserve players" — user expects seamless transition
- The visibility toggle workaround is the key clue — whatever that does internally, profile switch and save should do the same automatically
- Fix pattern: ensure proper redraw/refresh is triggered without requiring manual visibility toggle

</specifics>

<deferred>
## Deferred Ideas

- Switch transition animations/toasts — not discussed, keep simple
- Unsaved changes prompts — not discussed, handle during planning if needed

</deferred>

---

*Phase: 11-profile-system-fixes*
*Context gathered: 2026-01-18 (updated)*
