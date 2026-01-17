# Architecture

**Analysis Date:** 2026-01-17

## Pattern Overview

**Overall:** Event-Driven + Domain-Driven Architecture with Layered Separation

**Key Characteristics:**
- Event-driven processing via `GameSignal` system for cross-cutting concerns
- Domain-scoped crates with clear boundaries (core, overlay, types, app)
- Async service layer coordinating background parsing with frontend UI
- Custom overlay rendering pipeline bypassing web technologies for performance

## Layers

**Core Domain (`core/`):**
- Purpose: Pure business logic for parsing, encounter tracking, and data analysis
- Location: `core/src/`
- Contains: Combat log parsing, encounter state machines, signal processing, data storage
- Depends on: `types/` crate, standard libraries, Arrow/DataFusion
- Used by: `app/src-tauri/` (backend), `parse-worker/` (subprocess)

**Types (`types/`):**
- Purpose: Shared serializable types for cross-crate and WASM boundary communication
- Location: `types/src/lib.rs`
- Contains: Query result types, trigger definitions, config types, selectors
- Depends on: serde
- Used by: `core/`, `app/src-tauri/`, `app/src/` (frontend WASM)

**Tauri Backend (`app/src-tauri/`):**
- Purpose: Application shell coordinating services, state, and IPC
- Location: `app/src-tauri/src/`
- Contains: Tauri commands, CombatService, OverlayManager, state management
- Depends on: `core/`, `overlay/`, Tauri framework
- Used by: Frontend via Tauri IPC

**Overlay Rendering (`overlay/`):**
- Purpose: Platform-native overlay windows with custom software rendering
- Location: `overlay/src/`
- Contains: Platform abstractions, tiny-skia renderer, overlay implementations
- Depends on: tiny-skia, cosmic-text, platform-specific windowing (Wayland/X11/Win32)
- Used by: `app/src-tauri/` spawns overlay threads

**Frontend (`app/src/`):**
- Purpose: User interface for configuration, analytics, and data exploration
- Location: `app/src/`
- Contains: Dioxus components, API bindings, application shell
- Depends on: Dioxus framework, `types/` (via WASM)
- Used by: Users via WebView

## Data Flow

**Live Combat Parsing Flow:**

1. `CombatService` watches log directory via `DirectoryWatcher`
2. `Reader` tails active log file, yields `CombatEvent` stream
3. `EventProcessor.process_event()` updates `SessionCache`, emits `GameSignal`s
4. `SignalHandler` routes signals to: TimerManager, EffectTracker, ShieldContext
5. Service computes overlay metrics (PlayerMetrics, RaidFrameData, etc.)
6. `OverlayUpdate` messages sent via channel to `router`
7. Router dispatches to appropriate overlay threads via `OverlayCommand`
8. Overlays render via `Renderer` -> platform window

**Query/Analytics Flow:**

1. `EncounterWriter` writes combat events to Parquet files (per-encounter)
2. User selects encounter in frontend Data Explorer
3. Frontend calls Tauri command (e.g., `query_breakdown`)
4. Backend loads Parquet into `QueryContext` (DataFusion)
5. SQL query executed, results returned as typed structs
6. Frontend displays in reactive components

**State Management:**
- `SessionCache` (core): Encounter-scoped state (HP tracking, phases, player info)
- `SharedState` (app): Cross-session state (config, directory index, overlay status flags)
- `OverlayState` (app): Running overlay handles and channels
- Config persisted to `~/.config/baras/config.json`

## Key Abstractions

**GameSignal:**
- Purpose: Cross-cutting event notification for combat state changes
- Examples: `CombatStarted`, `EffectApplied`, `BossHpChanged`, `PhaseChanged`
- Pattern: EventProcessor emits signals; SignalHandler and other components react

**CombatEncounter:**
- Purpose: Single combat session state container
- Examples: `core/src/encounter/combat.rs`
- Pattern: Created on EnterCombat, finalized on ExitCombat, contains all encounter data

**Overlay Trait:**
- Purpose: Unified interface for all overlay window types
- Examples: `overlay/src/overlays/mod.rs` - MetricOverlay, RaidOverlay, TimerOverlay
- Pattern: Generic rendering loop calls `update_data()`, `render()`, `poll_events()`

**OverlayPlatform Trait:**
- Purpose: Platform abstraction for native windowing
- Examples: `overlay/src/platform/wayland.rs`, `windows.rs`, `x11.rs`, `macos.rs`
- Pattern: Runtime detection selects Wayland vs X11 on Linux

**BossEncounterDefinition (DSL):**
- Purpose: Declarative boss fight configuration
- Examples: `core/definitions/encounters/operations/*.toml`
- Pattern: TOML files define phases, timers, counters, challenges; loaded at area enter

## Entry Points

**Application Entry:**
- Location: `app/src-tauri/src/main.rs` -> `app_lib::run()`
- Triggers: Application launch
- Responsibilities: Tauri builder setup, plugin registration, service spawn

**Service Entry:**
- Location: `app/src-tauri/src/service/mod.rs` - `CombatService::run()`
- Triggers: Application startup
- Responsibilities: Event loop processing commands, parsing, overlay updates

**Frontend Entry:**
- Location: `app/src/main.rs` - `launch(App)`
- Triggers: WebView initialization
- Responsibilities: Dioxus reactive root, router setup

**Overlay Entry:**
- Location: `app/src-tauri/src/overlay/spawn.rs`
- Triggers: `OverlayManager::show()` or auto-show on startup
- Responsibilities: Spawn dedicated thread, create platform window, enter render loop

## Error Handling

**Strategy:** Graceful degradation with logging; avoid panics in hot paths

**Patterns:**
- Core parsing: `Result<T, String>` for recoverable errors; skip malformed lines
- Tauri commands: Return `Result` for frontend error display
- Overlay rendering: Log and continue; never crash overlay thread
- Query execution: Return empty results for missing tables/columns

## Cross-Cutting Concerns

**Logging:**
- Debug builds: `eprintln!` macros throughout
- Production: Minimal logging via `debug_log!` macro in core
- Frontend: dioxus_logger at INFO level

**Validation:**
- DSL files validated via `validate/` CLI tool
- Config schema enforced by serde defaults
- Trigger matching validated at definition load time

**Authentication:**
- None for local app
- Parsely.io integration: username/password stored in config (plaintext)

**Performance:**
- String interning via `IStr` for repeated names
- Arrow/Parquet for efficient columnar storage
- Lazy loading of boss definitions per-area
- Overlay render throttling (50ms min interval)

---

*Architecture analysis: 2026-01-17*
