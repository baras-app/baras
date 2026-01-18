# Phase 5: Frontend Error Handling - Context

**Gathered:** 2026-01-18
**Status:** Ready for planning

<domain>
## Phase Boundary

UI displays errors gracefully instead of freezing. Failed backend operations (Tauri IPC calls) show error feedback. Error states show actionable information. User can recover from errors without reloading application.

Note: This is about handling errors returned from backend commands, not overlay rendering (which is deterministic given valid data).

</domain>

<decisions>
## Implementation Decisions

### Error Display Style
- Use **toast notifications** for transient errors (save failed, load failed, etc.)
- Toast duration: **5-7 seconds** — enough time to read
- Toasts are **dismissible with X button**
- Toast position: **bottom-right** corner

### Error Message Content
- Messages are **user-friendly only** — no technical details exposed
- No inline retry button in toasts — user retries by repeating their action
- **Different styling for severity levels:**
  - Normal errors: standard toast styling
  - Critical errors: same toast format, **longer duration** (not modal or banner)

### Recovery Behavior
- UI state after failure: **context-dependent** (Claude decides per-action)
  - Config saves: revert to previous state
  - Form inputs: keep user's data for retry
- **Auto-retry once silently** before showing error — reduces spurious network errors
- **Retry buttons only for critical sections** — main data views, not minor widgets
- **Always log errors to console** — even in production, for debugging

### Error State Visuals
- Failed sections show: **error message + warning icon**
- Color scheme: **orange/warning colors** — noticeable but not alarming
- Form validation errors: **both border highlight AND error message below field**

### Claude's Discretion
- Exact toast component implementation
- Specific error message wording
- Which sections count as "critical" for retry buttons
- State revert vs keep logic per action type

</decisions>

<specifics>
## Specific Ideas

No specific requirements — standard error handling patterns apply.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 05-frontend-error-handling*
*Context gathered: 2026-01-18*
