# Phase 12: Overlay Improvements - Research

**Researched:** 2026-01-18
**Domain:** Dioxus frontend UI, Tauri backend state management, overlay data flow
**Confidence:** HIGH (codebase-driven research)

## Summary

This phase improves overlay customization UX through six targeted changes: move mode reset on startup, fixed save button position, live preview of settings, button tooltips, and startup data display. All requirements are implementable with the existing architecture.

The codebase already has the patterns needed for each feature:
- `OverlayState::default()` initializes `move_mode: false` - just need to ensure this happens on app startup
- Settings panel has `has_changes` signal tracking - can drive save button styling changes
- `refresh_overlay_settings()` command already sends config updates to running overlays - can be used for live preview
- Dioxus `title` attribute already used throughout for tooltips
- `last_combat_encounter()` returns the most recent encounter data - can be sent to overlays on spawn

**Primary recommendation:** Implement changes in dependency order: (1) move mode reset, (2) startup data display, (3) live preview, (4) save button styling, (5) button tooltips.

## Standard Stack

This phase uses existing stack - no new libraries needed.

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| Dioxus | existing | Frontend UI framework | Already in use |
| Tauri | existing | Backend/frontend bridge | Already in use |
| tokio | existing | Async runtime | Already in use |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| gloo-timers | existing | Debounce timers | For 300ms debounced preview |

## Architecture Patterns

### Current Settings Flow

```
User edits field in SettingsPanel
    |
    v
draft_settings.set(new_value) + has_changes.set(true)
    |
    v (on Save click)
api::update_config() -> backend persists to disk
    |
    v
api::refresh_overlay_settings() -> OverlayManager::refresh_settings()
    |
    v
For each running overlay: tx.send(OverlayCommand::UpdateConfig(...))
```

### Proposed Live Preview Flow

```
User edits field in SettingsPanel
    |
    v
draft_settings.set(new_value) + has_changes.set(true)
    |
    v (debounced 300ms)
api::preview_overlay_settings(draft) -> OverlayManager::preview_settings()
    |
    v
For each running overlay: tx.send(OverlayCommand::UpdateConfig(...))
    |
    v (NO disk persist - preview only)

On Save: existing flow (persists to disk)
On Cancel: restore from settings signal (original values)
```

### Move Mode State Location

```rust
// app/src-tauri/src/overlay/state.rs
pub struct OverlayState {
    pub overlays: HashMap<OverlayType, OverlayHandle>,
    pub move_mode: bool,         // <-- Reset this on startup
    pub rearrange_mode: bool,    // <-- Also reset on startup
    pub overlays_visible: bool,
}

impl Default for OverlayState {
    fn default() -> Self {
        Self {
            overlays: HashMap::new(),
            move_mode: false,       // Already false by default
            rearrange_mode: false,  // Already false by default
            overlays_visible: true,
        }
    }
}
```

The state is created fresh on each app startup via `Arc::new(Mutex::new(OverlayState::default()))` in `lib.rs`.

**Finding:** Move mode already resets to false on startup. OVLY-01 may already be satisfied - verify with testing.

### Settings Panel Structure

```
settings-panel (section)
  |-- settings-header (div, draggable)
  |     |-- h3 "Overlay Settings"
  |     |-- btn-close
  |
  |-- [Profiles collapsible]
  |-- [Tab buttons]
  |-- [Tab content - scrollable]
  |
  |-- settings-footer (div)
        |-- btn-save
        |-- save-status span
```

**Issue:** The footer is at the bottom of the panel content, not fixed. When content is long, user must scroll to see Save button.

### Overlay Data Flow on Startup

```rust
// lib.rs: spawn_auto_show_overlays()
async move {
    tokio::time::sleep(Duration::from_millis(500)).await;

    let config = service_handle.config().await;
    if !config.overlay_settings.overlays_visible {
        return;
    }

    let _ = OverlayManager::show_all(&overlay_state, &service_handle).await;
}

// OverlayManager::show() calls send_initial_data():
pub async fn send_initial_data(
    kind: OverlayType,
    tx: &Sender<OverlayCommand>,
    combat_data: Option<&CombatData>,  // <-- Only passed if service.is_tailing()
) {
    let Some(data) = combat_data else { return };
    // ... sends data to overlay
}

// Issue: is_tailing() returns false on fresh startup before any file is watched
// Result: Overlays spawn empty even if there's session data available
```

### Button Tooltip Pattern

Already used throughout the app:
```rust
button {
    class: "...",
    title: "Tooltip text shown on hover",  // <-- Standard HTML title attribute
    onclick: move |_| { ... },
    "Button Label"
}
```

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Debouncing | Custom timer management | `gloo_timers::callback::Timeout` | Already available, handles cleanup |
| Tooltips | Custom tooltip components | HTML `title` attribute | Native browser behavior, consistent |
| State sync | Manual prop drilling | Existing Dioxus signals | Already the established pattern |

## Common Pitfalls

### Pitfall 1: Race Conditions in Preview
**What goes wrong:** Preview sends config updates while user is still typing, causing visual jitter
**Why it happens:** No debouncing, every keystroke triggers update
**How to avoid:** Use 300ms debounce before sending preview updates
**Warning signs:** Overlays visually jumping/flickering during editing

### Pitfall 2: Memory of Preview State
**What goes wrong:** User cancels settings but overlay keeps preview appearance
**Why it happens:** Preview updated overlay but cancel doesn't restore original
**How to avoid:** On cancel, explicitly restore original settings via refresh_overlay_settings()
**Warning signs:** Overlay appearance doesn't match saved settings

### Pitfall 3: Save Button Not Visible
**What goes wrong:** User makes changes but can't find Save button
**Why it happens:** Save button scrolls with content instead of being fixed
**How to avoid:** Use CSS `position: sticky` or restructure panel with fixed footer
**Warning signs:** Users report not knowing how to save

### Pitfall 4: Startup Data Timing
**What goes wrong:** Overlays still empty even after "fix"
**Why it happens:** Data not available yet when overlays spawn (race condition)
**How to avoid:** Send data from cache regardless of tailing state, or retry after delay
**Warning signs:** Overlays blank on startup even with session data

## Code Examples

### Pattern 1: Debounced Preview (Dioxus)
```rust
// In SettingsPanel component
let mut debounce_handle = use_signal(|| None::<gloo_timers::callback::Timeout>);

let mut update_with_preview = move |new_settings: OverlaySettings| {
    draft_settings.set(new_settings.clone());
    has_changes.set(true);

    // Cancel previous debounce
    if let Some(handle) = debounce_handle.take() {
        handle.cancel();
    }

    // Set new debounced preview
    let handle = gloo_timers::callback::Timeout::new(300, move || {
        spawn(async move {
            api::preview_overlay_settings(&new_settings).await;
        });
    });
    debounce_handle.set(Some(handle));
};
```

### Pattern 2: Fixed Footer CSS
```css
.settings-panel {
    display: flex;
    flex-direction: column;
    max-height: 80vh;  /* Constrain total height */
}

.settings-content {
    flex: 1;
    overflow-y: auto;  /* Scrollable content */
}

.settings-footer {
    flex-shrink: 0;    /* Never shrink */
    position: sticky;
    bottom: 0;
    /* existing styles */
}
```

### Pattern 3: Unsaved Changes Save Button Styling
```rust
button {
    class: if has_changes() { "btn btn-save btn-unsaved" } else { "btn btn-save btn-disabled" },
    disabled: !has_changes(),
    onclick: save_to_backend,
    if has_changes() { "Save Settings*" } else { "Save Settings" }
}
```

```css
.btn-save.btn-unsaved {
    background: var(--color-warning-bg);
    border-color: var(--color-warning-border);
    animation: pulse-glow 2s infinite;
}
```

### Pattern 4: Button Tooltips
```rust
// Current (no tooltip)
button {
    class: "btn btn-overlay",
    onclick: move |_| { ... },
    "Boss Health"
}

// With tooltip
button {
    class: "btn btn-overlay",
    title: "Displays boss health bars and cast timers",
    onclick: move |_| { ... },
    "Boss Health"
}
```

### Pattern 5: Startup Data from Cache
```rust
// In OverlayManager::send_initial_data or show_all
// Don't gate on is_tailing(), use cached data if available
pub async fn send_initial_data_from_cache(
    kind: OverlayType,
    tx: &Sender<OverlayCommand>,
    service: &ServiceHandle,
) {
    // Always try to get data, regardless of tailing state
    let combat_data = service.current_combat_data().await;

    let Some(data) = combat_data else { return };
    if data.metrics.is_empty() { return; }

    match kind {
        OverlayType::Metric(metric_type) => {
            let entries = create_entries_for_type(metric_type, &data.metrics);
            let _ = tx.send(OverlayCommand::UpdateData(OverlayData::Metrics(entries))).await;
        }
        // ... other overlay types
    }
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Save-only settings | Live preview | Modern UX | User sees changes immediately |
| Scroll to save | Fixed/sticky footer | Modern UX | Critical actions always visible |

## Key Implementation Details

### OVLY-01: Move Mode Reset
- **Location:** `app/src-tauri/src/overlay/state.rs` - `OverlayState::default()`
- **Current behavior:** Already initializes `move_mode: false`
- **Action needed:** Verify this is actually called on startup (it is, in `lib.rs` line 58)
- **Profile switch:** Add `state.move_mode = false` in profile load handler

### OVLY-02: Fixed Save Button
- **Location:** `app/src/components/settings_panel.rs` - settings-footer div
- **CSS Location:** `app/assets/styles.css` - `.settings-footer`
- **Action needed:** Restructure panel to have fixed footer, scrollable content area

### OVLY-03: Live Preview
- **Frontend:** Add debounced preview call in `SettingsPanel` update handlers
- **Backend:** Create `preview_overlay_settings` command that updates without persisting
- **Restore:** On cancel, call `refresh_overlay_settings()` to restore persisted values

### OVLY-04: Button Tooltips
- **Location:** `app/src/app.rs` - overlay toggle buttons in Overlays tab
- **Pattern:** Add `title` attribute to non-metric overlay buttons
- **Buttons needing tooltips:** Personal Stats, Raid Frames, Boss Health, Encounter Timers, Challenges, Alerts, Effects A/B, Cooldowns, DOT Tracker

### OVLY-06: Customize Button Clarity
- **Location:** `app/src/app.rs` - line 792-797
- **Current:** `" Customize"`
- **Options:** Change to `" Customize Overlays"` or add `title` attribute

### EMPTY-02: Startup Data Display
- **Location:** `app/src-tauri/src/overlay/manager.rs` - `send_initial_data()`
- **Issue:** Data only sent if `service.is_tailing()` is true
- **Fix:** Always attempt to get cached data, regardless of tailing state

## Open Questions

None - all requirements have clear implementation paths based on existing patterns.

## Sources

### Primary (HIGH confidence)
- `/home/prescott/baras/app/src-tauri/src/overlay/state.rs` - OverlayState definition
- `/home/prescott/baras/app/src-tauri/src/overlay/manager.rs` - Overlay lifecycle management
- `/home/prescott/baras/app/src/components/settings_panel.rs` - Settings UI component
- `/home/prescott/baras/app/src/app.rs` - Main app UI with overlay buttons
- `/home/prescott/baras/app/assets/styles.css` - CSS styling
- `/home/prescott/baras/.planning/phases/12-overlay-improvements/12-CONTEXT.md` - User decisions

## Metadata

**Confidence breakdown:**
- Move mode reset: HIGH - code inspection shows default behavior
- Settings UI: HIGH - full component source reviewed
- Data flow: HIGH - traced through manager and service
- Button tooltips: HIGH - pattern already established in codebase

**Research date:** 2026-01-18
**Valid until:** 2026-02-18 (stable patterns, no external dependencies)
