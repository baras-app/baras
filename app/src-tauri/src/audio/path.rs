//! Sound file path resolution for the audio subsystem.
//!
//! Sound references in timer/effect configs may take three forms:
//! - Folder-relative: `"mechanic-sounds/Acid Deluge.mp3"` — looked up under
//!   `core/definitions/<folder>/<file>` (and, for `sounds/`, also under the
//!   user's `~/.config/baras/sounds/`).
//! - Bare filename (legacy): `"Alert.mp3"` — resolved against the General
//!   sounds folder (user dir → bundled `sounds/`).
//! - Absolute path: e.g. a file the user picked via Browse — returned verbatim
//!   if it exists.

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

/// Resolve the bundled `core/definitions/` directory.
///
/// Prefers the source tree when present: in dev it is always complete, so
/// folders not yet listed in `tauri.conf.json` `resources` (and therefore
/// absent from the copied resource dir) still resolve. In release the source
/// path does not exist on the user's machine, so fall back to bundled
/// resources.
pub fn resolve_bundled_definitions_dir(app: &AppHandle) -> PathBuf {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
        .join("core/definitions");

    if source.join("sounds").is_dir() {
        return source;
    }

    app.path()
        .resolve("definitions", tauri::path::BaseDirectory::Resource)
        .ok()
        .filter(|p| p.exists())
        .unwrap_or(source)
}

/// Resolve a sound file reference to an absolute on-disk path.
///
/// Returns `Some(path)` only if the file exists.
pub fn resolve_sound_path(file_ref: &str, user_sounds_dir: &Path, bundled_definitions_dir: &Path) -> Option<PathBuf> {
    if file_ref.is_empty() {
        return None;
    }

    let absolute = PathBuf::from(file_ref);
    if absolute.is_absolute() {
        return absolute.exists().then_some(absolute);
    }

    let (folder, name) = match file_ref.split_once('/') {
        Some((f, n)) if !f.is_empty() && !n.is_empty() => (f, n),
        _ => ("sounds", file_ref),
    };

    if folder == "sounds" {
        let user = user_sounds_dir.join(name);
        if user.exists() {
            return Some(user);
        }
    }

    let bundled = bundled_definitions_dir.join(folder).join(name);
    bundled.exists().then_some(bundled)
}
