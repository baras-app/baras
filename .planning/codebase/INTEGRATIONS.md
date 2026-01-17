# External Integrations

**Analysis Date:** 2026-01-17

## APIs & External Services

**Parsely.io (Log Upload):**
- Purpose: Upload combat logs for public parsing/sharing
- Endpoint: `https://parsely.io/api/upload2`
- Implementation: `app/src-tauri/src/commands/parsely.rs`
- Auth: Username/password stored in `AppConfig.parsely`
- Protocol: HTTP POST multipart form with gzip-compressed log file
- Response: XML format with `<file>` link or `<error>` message

**GitHub (Auto-Updater):**
- Purpose: Check for and download application updates
- Endpoint: `https://raw.githubusercontent.com/baras-app/baras/master/latest.json`
- Implementation: `app/src-tauri/src/updater.rs`
- Protocol: Tauri updater plugin with signature verification
- Auth: None (public releases)

**ECharts CDN:**
- Purpose: Chart rendering in frontend
- Endpoint: `https://cdn.jsdelivr.net/npm/echarts@5/dist/echarts.min.js`
- Loaded via: `app/Dioxus.toml` web resources

## Data Storage

**Local Parquet Storage:**
- Purpose: Persistent encounter data for queries
- Location: `~/.config/baras/data/{session_id}/`
- Format: Apache Parquet with Snappy/Zstd compression
- Implementation: `core/src/storage/mod.rs`, `core/src/storage/writer.rs`
- Schema: Denormalized combat events per encounter

**Configuration Storage:**
- Purpose: User settings persistence
- Location: `~/.config/baras/config.toml`
- Library: `confy` crate
- Implementation: `core/src/context/config.rs`

**Combat Log Files:**
- Source: SWTOR game client
- Windows: `Documents/Star Wars - The Old Republic/CombatLogs/`
- Linux (Steam/Proton): `~/.local/share/Steam/steamapps/compatdata/1286830/pfx/drive_c/users/steamuser/Documents/Star Wars - The Old Republic/CombatLogs/`
- Format: Text files named `combat_YYYY-MM-DD_HH_MM_SS_xxxxxx.txt`

**File Storage:**
- No cloud file storage
- All data stored locally in user's config directory

**Caching:**
- In-memory session cache: `core/src/state/cache.rs`
- String interning: `lasso` crate for entity/ability names
- No external caching service

## Authentication & Identity

**Auth Provider:**
- None (standalone desktop application)
- Parsely.io credentials stored locally in config file

**Implementation:**
- No OAuth, no JWT, no session management
- User identity not tracked

## Monitoring & Observability

**Error Tracking:**
- None (errors logged to stderr/stdout)

**Logs:**
- Console output via `eprintln!` macros
- Debug logging: `core/src/debug_log.rs`
- Tauri dev mode logs to `/tmp/baras.log`

**Metrics:**
- None (no telemetry)

## CI/CD & Deployment

**Hosting:**
- GitHub Releases for distribution
- No server-side hosting

**CI Pipeline:**
- GitHub Actions: `.github/workflows/release.yml`
- Triggers: Manual workflow dispatch with version input
- Build matrix: Ubuntu 24.04 (Linux), Windows-latest

**Release Process:**
1. Build parse-worker sidecar binary
2. Build Tauri app with `tauri-action`
3. Create GitHub release with AppImage/deb (Linux), NSIS installer (Windows)
4. Update `latest.json` manifest for auto-updater

**Signing:**
- Tauri signing keys stored in GitHub Secrets
- `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`

## Environment Configuration

**Required env vars (Development):**
- None required for local development

**Required env vars (CI/Release):**
- `GITHUB_TOKEN` - GitHub API access
- `TAURI_SIGNING_PRIVATE_KEY` - Update signature key
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` - Key password

**Secrets location:**
- GitHub repository secrets for CI
- Local config file for user credentials (Parsely)

## Webhooks & Callbacks

**Incoming:**
- None (desktop application)

**Outgoing:**
- None (no webhooks sent)

## File System Integrations

**Directory Watching:**
- Library: `notify` 8.2 crate
- Implementation: `core/src/context/watcher.rs`
- Purpose: Monitor combat log directory for new/removed files
- Mode: Non-recursive watch on configured log directory

**Memory-Mapped File Reading:**
- Library: `memmap2` 0.9.9
- Purpose: Efficient reading of large combat log files
- Implementation: `core/src/combat_log/reader.rs`

## Platform-Specific Integrations

**Windows:**
- Win32 APIs for transparent overlay windows
- NSIS installer for distribution

**Linux (Wayland):**
- `wlr-layer-shell` protocol for overlay positioning
- Shared memory buffers for rendering

**Linux (X11):**
- XShape extension for transparent windows
- XRandR for multi-monitor support

**macOS:**
- Cocoa/Core Graphics for window management
- Experimental support status

## Audio Subsystem

**Sound Playback:**
- Library: `rodio` 0.19
- Supported formats: WAV, Vorbis, MP3
- Sound files: `core/definitions/sounds/`
- Implementation: `app/src-tauri/src/audio/`

**Text-to-Speech:**
- Library: `tts` 0.26 (non-Linux only)
- Purpose: Countdown and alert announcements
- Linux limitation: TTS not available on Linux

## Icon/Asset Management

**Icon Sources:**
- Bundled in `icons/` directory
- Format: ZIP archives containing PNG ability icons
- Extraction: `zip` crate at runtime
- Implementation: `core/src/icons/mod.rs`, `overlay/src/icons.rs`

---

*Integration audit: 2026-01-17*
