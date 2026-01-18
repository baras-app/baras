# Phase 4: Backend Error Handling - Research

**Researched:** 2026-01-17
**Domain:** Tauri backend error handling - IPC error return patterns
**Confidence:** HIGH

## Summary

This research covers the systematic removal of `.unwrap()` and `.expect()` calls in `app/src-tauri/src` and ensuring all Tauri commands return proper `Result<T, String>` for frontend consumption. The goal is to prevent backend panics from breaking IPC and to provide user-friendly error messages.

After analyzing the codebase, there are **12 production unwrap calls** and **1 expect call** that need to be converted. These fall into three categories:

1. **Mutex lock poisoning** (3 unwraps) - updater.rs
2. **Path parent() fallback chains** (8 unwraps) - effects.rs, service/mod.rs (dev-mode fallbacks)
3. **Tauri application startup** (1 expect) - lib.rs
4. **Tray icon** (1 unwrap) - tray.rs

All Tauri commands already return `Result<T, String>` with `.map_err(|e| e.to_string())` pattern. The existing error handling pattern is consistent and follows Tauri best practices.

**Primary recommendation:** Convert the remaining unwraps using defensive fallbacks for dev paths, poison recovery for mutex locks, and explicit error messages for tray setup.

## Inventory

### Production Unwrap/Expect Calls (13 total)

| File | Line | Call | Category | Migration Strategy |
|------|------|------|----------|-------------------|
| updater.rs | 46 | `.lock().unwrap()` | Mutex poison | Recover or explicit error |
| updater.rs | 62 | `.lock().unwrap()` | Mutex poison | Recover or explicit error |
| updater.rs | 111 | `.lock().unwrap()` | Mutex poison | Recover or explicit error |
| tray.rs | 35 | `.unwrap().clone()` | Option access | Handle None case |
| commands/effects.rs | 496 | `.parent().unwrap()` | Dev fallback path | Use safer fallback |
| commands/effects.rs | 498 | `.parent().unwrap()` | Dev fallback path | Use safer fallback |
| commands/effects.rs | 549 | `.parent().unwrap()` | Dev fallback path | Use safer fallback |
| commands/effects.rs | 551 | `.parent().unwrap()` | Dev fallback path | Use safer fallback |
| service/mod.rs | 358 | `.parent().unwrap()` | Dev fallback path | Use safer fallback |
| service/mod.rs | 360 | `.parent().unwrap()` | Dev fallback path | Use safer fallback |
| service/mod.rs | 471 | `.parent().unwrap()` | Dev fallback path | Use safer fallback |
| service/mod.rs | 473 | `.parent().unwrap()` | Dev fallback path | Use safer fallback |
| lib.rs | 245 | `.expect("error while running tauri application")` | App startup | Keep (intentional startup failure) |

### Commands Already Returning Result<T, String>

All 60+ Tauri commands already follow the `Result<T, String>` pattern. Examples:

```rust
#[tauri::command]
pub async fn start_tailing(path: PathBuf, handle: State<'_, ServiceHandle>) -> Result<(), String> {
    handle.start_tailing(path).await
}

#[tauri::command]
pub async fn update_config(config: AppConfig, handle: State<'_, ServiceHandle>) -> Result<(), String> {
    handle.update_config(config).await
}
```

The pattern `.map_err(|e| e.to_string())` is used consistently throughout for converting internal errors to strings.

## Standard Stack

Tauri error handling uses the following established patterns:

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| tauri | 2.x | IPC framework | Commands return Result<T, E> where E: Serialize |
| thiserror | 1.0 | Error derivation | Already used in core crate |
| serde | 1.0 | Serialization | Required for error transport |
| tracing | 0.1 | Logging | Already configured in Phase 1 |

### Error Serialization
Tauri requires all command return types (including errors) to implement `serde::Serialize`. The project uses `Result<T, String>` which satisfies this requirement with minimal complexity.

## Architecture Patterns

### Pattern 1: Result<T, String> for Commands

**What:** All Tauri commands return `Result<T, String>` where the error is a human-readable message.

**Why:** Per CONTEXT.md decisions:
- "Errors sent over IPC as simple `Result<T, String>` - no structured JSON"
- "Backend logs full error chain; frontend gets clean message only"

**Example (existing pattern):**
```rust
#[tauri::command]
pub async fn load_profile(name: String, handle: State<'_, ServiceHandle>) -> Result<(), String> {
    let mut config = handle.config().await;
    config.load_profile(&name).map_err(|e| e.to_string())?;
    *handle.shared.config.write().await = config.clone();
    config.save().map_err(|e| e.to_string())?;
    Ok(())
}
```

### Pattern 2: Error Logging at Boundary

**What:** Log full error details at the command boundary, return simplified message to frontend.

**Per CONTEXT.md:** "Log only on errors, not on success"

**Pattern:**
```rust
#[tauri::command]
pub async fn some_command(/* ... */) -> Result<T, String> {
    match internal_operation().await {
        Ok(result) => Ok(result),
        Err(e) => {
            tracing::error!(error = %e, "Operation failed");
            Err(user_friendly_message(&e))
        }
    }
}
```

**Note:** Currently most commands just use `.map_err(|e| e.to_string())` without logging. Phase 4 should add tracing calls where appropriate.

### Pattern 3: Mutex Poison Recovery

**What:** For mutex locks that should never fail in practice, recover from poisoning.

**Why:** Mutex poisoning indicates a prior panic in another thread. For application state like `PendingUpdate`, recovery is preferable to propagating the panic.

**Pattern:**
```rust
// Before
let guard = state.0.lock().unwrap();

// After - recover from poison
let guard = state.0.lock().unwrap_or_else(|poisoned| {
    tracing::warn!("Mutex poisoned, recovering");
    poisoned.into_inner()
});
```

**Alternative - return error:**
```rust
let guard = state.0.lock().map_err(|_| "Lock poisoned")?;
```

### Pattern 4: Dev Fallback Path Chains

**What:** The codebase uses chained `.parent().unwrap()` calls for development-mode resource paths.

**Context:** These occur inside `unwrap_or_else` closures that are only reached when:
1. Production resource path doesn't exist, AND
2. Running in development mode

**Current pattern:**
```rust
let icons_dir = app_handle
    .path()
    .resolve("icons", tauri::path::BaseDirectory::Resource)
    .ok()
    .filter(|p| p.exists())
    .unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()  // src-tauri -> app
            .unwrap()
            .parent()  // app -> project root
            .unwrap()
            .join("icons")
    });
```

**Migration options:**

A. **Nested option handling with default:**
```rust
.unwrap_or_else(|| {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)  // Go up 2 levels
        .map(|p| p.join("icons"))
        .unwrap_or_else(|| PathBuf::from("icons"))  // Ultimate fallback
})
```

B. **Early return with error (for functions returning Result):**
```rust
let icons_dir = app_handle
    .path()
    .resolve("icons", tauri::path::BaseDirectory::Resource)
    .ok()
    .filter(|p| p.exists())
    .or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .map(|p| p.join("icons").to_path_buf())
    })
    .ok_or("Could not locate icons directory")?;
```

### Pattern 5: Validation Error Format

**Per CONTEXT.md:** "Validation errors are the exception: include field name in message"

**Format:** `"Invalid value for 'field_name': reason"`

**Example:**
```rust
if config.grid_columns < 1 {
    return Err("Invalid value for 'grid_columns': must be at least 1".to_string());
}
```

### Anti-Patterns to Avoid

- **Structured error JSON:** Decision was made to keep errors as simple strings
- **Error categories:** No explicit categorization needed
- **Recovery guidance:** "No guidance in error messages - just state what went wrong"
- **Chained source errors to frontend:** Frontend gets clean message only

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Error type for IPC | Custom enum with Serialize | `String` | CONTEXT.md decision: simple strings |
| Error logging | Manual println/eprintln | tracing macros | Already configured in Phase 1 |
| Error conversion | Manual From impls | `.map_err(\|e\| e.to_string())` | Consistent with existing code |

**Key insight:** The `Result<T, String>` pattern is already established and working. This phase is about removing the remaining unwraps, not redesigning error handling.

## Common Pitfalls

### Pitfall 1: Over-Engineering Error Types

**What goes wrong:** Creating complex error enums with Serialize for IPC when strings work fine.
**Why it happens:** General Rust best practice doesn't always apply to IPC boundaries.
**How to avoid:** Follow CONTEXT.md decision: `Result<T, String>` for all commands.
**Warning signs:** Creating new error types in src-tauri that implement Serialize.

### Pitfall 2: Forgetting to Log Before Converting

**What goes wrong:** Error context lost when converting to string.
**Why it happens:** `.map_err(|e| e.to_string())` discards source chain.
**How to avoid:** Log at tracing::error level before or instead of returning string.
**Pattern:**
```rust
.map_err(|e| {
    tracing::error!(error = %e, "Failed to save config");
    e.to_string()
})
```

### Pitfall 3: Technical Details in Release Errors

**What goes wrong:** Error messages expose internal details to users.
**Why it happens:** Using `e.to_string()` directly includes debug info.
**How to avoid:** Per CONTEXT.md: "Technical details in debug builds, user-friendly messages in release"
**Pattern:**
```rust
#[cfg(debug_assertions)]
let msg = format!("{:?}", e);  // Full debug info
#[cfg(not(debug_assertions))]
let msg = "Failed to save configuration".to_string();  // User-friendly
```

**Note:** For most cases in this app, `.to_string()` produces reasonable user messages since thiserror Display impls are already user-friendly.

### Pitfall 4: Ignoring Mutex Poison in Commands

**What goes wrong:** Command returns Result but panics on poisoned mutex.
**Why it happens:** Copy-pasting existing `.lock().unwrap()` pattern.
**How to avoid:** In Tauri commands, either recover or return error.

## Code Examples

### Example 1: Mutex Lock in Tauri Command

```rust
// Before (updater.rs:62)
let update = app
    .try_state::<PendingUpdate>()
    .and_then(|state| state.0.lock().unwrap().take())
    .ok_or("No pending update available")?;

// After - recover from poison
let update = app
    .try_state::<PendingUpdate>()
    .and_then(|state| {
        state.0.lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
    })
    .ok_or("No pending update available")?;

// OR - return explicit error
let state = app.try_state::<PendingUpdate>()
    .ok_or("Update state not available")?;
let update = state.0.lock()
    .map_err(|_| "An internal error occurred")?
    .take()
    .ok_or("No pending update available")?;
```

### Example 2: Dev Fallback Path

```rust
// Before (effects.rs:493-500)
.unwrap_or_else(|| {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("icons")
})

// After - using ancestors() for safety
.unwrap_or_else(|| {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
        .join("icons")
})
```

### Example 3: Tray Icon Handling

```rust
// Before (tray.rs:35)
.icon(app.default_window_icon().unwrap().clone())

// After - with fallback
.icon(
    app.default_window_icon()
        .cloned()
        .unwrap_or_else(|| {
            // Load a fallback icon or use default
            tauri::image::Image::new(&[], 32, 32)  // Empty placeholder
        })
)

// OR - propagate error (if setup_tray returns Result)
.icon(
    app.default_window_icon()
        .ok_or("No default window icon available")?
        .clone()
)
```

### Example 4: Adding Logging to Error Conversion

```rust
// Before
config.save().map_err(|e| e.to_string())?;

// After - with logging
config.save().map_err(|e| {
    tracing::error!(error = %e, "Failed to save configuration");
    e.to_string()
})?;
```

### Example 5: Internal Error Message Format

```rust
// Per CONTEXT.md: "Internal errors use honest wording: 'An internal error occurred'"

// For unexpected failures (mutex poison, missing state)
Err("An internal error occurred".to_string())

// For expected failures (file not found, invalid input)
Err("Profile 'default' not found".to_string())

// For validation errors
Err("Invalid value for 'grid_columns': must be at least 1".to_string())
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `.unwrap()` on mutex | Poison recovery or explicit error | Best practice | No IPC panic |
| Chained `.parent().unwrap()` | `.ancestors().nth(n)` | Rust idiom | Safer path traversal |
| No logging on error | `tracing::error!` before return | Phase 1 setup | Debugging support |

## Dependency on Phase 3

Phase 3 completed the following relevant changes:
- `config.save()` now returns `Result<(), ConfigError>` (not panicking)
- Error types in `core` crate are complete
- tracing is configured and ready

Handler code in `service/handler.rs` already uses fire-and-forget logging:
```rust
if let Err(e) = config.save() {
    tracing::error!(error = %e, "Failed to save configuration");
}
```

## Recommended Wave Structure

### Wave 1: Mutex Lock Handling (updater.rs)
**Count:** 3 unwraps
**Strategy:** Use `unwrap_or_else(|p| p.into_inner())` for poison recovery
**Risk:** Low - these are for transient state (pending update)

### Wave 2: Dev Fallback Paths (effects.rs, service/mod.rs)
**Count:** 8 unwraps
**Strategy:** Use `.ancestors().nth(2)` pattern with ultimate fallback
**Risk:** Low - only affects development mode

### Wave 3: Tray Setup (tray.rs)
**Count:** 1 unwrap
**Strategy:** Handle None case with fallback icon or propagate Result
**Risk:** Low - tray is non-critical functionality

### Wave 4: Application Startup (lib.rs)
**Count:** 1 expect
**Strategy:** Keep as-is OR convert to logging + graceful exit
**Note:** This is the final `.run().expect()` - panicking here is arguably correct since the app cannot function without Tauri

## Open Questions

1. **Application startup expect:** Should `tauri::Builder::run().expect()` be converted? Arguments:
   - **Keep:** App cannot run without Tauri, panic is appropriate
   - **Convert:** Consistent policy, could log and exit gracefully
   **Recommendation:** Keep. This is startup code, not command handler.

2. **Logging strategy:** Should every `.map_err(|e| e.to_string())` add a tracing call?
   - **All:** Consistent, but verbose
   - **Selective:** Only for important operations
   **Recommendation:** Selective - add logging for operations that could fail in ways users care about (file ops, config, network).

## Sources

### Primary (HIGH confidence)
- Codebase analysis - Direct grep and file reading of app/src-tauri/src
- Tauri official documentation - [Calling Rust from the Frontend](https://v2.tauri.app/develop/calling-rust/)
- Phase 3 RESEARCH.md - Error handling patterns established

### Secondary (MEDIUM confidence)
- Tauri community patterns - [Discussion #5008](https://github.com/tauri-apps/tauri/discussions/5008)
- Tauri error handling recipes - [tbt.qkation.com](https://tbt.qkation.com/posts/tauri-error-handling/)
- Rust mutex poisoning - [users.rust-lang.org](https://users.rust-lang.org/t/should-i-unwrap-a-mutex-lock/61519)

### Context Documents
- 04-CONTEXT.md - User decisions on error message format

## Metadata

**Confidence breakdown:**
- Inventory: HIGH - direct codebase analysis
- Migration strategies: HIGH - based on actual code patterns and Tauri docs
- Wave structure: HIGH - based on dependency analysis
- Error message format: HIGH - explicit decisions in CONTEXT.md

**Research date:** 2026-01-17
**Valid until:** Until Phase 4 completes

## Summary Statistics

| Category | Count | Strategy |
|----------|-------|----------|
| Mutex lock unwraps | 3 | Poison recovery |
| Dev fallback path unwraps | 8 | Use ancestors().nth() |
| Tray icon unwrap | 1 | Handle None |
| App startup expect | 1 | Keep (startup code) |
| Total unwrap/expect | 13 | |
| Commands needing changes | 0 | All already return Result<T, String> |
| Files to modify | 4-5 | updater.rs, effects.rs, service/mod.rs, tray.rs, (lib.rs optional) |
