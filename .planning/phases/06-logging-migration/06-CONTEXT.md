# Phase 6: Logging Migration - Context

**Gathered:** 2026-01-17
**Status:** Ready for planning

<domain>
## Phase Boundary

Replace all `eprintln!` calls with structured tracing macros. Configure file-based logging with rotation so logs are actually useful for end users (not stdout/stderr). Add DEBUG_LOGGING flag to enable verbose output for development.

</domain>

<decisions>
## Implementation Decisions

### Log file strategy
- Location: Config directory (`~/.config/baras/baras.log` on Linux, `%APPDATA%/baras/baras.log` on Windows)
- No subdirectory — single file since we only keep one
- Rotation: Size-based, rotate when file exceeds 10 MB
- Retention: Keep only the latest log file (delete old rotations)
- No date-based rotation — purely size-based

### Debug flag behavior
- Controlled by environment variable: `DEBUG_LOGGING=1`
- When enabled: debug + trace levels for baras crates only, info level for dependencies
- Default (flag unset): info + warn + error levels
- RUST_LOG does NOT override — single flag for simplicity

### Log level mapping
- **ERROR**: All caught errors (any error that gets handled)
- **WARN**: Degraded but working (fallback used, partial success, non-critical failure)
- **INFO**: Specific operations:
  - DSL CRUD operations
  - Log files being rotated
  - User toggling overlays on/off
  - Encounters written to files
  - Uploads to Parsely
  - Configuration option changes
- **DEBUG/TRACE**: Claude's discretion based on context

### Claude's Discretion
- Distinction between DEBUG and TRACE levels per case
- Exact tracing crate/appender to use for file rotation
- Span placement strategy

</decisions>

<specifics>
## Specific Ideas

- Users won't have the app open in a terminal (especially on Windows), so stdout/stderr logging is useless
- Only the developer needs debug logs — end users should never see them
- Keep it simple: one flag (DEBUG_LOGGING), not RUST_LOG complexity

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 06-logging-migration*
*Context gathered: 2026-01-17*
