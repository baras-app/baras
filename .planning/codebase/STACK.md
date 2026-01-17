# Technology Stack

**Analysis Date:** 2026-01-17

## Languages

**Primary:**
- Rust 2024 Edition - All backend logic, core parsing, overlay rendering, Tauri app

**Secondary:**
- TOML - DSL for boss encounters, effects, timers definitions (`core/definitions/`)
- CSS - Frontend styling (`app/assets/styles.css`, `app/assets/data-explorer.css`)
- JSON - Configuration files (`app/src-tauri/tauri.conf.json`)

## Runtime

**Environment:**
- Rust 1.92.0 (stable toolchain)
- WASM (wasm32-unknown-unknown) for frontend via Dioxus

**Package Manager:**
- Cargo (workspace-based monorepo)
- Lockfile: `Cargo.lock` present at root and `app/Cargo.lock`

## Frameworks

**Core:**
- Tauri 2.x - Desktop application framework (`app/src-tauri/Cargo.toml`)
- Dioxus 0.7.2 - Reactive UI framework for web frontend (`app/Cargo.toml`)
- tokio 1.48.0 - Async runtime for file watching, I/O operations

**Rendering:**
- tiny-skia 0.11 - 2D software rendering for overlays
- cosmic-text 0.15 - Text shaping and layout for overlays
- fontdb 0.23 - System font database

**Data Processing:**
- DataFusion 51 - SQL query engine for combat data analysis
- Arrow 57 - Columnar data format
- Parquet 57 - Persistent storage format for encounter data

**Build/Dev:**
- just - Task runner (`justfile`)
- dx (dioxus-cli) - Dioxus build/serve tooling
- cargo-tauri - Tauri build tooling

## Key Dependencies

**Critical (Core Logic):**
- `memmap2` 0.9.9 - Memory-mapped file reading for log parsing
- `memchr` 2.7.6 - Fast byte searching for log parsing
- `rayon` 1.11.0 - Parallel log processing
- `lasso` 0.7.3 - String interning for entity/ability names
- `hashbrown` 0.16.1 - Fast hash maps for entity tracking
- `notify` 8.2 - File system watching for new combat logs
- `confy` 2.0.0 - Configuration file management

**Serialization:**
- `serde` 1.0.x - Core serialization
- `serde_json` 1 - JSON for IPC with frontend
- `toml` 0.9 - DSL definition parsing

**Platform-Specific Overlay:**
- `wayland-client` 0.31 + `wayland-protocols` 0.32 - Linux Wayland support
- `x11rb` 0.13 - Linux X11 fallback support
- `windows` 0.58 - Windows native overlay APIs
- `cocoa` 0.26 + `core-graphics` 0.24 - macOS support (experimental)

**Audio:**
- `rodio` 0.19 - Audio playback for timer alerts
- `tts` 0.26 - Text-to-speech for countdown (non-Linux only)

**Network:**
- `reqwest` 0.12 - HTTP client for Parsely.io uploads

**Compression:**
- `flate2` 1.1 - Gzip compression for log uploads
- `zip` 2.x - Icon archive extraction

## Configuration

**Application Config:**
- Location: `~/.config/baras/config.toml` (via confy)
- Managed by: `core/src/context/config.rs`
- Type definitions: `types/src/lib.rs` (shared between backend/frontend)

**DSL Definitions:**
- Encounters: `core/definitions/encounters/operations/*.toml`
- Effects (HoTs, DOTs, DCDs): `core/definitions/effects/*.toml`
- Sounds: `core/definitions/sounds/`
- Absorb data: `core/definitions/absorbs.json`

**Tauri Config:**
- `app/src-tauri/tauri.conf.json` - Window settings, permissions, bundling
- Auto-updater endpoint: `https://raw.githubusercontent.com/baras-app/baras/master/latest.json`

**Dioxus Config:**
- `app/Dioxus.toml` - Build settings, asset paths

## Workspace Structure

```toml
[workspace]
members = [
  "types",        # Shared types (backend + WASM frontend)
  "core",         # Core business logic
  "overlay",      # Custom overlay rendering
  "app",          # Dioxus frontend (WASM)
  "app/src-tauri", # Tauri backend
  "validate",     # DSL validation CLI
  "parse-worker"  # Subprocess for parallel log parsing
]
```

**Build Profile (Release):**
```toml
lto = "thin"        # Link-time optimization
codegen-units = 1   # Better optimization
panic = "abort"     # No unwinding overhead
```

## Platform Requirements

**Development:**
- Rust 1.92.0+ with 2024 edition support
- Linux: `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `libasound2-dev`
- Windows: MSVC toolchain
- macOS: Xcode command line tools

**Production Targets:**
- Linux: AppImage, .deb (x86_64)
- Windows: NSIS installer (.exe)
- macOS: Experimental (requires additional testing)

**Runtime Dependencies:**
- Linux: Wayland compositor (preferred) or X11
- Windows: Windows 10+
- Ability icons: Bundled in `icons/` directory as `.zip` archives

---

*Stack analysis: 2026-01-17*
