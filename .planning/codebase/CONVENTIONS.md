# Coding Conventions

**Analysis Date:** 2026-01-17

## Naming Patterns

**Files:**
- Rust: `snake_case.rs` for all source files
- Module files: `mod.rs` in directories OR directory name matching module
- Test files: Co-located in same file with `#[cfg(test)] mod tests;` OR separate `*_tests.rs` file

**Functions:**
- `snake_case` for all functions
- Builder pattern: `new()` for constructors
- Accessors: No `get_` prefix, just noun (e.g., `config()` not `get_config()`)
- Predicates: `is_` prefix (e.g., `is_valid_grid()`, `is_any()`)
- Parsing: `parse_*` prefix (e.g., `parse_entity()`, `parse_timestamp()`)

**Variables:**
- `snake_case` for all local variables
- Mutable signals in Dioxus: `let mut signal_name = use_signal(|| ...)`

**Types:**
- `PascalCase` for structs, enums, traits
- Type aliases: `PascalCase` (e.g., `type IStr = Spur`, `type Color = [u8; 4]`)
- Enum variants: `PascalCase` with data fields where needed

**Constants:**
- `SCREAMING_SNAKE_CASE` for constants
- Static globals: `SCREAMING_SNAKE_CASE` with `OnceLock` for lazy init

## Code Style

**Formatting:**
- Rust 2024 edition
- No explicit formatter config found; uses `rustfmt` defaults
- Line width: ~100 characters observed

**Linting:**
- Workspace-level clippy config in `Cargo.toml`:
  ```toml
  [workspace.lints.clippy]
  too_many_arguments = "allow"
  ```
- File-level allows: `#![allow(clippy::too_many_arguments)]` in overlay renderer

**Control Flow:**
- Use Rust 2024 let-chains for simplified conditionals:
  ```rust
  // Good
  if let Some(config_path) = boss_config_path
      && let Some(config) = load_boss_config(config_path)
  {
      cache.load_boss_definitions(config.bosses);
  }

  // Avoid
  if let Some(config_path) = boss_config_path {
      if let Some(config) = load_boss_config(config_path) {
          cache.load_boss_definitions(config.bosses);
      }
  }
  ```

## Import Organization

**Order:**
1. Standard library (`std::`)
2. External crates
3. Crate-level imports (`crate::`)
4. Super/self imports (`super::`, `self::`)

**Path Aliases:**
- `baras_core` - Core business logic crate
- `baras_types` - Shared type definitions
- `baras_overlay` - Overlay rendering crate

**Re-exports:**
- Crate roots (`lib.rs`) re-export public API with `pub use`
- Module `mod.rs` files re-export submodule types

## Error Handling

**Patterns:**
- Parsing functions return `Option<T>` for recoverable failures
- File I/O returns `Result<T, String>` with formatted error messages
- Tauri commands return `Result<T, String>` for frontend consumption

**Examples from codebase:**
```rust
// Option for parsing (core/src/combat_log/parser.rs)
pub fn parse_line(&self, line_number: u64, _line: &str) -> Option<CombatEvent>

// Result with String error for file operations (core/src/dsl/loader.rs)
pub fn load_bosses_from_file(path: &Path) -> Result<Vec<BossEncounterDefinition>, String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    // ...
}

// Custom error types where needed (core/src/timers/preferences.rs)
pub fn load(path: &Path) -> Result<Self, PreferencesError>
```

**Default Values:**
- Use `unwrap_or_default()` for safe fallbacks
- Use `unwrap_or(sensible_default)` when default should be explicit
- Parser macros use `unwrap_or_default()` for numeric parsing:
  ```rust
  macro_rules! parse_i64 {
      ($s:expr) => {
          $s.parse::<i64>().unwrap_or_default()
      };
  }
  ```

## Logging

**Framework:** `tracing` via `dioxus_logger`

**Patterns:**
- Frontend: `dioxus_logger::init(Level::INFO)` at startup
- Backend: `eprintln!("[CATEGORY]` format for debug output in tests
- Web console: `web_sys::console::error_1()` for API errors

## Comments

**When to Comment:**
- Module-level doc comments (`//!`) for crate/module purpose
- Function doc comments (`///`) for public API only
- Architecture diagrams in lib.rs using ASCII art

**Doc Comments:**
```rust
//! Baras Overlay Library
//!
//! Cross-platform overlay rendering for combat log statistics.
//!
//! # Architecture
//!
//! ```text
//! +--------------------+
//! |     overlays/      |
//! +--------------------+
//! ```
```

**Inline Comments:**
- Section dividers using Unicode box drawing:
  ```rust
  // ─────────────────────────────────────────────────────────────────────────────
  // Section Name
  // ─────────────────────────────────────────────────────────────────────────────
  ```

## Function Design

**Size:**
- Keep functions under 50 lines when possible
- Large parsing functions acceptable for self-contained logic

**Parameters:**
- Use references for input (`&str`, `&Path`)
- Use owned types when ownership transfer is needed
- Use builder pattern for complex configurations

**Return Values:**
- Return owned types from constructors
- Return `Option` for fallible lookups
- Return `Result` for I/O operations

## Module Design

**Exports:**
- Re-export commonly used types at crate root
- Keep internal helpers private (no `pub`)
- Use `pub(crate)` for crate-internal sharing

**Barrel Files:**
- `mod.rs` pattern for module organization:
  ```rust
  // core/src/encounter/mod.rs
  pub mod challenge;
  pub mod combat;
  pub mod effect_instance;

  pub use challenge::{ChallengeTracker, ChallengeValue};
  pub use combat::{ActiveBoss, CombatEncounter, ProcessingMode};
  ```

## Serde Conventions

**Field Defaults:**
- Use `#[serde(default)]` for optional fields
- Use `#[serde(default = "default_fn")]` for non-Default defaults:
  ```rust
  #[serde(default = "default_true")]
  pub show_header: bool,

  fn default_true() -> bool { true }
  ```

**Enum Serialization:**
- Tagged enums: `#[serde(tag = "type", rename_all = "snake_case")]`
- Untagged selectors: `#[serde(untagged)]` for ID-or-Name patterns
  ```rust
  #[derive(Serialize, Deserialize)]
  #[serde(untagged)]
  pub enum AbilitySelector {
      Id(u64),
      Name(String),
  }
  ```

**Aliases:**
- Use `#[serde(alias = "old_name")]` for backwards compatibility

## Memory Optimization

**String Interning:**
- Use `IStr` (interned string) for frequently repeated strings
- Located in `core/src/context/interner.rs`
- Pattern: `intern(&str) -> IStr`, `resolve(IStr) -> &'static str`

**Lazy Initialization:**
- Use `OnceLock` for global singletons:
  ```rust
  static INTERNER: OnceLock<ThreadedRodeo> = OnceLock::new();

  pub fn interner() -> &'static ThreadedRodeo {
      INTERNER.get_or_init(ThreadedRodeo::default)
  }
  ```

**Allocation Avoidance:**
- Use fixed-size arrays over `Vec` for small known-size collections:
  ```rust
  let mut brackets = [0usize; 5];  // Instead of Vec
  ```
- Use `memchr` for fast byte scanning instead of string operations

## Tauri Command Conventions

**Command Naming:**
- `snake_case` matching Rust function names
- Grouped by feature in separate modules under `commands/`

**Return Types:**
- Return `Result<T, String>` for error propagation to frontend
- Return simple types (`bool`, `Vec<T>`) for success-only operations

**State Access:**
- Use `State<'_, ServiceHandle>` for service access
- Commands are async when calling async service methods

---

*Convention analysis: 2026-01-17*
