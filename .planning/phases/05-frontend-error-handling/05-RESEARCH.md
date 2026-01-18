# Phase 5: Frontend Error Handling - Research

**Researched:** 2026-01-17
**Domain:** Dioxus frontend error handling, toast notifications, graceful degradation
**Confidence:** HIGH

## Summary

This research covers systematic error handling for the Dioxus frontend in `app/src/`. The goal is to:
1. Remove all `.unwrap()` calls from production code
2. Display toast notifications for transient errors
3. Show error states gracefully without freezing the UI
4. Enable user recovery without reloading

After analyzing the codebase, there are **approximately 180 `.unwrap()` calls** in `app/src/`, primarily falling into two categories:
1. **JS interop via `js_sys::Reflect::set`** - These are infallible in practice but panic on theory
2. **Sorting/comparison operations** - `partial_cmp().unwrap()` for floats

The existing codebase already demonstrates good error handling patterns in several components (effect_editor with save_status, data_explorer with LoadState enum). The main work is:
- Building a global toast notification system
- Converting API calls to propagate errors to toasts
- Removing remaining unwrap calls with fallible helpers

**Primary recommendation:** Create a custom toast system using Dioxus GlobalSignal (dioxus-toast is not compatible with Dioxus 0.7), then convert API functions to show toasts on failure while keeping UI functional.

## Standard Stack

The established libraries/tools for this phase:

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| dioxus | 0.7.2 | UI framework | Already in use |
| web-sys | 0.3 | Browser APIs (console logging) | Already in use |
| gloo-timers | 0.3 | Toast auto-dismiss timers | Already in use |

### Not Using
| Library | Why Not |
|---------|---------|
| dioxus-toast | Only supports Dioxus 0.6, not compatible with 0.7 |
| dioxus-notification | Native notifications, not UI toasts |

**Installation:**
No new dependencies required. All necessary libraries are already present.

## Architecture Patterns

### Recommended Project Structure

```
app/src/
├── api.rs                    # Tauri API wrappers (already exists)
├── components/
│   ├── mod.rs               # Add toast exports
│   ├── toast.rs             # NEW: Toast component + manager
│   └── ... existing ...
└── app.rs                   # Add toast provider at root
```

### Pattern 1: Global Toast via Context

**What:** A toast manager stored in context, accessible from any component.

**When to use:** All error display throughout the app.

**Example:**
```rust
// toast.rs
use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToastSeverity {
    Normal,
    Critical,
}

#[derive(Clone)]
pub struct Toast {
    pub id: u32,
    pub message: String,
    pub severity: ToastSeverity,
}

#[derive(Clone)]
pub struct ToastManager {
    toasts: Signal<Vec<Toast>>,
    next_id: Signal<u32>,
}

impl ToastManager {
    pub fn new() -> Self {
        Self {
            toasts: Signal::new(vec![]),
            next_id: Signal::new(0),
        }
    }

    pub fn show(&self, message: impl Into<String>, severity: ToastSeverity) {
        let id = self.next_id.peek().clone();
        *self.next_id.write() += 1;

        let toast = Toast {
            id,
            message: message.into(),
            severity,
        };

        self.toasts.write().push(toast);

        // Auto-dismiss
        let toasts = self.toasts;
        let duration = match severity {
            ToastSeverity::Normal => 5000,
            ToastSeverity::Critical => 7000,
        };

        spawn(async move {
            TimeoutFuture::new(duration).await;
            toasts.write().retain(|t| t.id != id);
        });
    }

    pub fn dismiss(&self, id: u32) {
        self.toasts.write().retain(|t| t.id != id);
    }
}

// Provide at app root
pub fn use_toast_provider() -> ToastManager {
    use_context_provider(ToastManager::new)
}

// Access from any component
pub fn use_toast() -> ToastManager {
    use_context::<ToastManager>()
}
```

### Pattern 2: API Result Handling with Toast

**What:** API functions that show toast on error but don't block UI.

**When to use:** All fire-and-forget operations (save, update, delete).

**Example:**
```rust
// In a component
let toast = use_toast();

let on_save = move |_| {
    spawn(async move {
        match api::update_config(&config).await {
            Ok(_) => {
                // Silent success OR optional success toast
            }
            Err(e) => {
                toast.show(&e, ToastSeverity::Normal);
                // UI continues - form keeps user data
            }
        }
    });
};
```

### Pattern 3: LoadState Enum for Async Data

**What:** Track loading/loaded/error states explicitly.

**When to use:** Components that fetch data on mount.

**Already exists in codebase (data_explorer.rs):**
```rust
#[derive(Clone, PartialEq, Default)]
enum LoadState {
    #[default]
    Idle,
    Loading,
    Loaded,
    Error(String),
}
```

### Pattern 4: Fallible JS Interop Helper

**What:** A helper that catches js_sys::Reflect failures instead of panicking.

**When to use:** All `js_sys::Reflect::set` calls (currently ~160 unwraps).

**Example:**
```rust
// utils.rs
use wasm_bindgen::JsValue;

/// Set a property on a JS object. Logs error and continues on failure.
pub fn js_set(obj: &JsValue, key: &str, value: &JsValue) {
    if let Err(e) = js_sys::Reflect::set(obj, &JsValue::from_str(key), value) {
        web_sys::console::error_1(&format!("Failed to set {}: {:?}", key, e).into());
    }
}

/// Set a property, returning Result for cases that need error handling.
pub fn try_js_set(obj: &JsValue, key: &str, value: &JsValue) -> Result<(), String> {
    js_sys::Reflect::set(obj, &JsValue::from_str(key), value)
        .map(|_| ())
        .map_err(|e| format!("Failed to set {}: {:?}", key, e))
}
```

### Pattern 5: Safe Float Comparison

**What:** Replace `partial_cmp().unwrap()` with total ordering.

**When to use:** Sorting floats (charts_panel.rs line 98).

**Example:**
```rust
// Before
windows.sort_by(|a, b| a.start_secs.partial_cmp(&b.start_secs).unwrap());

// After - handle NaN gracefully
windows.sort_by(|a, b| {
    a.start_secs.partial_cmp(&b.start_secs).unwrap_or(std::cmp::Ordering::Equal)
});
```

### Anti-Patterns to Avoid

- **Silent failures:** Always log errors even if not showing to user
- **Blocking UI on error:** UI should remain interactive
- **Technical messages to user:** Per CONTEXT.md, user-friendly only
- **Modal error dialogs:** Use toasts, not blocking modals
- **Retry buttons everywhere:** Only for critical sections

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Toast library | Import dioxus-toast | Custom ToastManager | dioxus-toast incompatible with 0.7 |
| Complex error enum | ErrorType enum for frontend | Simple String | Backend already converts to String |
| Error boundary | Try to catch panics | Prevent panics with Result | WASM panics are unrecoverable |
| Retry logic | Complex retry system | Single silent retry | Per CONTEXT.md decision |

**Key insight:** The frontend error handling is primarily about UI feedback. The error messages are already user-friendly strings from the backend (Phase 4). This phase wraps those in toast UI.

## Common Pitfalls

### Pitfall 1: Ignoring JS Interop Errors

**What goes wrong:** `js_sys::Reflect::set().unwrap()` panics on edge cases.
**Why it happens:** Seems infallible but can fail with invalid keys or frozen objects.
**How to avoid:** Use the `js_set` helper that logs and continues.
**Warning signs:** Any `.unwrap()` after `js_sys::Reflect::set`.

### Pitfall 2: Blocking UI During Error State

**What goes wrong:** Component becomes unresponsive after error.
**Why it happens:** Not separating "data error" from "component usability".
**How to avoid:** Keep UI interactive, show error message inline or toast.
**Pattern:** Show error banner but keep form inputs editable.

### Pitfall 3: Losing Context on Async Errors

**What goes wrong:** Toast shows but user doesn't know what failed.
**Why it happens:** Generic error messages like "Operation failed".
**How to avoid:** Include operation context: "Failed to save profile".
**Per CONTEXT.md:** Messages user-friendly but specific.

### Pitfall 4: Memory Leaks from Uncleared Toasts

**What goes wrong:** Toasts accumulate if auto-dismiss fails.
**Why it happens:** Spawn doesn't complete if component unmounts.
**How to avoid:** Store toasts with timestamps, periodic cleanup.
**Pattern:** Max toast limit (5-10), oldest removed first.

### Pitfall 5: Race Conditions in Toast Display

**What goes wrong:** Rapid operations show overlapping/conflicting toasts.
**Why it happens:** Each operation spawns independent toast.
**How to avoid:** Debounce similar errors, or queue with dedup.
**Per CONTEXT.md:** Auto-retry once silently before showing error.

## Code Examples

### Example 1: Toast Component

```rust
// components/toast.rs
use dioxus::prelude::*;

#[component]
pub fn ToastFrame() -> Element {
    let manager = use_toast();
    let toasts = manager.toasts.read();

    rsx! {
        div { class: "toast-container",
            for toast in toasts.iter() {
                div {
                    key: "{toast.id}",
                    class: match toast.severity {
                        ToastSeverity::Normal => "toast",
                        ToastSeverity::Critical => "toast toast-critical",
                    },
                    span { class: "toast-icon",
                        i { class: "fa-solid fa-triangle-exclamation" }
                    }
                    span { class: "toast-message", "{toast.message}" }
                    button {
                        class: "toast-close",
                        onclick: {
                            let id = toast.id;
                            move |_| manager.dismiss(id)
                        },
                        "X"
                    }
                }
            }
        }
    }
}
```

### Example 2: Toast CSS

```css
/* In styles.css */
.toast-container {
    position: fixed;
    bottom: 20px;
    right: 20px;
    z-index: 9999;
    display: flex;
    flex-direction: column;
    gap: 10px;
    max-width: 400px;
}

.toast {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 16px;
    background: var(--color-surface-elevated);
    border: 1px solid var(--color-warning);
    border-radius: 8px;
    color: var(--color-text);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
    animation: slideIn 0.3s ease-out;
}

.toast-critical {
    border-color: var(--color-error);
    background: rgba(255, 100, 100, 0.1);
}

.toast-icon {
    color: var(--color-warning);
}

.toast-critical .toast-icon {
    color: var(--color-error);
}

.toast-close {
    margin-left: auto;
    background: none;
    border: none;
    color: var(--color-text-muted);
    cursor: pointer;
    padding: 4px;
}

@keyframes slideIn {
    from {
        transform: translateX(100%);
        opacity: 0;
    }
    to {
        transform: translateX(0);
        opacity: 1;
    }
}
```

### Example 3: Converting API Function

```rust
// Before (api.rs)
pub async fn save_profile(name: &str) -> bool {
    let _result = invoke("save_profile", build_args("name", &name)).await;
    true  // Always returns true, ignores errors
}

// After (api.rs)
pub async fn save_profile(name: &str) -> Result<(), String> {
    try_invoke("save_profile", build_args("name", &name)).await?;
    Ok(())
}

// Usage in component
let on_save = move |_| {
    let name = profile_name.read().clone();
    spawn(async move {
        if let Err(e) = api::save_profile(&name).await {
            toast.show(format!("Failed to save profile: {}", e), ToastSeverity::Normal);
        }
    });
};
```

### Example 4: Silent Retry Pattern

```rust
// Per CONTEXT.md: "Auto-retry once silently before showing error"
async fn with_retry<T, F, Fut>(operation: F) -> Result<T, String>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, String>>,
{
    match operation().await {
        Ok(result) => Ok(result),
        Err(_first_error) => {
            // Silent retry once
            operation().await
        }
    }
}

// Usage
let result = with_retry(|| api::update_config(&config)).await;
if let Err(e) = result {
    toast.show(e, ToastSeverity::Normal);
}
```

### Example 5: Error State in Section

```rust
// Per CONTEXT.md: "Failed sections show: error message + warning icon"
#[component]
fn DataSection(data: Signal<Option<Vec<Item>>>, error: Signal<Option<String>>) -> Element {
    rsx! {
        if let Some(ref msg) = error() {
            div { class: "section-error",
                i { class: "fa-solid fa-triangle-exclamation" }
                span { "{msg}" }
            }
        } else if let Some(ref items) = data() {
            // Render normal content
            for item in items {
                ItemRow { item: item.clone() }
            }
        } else {
            div { class: "section-loading", "Loading..." }
        }
    }
}
```

## Inventory: Unwrap Calls in app/src/

### JS Interop (js_sys::Reflect::set)

| File | Count | Pattern |
|------|-------|---------|
| api.rs | ~100 | Building Tauri invoke args |
| charts_panel.rs | ~50 | Building ECharts options |
| data_explorer.rs | ~30 | Building ECharts options |

**Strategy:** Create `js_set` helper, bulk replace.

### Float Comparisons

| File | Line | Call |
|------|------|------|
| charts_panel.rs | 98 | `partial_cmp().unwrap()` |

**Strategy:** Use `.unwrap_or(Ordering::Equal)`.

### Other

| File | Line | Call | Strategy |
|------|------|------|----------|
| main.rs | 13 | `.expect("failed to init logger")` | Keep - startup code |

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Silent API failures | Return Result, show toast | This phase | User sees feedback |
| `.unwrap()` on JS interop | Fallible helper | This phase | No panic risk |
| Per-component error state | Global toast manager | This phase | Consistent UX |

**Deprecated/outdated:**
- dioxus-toast: Only supports Dioxus 0.6, not 0.7

## Integration with Phase 4

Phase 4 ensured all Tauri commands return `Result<T, String>` with user-friendly error messages. This phase:
- Leverages those error strings in toasts
- Uses `try_invoke` (already exists in api.rs) for fallible calls
- Converts existing `invoke` calls that ignore errors to use `try_invoke`

The `try_invoke` function already catches promise rejections:
```rust
async fn try_invoke(cmd: &str, args: JsValue) -> Result<JsValue, String> {
    // ... catches rejections, returns Err(String)
}
```

## Open Questions

1. **Toast stacking limit:** How many toasts before oldest is removed?
   - Recommendation: 5 max, remove oldest when exceeded

2. **Success toasts:** Should successful operations show toasts?
   - Per CONTEXT.md: No guidance given
   - Recommendation: No, only show errors. Success is implicit.

3. **Form validation timing:** Validate on blur or submit?
   - Per CONTEXT.md: "Both border highlight AND error message below field"
   - Recommendation: Validate on change/blur for immediate feedback

## Sources

### Primary (HIGH confidence)
- [Dioxus 0.7 Global Context](https://dioxuslabs.com/learn/0.7/essentials/basics/context/) - Context provider pattern
- [Dioxus 0.7 Async](https://dioxuslabs.com/learn/0.7/essentials/basics/async/) - spawn, cancel safety
- Codebase analysis - Direct reading of app/src/

### Secondary (MEDIUM confidence)
- [dioxus-toast](https://github.com/mrxiaozhuox/dioxus-toast) - API inspiration (not compatible with 0.7)
- [Dioxus State Management](https://deepwiki.com/DioxusLabs/dioxus/3-reactivity-and-state-management) - Signal patterns

### Context Documents
- 05-CONTEXT.md - User decisions on toast UX
- 04-RESEARCH.md - Backend error patterns

## Metadata

**Confidence breakdown:**
- Toast pattern: HIGH - Based on Dioxus context docs + existing patterns
- JS interop cleanup: HIGH - Direct codebase analysis
- API conversion: HIGH - try_invoke already exists
- CSS/styling: MEDIUM - Based on project patterns, may need adjustment

**Research date:** 2026-01-17
**Valid until:** Until Phase 5 completes

## Summary Statistics

| Category | Count | Strategy |
|----------|-------|----------|
| JS Reflect unwraps | ~180 | Create js_set helper |
| Float comparison unwraps | 1 | Use unwrap_or |
| API functions returning bool | ~20 | Convert to Result |
| Startup expects | 1 | Keep (initialization) |
| **Total unwrap/expect** | ~180 | |
| Components needing toast | ~15 | Add use_toast() |
| CSS additions | 1 | Toast container styles |
