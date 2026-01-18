# Phase 8: Platform Foundation - Research

**Researched:** 2026-01-18
**Domain:** Tauri platform features, Dioxus frontend, cross-platform compatibility
**Confidence:** HIGH

## Summary

Phase 8 involves four platform infrastructure improvements that are largely independent:

1. **Single Instance Enforcement** - Tauri has an official plugin (`tauri-plugin-single-instance`) that handles this directly. The plugin must be registered FIRST and provides a callback to focus the existing window.

2. **Windows Font Rendering (StarJedi TTF)** - The BARAS header uses a custom "StarJedi" font loaded via `@font-face`. The font path and loading mechanism are correct, but Windows WebView2 has known issues with custom fonts. Root cause is likely path resolution or font format compatibility in WebView2.

3. **Hotkey Settings Page** - Already exists in the General Settings modal. Currently shows "Windows only" hint but needs to explain Wayland/Linux limitations more clearly. The backend already conditionally compiles hotkeys module out for Linux.

4. **Alacrity/Latency Fields** - Currently buried in the Effects tab (`PlayerStatsBar` component). Context indicates they should move to the Session page near player info. Uses standard HTML `title` attribute for tooltips (already used throughout codebase).

**Primary recommendation:** Use `tauri-plugin-single-instance` for PLAT-01, investigate Windows font path/format for PLAT-02, enhance existing hotkey UI text for PLAT-03, and relocate `PlayerStatsBar` component for PLAT-04.

## Standard Stack

### Core Libraries (Already in Use)
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `tauri` | 2.x | Desktop app framework | Already in use |
| `dioxus` | 0.6.x | Frontend framework | Already in use |
| `tauri-plugin-global-shortcut` | 2.x | Hotkey registration | Already in use |

### New Dependencies Required
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `tauri-plugin-single-instance` | 2.3.x | Single instance enforcement | PLAT-01 |

### No Alternatives Needed
The existing stack handles all requirements. No new UI libraries or font rendering libraries are needed.

**Installation:**
```bash
# In app/src-tauri directory
cargo add tauri-plugin-single-instance --target 'cfg(any(target_os = "macos", windows, target_os = "linux"))'
```

## Architecture Patterns

### Single Instance Plugin Registration

**CRITICAL:** The single-instance plugin MUST be registered FIRST before all other plugins.

```rust
// app/src-tauri/src/lib.rs - Current structure
tauri::Builder::default()
    .plugin(tauri_plugin_updater::Builder::new().build())
    // ... other plugins
```

**Pattern:** Insert single-instance plugin at the very start:
```rust
// Source: https://v2.tauri.app/plugin/single-instance/
tauri::Builder::default()
    .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        // Focus existing window
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }))
    .plugin(tauri_plugin_updater::Builder::new().build())
    // ... rest of plugins
```

### Font Loading Pattern (Current)

The current pattern uses Dioxus asset loading:
```rust
// app/src/app.rs line 20
static FONT: Asset = asset!("/assets/StarJedi.ttf");

// line 324
style { "@font-face {{ font-family: 'StarJedi'; src: url('{FONT}') format('truetype'); }}" }
```

CSS usage in `app/assets/styles.css` line 219:
```css
.app-header h1 {
  font-family: "StarJedi", sans-serif;
}
```

### Component Relocation Pattern

For moving `PlayerStatsBar` from Effects tab to Session page:
```rust
// Current location: app/src/components/effect_editor.rs lines 234-298
// Move to: app/src/app.rs (inline in session tab) or create app/src/components/player_stats.rs

// Session tab currently at lines ~600-650 in app.rs
// Add PlayerStatsBar after the session-grid div
```

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Single instance detection | Custom lock file, IPC, or mutex | `tauri-plugin-single-instance` | Handles platform differences (Windows mutex, DBus on Linux) |
| Tooltips | Custom tooltip components | HTML `title` attribute | Already used throughout codebase, works cross-platform |
| Font loading | Custom font loading JS | Dioxus `asset!` macro + CSS `@font-face` | Current pattern is correct, issue is Windows WebView2 |

**Key insight:** The single-instance problem has many platform-specific edge cases (sandboxed environments, DBus on Linux, etc.) that the official plugin handles.

## Common Pitfalls

### Pitfall 1: Single Instance Plugin Order
**What goes wrong:** Plugin doesn't work or interferes with other plugins
**Why it happens:** Single-instance must check for existing instances BEFORE other plugins initialize state
**How to avoid:** Register `tauri_plugin_single_instance::init()` as the VERY FIRST plugin
**Warning signs:** App still opens multiple instances even with plugin added

### Pitfall 2: Linux Single Instance in Sandboxed Environments
**What goes wrong:** Snap/Flatpak packages can run multiple instances despite plugin
**Why it happens:** Plugin uses DBus which is sandboxed differently per package type
**How to avoid:** For now, only deb/rpm/AppImage are supported. Document this limitation.
**Warning signs:** Works in development but not in packaged form

### Pitfall 3: Windows WebView2 Font Path Resolution
**What goes wrong:** Custom fonts work in development but not in production builds
**Why it happens:** WebView2 has strict CSP and path resolution differences from browsers
**How to avoid:**
- Ensure font file has no spaces in filename (StarJedi.ttf is fine)
- Use absolute paths from web root (`/assets/`)
- Consider adding `font-display: block` to @font-face
**Warning signs:** Font works on Linux but shows fallback on Windows

### Pitfall 4: Hotkey Registration on Wayland
**What goes wrong:** User sets hotkeys on Linux/Wayland and they don't work
**Why it happens:** Wayland security model doesn't allow global hotkey interception by design
**How to avoid:** Display clear warning BEFORE user attempts to set hotkeys on Linux
**Warning signs:** Hotkey fields visible but non-functional on Linux

## Code Examples

### Single Instance with Window Focus
```rust
// Source: https://v2.tauri.app/plugin/single-instance/
use tauri::{AppHandle, Manager};

#[cfg(desktop)]
fn init_single_instance() -> impl tauri::plugin::Plugin<tauri::Wry> {
    tauri_plugin_single_instance::init(|app: &AppHandle, _args, _cwd| {
        // Get main window and focus it
        if let Some(window) = app.get_webview_window("main") {
            // Show if hidden (e.g., minimized to tray)
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
    })
}
```

### Tooltip in Dioxus (Already Used Pattern)
```rust
// Source: app/src/app.rs - existing pattern
button {
    title: "Your alacrity percentage for GCD calculations",
    // ... rest of button
}

// For input fields
div { class: "stat-input",
    label {
        title: "Your character's alacrity percentage, affects cooldown calculations",
        "Alacrity %"
    }
    input { ... }
}
```

### Platform-Conditional UI Warning
```rust
// For hotkey settings - detect platform
#[cfg(target_os = "linux")]
let is_linux = true;
#[cfg(not(target_os = "linux"))]
let is_linux = false;

// In RSX - this won't work directly in WASM frontend
// Instead, use a constant or config value passed from backend

// Simpler approach - always show the warning on the settings page:
p { class: "hint hint-warning",
    i { class: "fa-solid fa-triangle-exclamation" }
    " Global hotkeys require Windows. Linux/Wayland does not support global hotkeys due to security restrictions."
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Custom single-instance via mutex/lock file | `tauri-plugin-single-instance` v2 | Tauri 2.0 | Official plugin handles platform differences |
| WebView font loading varies | Still platform-specific | Ongoing | WebView2 (Windows) vs WebKitGTK (Linux) differ |

**Deprecated/outdated:**
- Tauri 1.x single-instance patterns are different from v2 API

## Open Questions

1. **Windows Font Root Cause**
   - What we know: Font works on Linux, fails on Windows; path looks correct
   - What's unclear: Is it path resolution, font format, or WebView2 caching?
   - Recommendation: Start by adding `font-display: block`, ensure absolute path, check DevTools console for errors on Windows

2. **Linux Platform Detection in Frontend**
   - What we know: Backend uses `#[cfg(target_os = "linux")]` to conditionally compile
   - What's unclear: Frontend (WASM) can't detect OS directly
   - Recommendation: Add a Tauri command to return OS info, or always show the warning

## Key Files Identified

### Single Instance (PLAT-01)
- `/home/prescott/baras/app/src-tauri/src/lib.rs` - Main Tauri setup (lines 60-118)
- `/home/prescott/baras/app/src-tauri/Cargo.toml` - Add dependency here
- `/home/prescott/baras/app/src-tauri/src/tray.rs` - Window show/focus logic already here (reference)

### Windows Font (PLAT-02)
- `/home/prescott/baras/app/src/app.rs` - Font loading at line 20, @font-face at line 324
- `/home/prescott/baras/app/assets/styles.css` - Font usage at line 219
- `/home/prescott/baras/app/assets/StarJedi.ttf` - The font file

### Hotkey Settings (PLAT-03)
- `/home/prescott/baras/app/src/app.rs` - Hotkey settings UI at lines 1158-1205 (in general_settings modal)
- `/home/prescott/baras/app/src-tauri/src/hotkeys.rs` - Backend hotkey registration (conditional compile)
- `/home/prescott/baras/app/src-tauri/src/lib.rs` - Hotkey conditional compile at lines 14-15, 99-105

### Alacrity/Latency (PLAT-04)
- `/home/prescott/baras/app/src/components/effect_editor.rs` - `PlayerStatsBar` at lines 234-298
- `/home/prescott/baras/app/src/app.rs` - Session tab UI at lines 600-654
- `/home/prescott/baras/types/src/lib.rs` - `AppConfig` with `alacrity_percent` and `latency_ms` at lines 1622-1629

## Sources

### Primary (HIGH confidence)
- [Tauri Single Instance Plugin](https://v2.tauri.app/plugin/single-instance/) - Official documentation
- [tauri-plugin-single-instance crate](https://crates.io/crates/tauri-plugin-single-instance) - Latest version 2.3.6
- Codebase analysis - Direct file reading

### Secondary (MEDIUM confidence)
- [Tauri Font Rendering Issue #12638](https://github.com/tauri-apps/tauri/issues/12638) - WebView font differences
- [Tauri Web Fonts Issue #6815](https://github.com/tauri-apps/tauri/issues/6815) - Font loading issues

### Tertiary (LOW confidence)
- [Tauri Custom Font Issue #12763](https://github.com/tauri-apps/tauri/issues/12763) - Spaces in font names (not applicable here)

## Metadata

**Confidence breakdown:**
- Single instance: HIGH - Official plugin, clear documentation
- Architecture: HIGH - Based on direct codebase analysis
- Font rendering: MEDIUM - Known issue, root cause uncertain
- Hotkey limitations: HIGH - Clear from code and Wayland architecture
- Alacrity/Latency: HIGH - Direct code analysis of existing implementation

**Research date:** 2026-01-18
**Valid until:** 2026-02-18 (30 days - stable domain)
