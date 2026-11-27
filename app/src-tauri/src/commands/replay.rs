//! Replay tab: the maps of the boss an encounter was fought on, and their images.

use std::path::PathBuf;

use base64::Engine;
use tauri::{AppHandle, Manager, State};

use baras_core::boss::{build_area_index, load_bosses_with_custom};
use baras_types::MapDefinition;

use super::encounters::{get_bundled_encounters_dir, get_user_encounters_dir};
use crate::service::ServiceHandle;

/// Directories map images are looked up in, user files first
fn map_dirs(app_handle: &AppHandle) -> Vec<PathBuf> {
    let user = baras_core::paths::config_dir().map(|p| p.join("definitions").join("maps"));
    let bundled = app_handle
        .path()
        .resolve("definitions/maps", tauri::path::BaseDirectory::Resource)
        .ok();
    user.into_iter().chain(bundled).collect()
}

/// Maps of the boss fought in an encounter; empty when its definition has none.
#[tauri::command]
pub async fn get_replay_maps(
    app_handle: AppHandle,
    handle: State<'_, ServiceHandle>,
    encounter_idx: u32,
) -> Result<Vec<MapDefinition>, String> {
    let (area_id, boss_name) = {
        let session_guard = handle.shared.session.read().await;
        let session = session_guard.as_ref().ok_or("No active session")?;
        let session = session.read().await;
        let summary = session
            .session_cache
            .as_ref()
            .and_then(|cache| {
                cache
                    .encounter_history
                    .summaries()
                    .iter()
                    .find(|s| s.encounter_id == encounter_idx as u64)
            })
            .ok_or("Encounter not found")?;
        let Some(boss_name) = summary.boss_name.clone() else {
            return Ok(Vec::new());
        };
        (summary.area_id, boss_name)
    };

    let user_dir = get_user_encounters_dir();
    let file = [get_bundled_encounters_dir(&app_handle), user_dir.clone()]
        .into_iter()
        .flatten()
        .find_map(|dir| build_area_index(&dir).ok()?.remove(&area_id))
        .map(|entry| entry.file_path);
    let Some(file) = file else { return Ok(Vec::new()) };
    let bosses = load_bosses_with_custom(&file, user_dir.as_deref())?;
    Ok(bosses
        .into_iter()
        .find(|b| b.name == boss_name && !b.maps.is_empty())
        .map(|b| b.maps)
        .unwrap_or_default())
}

/// A map image as a data URL. `image` is a bare file name: anything that
/// could leave the maps directories is refused.
#[tauri::command]
pub async fn get_replay_map_image(app_handle: AppHandle, image: String) -> Result<String, String> {
    if image.is_empty() || image.contains(['/', '\\']) || image.contains("..") {
        return Err(format!("Invalid map image name: {image}"));
    }
    let mime = match image
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        _ => return Err(format!("Unsupported map image type: {image}")),
    };
    let path = map_dirs(&app_handle)
        .into_iter()
        .map(|dir| dir.join(&image))
        .find(|path| path.is_file())
        .ok_or_else(|| format!("Map image not found: {image}"))?;
    let bytes = std::fs::read(&path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    Ok(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}
