# Phase 12: Overlay Improvements - Context

**Gathered:** 2026-01-18
**Status:** Ready for planning

<domain>
## Phase Boundary

Make overlay customization intuitive through better controls, immediate feedback, and sensible defaults. This phase improves the settings experience for existing overlays — controls, feedback, startup behavior. It does NOT add new overlay types, new metrics, or new customization options.

</domain>

<decisions>
## Implementation Decisions

### Live preview behavior
- Debounced preview (~300ms after typing stops)
- All settings preview live (colors, fonts, sizes, positions)
- Confirm discard on cancel/navigate away with unsaved changes
- Save button changes appearance when there are unsaved changes

### Button descriptions
- Tooltips on hover (not inline text)
- Non-metric overlay buttons only (Boss Frame, Raid Frames, etc.)
- Functional tone (e.g., "Displays boss health and cast bar")

### Move mode reset
- Reset on app startup only (not on save/cancel)
- Reset on profile switch
- No additional indicator needed — overlay display format makes it obvious

### Startup data display
- Show last encounter data on startup
- If no data exists, render overlays empty (no placeholders, no sample data)
- No loading states — overlays are passive renderers that display whatever data they receive

### Descoped requirements
- OVLY-05 (REARRANGE FRAMES / CLEAR FRAMES grouping) — keep current location

### Claude's Discretion
- Exact debounce timing
- Save button visual change (color, text, or both)
- Which specific buttons get tooltips

</decisions>

<specifics>
## Specific Ideas

- Overlays are passive — they have no internal logic (except raid frames). They just render what they receive.
- Move mode is visually obvious from overlay display format, so no badge/indicator needed in settings.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 12-overlay-improvements*
*Context gathered: 2026-01-18*
