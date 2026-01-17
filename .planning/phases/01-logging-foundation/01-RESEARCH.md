# Phase 01: Logging Foundation - Research

**Researched:** 2026-01-17
**Domain:** Rust tracing infrastructure for Tauri 2.x workspace
**Confidence:** HIGH

## Summary

The `tracing` ecosystem is the established Rust solution for structured logging and diagnostics. For a Tauri 2.x workspace with multiple crates, the pattern is:

1. **Library crates** (`core`, `overlay`, `types`): Add `tracing` as a dependency and use macros (`info!`, `debug!`, `error!`, etc.) - no subscriber initialization
2. **Binary crate** (`app/src-tauri`): Initialize `tracing-subscriber` once in startup, before any other code runs
3. **Subprocess** (`parse-worker`): Initialize its own subscriber since it runs as a separate process

Tauri 2.x is fully compatible with the `tracing` crate. A known pitfall is that the RUST_LOG filter target must use `app_lib` (the lib name in Cargo.toml), not `app` (the package name), due to how Tauri structures its library crate.

**Primary recommendation:** Use `tracing` + `tracing-subscriber` with `env-filter` feature. Initialize subscriber in `app/src-tauri/src/lib.rs::run()` before `tauri::Builder`. Configure sensible defaults (INFO for release, DEBUG for debug builds) with RUST_LOG override.

## Standard Stack

The established libraries for Rust structured logging:

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `tracing` | 0.1.44 | Logging/tracing macros | Tokio ecosystem standard, async-aware, structured events |
| `tracing-subscriber` | 0.3.22 | Subscriber implementation | Official companion, provides fmt output and filtering |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `tracing-appender` | 0.2.x | File output with rotation | If file logging needed (LOG-05 deferred to v2) |
| `tracing-log` | 0.2.x | `log` crate compatibility | If dependencies use `log` crate (not needed here) |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `tracing` | `log` + `env_logger` | Simpler but no spans, less async-friendly |
| `tracing` | `tauri-plugin-log` | Easier frontend integration, but based on `log` crate, loses tracing features |
| `tracing-subscriber` | Custom subscriber | Full control but significant effort |

**Note:** `tauri-plugin-log` is based on `log` + `fern`, not `tracing`. While it offers frontend JS logging integration, the PROJECT.md constraint specifies `tracing`. For this phase, we use pure `tracing` without the Tauri plugin.

**Installation (workspace root Cargo.toml):**
```toml
[workspace.dependencies]
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
```

**Per-crate usage:**
```toml
# In core/Cargo.toml, overlay/Cargo.toml, types/Cargo.toml
[dependencies]
tracing = { workspace = true }

# In app/src-tauri/Cargo.toml only
[dependencies]
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
```

## Architecture Patterns

### Recommended Project Structure

No new files needed - tracing integrates into existing structure:

```
app/src-tauri/
├── src/
│   └── lib.rs          # Initialize subscriber in run() BEFORE tauri::Builder
├── Cargo.toml          # Add tracing + tracing-subscriber

core/
├── src/
│   └── *.rs            # Use tracing macros (info!, debug!, error!)
├── Cargo.toml          # Add tracing only

overlay/
├── src/
│   └── *.rs            # Use tracing macros
├── Cargo.toml          # Add tracing only

parse-worker/
├── src/
│   └── main.rs         # Initialize its own subscriber (separate process)
├── Cargo.toml          # Add tracing + tracing-subscriber
```

### Pattern 1: Subscriber Initialization (Tauri Backend)

**What:** Initialize tracing subscriber before any Tauri setup
**When to use:** In the main binary entry point, before other initialization

```rust
// Source: https://docs.rs/tracing-subscriber/latest/tracing_subscriber/
// In app/src-tauri/src/lib.rs

use tracing_subscriber::{fmt, filter::EnvFilter};

pub fn run() {
    // Initialize tracing FIRST, before any other code
    let filter = EnvFilter::builder()
        .with_default_directive(tracing::Level::INFO.into())
        .from_env_lossy();

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .init();

    // Now existing Tauri setup...
    tauri::Builder::default()
        // ...
}
```

### Pattern 2: Debug vs Release Defaults

**What:** Different default log levels for development vs production
**When to use:** Always - prevents verbose production logs while enabling debug during development

```rust
// Source: Common Rust pattern for conditional compilation
#[cfg(debug_assertions)]
const DEFAULT_LOG_LEVEL: tracing::Level = tracing::Level::DEBUG;

#[cfg(not(debug_assertions))]
const DEFAULT_LOG_LEVEL: tracing::Level = tracing::Level::INFO;

pub fn run() {
    let filter = EnvFilter::builder()
        .with_default_directive(DEFAULT_LOG_LEVEL.into())
        .from_env_lossy();

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .init();
    // ...
}
```

### Pattern 3: Using Tracing Macros in Library Crates

**What:** Replace `eprintln!` with structured tracing macros
**When to use:** Throughout all library code (core, overlay, types)

```rust
// Source: https://docs.rs/tracing/latest/tracing/

// Instead of:
eprintln!("[STARTUP] Failed to clear data directory: {}", e);

// Use:
tracing::error!(error = %e, "Failed to clear data directory");

// Or with more context:
tracing::error!(
    error = %e,
    directory = ?data_dir,
    "Failed to clear data directory"
);

// For debug info:
tracing::debug!(file_count = count, "Indexed log files");

// For informational messages:
tracing::info!(path = %file_path, "Started tailing log file");
```

### Pattern 4: Module-Level Filtering with RUST_LOG

**What:** Filter log output by crate/module using environment variable
**When to use:** During development to focus on specific areas

```bash
# Show all crates at info, baras-core at debug
RUST_LOG=info,baras_core=debug ./app

# Show only errors except for specific module
RUST_LOG=error,baras_core::signal_processor=debug ./app

# Note: crate names with dashes use underscores in RUST_LOG
# baras-core -> baras_core
# app (with lib name app_lib) -> app_lib
```

### Anti-Patterns to Avoid

- **Initializing subscriber multiple times:** Only one global subscriber can be set. Library crates must NEVER call `.init()` or `set_global_default()`
- **Using `Span::enter()` across `.await` points:** Causes incorrect traces. Use `#[instrument]` or `.instrument()` for async
- **Forgetting the `env-filter` feature:** Without it, `EnvFilter` is not available
- **Using package name in RUST_LOG for Tauri:** The target is `app_lib` (lib name), not `app` (package name)

## Don't Hand-Roll

Problems that look simple but have existing solutions:

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Log level filtering | Custom filter logic | `EnvFilter` | Handles RUST_LOG syntax, per-module filtering |
| Formatted output | Custom formatters | `tracing_subscriber::fmt` | Handles timestamps, colors, structured fields |
| Async span context | Manual context passing | `#[instrument]` macro | Correctly handles async/await suspension |
| Non-blocking file writes | Spawned writer thread | `tracing-appender` | Handles buffering, flushing on shutdown |

**Key insight:** The tracing ecosystem has solved these problems with battle-tested implementations. Custom solutions will miss edge cases (async context loss, buffered output on crash, etc.).

## Common Pitfalls

### Pitfall 1: Span Guards Across Await Points

**What goes wrong:** Holding `Span::enter()` guard across `.await` causes incorrect parent spans
**Why it happens:** Async tasks can suspend and resume on different threads; guard-based context is thread-local
**How to avoid:** Use `#[instrument]` attribute or `.instrument()` combinator for async functions
**Warning signs:** Spans appearing under wrong parents in logs, missing span context in async code

```rust
// WRONG - guard held across await
async fn bad_example() {
    let span = tracing::info_span!("my_span");
    let _guard = span.enter(); // Guard held...
    some_async_work().await;   // ...across await point!
}

// CORRECT - use #[instrument]
#[tracing::instrument]
async fn good_example() {
    some_async_work().await;
}
```

### Pitfall 2: Wrong Target Name for Tauri App

**What goes wrong:** RUST_LOG filters don't work for the Tauri backend
**Why it happens:** Tauri apps have lib name (`app_lib`) different from package name (`app`)
**How to avoid:** Use `app_lib` as the target in RUST_LOG, not `app`
**Warning signs:** Filters like `RUST_LOG=app=debug` have no effect

```bash
# WRONG
RUST_LOG=app=debug ./baras

# CORRECT
RUST_LOG=app_lib=debug ./baras
```

### Pitfall 3: Missing env-filter Feature

**What goes wrong:** `EnvFilter` type not found, compilation fails
**Why it happens:** Feature not enabled in Cargo.toml
**How to avoid:** Always include `features = ["env-filter"]` for tracing-subscriber
**Warning signs:** Compile error about `EnvFilter` not existing

### Pitfall 4: Subscriber Init After Logging Calls

**What goes wrong:** Early log messages are silently dropped
**Why it happens:** No subscriber registered yet when tracing macros are called
**How to avoid:** Initialize subscriber as the FIRST thing in `run()`
**Warning signs:** Missing startup logs, logs only appear after certain point

## Code Examples

Verified patterns from official documentation:

### Complete Subscriber Setup (Tauri Backend)

```rust
// Source: https://docs.rs/tracing-subscriber/latest/tracing_subscriber/
// File: app/src-tauri/src/lib.rs

use tracing_subscriber::{fmt, filter::EnvFilter};

#[cfg(debug_assertions)]
const DEFAULT_LOG_LEVEL: tracing::Level = tracing::Level::DEBUG;

#[cfg(not(debug_assertions))]
const DEFAULT_LOG_LEVEL: tracing::Level = tracing::Level::INFO;

pub fn run() {
    // Initialize tracing subscriber FIRST
    let filter = EnvFilter::builder()
        .with_default_directive(DEFAULT_LOG_LEVEL.into())
        .from_env_lossy();

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)      // Show target (module path) in logs
        .with_thread_ids(false) // Don't show thread IDs (reduces noise)
        .with_file(false)       // Don't show file:line (reduces noise)
        .init();

    tracing::info!("BARAS starting up");

    // Existing Tauri setup continues...
    tauri::Builder::default()
        // ...
}
```

### Replacing eprintln! with tracing

```rust
// Source: https://docs.rs/tracing/latest/tracing/

// BEFORE (current code)
if let Err(e) = baras_core::storage::clear_data_dir() {
    eprintln!("[STARTUP] Failed to clear data directory: {}", e);
}

// AFTER (with tracing)
if let Err(e) = baras_core::storage::clear_data_dir() {
    tracing::error!(error = %e, "Failed to clear data directory");
}

// For warnings
// BEFORE
eprintln!("Warning: Config file not found, using defaults");
// AFTER
tracing::warn!("Config file not found, using defaults");

// For debug info
// BEFORE
eprintln!("Debug: Parsed {} lines", count);
// AFTER
tracing::debug!(lines = count, "Parsed log file");
```

### Parse Worker Subscriber (Separate Binary)

```rust
// Source: Standard pattern for subprocess
// File: parse-worker/src/main.rs

fn main() {
    // Parse worker needs its own subscriber (separate process)
    let filter = tracing_subscriber::filter::EnvFilter::builder()
        .with_default_directive(tracing::Level::INFO.into())
        .from_env_lossy();

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .init();

    tracing::info!("Parse worker starting");

    // ... existing parse worker code
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `println!`/`eprintln!` | `tracing` macros | ~2020 | Structured logging, filtering, spans |
| `log` crate | `tracing` crate | ~2021 widespread | Async-aware, spans, better Tokio integration |
| `env_logger` | `tracing-subscriber` | ~2021 widespread | Composable layers, better formatting |

**Deprecated/outdated:**
- `log` + `env_logger`: Still works but `tracing` is preferred for async Rust
- `tracing-subscriber` < 0.3: Use 0.3.x for current features

## Open Questions

Things that couldn't be fully resolved:

1. **File logging destination for production**
   - What we know: `tracing-appender` supports file output with rotation
   - What's unclear: Where should log files go? (Platform-specific app data dir?)
   - Recommendation: Defer to LOG-05 (v2 requirement). For now, stdout only.

2. **Frontend logging integration**
   - What we know: `tauri-plugin-log` provides JS logging API
   - What's unclear: How to bridge frontend logs to Rust tracing (if needed)
   - Recommendation: Out of scope for Phase 1. Frontend can use console.log for now.

## Sources

### Primary (HIGH confidence)
- [tracing docs.rs](https://docs.rs/tracing) - Macro usage, async patterns
- [tracing-subscriber docs.rs](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/) - Subscriber setup, EnvFilter
- [EnvFilter docs](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html) - Filter configuration
- [Tokio tracing guide](https://tokio.rs/tokio/topics/tracing) - Official getting started

### Secondary (MEDIUM confidence)
- [Tauri v2 logging plugin docs](https://v2.tauri.app/plugin/logging/) - Tauri-specific considerations
- [GitHub issue #9452](https://github.com/tauri-apps/tauri/issues/9452) - Confirmed tracing works with Tauri 2, target naming

### Tertiary (LOW confidence)
- [Shuttle tracing guide](https://www.shuttle.dev/blog/2024/01/09/getting-started-tracing-rust) - General patterns (verified against official docs)

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - Official Tokio ecosystem, widely used
- Architecture: HIGH - Documented patterns, verified against official docs
- Pitfalls: HIGH - Documented in official tracing docs, confirmed by GitHub issues

**Research date:** 2026-01-17
**Valid until:** Stable - tracing 0.1.x has been stable for years
