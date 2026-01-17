# Phase 4: Backend Error Handling - Context

**Gathered:** 2026-01-17
**Status:** Ready for planning

<domain>
## Phase Boundary

Tauri commands return errors to frontend instead of panicking. All `.unwrap()` and `.expect()` calls in `app/src-tauri/src` are replaced with proper error handling that returns `Result<T, String>` to the frontend. The frontend receives actionable error messages.

</domain>

<decisions>
## Implementation Decisions

### Error message format
- Technical details in debug builds, user-friendly messages in release
- Backend logs full error chain; frontend gets clean message only (no source chain)
- Errors sent over IPC as simple `Result<T, String>` — no structured JSON
- English only, no localization keys

### Error categorization
- No explicit error categories — all errors are strings
- Category is implicit in message text
- Validation errors are the exception: include field name in message
  - Format: "Invalid value for 'field_name': reason"
- No distinction between transient and permanent errors

### Logging behavior
- Log only on errors, not on success
- Error message clarity should be sufficient without command name prefix
- No redaction needed — local desktop app, user owns their logs

### Claude's Discretion
- Appropriate logging level per error type (error vs warn)
- Whether to include command name in tracing spans

### Recovery hints
- No guidance in error messages — just state what went wrong
- Internal errors use honest wording: "An internal error occurred"
- No "contact support" or "report bug" suggestions
- No partial completion state in error messages

</decisions>

<specifics>
## Specific Ideas

- Keep error messages simple and honest
- Validation errors should help users fix their input by naming the field
- This is a local app — don't overthink security/redaction

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 04-backend-error-handling*
*Context gathered: 2026-01-17*
