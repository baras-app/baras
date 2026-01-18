# Phase 9: Session Page Polish - Research

**Researched:** 2026-01-18
**Domain:** Dioxus frontend UI, Rust backend session state, Tauri IPC
**Confidence:** HIGH

## Summary

This phase polishes the session page UI with better empty states, historical session display improvements, live/historical indicators, and Parsely upload access. The codebase already has all the infrastructure needed:

- Toast notification system for upload feedback
- Parsely upload API in `api.rs` and backend in `commands/parsely.rs`
- Session state tracking via `SessionInfo` struct
- Live vs historical detection via `is_live_tailing()` flag
- Font Awesome icons throughout the UI

**Primary recommendation:** This is primarily a frontend UI task with a small backend bug fix. No new dependencies needed - all work uses existing Dioxus patterns, signals, and API bindings.

## Standard Stack

The established libraries/tools for this domain:

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| Dioxus | 0.7.2 | Reactive UI framework | Already in use, handles all frontend rendering |
| Font Awesome | 6.5.1 | Icon library | Already loaded, provides play/pause/upload icons |
| Tauri | 2.x | Desktop framework + IPC | Existing API layer handles all backend communication |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| gloo-timers | (bundled) | Async timeouts | Used by toast system for auto-dismiss |
| wasm-bindgen | (bundled) | JS interop | Event listeners for session updates |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Font Awesome icons | Custom SVG | FA already loaded, consistent with existing UI |
| Toast for feedback | Modal dialog | Context decided on toast - simpler, non-blocking |

**Installation:**
No new dependencies required - all functionality uses existing stack.

## Architecture Patterns

### Existing Component Structure
```
app/src/
├── app.rs              # Main app component with session page
├── api.rs              # Tauri command bindings
├── types.rs            # SessionInfo, LogFileInfo types
└── components/
    ├── toast.rs        # Toast notification system
    └── history_panel.rs # Encounter history display
```

### Pattern 1: Signal-Based State Updates
**What:** Dioxus signals for reactive UI updates
**When to use:** All UI state that needs to react to changes
**Example:**
```rust
// Existing pattern from app.rs
let mut is_live_tailing = use_signal(|| true);
let mut session_info = use_signal(|| None::<SessionInfo>);

// Session update listener already exists
use_future(move || async move {
    let closure = Closure::new(move |_event: JsValue| {
        spawn_local(async move {
            let info = api::get_session_info().await;
            let tailing = api::is_live_tailing().await;
            let _ = session_info.try_write().map(|mut w| *w = info);
            let _ = is_live_tailing.try_write().map(|mut w| *w = tailing);
        });
    });
    api::tauri_listen("session-updated", &closure).await;
    closure.forget();
});
```

### Pattern 2: Toast Notifications
**What:** Use `use_toast()` for user feedback
**When to use:** Success/error feedback for actions like upload
**Example:**
```rust
// Existing pattern from app.rs
let mut toast = use_toast();
spawn(async move {
    match api::upload_to_parsely(&path).await {
        Ok(resp) if resp.success => {
            toast.show(format!("Uploaded: {}", resp.link.unwrap_or_default()), ToastSeverity::Normal);
        }
        Ok(resp) => {
            toast.show(format!("Upload failed: {}", resp.error.unwrap_or_default()), ToastSeverity::Normal);
        }
        Err(e) => {
            toast.show(format!("Upload error: {}", e), ToastSeverity::Critical);
        }
    }
});
```

### Pattern 3: Conditional Rendering Based on State
**What:** Show different UI based on session state
**When to use:** Empty states, live vs historical views
**Example:**
```rust
// Existing pattern - session panel rendering
if let Some(ref info) = session {
    section { class: "session-panel",
        if is_live_tailing() {
            h3 { "Live Session" }
        } else {
            h3 { "Historical Session" }
        }
        // ... content
    }
}
```

### Anti-Patterns to Avoid
- **Inline CSS:** Use CSS classes in styles.css, not inline styles
- **Direct DOM manipulation:** Use Dioxus signals and reactive patterns
- **Polling for state:** Use event listeners via `tauri_listen()` (already in place)

## Don't Hand-Roll

Problems that look simple but have existing solutions:

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Toast notifications | Custom alert system | `use_toast()` + `ToastFrame` | Already handles timing, queue, styling |
| Parsely upload | Custom HTTP client | `api::upload_to_parsely()` | Handles compression, auth, error parsing |
| Live/historical detection | Custom state tracking | `is_live_tailing()` signal | Backend already tracks this |
| Icons | Custom SVG components | Font Awesome classes | Already loaded, consistent styling |
| Session events | Custom polling | `tauri_listen("session-updated")` | Event-driven, already implemented |

**Key insight:** The infrastructure is already built. Focus on UI composition, not new systems.

## Common Pitfalls

### Pitfall 1: SessionInfo Returns None for Empty Files
**What goes wrong:** When a new empty log file is created, `session_info()` returns None because there's no parsed session cache yet
**Why it happens:** `ParsingSession` isn't created until file has content to parse
**How to avoid:** Check for `session_info() == None` with `is_watching() == true` to distinguish "no session" from "waiting for data"
**Warning signs:** UI shows nothing when it should show "Waiting for character..."

### Pitfall 2: Historical Session Missing End Time
**What goes wrong:** `SessionInfo` doesn't have end_time field, only session_start
**Why it happens:** SessionInfo was designed for live sessions
**How to avoid:** For historical sessions, calculate end time from last encounter's end_time or use file modification time
**Warning signs:** Historical session shows start time but no end time or duration

### Pitfall 3: Upload Button Path Availability
**What goes wrong:** Need file path to upload, but session page doesn't have direct access to `active_file`
**Why it happens:** `active_file` is tracked in app.rs, session panel is inside the same component
**How to avoid:** Pass `active_file` signal to session panel props OR use the existing signal already in app.rs scope
**Warning signs:** Upload button has no path to upload

### Pitfall 4: Backend Bug - Character Not Re-read
**What goes wrong:** When SWTOR creates new empty log file, watcher detects it but character shows stale data
**Why it happens:** DirectoryWatcher.handle_new_file() waits for content once, but if file stays empty for a while, character cached from previous file persists
**How to avoid:** Backend fix needed - re-read character on subsequent polls if current file is empty/short
**Warning signs:** "Waiting for character..." never transitions to character name even after login

## Code Examples

Verified patterns from the existing codebase:

### Session Panel Conditional Rendering
```rust
// Source: app/src/app.rs lines 603-659
if active_tab() == "session" {
    if let Some(ref info) = session {
        section { class: "session-panel",
            if is_live_tailing() {
                h3 { "Live Session" }
            } else {
                h3 { "Historical Session" }
            }
            // Session grid items...
        }
    }
    // History panel follows
}
```

### Toast Notification Usage
```rust
// Source: app/src/app.rs lines 1524-1542 (file browser upload)
let mut toast = use_toast();
spawn(async move {
    match api::upload_to_parsely(&p).await {
        Ok(resp) => {
            if resp.success {
                let link = resp.link.unwrap_or_default();
                upload_status.set(Some((p, true, link)));
            } else {
                let err = resp.error.unwrap_or_else(|| "Upload failed".to_string());
                upload_status.set(Some((p, false, err)));
            }
        }
        Err(e) => {
            upload_status.set(Some((p, false, e)));
        }
    }
});
```

### Status Indicator Pattern
```rust
// Source: app/src/app.rs lines 392-398
span {
    class: if !live_tailing { "status-dot paused" }
        else if watching { "status-dot watching" }
        else { "status-dot not-watching" },
    title: if !live_tailing { "Paused" } else if watching { "Watching" } else { "Not watching" }
}
```

### Empty State Pattern
```rust
// Source: app/src/components/history_panel.rs lines 232-237
if history.is_empty() {
    div { class: "history-empty",
        i { class: "fa-solid fa-inbox" }
        p { "No encounters yet" }
        p { class: "hint", "Encounters will appear here as combat occurs" }
    }
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Polling for session updates | Event-driven via `tauri_listen` | Already implemented | Lower CPU usage |
| Manual file path tracking | `active_file` signal in app.rs | Already implemented | Centralized state |

**Deprecated/outdated:**
- N/A - codebase is current

## Key Data Structures

### SessionInfo (Frontend Type)
```rust
// Source: app/src/types.rs lines 50-59
pub struct SessionInfo {
    pub player_name: Option<String>,
    pub player_class: Option<String>,
    pub player_discipline: Option<String>,
    pub area_name: Option<String>,
    pub in_combat: bool,
    pub encounter_count: usize,
    pub session_start: Option<String>,
}
```

**Note:** Missing `session_end` and `duration` for historical sessions. Backend returns same struct for both live and historical.

### LogFileInfo (Has File Path)
```rust
// Source: app/src/types.rs lines 92-100
pub struct LogFileInfo {
    pub path: String,
    pub display_name: String,
    pub character_name: Option<String>,
    pub date: String,
    pub is_empty: bool,
    pub file_size: u64,
}
```

**Note:** The `active_file` signal contains the path string, which can be used for Parsely upload.

## Implementation Approach

### Frontend Changes (app.rs)
1. **Empty state logic:** Check `session_info() == None` with `is_watching() == true`
2. **Session header:** Modify existing session panel section to show different content for live vs historical
3. **Upload button:** Add button to session header, use `active_file()` signal for path
4. **Indicator icons:** Use Font Awesome classes (fa-play, fa-pause) with color classes

### Backend Changes (Minimal)
1. **Watcher bug fix:** In `core/src/context/watcher.rs` or service handler, ensure character re-read on empty files
2. **SessionInfo enhancement:** Optionally add `session_end` and `duration_formatted` fields for historical sessions

### CSS Changes (styles.css)
1. **Historical session tint:** Add `.session-panel.historical` with subtle background variation
2. **Upload button:** Style `.btn-session-upload` consistent with existing buttons
3. **Indicator badges:** Style `.session-indicator-live` and `.session-indicator-historical`

## Open Questions

Things that couldn't be fully resolved:

1. **Duration Calculation for Historical Sessions**
   - What we know: Session start is extracted from filename; encounters have end times
   - What's unclear: Best source for session end time (last encounter end? file mod time?)
   - Recommendation: Use last encounter's end_time if available, fallback to file modification time

2. **Exact Bug Location for Character Re-read**
   - What we know: Bug is described in CONTEXT.md - watcher caches character
   - What's unclear: Exact location in code needs investigation during implementation
   - Recommendation: Trace from `DirectoryWatcher::handle_new_file()` through session creation

## Sources

### Primary (HIGH confidence)
- `app/src/app.rs` - Session panel implementation, signal patterns
- `app/src/api.rs` - Parsely upload API binding
- `app/src/components/toast.rs` - Toast notification system
- `app/src/types.rs` - SessionInfo, LogFileInfo definitions
- `app/src-tauri/src/service/handler.rs` - session_info() implementation
- `app/src-tauri/src/commands/parsely.rs` - Upload backend

### Secondary (MEDIUM confidence)
- `core/src/context/watcher.rs` - DirectoryWatcher implementation
- `app/assets/styles.css` - Existing CSS patterns

### Tertiary (LOW confidence)
- None - all research based on codebase inspection

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - All patterns verified in existing codebase
- Architecture: HIGH - Following established Dioxus/Tauri patterns already in use
- Pitfalls: HIGH - Identified from code inspection and CONTEXT.md requirements

**Research date:** 2026-01-18
**Valid until:** Indefinite - research is specific to this codebase

---

## Implementation Checklist

Based on research, the planner should create tasks for:

1. **EMPTY-01**: Empty state messaging
   - Frontend: Add conditional rendering for "No Active Session" / "Waiting for character..."
   - Backend: May need watcher fix for character re-read

2. **SESS-01**: Historical session end time and duration
   - Backend: Enhance SessionInfo or calculate in frontend from available data
   - Frontend: Display end time and duration prominently

3. **SESS-02**: Hide Area/Class/Discipline for historical
   - Frontend: Conditional rendering based on `is_live_tailing()`

4. **SESS-03**: Live vs historical icons
   - Frontend: Add indicator icons to session header and nav bar
   - CSS: Style indicator badges

5. **NAV-04**: Parsely upload button
   - Frontend: Add upload button to session header
   - Use existing `api::upload_to_parsely()` and toast feedback
