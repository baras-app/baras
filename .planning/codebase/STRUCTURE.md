# Codebase Structure

**Analysis Date:** 2026-01-17

## Directory Layout

```
baras/
├── app/                    # Tauri + Dioxus application
│   ├── src/                # Frontend (Dioxus WASM)
│   │   ├── components/     # UI components
│   │   ├── api.rs          # Tauri IPC bindings
│   │   ├── app.rs          # Main application component
│   │   ├── types.rs        # Frontend-specific types
│   │   └── main.rs         # Dioxus entry point
│   ├── src-tauri/          # Tauri backend (Rust)
│   │   ├── src/
│   │   │   ├── commands/   # Tauri command handlers
│   │   │   ├── overlay/    # Overlay spawning & management
│   │   │   ├── service/    # CombatService background task
│   │   │   ├── state/      # SharedState, RaidSlotRegistry
│   │   │   ├── audio/      # Audio playback service
│   │   │   ├── lib.rs      # App entry point (run())
│   │   │   └── router.rs   # Overlay update routing
│   │   └── Cargo.toml
│   └── assets/             # Static assets (icons)
├── core/                   # Core business logic
│   ├── src/
│   │   ├── combat_log/     # Log parsing
│   │   ├── context/        # Config, interner, watcher
│   │   ├── dsl/            # Boss/timer definitions
│   │   ├── effects/        # Effect tracking
│   │   ├── encounter/      # Combat encounter state
│   │   ├── game_data/      # SWTOR constants & lookups
│   │   ├── query/          # DataFusion queries
│   │   ├── signal_processor/ # Event -> Signal transform
│   │   ├── state/          # SessionCache
│   │   ├── storage/        # Parquet writer
│   │   ├── timers/         # Timer manager
│   │   └── lib.rs          # Public API re-exports
│   └── definitions/        # DSL config files
│       ├── encounters/     # Boss encounter TOML files
│       ├── effects/        # Effect tracking definitions
│       └── sounds/         # Audio file packs
├── overlay/                # Custom overlay rendering
│   ├── src/
│   │   ├── overlays/       # Overlay implementations
│   │   ├── platform/       # OS-specific windowing
│   │   ├── widgets/        # Reusable UI components
│   │   ├── manager.rs      # OverlayWindow wrapper
│   │   ├── renderer.rs     # tiny-skia rendering
│   │   └── lib.rs          # Public API
│   └── examples/           # Standalone overlay demos
├── types/                  # Shared serializable types
│   └── src/lib.rs          # All types in single file
├── parse-worker/           # Subprocess for parallel parsing
│   └── src/main.rs         # IPC-based log parser
├── validate/               # DSL validation CLI
│   └── src/                # Verification logic
├── test-log-files/         # Test data (not committed)
├── Cargo.toml              # Workspace definition
└── justfile                # Build/dev commands
```

## Directory Purposes

**`app/src/` (Frontend):**
- Purpose: Dioxus reactive UI components
- Contains: Components, API bindings, application state
- Key files: `app.rs` (main component), `api.rs` (Tauri commands), `types.rs` (frontend types)

**`app/src-tauri/src/` (Backend):**
- Purpose: Tauri application shell and services
- Contains: Command handlers, service loop, overlay management, state
- Key files: `lib.rs` (entry), `service/mod.rs` (CombatService), `router.rs` (overlay dispatch)

**`core/src/` (Domain Logic):**
- Purpose: Pure combat parsing and analysis logic
- Contains: Parser, encounter state machine, signal system, queries
- Key files: `lib.rs` (API), `signal_processor/processor.rs` (event handling)

**`core/src/combat_log/`:**
- Purpose: Log file parsing
- Contains: Line parser, event structs, reader
- Key files: `parser.rs` (line parsing), `reader.rs` (file tailing), `combat_event.rs` (event struct)

**`core/src/encounter/`:**
- Purpose: Combat encounter state tracking
- Contains: Encounter lifecycle, metrics, challenge tracking
- Key files: `combat.rs` (CombatEncounter), `metrics.rs` (PlayerMetrics), `challenge.rs` (ChallengeTracker)

**`core/src/signal_processor/`:**
- Purpose: Transform raw events into high-level signals
- Contains: EventProcessor, signal definitions, phase/counter logic
- Key files: `processor.rs` (main processor), `signal.rs` (GameSignal enum)

**`core/src/dsl/`:**
- Purpose: Boss encounter definition loading and types
- Contains: TOML loaders, definition structs, trigger matching
- Key files: `definition.rs` (BossEncounterDefinition), `loader.rs` (TOML parsing), `triggers/matchers.rs`

**`overlay/src/overlays/`:**
- Purpose: Complete overlay implementations
- Contains: One file per overlay type
- Key files: `metric.rs`, `raid.rs`, `timers.rs`, `boss_health.rs`, `effects_ab.rs`

**`overlay/src/platform/`:**
- Purpose: OS-specific window management
- Contains: Platform trait + implementations
- Key files: `mod.rs` (trait + Linux enum), `wayland.rs`, `x11.rs`, `windows.rs`, `macos.rs`

## Key File Locations

**Entry Points:**
- `app/src-tauri/src/main.rs`: Tauri main (calls lib.rs)
- `app/src-tauri/src/lib.rs`: `run()` function, Tauri setup
- `app/src/main.rs`: Dioxus frontend entry
- `overlay/src/main.rs`: Standalone overlay binary (examples)

**Configuration:**
- `app/src-tauri/tauri.conf.json`: Tauri app config
- `~/.config/baras/config.json`: User settings (runtime)
- `core/definitions/`: DSL definition files

**Core Logic:**
- `core/src/signal_processor/processor.rs`: Event processing state machine
- `core/src/encounter/combat.rs`: CombatEncounter struct
- `core/src/state/cache.rs`: SessionCache (session state)

**Testing:**
- `core/src/combat_log/parser/tests.rs`: Parser tests
- `core/src/signal_processor/processor_tests.rs`: Processor tests
- `core/src/timers/manager_tests.rs`: Timer tests
- `test-log-files/`: Test fixtures (gitignored except fixtures/)

## Naming Conventions

**Files:**
- Snake_case: `combat_event.rs`, `boss_health.rs`, `signal_processor/`
- `mod.rs`: Module roots with re-exports

**Directories:**
- Lowercase snake_case: `combat_log/`, `signal_processor/`, `game_data/`
- Plural for collections: `overlays/`, `widgets/`, `commands/`

**Structs/Enums:**
- PascalCase: `CombatEncounter`, `GameSignal`, `OverlayPlatform`
- Suffix `-Config` for config structs: `RaidOverlayConfig`, `AppConfig`
- Suffix `-Data` for data transfer: `RaidFrameData`, `BossHealthData`

**Functions:**
- snake_case: `process_event()`, `update_data()`, `poll_events()`
- Prefix `get_` sparingly; prefer direct noun: `config()`, `position()`

## Where to Add New Code

**New Overlay Type:**
- Implementation: `overlay/src/overlays/{name}.rs`
- Add to: `overlay/src/overlays/mod.rs` (exports)
- Add variant to: `OverlayData`, `OverlayConfigUpdate` enums
- Add to router: `app/src-tauri/src/router.rs`
- Add spawn logic: `app/src-tauri/src/overlay/spawn.rs`
- Add Tauri commands: `app/src-tauri/src/commands/overlay.rs`

**New Tauri Command:**
- Handler: `app/src-tauri/src/commands/{category}.rs`
- Register in: `app/src-tauri/src/lib.rs` `invoke_handler![]`
- Frontend binding: `app/src/api.rs`

**New GameSignal:**
- Define: `core/src/signal_processor/signal.rs`
- Emit from: `core/src/signal_processor/processor.rs`
- Handle in: `core/src/timers/signal_handlers.rs` (if timer-related)

**New Query:**
- Implementation: `core/src/query/{name}.rs`
- Add to: `core/src/query/mod.rs`
- Expose via command: `app/src-tauri/src/commands/query.rs`

**New DSL Element (phase/counter/timer):**
- Type definition: `core/src/dsl/{element}.rs`
- Parser support: `core/src/dsl/loader.rs`
- Runtime handling: `core/src/signal_processor/` or `core/src/timers/`

**Utilities/Helpers:**
- Core domain: `core/src/` appropriate module
- Overlay rendering: `overlay/src/utils.rs` or `overlay/src/widgets/`
- Frontend: `app/src/utils.rs`

## Special Directories

**`core/definitions/`:**
- Purpose: DSL configuration files (TOML)
- Generated: No (manually authored)
- Committed: Yes

**`app/target/` and `target/`:**
- Purpose: Cargo build artifacts
- Generated: Yes
- Committed: No (gitignored)

**`test-log-files/`:**
- Purpose: Combat log fixtures for testing
- Generated: No (collected from game)
- Committed: Partially (small fixtures only, large files gitignored)

**`.planning/`:**
- Purpose: GSD planning documents
- Generated: By Claude tools
- Committed: Optional (project-specific)

**`~/.config/baras/`:**
- Purpose: Runtime user data (config, parquet cache)
- Generated: Yes (at runtime)
- Committed: No (user-specific)

---

*Structure analysis: 2026-01-17*
