# Phase 6: Logging Migration - Research

**Researched:** 2026-01-17
**Domain:** Migrating eprintln! to tracing with file-based logging and size rotation
**Confidence:** HIGH

## Summary

Phase 6 completes the logging infrastructure by:
1. Migrating all remaining `eprintln!` calls to structured tracing macros
2. Adding file-based logging with size-based rotation (10 MB, keep latest only)
3. Implementing the DEBUG_LOGGING=1 environment variable for verbose output

The codebase has approximately 140 `eprintln!` calls in production code (excluding tests and planning docs). The standard approach is to use `rolling-file` or `tracing-rolling-file` for size-based rotation since `tracing-appender` only supports time-based rotation. A layered subscriber architecture with `tracing_subscriber::registry()` enables writing to both file and stdout simultaneously with different filters.

**Primary recommendation:** Use `rolling-file` crate (v0.2.0) with `tracing_subscriber::registry()` for dual-output logging. Create a `logging` module in `app/src-tauri` that configures file output with size-based rotation and conditional DEBUG_LOGGING behavior.

## Standard Stack

The established libraries for this domain:

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `tracing` | 0.1 | Logging/tracing macros | Already in workspace, Tokio ecosystem standard |
| `tracing-subscriber` | 0.3 | Subscriber implementation | Already in workspace, provides layers and filtering |
| `rolling-file` | 0.2.0 | Size-based file rotation | Mature, Debian-style naming, configurable max files |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `tracing-appender` | 0.2 | Non-blocking file output | For wrapping rolling-file writer with non-blocking |
| `dirs` | existing | Config directory lookup | Already used throughout codebase |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `rolling-file` | `tracing-rolling-file` | Similar API, but rolling-file is more mature |
| `rolling-file` | `logroller` | Newer (v0.1), has compression but less battle-tested |
| `rolling-file` | `tracing-appender` | Only time-based rotation, no size-based |

**Installation (add to workspace Cargo.toml):**
```toml
[workspace.dependencies]
rolling-file = "0.2"
```

**Per-crate usage:**
```toml
# In app/src-tauri/Cargo.toml only
[dependencies]
rolling-file = { workspace = true }
```

## Architecture Patterns

### Recommended Logging Module Structure

```
app/src-tauri/src/
├── logging.rs          # NEW: Subscriber configuration, file rotation setup
└── lib.rs              # Calls logging::init() before tauri::Builder
```

### Pattern 1: Dual-Output Subscriber with Layers

**What:** Configure tracing to write to both file and stdout with different filters
**When to use:** Main application (app/src-tauri) startup

```rust
// Source: https://docs.rs/tracing-subscriber/latest/tracing_subscriber/layer/
// File: app/src-tauri/src/logging.rs

use rolling_file::{BasicRollingFileAppender, RollingConditionBasic};
use std::io;
use std::sync::Arc;
use tracing_subscriber::{filter::LevelFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

pub fn init() -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let debug_logging = std::env::var("DEBUG_LOGGING").is_ok();

    // Determine log directory: ~/.config/baras/logs
    let log_dir = dirs::config_dir()
        .map(|p| p.join("baras").join("logs"))
        .unwrap_or_else(|| std::path::PathBuf::from("logs"));

    // Create log directory
    if let Err(e) = std::fs::create_dir_all(&log_dir) {
        eprintln!("Failed to create log directory: {}", e);
        return None;
    }

    // Create size-based rolling file appender (10 MB, keep 1 file)
    let file_appender = BasicRollingFileAppender::new(
        log_dir.join("baras.log"),
        RollingConditionBasic::new().max_size(10 * 1024 * 1024), // 10 MB
        1, // Keep only the latest rotated file (baras.log and baras.log.1)
    );

    let file_appender = match file_appender {
        Ok(appender) => appender,
        Err(e) => {
            eprintln!("Failed to create log file: {}", e);
            return None;
        }
    };

    // Wrap in non-blocking writer
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    // File layer: always INFO+ for baras crates
    let file_layer = fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_target(true);

    // Stdout layer: different behavior based on DEBUG_LOGGING
    let stdout_layer = fmt::layer()
        .with_writer(io::stdout)
        .with_target(true);

    if debug_logging {
        // DEBUG_LOGGING=1: debug+trace for baras, info for deps
        let filter = tracing_subscriber::filter::EnvFilter::new(
            "info,app_lib=debug,baras_core=debug,overlay=debug"
        );

        tracing_subscriber::registry()
            .with(stdout_layer.with_filter(filter))
            .with(file_layer.with_filter(LevelFilter::DEBUG))
            .init();
    } else {
        // Default: info+warn+error only
        tracing_subscriber::registry()
            .with(stdout_layer.with_filter(LevelFilter::INFO))
            .with(file_layer.with_filter(LevelFilter::INFO))
            .init();
    }

    tracing::info!("BARAS logging initialized");
    Some(guard)
}
```

### Pattern 2: Level Mapping for Migration

**What:** Map existing eprintln! patterns to appropriate tracing levels
**When to use:** When converting each eprintln! call

| Existing Pattern | Tracing Level | Rationale |
|------------------|---------------|-----------|
| `"Failed to..."`, `"Error:"` | `error!` | All caught errors |
| `"Warning:"`, `"Skipping..."` | `warn!` | Degraded but working |
| `"Loaded X items"`, DSL operations | `info!` | CRUD operations per CONTEXT.md |
| `"[DEBUG]"`, detailed state | `debug!` | Development diagnostics |
| Position calculations, internal state | `trace!` | Verbose internals |

```rust
// ERROR: All caught errors
tracing::error!(error = %e, "Failed to create output dir");

// WARN: Degraded but working
tracing::warn!(error = %e, "Failed to load bundled definitions");
tracing::warn!(timer_id = %id, chains_to = %missing, "Broken timer chain reference");

// INFO: DSL CRUD, file rotation, overlay toggles, encounters, uploads, config changes
tracing::info!(count = bosses.len(), "Loaded bundled boss definitions");
tracing::info!(count = timers.len(), "Loaded timer definitions from {:?}", path);
tracing::info!(file = %filename, "Log file rotated");

// DEBUG: Internal state, optional diagnostics
tracing::debug!(timer_id = %id, "Timer expired");
tracing::debug!(x = pos_x, y = pos_y, "Overlay position updated");

// TRACE: Highly verbose, rarely needed
tracing::trace!(event = ?event, "Processing combat event");
```

### Pattern 3: Test Code Handling

**What:** Keep eprintln! in test code, or use conditional compilation
**When to use:** Files with `_tests.rs` suffix or `#[cfg(test)]` modules

Test code can keep `eprintln!` since tests run in a controlled environment where stdout is visible. The success criteria specifies "zero eprintln! in production code" - test code is not production code.

```rust
// In test files - OK to keep eprintln! for test output
#[cfg(test)]
mod tests {
    #[test]
    fn test_timer_chain() {
        eprintln!("Test output: {:?}", result); // OK - test code
    }
}
```

### Anti-Patterns to Avoid

- **Migrating test eprintln! to tracing:** Tests use stdout for visibility; don't change them
- **Using RUST_LOG override:** CONTEXT.md specifies DEBUG_LOGGING=1 only, not RUST_LOG
- **Forgetting WorkerGuard:** The non-blocking guard must be held for the app lifetime
- **Mixing levels inconsistently:** All caught errors must be ERROR level per CONTEXT.md

## Don't Hand-Roll

Problems that look simple but have existing solutions:

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Size-based rotation | Custom rotation logic | `rolling-file` crate | Edge cases with concurrent access, atomic rename |
| Non-blocking file writes | Spawned thread | `tracing_appender::non_blocking` | Handles shutdown flushing, backpressure |
| Log directory location | Hardcoded paths | `dirs::config_dir()` | Cross-platform, follows XDG/AppData conventions |
| Filter configuration | Manual if/else | `EnvFilter` with directive string | Handles crate targeting, level precedence |

**Key insight:** File rotation during active writes has subtle failure modes (partial writes, rename races). The `rolling-file` crate handles these correctly.

## Common Pitfalls

### Pitfall 1: WorkerGuard Dropped Too Early

**What goes wrong:** Logs buffered in non-blocking writer are lost on shutdown
**Why it happens:** Guard dropped when logging::init() returns, before app shuts down
**How to avoid:** Store guard in Tauri state or use `_guard` pattern in run()
**Warning signs:** Missing logs near shutdown, especially error logs

```rust
// WRONG - guard dropped immediately
pub fn run() {
    logging::init(); // Guard dropped here!
    tauri::Builder::default().run(...);
}

// CORRECT - hold guard for app lifetime
pub fn run() {
    let _logging_guard = logging::init();
    tauri::Builder::default().run(...);
    // Guard dropped after app exits
}
```

### Pitfall 2: Validate Tool Should Not Use File Logging

**What goes wrong:** Validate tool writes to user's log directory unexpectedly
**Why it happens:** Adding tracing-subscriber to validate crate
**How to avoid:** Validate is a dev tool - use stdout only, no file logging
**Warning signs:** Extra log files appearing during DSL validation

### Pitfall 3: Parse-Worker Double Initialization

**What goes wrong:** Parse-worker already has tracing setup, don't duplicate
**Why it happens:** Adding file logging to subprocess
**How to avoid:** Parse-worker is ephemeral subprocess - stdout logging only
**Warning signs:** File handle conflicts, duplicate log entries

### Pitfall 4: Inconsistent Log Levels

**What goes wrong:** Some errors logged at warn, some warnings at error
**Why it happens:** Ad-hoc migration without clear rules
**How to avoid:** Follow the level mapping strictly per CONTEXT.md
**Warning signs:** Important errors missed when filtering at ERROR level

## Code Examples

Verified patterns from official documentation:

### Complete Main App Initialization

```rust
// Source: Combined from tracing-subscriber docs and CONTEXT.md decisions
// File: app/src-tauri/src/lib.rs

mod logging;

pub fn run() {
    // Initialize logging FIRST - returns guard that must outlive app
    let _logging_guard = logging::init();

    tracing::info!("BARAS starting up");

    tauri::Builder::default()
        // ... existing setup
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

### Migrating Timer Manager eprintln!

```rust
// Source: CONTEXT.md level mapping
// File: core/src/timers/manager.rs

// BEFORE
eprintln!(
    "[TIMER WARNING] Duplicate timer ID '{}' found!",
    def.id
);

// AFTER
tracing::warn!(
    timer_id = %def.id,
    existing_name = %existing.name,
    duplicate_name = %def.name,
    "Duplicate timer ID found, keeping first"
);

// BEFORE
eprintln!(
    "TimerManager: loaded {} enabled definitions",
    self.definitions.len()
);

// AFTER
tracing::info!(
    count = self.definitions.len(),
    "Loaded enabled timer definitions"
);
```

### Migrating Wayland Debug Output

```rust
// Source: CONTEXT.md - DEBUG/TRACE at Claude's discretion
// File: overlay/src/platform/wayland.rs

// BEFORE
eprintln!(
    "Rebinding to output {} at ({}, {}) size {}x{}",
    new_info.id(), new_info.x, new_info.y,
    new_info.logical_width(), new_info.logical_height()
);

// AFTER - debug level for position changes
tracing::debug!(
    output = %new_info.id(),
    x = new_info.x,
    y = new_info.y,
    width = new_info.logical_width(),
    height = new_info.logical_height(),
    "Rebinding to output"
);

// BEFORE
eprintln!("Rebind failed: output {} not found", output_name);

// AFTER - error level for failures
tracing::error!(output = %output_name, "Rebind failed: output not found");
```

### Migrating Service Module

```rust
// Source: CONTEXT.md level mapping
// File: app/src-tauri/src/service/mod.rs

// BEFORE
eprintln!("[PARSE] Using worker: {:?}", worker_path);

// AFTER - debug level for internal diagnostics
tracing::debug!(worker_path = ?worker_path, "Using parse worker");

// BEFORE
eprintln!("[PARSE] Total time: {:.0}ms", timer.elapsed().as_millis());

// AFTER - info level for performance metrics
tracing::info!(elapsed_ms = timer.elapsed().as_millis(), "Parse completed");

// BEFORE
eprintln!("[ICONS] Failed to load icon cache: {}", e);

// AFTER - error level for all caught errors
tracing::error!(error = %e, "Failed to load icon cache");
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `eprintln!` everywhere | Structured `tracing` | Phase 1 (foundation) | Filter by level, add context |
| Time-based rotation only | Size-based with `rolling-file` | 2023 | Control disk usage |
| Single output | Multiple layers | tracing-subscriber 0.3 | File + stdout simultaneously |

**Deprecated/outdated:**
- `tracing-appender` for size-based rotation: Only supports time-based, use `rolling-file` instead
- `log` + `env_logger`: Superseded by `tracing` ecosystem

## Open Questions

Things that couldn't be fully resolved:

1. **Overlay crate tracing initialization**
   - What we know: Overlay is a library crate, shouldn't init subscriber
   - What's unclear: Overlay also runs as standalone binary (overlay/src/main.rs)
   - Recommendation: Check if overlay binary needs its own subscriber init (likely yes for standalone debugging)

2. **Log file cleanup on startup**
   - What we know: CONTEXT.md says "keep only latest file"
   - What's unclear: Should old logs be deleted on app startup, or only during rotation?
   - Recommendation: Let rolling-file handle via max_filecount=1; don't add manual cleanup

## Sources

### Primary (HIGH confidence)
- [tracing-subscriber layer docs](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/layer/) - Layer composition API
- [rolling-file docs](https://docs.rs/rolling-file/) - Size-based rotation API
- [tracing-appender non_blocking](https://docs.rs/tracing-appender/latest/tracing_appender/non_blocking/) - Non-blocking writer pattern
- Phase 1 Research: `.planning/phases/01-logging-foundation/01-RESEARCH.md` - Existing tracing setup

### Secondary (MEDIUM confidence)
- [GitHub: rolling-file-rs](https://github.com/Axcient/rolling-file-rs) - README examples
- [GitHub: tracing-rolling-file](https://github.com/cavivie/tracing-rolling-file) - Alternative implementation reference
- [GitHub issue #1940](https://github.com/tokio-rs/tracing/issues/1940) - tracing-appender size rotation feature request

### Tertiary (LOW confidence)
- [logroller crate](https://github.com/trayvonpan/logroller/) - Newer alternative (v0.1, less battle-tested)

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - rolling-file is mature (0.2.0), tracing setup verified in Phase 1
- Architecture: HIGH - Layer composition pattern documented in official tracing-subscriber docs
- Pitfalls: HIGH - Based on tracing-appender docs and Phase 1 research

**eprintln! count breakdown:**
- Production code (needs migration): ~140 calls
  - core/src/: ~50 calls (timers, dsl, context)
  - overlay/src/: ~40 calls (wayland, main)
  - app/src-tauri/src/: ~40 calls (service, commands, hotkeys)
- Test code (keep as-is): ~60 calls
  - *_tests.rs files
- Dev tools (keep as-is or minimal tracing): ~15 calls
  - validate/src/main.rs

**Research date:** 2026-01-17
**Valid until:** 60 days - rolling-file 0.2.x is stable
