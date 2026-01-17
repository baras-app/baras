# Phase 3: Core Error Handling - Research

**Researched:** 2026-01-17
**Domain:** Rust error handling - unwrap/expect removal
**Confidence:** HIGH

## Summary

This research covers the systematic removal of `.unwrap()` and `.expect()` calls in production code within `core/src`. The goal is to replace these with proper `Result` handling using the error types created in Phase 2.

After filtering out test files (which are excluded from the requirements), there are **14 production unwrap/expect calls** that need to be converted. These fall into three categories:

1. **Configuration saving** (1 expect) - config.rs
2. **Log file reading** (1 expect) - reader.rs
3. **Signal processor logic** (11 unwraps) - phase.rs, counter.rs, challenge.rs, processor.rs
4. **Shared helpers** (2 unwraps) - effects/tracker.rs, timers/signal_handlers.rs, storage/writer.rs

**Primary recommendation:** Handle signal processor unwraps as logical invariants (unreachable guards since encounter existence is checked earlier in the same function), storage writer as safe array operations, and convert config/reader to proper Result returns.

## Inventory

### Production Unwrap/Expect Calls (14 total)

| File | Line | Call | Category | Error Type | Migration Strategy |
|------|------|------|----------|------------|-------------------|
| context/config.rs | 69 | `.expect("Failed to save configuration")` | Config save | ConfigError | Return Result |
| combat_log/reader.rs | 131 | `.expect("failed to find game_session_date")` | Reader state | ReaderError | Return Result |
| signal_processor/phase.rs | 72 | `.unwrap()` | Encounter access | N/A - invariant | Debug assert or expect with context |
| signal_processor/phase.rs | 134 | `.unwrap()` | Encounter access | N/A - invariant | Debug assert or expect with context |
| signal_processor/phase.rs | 198 | `.unwrap()` | Encounter access | N/A - invariant | Debug assert or expect with context |
| signal_processor/phase.rs | 236 | `.unwrap()` | Encounter access | N/A - invariant | Debug assert or expect with context |
| signal_processor/phase.rs | 237 | `.unwrap()` | Boss index access | N/A - invariant | Debug assert or expect with context |
| signal_processor/phase.rs | 268 | `.unwrap()` | Encounter access | N/A - invariant | Debug assert or expect with context |
| signal_processor/counter.rs | 37, 56, 73, 120, 134, 151 | `.unwrap()` | Encounter access | N/A - invariant | Debug assert or expect with context |
| signal_processor/challenge.rs | 27 | `.unwrap()` | Encounter access | N/A - invariant | Debug assert or expect with context |
| signal_processor/processor.rs | 344, 424, 436 | `.unwrap()` | Encounter access | N/A - invariant | Debug assert or expect with context |
| effects/tracker.rs | 29 | `.unwrap()` | Encounter access | N/A - invariant | Refactor with if-let |
| timers/signal_handlers.rs | 21 | `.unwrap()` | Encounter access | N/A - invariant | Refactor with if-let |
| storage/writer.rs | 550 | `.unwrap()` | Array last element | N/A - safe | Assert or comment |
| encounter/shielding.rs | 90 | `.unwrap()` | Option after filter | N/A - safe | Document invariant |

### Test File Unwrap/Expect Calls (EXCLUDED - 45+ total)

These are intentionally excluded per requirements (tests can use unwrap/expect):
- combat_log/parser/tests.rs (24 unwraps)
- dsl/loader.rs test functions (4 expects)
- dsl/triggers/mod.rs test functions (4 unwraps)
- dsl/challenge.rs test functions (4 unwraps)
- timers/manager_tests.rs (4 expects)
- timers/preferences.rs test functions (2 unwraps)
- signal_processor/processor_tests.rs (8 expects)

## Architecture Patterns

### Pattern 1: Invariant Unwraps in Signal Processor

**What:** The signal processor files have many `.unwrap()` calls on `cache.current_encounter_mut()` that appear AFTER an early return guard on `cache.current_encounter()`.

**Example pattern:**
```rust
pub fn check_hp_phase_transitions(...) -> Vec<GameSignal> {
    let Some(enc) = cache.current_encounter() else {
        return Vec::new();  // Guard: no encounter
    };
    let Some(def_idx) = enc.active_boss_idx() else {
        return Vec::new();  // Guard: no boss
    };

    // ...logic that reads enc immutably...

    // Later, need mutable access:
    let enc = cache.current_encounter_mut().unwrap();  // <-- INVARIANT: we know it exists
    enc.set_phase(...);
}
```

**Why it's safe:** The function already returned early if encounter didn't exist. The unwrap is accessing the same data that was just verified.

**Migration strategy:** Keep as debug assertions with context:
```rust
let enc = cache.current_encounter_mut()
    .expect("BUG: encounter disappeared mid-function");
```

Or use `debug_assert!` wrapper:
```rust
let enc = cache.current_encounter_mut()
    .unwrap_or_else(|| unreachable!("BUG: encounter disappeared mid-function"));
```

**Note:** Per clean policy, even these "safe" invariants should be converted. Options:
1. Restructure to hold mutable reference from the start (requires borrow checker work)
2. Use `.ok_or()` + early return with logged error
3. Add `expect_always!` macro that logs + panics in debug, logs + recovers in release

### Pattern 2: Configuration Save Error

**What:** `context/config.rs` line 69 uses `.expect()` when saving config.

**Current:**
```rust
fn save(self) {
    confy::store("baras", "config", self).expect("Failed to save configuration");
}
```

**Migration:** Change to return `Result<(), ConfigError>`:
```rust
fn save(self) -> Result<(), ConfigError> {
    confy::store("baras", "config", self).map_err(|e| ConfigError::Save(e))?;
    Ok(())
}
```

**Note:** This changes the trait signature and all call sites. Requires coordinated update.

### Pattern 3: Reader Session State Access

**What:** `combat_log/reader.rs` line 131 expects session date to exist.

**Current:**
```rust
let session_date = self.state.read().await.game_session_date
    .expect("failed to find game_session_date");
```

**Migration:** Add error variant and propagate:
```rust
let session_date = self.state.read().await.game_session_date
    .ok_or(ReaderError::SessionDateMissing)?;
```

Requires adding `SessionDateMissing` variant to `ReaderError` and changing return type.

### Pattern 4: Safe Array Operations

**What:** `storage/writer.rs` line 550 unwraps `.last()` on an array that was just pushed to.

**Current:**
```rust
offsets.push(0);
for offset in &shield_list_offsets {
    match offset {
        Some((_, end)) => {
            offsets.push(*end as i32);
        }
        None => {
            offsets.push(*offsets.last().unwrap());  // <-- always has at least 0
        }
    }
}
```

**Why it's safe:** `offsets` was initialized with `push(0)`, so `.last()` always succeeds.

**Migration:** Can use `unwrap_or(&0)` or document with comment:
```rust
// SAFETY: offsets always has at least one element (initialized with 0)
offsets.push(*offsets.last().unwrap());
```

Per clean policy, use the safer version:
```rust
offsets.push(offsets.last().copied().unwrap_or(0));
```

### Pattern 5: Helper Function Encounter Access

**What:** `effects/tracker.rs` and `timers/signal_handlers.rs` have identical helper functions:
```rust
fn get_entities(encounter: Option<&CombatEncounter>) -> &[EntityDefinition] {
    static EMPTY: &[EntityDefinition] = &[];
    encounter
        .and_then(|e| e.active_boss_idx())
        .map(|idx| {
            encounter.unwrap().boss_definitions()[idx]  // <-- unwrap after checking
                .entities
                .as_slice()
        })
        .unwrap_or(EMPTY)
}
```

**Migration:** Refactor to avoid the unwrap:
```rust
fn get_entities(encounter: Option<&CombatEncounter>) -> &[EntityDefinition] {
    static EMPTY: &[EntityDefinition] = &[];
    let Some(enc) = encounter else { return EMPTY; };
    let Some(idx) = enc.active_boss_idx() else { return EMPTY; };
    enc.boss_definitions()[idx].entities.as_slice()
}
```

## Dependency Graph

```
Phase 2 Error Types (must complete first):
  02-01: combat_log/error.rs (ParseError, ReaderError) [DONE]
         dsl/error.rs (DslError) [DONE]
  02-02: query/error.rs (QueryError) [PENDING]
         storage/error.rs (StorageError) [PENDING]
  02-03: context/error.rs (WatcherError, ConfigError) [PENDING]
         timers/error.rs (TimerError) [PENDING]

Phase 3 Dependencies:
  03-01: config.rs -> needs ConfigError from 02-03
  03-02: reader.rs -> needs ReaderError (exists from 02-01)
  03-03: signal_processor/* -> no error type needed (invariants)
  03-04: effects/tracker.rs, timers/signal_handlers.rs -> refactor only
  03-05: storage/writer.rs -> no error type needed (safe operation)
  03-06: encounter/shielding.rs -> no error type needed (safe filter)
```

## Recommended Wave Structure

### Wave 1: Simple Refactors (No Error Type Needed)
**Files:** effects/tracker.rs, timers/signal_handlers.rs, storage/writer.rs, encounter/shielding.rs
**Count:** 4 unwraps
**Strategy:** Refactor code to avoid unwrap without changing function signatures.

### Wave 2: Signal Processor Invariants
**Files:** signal_processor/phase.rs, counter.rs, challenge.rs, processor.rs
**Count:** 13 unwraps
**Strategy:** Convert to explicit expect with context message OR refactor to propagate Option.
**Note:** These are all in internal functions - no public API changes.

### Wave 3: Public API Changes
**Files:** context/config.rs (depends on Phase 2-03), combat_log/reader.rs
**Count:** 2 expects
**Strategy:** Change function signatures to return Result, update call sites.
**Dependencies:** Requires ConfigError from Phase 2-03 to be complete.

## Common Pitfalls

### Pitfall 1: Borrow Checker Conflicts in Signal Processor

**What goes wrong:** Trying to hold mutable reference while also reading encounter data.
**Why it happens:** The current pattern reads immutably, computes, then mutates.
**How to avoid:** Either:
1. Clone necessary data before getting mutable reference
2. Split into separate functions (check vs apply)
3. Accept the `expect()` with context for invariants

**Example of the problem:**
```rust
let Some(enc) = cache.current_encounter() else { return vec![]; };
let phases = &enc.phases;  // immutable borrow
// ... check phases ...
let enc = cache.current_encounter_mut().unwrap();  // need mutable, but...
// Can't hold immutable ref while getting mutable
```

**Current solution (already in codebase):** Clone what's needed:
```rust
let phases: Vec<_> = def.phases.clone();  // Clone to release borrow
// ... later ...
let enc = cache.current_encounter_mut().unwrap();
```

### Pitfall 2: Cascading Signature Changes

**What goes wrong:** Changing `save()` to return Result requires updating all callers.
**Why it happens:** Error propagation is viral.
**How to avoid:** Plan the full call chain before starting. Identify all call sites.

### Pitfall 3: Adding Unhelpful Error Types

**What goes wrong:** Creating error variants that just say "failed" without context.
**Why it happens:** Mechanical conversion without thinking about debugging.
**How to avoid:** Every error variant should answer "what failed?" and "where?".

## Code Examples

### Example 1: Refactoring get_entities Helper

```rust
// Before (unwrap after check)
fn get_entities(encounter: Option<&CombatEncounter>) -> &[EntityDefinition] {
    static EMPTY: &[EntityDefinition] = &[];
    encounter
        .and_then(|e| e.active_boss_idx())
        .map(|idx| {
            encounter.unwrap().boss_definitions()[idx]
                .entities
                .as_slice()
        })
        .unwrap_or(EMPTY)
}

// After (no unwrap needed)
fn get_entities(encounter: Option<&CombatEncounter>) -> &[EntityDefinition] {
    static EMPTY: &[EntityDefinition] = &[];
    let Some(enc) = encounter else { return EMPTY; };
    let Some(idx) = enc.active_boss_idx() else { return EMPTY; };
    enc.boss_definitions()[idx].entities.as_slice()
}
```

### Example 2: Config Save with Result

```rust
// Before
fn save(self) {
    confy::store("baras", "config", self).expect("Failed to save configuration");
}

// After (requires changing trait)
fn save(self) -> Result<(), ConfigError> {
    confy::store("baras", "config", self).map_err(ConfigError::Save)?;
    tracing::debug!("Configuration saved");
    Ok(())
}

// Call site handling
match config.save() {
    Ok(()) => {}
    Err(e) => {
        tracing::error!(error = %e, "Failed to save configuration");
        // Decide: propagate, ignore, or show user notification
    }
}
```

### Example 3: Reader Session Date with Result

```rust
// Before
let session_date = self.state.read().await.game_session_date
    .expect("failed to find game_session_date");

// After (add variant to ReaderError)
#[derive(Debug, Error)]
pub enum ReaderError {
    // ... existing variants ...

    #[error("session date not initialized before tailing")]
    SessionDateMissing,
}

// In function
let session_date = self.state.read().await.game_session_date
    .ok_or(ReaderError::SessionDateMissing)?;
```

### Example 4: Signal Processor Invariant with Context

```rust
// Before
let enc = cache.current_encounter_mut().unwrap();

// Option A: Expect with context (still panics but documents invariant)
let enc = cache.current_encounter_mut()
    .expect("BUG: encounter checked at function entry but missing at mutation point");

// Option B: Early return with log (no panic, defensive)
let Some(enc) = cache.current_encounter_mut() else {
    tracing::error!("BUG: encounter disappeared mid-function in check_hp_phase_transitions");
    return Vec::new();
};

// Option C: Macro for consistent handling
macro_rules! get_encounter_mut {
    ($cache:expr, $fn_name:literal) => {
        match $cache.current_encounter_mut() {
            Some(enc) => enc,
            None => {
                tracing::error!(
                    "BUG: encounter missing in {} after guard check",
                    $fn_name
                );
                return Vec::new();
            }
        }
    };
}
let enc = get_encounter_mut!(cache, "check_hp_phase_transitions");
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `.unwrap()` everywhere | Explicit error handling | Rust best practice | Safer, debuggable |
| `.expect("msg")` | `Result` propagation | Modern Rust | Recoverable errors |
| Panic on invariant failure | Log + defensive return | Reliability focus | Graceful degradation |

## Open Questions

1. **Signal processor invariants:** Should these panic (expect) or gracefully return empty? The current code logic assumes they can't fail, but for absolute robustness, logging + returning empty might be preferred.

2. **Config save failure handling:** What should the app do if config save fails? Currently it panics. Options:
   - Propagate to UI and show error
   - Log and continue (data loss risk)
   - Retry with exponential backoff

3. **Clean policy interpretation:** Does "no unwrap in production" mean:
   a) Convert all to `.expect()` with context (still panics but documented)?
   b) Convert all to `Result` returns (never panic)?
   c) Allow documented invariant expects but no naked unwraps?

**Recommendation:** Based on requirements stating "Zero .unwrap() calls remain" and "Zero .expect() calls remain", interpret as option (b) - convert everything to Result or restructure code to avoid the need entirely.

## Sources

### Primary (HIGH confidence)
- Phase 2 RESEARCH.md - Error type patterns
- Phase 2 PLAN files - Specific error types being created
- Codebase analysis - Direct grep and file reading

### Secondary (MEDIUM confidence)
- Rust error handling best practices (general knowledge)

## Metadata

**Confidence breakdown:**
- Inventory: HIGH - direct codebase analysis
- Migration strategies: HIGH - based on actual code patterns
- Wave structure: HIGH - based on dependency analysis
- Error type mapping: MEDIUM - depends on Phase 2 completion

**Research date:** 2026-01-17
**Valid until:** Until Phase 2 completes and error types are finalized

## Summary Statistics

| Category | Count | Strategy |
|----------|-------|----------|
| Production unwrap calls | 11 | Refactor or convert |
| Production expect calls | 3 | Convert to Result |
| Test unwrap/expect | 45+ | Excluded per requirements |
| Files to modify | 9 | Listed above |
| Error types needed | 2 | ConfigError (Phase 2), ReaderError (exists) |
