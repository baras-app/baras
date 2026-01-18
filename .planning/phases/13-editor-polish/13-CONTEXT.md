# Phase 13: Editor Polish - Context

**Gathered:** 2026-01-18
**Status:** Ready for planning

<domain>
## Phase Boundary

Make Effects and Encounter Builder editors intuitive with clear controls, guidance, and efficient interactions. Form fields get tooltips, sections have clear visual hierarchy, and interaction patterns are streamlined.

</domain>

<decisions>
## Implementation Decisions

### Form Field Tooltips
- Functional tone ("Shows when target drops below 30% health" — direct, task-focused)
- Icon hover pattern — small ? icon next to field label, tooltip on hover/click
- Ambiguous fields only — Display Target, Trigger, Comparison, etc. (not every field)
- No examples in tooltips — keep them brief, just explain purpose

### Visual Hierarchy
- Card-based sections — each section in distinct card with header for clear separation
- No collapsible sections — all sections visible at once
- Section grouping for Effects editor:
  1. Identity/Display (name, display_text, icon, color, display_target)
  2. Trigger logic
  3. Options
  4. Alerts
  5. Audio
- Icons + text for card headers — small icon before each header for quick scanning

### Interaction Patterns
- New effects/timers appear at top of list (most recent visible immediately)
- Combat log table scroll resets on encounter select (not on ability navigation)
- Raw file path removed from Encounter Builder entirely (just remove it, no replacement)
- Drag-and-drop NOT feasible — Dioxus lacks support, keep existing up/down buttons

### Claude's Discretion
- Which specific fields need tooltips (identify during implementation)
- Exact tooltip wording
- Card styling details (padding, borders, shadows)
- Icon choices for section headers

</decisions>

<specifics>
## Specific Ideas

No specific requirements — open to standard approaches that match existing UI patterns.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 13-editor-polish*
*Context gathered: 2026-01-18*
