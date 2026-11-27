//! Replay map images and their index (`pages.toml`).
//!
//! Both live in the maps repository and are downloaded on demand: the index
//! once a day at most, an image the first time a fight needs it. Downloads are
//! kept under the config directory; the user's own maps and a local checkout
//! of the repository (`BARAS_MAPS_DIR`) are looked up before them.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::Duration;

use futures_util::{StreamExt, TryStreamExt, stream};

use baras_types::MapDefinition;

use crate::commands::USER_AGENT;

/// jsDelivr's mirror of the maps repository's main branch
const REPO_URL: &str = "https://cdn.jsdelivr.net/gh/baras-app/baras-maps@main";
const INDEX_FILE: &str = "pages.toml";
const INDEX_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const FETCH_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;
const PARALLEL_DOWNLOADS: usize = 8;
/// A checkout of the maps repository to read instead of downloading
const LOCAL_REPO_ENV: &str = "BARAS_MAPS_DIR";

/// `pages.toml`: the game's map pages per area
#[derive(serde::Deserialize)]
struct MapIndex {
    #[serde(default)]
    area: Vec<AreaPages>,
}

#[derive(serde::Deserialize)]
struct AreaPages {
    id: i64,
    #[serde(default)]
    maps: Vec<MapDefinition>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Local files
// ─────────────────────────────────────────────────────────────────────────────

fn user_dir() -> Option<PathBuf> {
    baras_core::paths::config_dir().map(|p| p.join("definitions").join("maps"))
}

fn download_dir() -> Option<PathBuf> {
    baras_core::paths::config_dir().map(|p| p.join("maps"))
}

fn local_repo() -> Option<PathBuf> {
    std::env::var_os(LOCAL_REPO_ENV)
        .map(PathBuf::from)
        .filter(|dir| dir.is_dir())
}

/// Directories an image is looked up in, the user's own files first
fn image_dirs() -> impl Iterator<Item = PathBuf> {
    user_dir()
        .into_iter()
        .chain(local_repo().map(|repo| repo.join("maps")))
        .chain(download_dir())
}

fn index_files() -> impl Iterator<Item = PathBuf> {
    local_repo()
        .into_iter()
        .chain(download_dir())
        .map(|dir| dir.join(INDEX_FILE))
}

fn read_index() -> Option<MapIndex> {
    index_files()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .find_map(|text| toml::from_str(&text).ok())
}

/// A bare file name of a supported image type, and its MIME type. Anything
/// that could leave the maps directories is refused; `:` covers Windows
/// drive-relative names, which `join` would follow elsewhere.
pub fn image_mime(image: &str) -> Result<&'static str, String> {
    if image.is_empty() || image.contains(['/', '\\', ':']) || image.contains("..") {
        return Err(format!("Invalid map image name: {image}"));
    }
    match image
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => Ok("image/png"),
        Some("jpg" | "jpeg") => Ok("image/jpeg"),
        Some("webp") => Ok("image/webp"),
        _ => Err(format!("Unsupported map image type: {image}")),
    }
}

pub fn find_image(image: &str) -> Option<PathBuf> {
    image_dirs().map(|dir| dir.join(image)).find(|path| path.is_file())
}

/// Every page the game has for an area; empty without an index or for an area it does not list
pub fn area_pages(area_id: i64) -> Vec<MapDefinition> {
    read_index()
        .and_then(|index| index.area.into_iter().find(|a| a.id == area_id))
        .map(|a| a.maps)
        .unwrap_or_default()
}

pub fn has_index() -> bool {
    read_index().is_some()
}

/// The images of `maps` that are not on disk and that the repository has
pub fn missing_images(maps: &[MapDefinition]) -> Vec<String> {
    let published: HashSet<String> = read_index()
        .into_iter()
        .flat_map(|index| index.area)
        .flat_map(|area| area.maps)
        .map(|map| map.image)
        .collect();
    let mut missing: Vec<String> = maps
        .iter()
        .map(|map| &map.image)
        .filter(|image| published.contains(*image) && image_mime(image).is_ok() && find_image(image).is_none())
        .cloned()
        .collect();
    missing.sort();
    missing.dedup();
    missing
}

// ─────────────────────────────────────────────────────────────────────────────
// Downloads
// ─────────────────────────────────────────────────────────────────────────────

async fn fetch(path: &str) -> Result<Vec<u8>, String> {
    static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(reqwest::Client::new);
    let failed = |e: reqwest::Error| format!("Failed to download {path}: {e}");
    let bytes = CLIENT
        .get(format!("{REPO_URL}/{path}"))
        .header("User-Agent", USER_AGENT)
        .timeout(FETCH_TIMEOUT)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(failed)?
        .bytes()
        .await
        .map_err(failed)?;
    Ok(bytes.to_vec())
}

/// Write through a sibling file, so an interrupted download never leaves a half-written one behind
fn store(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let failed = |e: std::io::Error| format!("Failed to write {}: {e}", path.display());
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(failed)?;
    }
    let part = path.with_extension("part");
    std::fs::write(&part, bytes).map_err(failed)?;
    std::fs::rename(&part, path).map_err(failed)
}

/// Download the index when there is none or the one on disk is a day old.
/// A failed refresh is only an error when no index is left to work from.
pub async fn refresh_index() -> Result<(), String> {
    if local_repo().is_some() {
        return Ok(());
    }
    let path = download_dir().ok_or("No config directory")?.join(INDEX_FILE);
    let is_fresh = std::fs::metadata(&path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|at| at.elapsed().ok())
        .is_some_and(|age| age < INDEX_MAX_AGE);
    if is_fresh {
        return Ok(());
    }
    let downloaded = async {
        let bytes = fetch(INDEX_FILE).await?;
        let text = std::str::from_utf8(&bytes).map_err(|e| format!("Map index is not text: {e}"))?;
        toml::from_str::<MapIndex>(text).map_err(|e| format!("Map index is not valid: {e}"))?;
        store(&path, &bytes)
    }
    .await;
    match downloaded {
        Err(e) if !path.is_file() => Err(e),
        Err(e) => {
            tracing::warn!(error = %e, "Keeping the stored map index");
            Ok(())
        }
        Ok(()) => Ok(()),
    }
}

/// Download images into the download directory, a few at a time
pub async fn download_images(images: Vec<String>) -> Result<(), String> {
    let dir = download_dir().ok_or("No config directory")?;
    stream::iter(images)
        .map(|image| {
            let path = dir.join(&image);
            async move {
                let bytes = fetch(&format!("maps/{image}")).await?;
                if bytes.is_empty() || bytes.len() > MAX_IMAGE_BYTES {
                    return Err(format!("Unexpected size for {image}: {} bytes", bytes.len()));
                }
                store(&path, &bytes)
            }
        })
        .buffer_unordered(PARALLEL_DOWNLOADS)
        .try_collect::<()>()
        .await
}
