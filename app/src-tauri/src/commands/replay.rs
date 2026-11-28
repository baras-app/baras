//! Replay tab: the maps of the boss or PvP zone an encounter was fought on, and their images.

use base64::Engine;
use tauri::{AppHandle, State};

use baras_core::boss::{build_area_index, load_area_config, load_bosses_with_custom};
use baras_core::game_data::is_pvp_area;
use baras_types::{BossMapPages, MapDefinition, MapPage, ReplayMapStatus, ReplayMaps};

use super::encounters::{get_bundled_encounters_dir, get_user_encounters_dir};
use crate::service::{ServiceHandle, map_store};

/// Maps of the boss or PvP zone of an encounter, downloading the images that are
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
        // A PvP match has no boss: it is replayed on the pages of its zone
        if summary.boss_name.is_none() && !is_pvp_area(summary.area_id) {
            return Ok(ready(Vec::new()));
        }
        (summary.area_id, summary.boss_name.clone())
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
            .find(|b| Some(&b.name) == boss_name.as_ref() && !b.maps.is_empty())
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

fn ready(maps: Vec<MapDefinition>) -> ReplayMaps {
    ReplayMaps {
        maps,
        status: ReplayMapStatus::Ready,
    }
}

fn unavailable(error: String) -> ReplayMapStatus {
    tracing::warn!(error = %error, "Replay maps unavailable");
    ReplayMapStatus::Unavailable
}

/// What the replay draws a boss on, for the editor's Maps tab: `maps` (the boss's
/// own entries), else the pages the repository has for the area of `file_path`.
/// Nothing is downloaded here.
#[tauri::command]
pub async fn get_boss_map_pages(file_path: String, maps: Vec<MapDefinition>) -> Result<BossMapPages, String> {
    let from_area = maps.is_empty();
    let maps = match from_area {
        true => load_area_config(std::path::Path::new(&file_path))?
            .map(|area| map_store::area_pages(area.area_id))
            .unwrap_or_default(),
        false => maps,
    };
    let pages = maps
        .into_iter()
        .map(|map| MapPage {
            on_disk: map_store::find_image(&map.image).is_some(),
            map,
        })
        .collect();
    Ok(BossMapPages {
        pages,
        from_area,
        has_index: map_store::has_index(),
    })
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

/// Bytes of downloaded maps on disk
#[tauri::command]
pub async fn get_replay_map_storage() -> u64 {
    map_store::stored_bytes()
}

/// Delete the downloaded maps; they are fetched again when next needed
#[tauri::command]
pub async fn clear_replay_maps() -> Result<(), String> {
    map_store::clear()
}
