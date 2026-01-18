# Phase 7: Clone Cleanup - Context

**Gathered:** 2026-01-17
**Status:** Ready for planning

<domain>
## Phase Boundary

Reduce unnecessary `.clone()` calls in hot paths (signal_processor/phase.rs, timers/manager.rs, effects/tracker.rs) to improve parsing performance and reduce memory allocation pressure. This is optimization work — no functional changes.

</domain>

<decisions>
## Implementation Decisions

### Scope priority
- Equal treatment across all three hot path files — 50% reduction target each
- Claude's discretion to include adjacent files if clones are obviously wasteful
- Group plans by pattern (String clones, Vec clones, etc.) rather than one plan per file
- Let research discover specific wasteful patterns

### Removal strategy
- Moderate aggressiveness — remove obvious waste + simple lifetime changes that don't ripple far
- Lifetime parameters acceptable if contained to one file/module
- Prefer `&str` over `String` in struct fields for temporary/processing data
- Restructure hot loops if there's a clear win for per-event allocations

### Safety margins
- Keep defensive clones at public API boundaries, remove only internal ones
- Prefer `.iter()` and references over cloning during iteration
- Restructure to avoid borrow-checker clones if the fix is readable
- Audit small ID wrapper types (EntityId, EffectId, etc.) — ensure they're Copy, not Clone

### Success criteria
- 50% clone count reduction per targeted file is the goal (absolute count, not percentage-based exclusions)
- No benchmarks required — trust the analysis
- Files with few clones still count — removing any unnecessary clone is success
- No comment documentation for kept clones
- Clippy must pass clean after each plan

### Claude's Discretion
- Exact grouping of patterns into plans
- Which adjacent files to include if clones are discovered
- Case-by-case judgment on loop restructuring complexity
- Whether specific borrow-checker workarounds are worth the restructure

</decisions>

<specifics>
## Specific Ideas

No specific requirements — open to standard approaches.

**Expected impact (for context):**
- String clones in hot loops are the biggest wins
- Expect 5-15% faster parse time for large logs (50MB+)
- 10-30% less peak memory allocation during parsing
- Smoother UI during live parsing (fewer allocation pauses)

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 07-clone-cleanup*
*Context gathered: 2026-01-17*
