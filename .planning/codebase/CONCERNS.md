# Codebase Concerns

**Analysis Date:** 2026-01-17

## Tech Debt

**Excessive `.unwrap()` Usage in Production Code:**
- Issue: 320+ uses of `.unwrap()` and `.expect()` outside test files, many in critical paths
- Files: `core/src/signal_processor/phase.rs` (lines 72, 134, 198, 236, 237, 268), `core/src/signal_processor/counter.rs` (lines 37, 56, 73, 120, 134, 151), `core/src/signal_processor/processor.rs` (lines 344, 424, 436), `core/src/effects/tracker.rs` (line 29), `core/src/encounter/shielding.rs` (line 90), `core/src/combat_log/reader.rs` (line 131)
- Impact: Runtime panics in edge cases (missing encounters, uninitialized state)
- Fix approach: Replace with proper `Option`/`Result` handling, return early with `?` operator or log and continue

**Massive Static Data Files:**
- Issue: Hand-maintained static data arrays exceeding 4000 lines
- Files: `core/src/game_data/raid_bosses.rs` (4144 lines), `core/src/game_data/flashpoint_bosses.rs` (2614 lines)
- Impact: Hard to maintain, error-prone manual updates, no compile-time validation
- Fix approach: Move to external data files (TOML/JSON) loaded at runtime or build-time code generation

**Clone-Heavy Phase Transition Logic:**
- Issue: 35 `.clone()` calls in a single file for phase transition checks
- Files: `core/src/signal_processor/phase.rs`
- Impact: Unnecessary allocations on every event during combat
- Fix approach: Pass references, use copy-on-write patterns, cache computed values

**Frontend JS Interop Boilerplate:**
- Issue: 200+ raw `.unwrap()` calls on `js_sys::Reflect::set` operations
- Files: `app/src/components/charts_panel.rs` (68 instances), `app/src/components/data_explorer.rs` (35 instances), `app/src/api.rs` (100+ instances)
- Impact: Any JS error causes panic, no graceful degradation
- Fix approach: Create helper macros or wrapper functions with proper error handling

**Inconsistent Logging Strategy:**
- Issue: Mix of `eprintln!` (382 occurrences) and no structured logging
- Files: Throughout codebase, especially `core/src/dsl/challenge.rs` (16), `overlay/src/platform/wayland.rs` (17), `app/src-tauri/src/service/mod.rs` (25)
- Impact: No log levels, can't filter debug output in production, no structured fields
- Fix approach: Adopt `tracing` crate consistently with proper levels and spans

## Known Bugs

**SWTOR Charge Bug Workaround:**
- Symptoms: Trauma Probe and Kolto Shell report 6 charges instead of 7 on ApplyEffect
- Files: `core/src/game_data/effects.rs` (lines 34-47)
- Trigger: When these abilities are applied to targets
- Workaround: `correct_apply_charges()` function adds 1 to charge count for these abilities

**Debug Code Left in Production:**
- Symptoms: Debug file writes to `/tmp/parse_worker_shield.txt` in parse-worker
- Files: `parse-worker/src/main.rs` (lines 312-325)
- Trigger: Processing shield data in subprocess
- Workaround: Remove or gate behind feature flag

## Security Considerations

**Unsafe Code in Platform Layers:**
- Risk: Memory safety bugs in overlay rendering (memory-mapped shared memory, raw pointers)
- Files: `overlay/src/platform/wayland.rs` (lines 561-612, `unsafe` mmap operations), `overlay/src/platform/windows.rs` (15 `unsafe` blocks for Win32 API), `overlay/src/platform/x11.rs` (lines 165-198), `overlay/src/platform/macos.rs` (14 `unsafe` blocks for Cocoa)
- Current mitigation: Contained to platform-specific modules, proper cleanup in `Drop` implementations
- Recommendations: Add safety comments documenting invariants, consider safe wrappers where possible

**Memory-Mapped File Parsing:**
- Risk: Processing untrusted combat log files via mmap
- Files: `core/src/combat_log/reader.rs` (lines 39, 84), `parse-worker/src/main.rs` (line 652)
- Current mitigation: Files come from known game installation directory
- Recommendations: Add file size limits, validate file format before full parsing

**Configuration File Path:**
- Risk: Config stored in platform config directory without validation
- Files: `core/src/context/config.rs` (line 69 - `confy::store` with `.expect()`)
- Current mitigation: Using `confy` library with standard paths
- Recommendations: Handle config save failures gracefully, validate config on load

## Performance Bottlenecks

**Cloning in Hot Paths:**
- Problem: Excessive cloning of `Vec`, `String`, `HashMap` during combat event processing
- Files: `core/src/signal_processor/phase.rs` (35 clones), `core/src/timers/manager.rs` (36 clones), `core/src/effects/tracker.rs` (28 clones)
- Cause: Borrow checker workarounds, defensive copies
- Improvement path: Use `Cow<str>`, `Arc<T>` for shared data, refactor to avoid borrow conflicts

**Large Component Files:**
- Problem: UI components exceeding 1000-1800 lines impacting compile times
- Files: `app/src/components/encounter_editor/triggers.rs` (1985 lines), `app/src/components/data_explorer.rs` (1822 lines), `app/src/components/settings_panel.rs` (1784 lines), `app/src/components/effect_editor.rs` (1435 lines)
- Cause: Monolithic component design
- Improvement path: Extract sub-components, use composition patterns

**Async Lock Contention:**
- Problem: 322 uses of `.lock()`, `.read()`, `.write()` on `RwLock`/`Mutex` across service layer
- Files: `app/src-tauri/src/service/handler.rs` (50 instances), `app/src-tauri/src/service/mod.rs` (53 instances), `app/src-tauri/src/overlay/manager.rs` (15 instances)
- Cause: Shared state architecture with multiple async accessors
- Improvement path: Reduce lock scope, use message passing for overlay updates, consider lock-free structures

## Fragile Areas

**Signal Processor State Machine:**
- Files: `core/src/signal_processor/processor.rs`, `core/src/signal_processor/phase.rs`, `core/src/signal_processor/counter.rs`
- Why fragile: Complex interdependencies between phase transitions, counter updates, and timer triggers. Multiple `unwrap()` calls assume encounter state exists
- Safe modification: Always verify encounter exists before state mutations, add comprehensive tests for edge cases
- Test coverage: `processor_tests.rs` (871 lines) covers happy paths but limited edge case coverage

**Effects Tracker AoE Correlation:**
- Files: `core/src/effects/tracker.rs` (lines 134-195)
- Why fragile: Stateful tracking of pending AoE refreshes with timing windows (10ms tolerance)
- Safe modification: Test with varied network latencies, consider race conditions
- Test coverage: Limited - timing-sensitive logic is hard to unit test

**Wayland Protocol Implementation:**
- Files: `overlay/src/platform/wayland.rs` (1675 lines)
- Why fragile: Complex protocol state machine with wl_output, xdg-output, fractional scaling interactions
- Safe modification: Test on multiple compositors (GNOME, KDE, Sway), handle protocol version differences
- Test coverage: None - requires manual testing on different desktops

## Scaling Limits

**In-Memory Event Storage:**
- Current capacity: All combat events held in memory during session
- Limit: 50MB+ log files cause memory pressure, especially in historical mode
- Scaling path: Stream events to parquet storage, query from disk. Parse-worker subprocess already helps but results still loaded into main process

**Timer Definition Loading:**
- Current capacity: All timer definitions loaded into `HashMap<String, Arc<TimerDefinition>>`
- Limit: As encounter definitions grow, startup time increases
- Scaling path: Lazy loading by area_id, unload definitions when leaving area

## Dependencies at Risk

**Dioxus (Frontend Framework):**
- Risk: Pre-1.0 framework with potential breaking changes
- Impact: UI code may need updates on upgrades
- Migration plan: Dioxus API is stabilizing, monitor changelogs carefully

**cosmic-text (Text Rendering):**
- Risk: Relatively new crate for text layout/shaping in overlay
- Impact: Text rendering bugs, glyph issues
- Migration plan: Can fallback to simpler text rendering if needed

## Missing Critical Features

**Error Recovery:**
- Problem: No graceful error recovery in service layer - panics terminate background tasks
- Blocks: Robust long-running operation (users have to restart app after crashes)

**Structured Logging:**
- Problem: No structured logging framework, just `eprintln!` scattered throughout
- Blocks: Production debugging, telemetry, log aggregation

**Input Validation:**
- Problem: DSL definitions loaded without comprehensive validation
- Blocks: User-created timers/effects may cause panics with malformed config

## Test Coverage Gaps

**Platform Overlay Code:**
- What's not tested: All platform-specific overlay implementations (Wayland, Windows, X11, macOS)
- Files: `overlay/src/platform/wayland.rs`, `overlay/src/platform/windows.rs`, `overlay/src/platform/x11.rs`, `overlay/src/platform/macos.rs`
- Risk: Platform-specific regressions go unnoticed
- Priority: Medium - manual testing covers most cases but CI verification would help

**Frontend Components:**
- What's not tested: All Dioxus UI components (settings, editors, charts)
- Files: `app/src/components/` (entire directory)
- Risk: UI regressions, state management bugs
- Priority: Medium - reactive UI reduces some bugs but complex editors need coverage

**Historical File Parsing:**
- What's not tested: Full historical log file processing with encounter summaries
- Files: `core/src/context/parser.rs`, `parse-worker/src/main.rs`
- Risk: Incorrect encounter classification, missing events
- Priority: High - core functionality that should have integration tests

**Timer Chaining Logic:**
- What's not tested: Complex timer chains (timer A expires -> starts timer B -> triggers counter)
- Files: `core/src/timers/manager.rs` (expired_this_tick, started_this_tick interactions)
- Risk: Boss mechanic timers may not chain correctly
- Priority: High - affects raid utility accuracy

---

*Concerns audit: 2026-01-17*
