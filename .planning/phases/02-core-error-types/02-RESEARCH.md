# Phase 2: Core Error Types - Research

**Researched:** 2026-01-17
**Domain:** Rust error handling with thiserror
**Confidence:** HIGH

## Summary

This research covers best practices for defining custom error types in a multi-module Rust crate using thiserror 2.x. The codebase currently uses `Result<T, String>` throughout and has 70+ `unwrap()`/`expect()` calls that need proper error handling.

The standard approach is to define **per-module error enums** using thiserror derive macros, with `#[from]` for automatic conversions from underlying errors and meaningful context fields (file paths, line numbers, entity IDs). The existing `PreferencesError` in `timers/preferences.rs` provides a good starting pattern.

**Primary recommendation:** Create dedicated error types per module with thiserror, include contextual information as fields, and use `#[from]` to implement conversions up the call chain. Do not create a single monolithic error enum for the entire crate.

## Standard Stack

The established libraries/tools for this domain:

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| thiserror | 2.0 | Error type derive macros | Industry standard, 609M+ downloads, zero runtime overhead |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| tracing | (workspace) | Error logging | Already in project, use `tracing::error!` when errors occur |

### Not Needed
| Library | Why Not |
|---------|---------|
| anyhow | For applications, not libraries; this is a library crate |
| snafu | Overkill for this crate size; thiserror sufficient |
| tracing-error | Adds complexity; basic tracing integration sufficient |

**Installation:**
```bash
# Add to core/Cargo.toml
cargo add thiserror@2 --package baras-core
```

## Architecture Patterns

### Recommended Error Type Structure

Per-module error types, NOT a single crate-wide error:

```
core/src/
  combat_log/
    error.rs          # ParseError, ReaderError
    mod.rs
  dsl/
    error.rs          # DslError (loading/parsing TOML)
    mod.rs
  storage/
    error.rs          # StorageError (parquet writing)
    mod.rs
  context/
    error.rs          # ConfigError, WatcherError
    mod.rs
  query/
    error.rs          # QueryError (SQL/datafusion errors)
    mod.rs
  timers/
    preferences.rs    # PreferencesError (already exists, good pattern)
    error.rs          # TimerError (if needed)
```

### Pattern 1: Module Error Enum with Context

**What:** Define an enum with variants for each failure mode, including relevant context as fields
**When to use:** When callers need to handle different error cases programmatically

```rust
// Source: thiserror 2.x docs
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DslError {
    #[error("failed to read definition file at {path}")]
    ReadFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse TOML in {path}")]
    ParseToml {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("invalid boss definition '{boss_id}' in {path}: {reason}")]
    InvalidDefinition {
        path: PathBuf,
        boss_id: String,
        reason: String,
    },
}
```

### Pattern 2: Error with Automatic From Conversions

**What:** Use `#[from]` to implement `From<SourceError>` automatically
**When to use:** When wrapping underlying library errors without additional context

```rust
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("IO error")]
    Io(#[from] std::io::Error),

    #[error("Arrow error")]
    Arrow(#[from] arrow::error::ArrowError),

    #[error("Parquet error")]
    Parquet(#[from] parquet::errors::ParquetError),
}
```

### Pattern 3: Context-Rich Wrapper

**What:** Wrapper struct with context + inner error kind enum
**When to use:** When all errors share common context (e.g., file path)

```rust
// Source: nrc.github.io/error-docs/error-design
#[derive(Debug, Error)]
#[error("error processing {path}")]
pub struct FileError {
    pub path: PathBuf,
    #[source]
    pub kind: FileErrorKind,
}

#[derive(Debug, Error)]
pub enum FileErrorKind {
    #[error("failed to read file")]
    Read(#[from] std::io::Error),
    #[error("failed to parse content")]
    Parse(#[from] toml::de::Error),
}
```

### Pattern 4: Transparent Error Delegation

**What:** Use `#[error(transparent)]` to delegate Display/source to inner error
**When to use:** For catch-all variants or when you want the inner error to speak for itself

```rust
#[derive(Debug, Error)]
pub enum QueryError {
    #[error("SQL execution failed: {query}")]
    SqlExecution {
        query: String,
        #[source]
        source: datafusion::error::DataFusionError,
    },

    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}
```

### Anti-Patterns to Avoid

- **Monolithic crate error:** Don't create a single `CoreError` enum with 30+ variants
- **Naked error wrapping:** Don't just wrap errors without adding context
- **String errors:** Don't use `Result<T, String>` - use typed errors
- **Centralized errors module:** Keep errors in the module they serve

## Don't Hand-Roll

Problems that look simple but have existing solutions:

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Error trait impl | Manual Display + Error impls | `#[derive(Error)]` | Boilerplate-free, correct |
| From conversions | Manual `impl From<E>` | `#[from]` attribute | Less code, can't forget |
| Error sources | Manual source() method | `#[source]` attribute | Automatic chaining |
| Display formatting | `impl fmt::Display` | `#[error("...")]` | Field interpolation |

**Key insight:** thiserror generates all the boilerplate correctly. Manual implementations are error-prone and verbose.

## Common Pitfalls

### Pitfall 1: Losing Context When Converting Errors

**What goes wrong:** Using `?` with `#[from]` loses the file path or other context
**Why it happens:** `#[from]` converts automatically but can't add context
**How to avoid:** Use `.map_err()` when context is needed

```rust
// Bad: loses file path
fn load(path: &Path) -> Result<Config, DslError> {
    let content = std::fs::read_to_string(path)?; // Error loses path!
    Ok(toml::from_str(&content)?)
}

// Good: preserves file path
fn load(path: &Path) -> Result<Config, DslError> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| DslError::ReadFile { path: path.into(), source: e })?;
    toml::from_str(&content)
        .map_err(|e| DslError::ParseToml { path: path.into(), source: e })
}
```

**Warning signs:** Using `?` directly on IO operations without wrapping

### Pitfall 2: Over-Using #[from]

**What goes wrong:** Too many automatic conversions make error chains unclear
**Why it happens:** `#[from]` is convenient but hides the conversion point
**How to avoid:** Only use `#[from]` for 1:1 error wrapping without context needs

**Warning signs:** Multiple `#[from]` variants for the same underlying error type

### Pitfall 3: Non-Exhaustive Without Thought

**What goes wrong:** Adding `#[non_exhaustive]` when callers need to match all variants
**Why it happens:** Trying to future-proof
**How to avoid:** Only use `#[non_exhaustive]` on public enums that will grow

**Warning signs:** Internal errors marked non_exhaustive

### Pitfall 4: Forgetting Debug Derive

**What goes wrong:** Error can't be printed for debugging
**Why it happens:** Focus on Display, forget Debug
**How to avoid:** Always `#[derive(Debug, Error)]`

## Code Examples

### Example 1: Combat Log Parser Error

```rust
// core/src/combat_log/error.rs
use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur during combat log parsing
#[derive(Debug, Error)]
pub enum ParseError {
    #[error("invalid line format at line {line_number}: expected 5 bracket pairs")]
    InvalidLineFormat { line_number: u64 },

    #[error("invalid timestamp at line {line_number}: {segment}")]
    InvalidTimestamp {
        line_number: u64,
        segment: String,
    },

    #[error("invalid entity format at line {line_number}")]
    InvalidEntity { line_number: u64 },
}

/// Errors that can occur during log file reading
#[derive(Debug, Error)]
pub enum ReaderError {
    #[error("failed to open log file {path}")]
    OpenFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to memory map file {path}")]
    MemoryMap {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("encoding error in file {path}")]
    Encoding { path: PathBuf },
}
```

### Example 2: DSL Loader Error

```rust
// core/src/dsl/error.rs
use std::path::PathBuf;
use thiserror::Error;

/// Errors during DSL definition loading
#[derive(Debug, Error)]
pub enum DslError {
    #[error("failed to read {path}")]
    ReadFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse TOML in {path}")]
    ParseToml {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("failed to read directory {path}")]
    ReadDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to serialize boss config")]
    Serialize(#[from] toml::ser::Error),

    #[error("failed to create directory {path}")]
    CreateDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to write {path}")]
    WriteFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}
```

### Example 3: Storage Error

```rust
// core/src/storage/error.rs
use std::path::PathBuf;
use thiserror::Error;

/// Errors during parquet file operations
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("failed to create parquet file {path}")]
    CreateFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("arrow conversion error")]
    Arrow(#[from] arrow::error::ArrowError),

    #[error("parquet write error")]
    Parquet(#[from] parquet::errors::ParquetError),

    #[error("failed to create data directory")]
    CreateDir(#[from] std::io::Error),
}
```

### Example 4: Query Error

```rust
// core/src/query/error.rs
use thiserror::Error;

/// Errors during data queries
#[derive(Debug, Error)]
pub enum QueryError {
    #[error("datafusion error: {0}")]
    DataFusion(#[from] datafusion::error::DataFusionError),

    #[error("column {name} not found in result")]
    ColumnNotFound { name: String },

    #[error("unexpected column type for {name}: expected {expected}")]
    UnexpectedColumnType {
        name: String,
        expected: &'static str,
    },

    #[error("no data available")]
    NoData,
}
```

### Example 5: Config Error

```rust
// core/src/context/error.rs
use std::path::PathBuf;
use thiserror::Error;

/// Errors during configuration operations
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to load config")]
    Load(#[from] confy::ConfyError),

    #[error("failed to save config")]
    Save(#[from] confy::ConfyError),

    #[error("profile '{name}' not found")]
    ProfileNotFound { name: String },

    #[error("maximum profiles reached ({max})")]
    MaxProfilesReached { max: usize },

    #[error("profile name '{name}' already exists")]
    ProfileNameTaken { name: String },
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `Result<T, String>` | Typed error enums | Always better | Programmatic handling |
| Manual Error impl | thiserror derive | thiserror 1.0 (2019) | Less boilerplate |
| Single crate error | Per-module errors | Best practice | Better organization |
| Error wrapping only | Error + context | Best practice | Better debugging |

**Deprecated/outdated:**
- `failure` crate: Replaced by thiserror + anyhow ecosystem
- `error-chain`: Unmaintained, use thiserror instead

## Open Questions

None - thiserror 2.x is stable and well-documented.

## Sources

### Primary (HIGH confidence)
- [thiserror 2.0.17 docs](https://docs.rs/thiserror/2.0.17/thiserror/) - Official documentation
- [nrc.github.io/error-docs](https://nrc.github.io/error-docs/error-design/error-type-design.html) - Error type design patterns

### Secondary (MEDIUM confidence)
- [Rust Error Handling 2025](https://markaicode.com/rust-error-handling-2025-guide/) - Current best practices
- [GreptimeDB Error Handling](https://greptime.com/blogs/2024-05-07-error-rust) - Large project patterns
- [tracing-error docs](https://docs.rs/tracing-error) - Span trace integration

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - thiserror is universally adopted
- Architecture: HIGH - per-module pattern well-established
- Pitfalls: HIGH - documented in official sources
- Code examples: HIGH - based on official thiserror patterns

**Research date:** 2026-01-17
**Valid until:** 2026-04-17 (stable patterns, unlikely to change)
