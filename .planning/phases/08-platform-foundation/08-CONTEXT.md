# Phase 8: Platform Foundation - Context

**Gathered:** 2026-01-18
**Status:** Ready for planning

<domain>
## Phase Boundary

Infrastructure and platform-specific fixes that enable a cleaner user experience. Includes single instance enforcement, Windows font rendering fix, Wayland hotkey documentation, and Alacrity/Latency field discoverability.

</domain>

<decisions>
## Implementation Decisions

### Single Instance Behavior
- Focus existing window silently when second instance launched (no notification)
- Attempt to restore from tray if window is minimized/hidden
- No visual feedback (flash/pulse) when focusing
- Enforce on all platforms (Windows and Linux/Wayland)

### Alacrity/Latency Fields
- Move from current location to session page
- Position near player name/class info area
- Editable inline (not view-only)
- Brief tooltip explaining purpose (one-liner: "Your alacrity percentage for GCD calculations")

### Windows Font Rendering
- Issue: TTF file not rendering correctly on Windows via Tauri/webview
- Font works correctly on Linux
- Must get original font working cross-platform (no fallback to substitute fonts)
- Root cause believed to be Tauri/webview handling

### Claude's Discretion
- Hotkey limitation UX for Wayland/Linux (user didn't select for discussion)
- Exact tooltip wording
- Technical approach to single-instance implementation per platform
- Investigation approach for Windows font issue

</decisions>

<specifics>
## Specific Ideas

- User strongly prefers getting the original TTF font working everywhere rather than substituting
- Alacrity/Latency should feel contextual to the session, not buried in settings

</specifics>

<deferred>
## Deferred Ideas

None - discussion stayed within phase scope

</deferred>

---

*Phase: 08-platform-foundation*
*Context gathered: 2026-01-18*
