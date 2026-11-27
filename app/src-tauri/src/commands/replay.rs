//! Replay tab: the maps of the boss an encounter was fought on, and their images.

use base64::Engine;
use tauri::{AppHandle, State};

use baras_core::boss::{build_area_index, load_bosses_with_custom};
use baras_types::{ReplayMapStatus, ReplayMaps};

use super::encounters::{get_bundled_encounters_dir, get_user_encounters_dir};
use crate::service::{ServiceHandle, map_store};

/// Maps of the boss fought in an encounter, downloading the images that are
/// not on disk yet. `allow` records the user's agreement to those downloads.
#[tauri::command]
pub async fn get_replay_maps(
    app_handle: AppHandle,
    handle: State<'_, ServiceHandle>,
    encounter_idx: u32,
    allow: bool,
) -> Result<ReplayMaps, String> {
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
            return Ok(ready(Vec::new()));
        };
        (summary.area_id, boss_name)
    };

    let user_dir = get_user_encounters_dir();
    let file = [get_bundled_encounters_dir(&app_handle), user_dir.clone()]
        .into_iter()
        .flatten()
        .find_map(|dir| build_area_index(&dir).ok()?.remove(&area_id))
        .map(|entry| entry.file_path);
    let mut config = handle.config().await;
    if allow && !config.replay_map_downloads {
        config.replay_map_downloads = true;
        handle.update_config(config.clone()).await?;
    }
    let may_download = config.replay_map_downloads;
    let index_error = match may_download {
        true => map_store::refresh_index().await.err(),
        false => None,
    };

    // The boss's own maps win; otherwise every page the game has for the area
    let own = match file {
        Some(file) => load_bosses_with_custom(&file, user_dir.as_deref())?
            .into_iter()
            .find(|b| b.name == boss_name && !b.maps.is_empty())
            .map(|b| b.maps),
        None => None,
    };
    let has_own = own.is_some();
    let maps = own.unwrap_or_else(|| map_store::area_pages(area_id));
    let missing = map_store::missing_images(&maps);
    let status = if !has_own && !map_store::has_index() {
        // Without the index there is no telling whether the area has maps
        match may_download {
            true => unavailable(index_error.unwrap_or_default()),
            false => ReplayMapStatus::NeedsConsent,
        }
    } else if missing.is_empty() {
        ReplayMapStatus::Ready
    } else if !may_download {
        ReplayMapStatus::NeedsConsent
    } else {
        match map_store::download_images(missing).await {
            Ok(()) => ReplayMapStatus::Ready,
            Err(e) => unavailable(e),
        }
    };
    Ok(ReplayMaps { maps, status })
}

fn ready(maps: Vec<baras_types::MapDefinition>) -> ReplayMaps {
    ReplayMaps {
        maps,
        status: ReplayMapStatus::Ready,
    }
}

fn unavailable(error: String) -> ReplayMapStatus {
    tracing::warn!(error = %error, "Replay maps unavailable");
    ReplayMapStatus::Unavailable
}

/// A map image as a data URL
#[tauri::command]
pub async fn get_replay_map_image(image: String) -> Result<String, String> {
    let mime = map_store::image_mime(&image)?;
    let path = map_store::find_image(&image).ok_or_else(|| format!("Map image not found: {image}"))?;
    let bytes = std::fs::read(&path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    Ok(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}
