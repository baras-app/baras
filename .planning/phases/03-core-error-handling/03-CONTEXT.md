# Phase 3: Core Error Handling - Context

**Gathered:** 2026-01-17
**Status:** Ready for planning

<domain>
## Phase Boundary

Core crate returns Results instead of panicking. Replace all `.unwrap()` and `.expect()` calls in `core/src` (tests excluded) with proper error handling. Functions that can fail return Result with appropriate error types from Phase 2.

</domain>

<decisions>
## Implementation Decisions

### Missing Encounter Handling
- When signal processor code runs without an active encounter: **warn log + early return**
- This is unexpected state during active processing, but shouldn't crash the app
- Boss index lookups that fail: **error log + early return** (more severe than missing encounter)

### Error Propagation Strategy
- **Mixed approach:**
  - Event handlers (signal processing callbacks): early return at site, no signature changes
  - Public API functions (load_config, save_preferences, etc.): propagate Result to caller
- Rationale: Internal event handling is forgiving, external interfaces are explicit

### Config Save Failures
- When config save fails: **propagate error to frontend** (user sees "Settings couldn't be saved")
- On save failure: **rollback in-memory state** — keeps memory consistent with disk
- Timer preferences: **same treatment** — consistent behavior across all persistent settings

### Invalid State Recovery
- "Should never happen" None states: **error log + skip operation, continue running**
- Error messages: **include context** (effect ID, line number, operation name for debugging)
- Don't panic on invariant violations — log at error level and recover gracefully

### Claude's Discretion
- Exact wording of log messages
- Whether specific cases warrant warn vs error level (within the guidelines above)
- Helper function extraction for repeated early-return patterns

</decisions>

<specifics>
## Specific Ideas

No specific requirements — standard Rust error handling patterns apply.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 03-core-error-handling*
*Context gathered: 2026-01-17*
