//! Replay map images and their index (`pages.toml`).
//!
//! Both live in the maps repository and are downloaded on demand: the index
//! once a day at most, an image the first time a fight needs it. Downloads are
//! kept under the config directory; the user's own maps and a local checkout
//! of the repository (`BARAS_MAPS_DIR`) are looked up before them.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant, SystemTime};

use futures_util::{StreamExt, TryStreamExt, stream};
use sha2::{Digest, Sha256};

use baras_types::MapDefinition;

use crate::commands::USER_AGENT;

/// jsDelivr's mirror of the maps repository's main branch
const REPO_URL: &str = "https://cdn.jsdelivr.net/gh/baras-app/baras-maps@main";
const INDEX_FILE: &str = "pages.toml";
const INDEX_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);
/// How long to keep the stored index after a failed refresh before trying again
const INDEX_RETRY: Duration = Duration::from_secs(10 * 60);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const FETCH_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_INDEX_BYTES: usize = 1024 * 1024;
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
    maps: Vec<IndexedMap>,
}

/// A page and its image's SHA-256: only an image with a hash is downloaded,
/// and a download that does not match it is never stored
#[derive(serde::Deserialize)]
struct IndexedMap {
    #[serde(flatten)]
    map: MapDefinition,
    sha256: Option<String>,
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

/// Directories of images that are used as they are, the user's own files first
fn local_dirs() -> impl Iterator<Item = PathBuf> {
    user_dir().into_iter().chain(local_repo().map(|repo| repo.join("maps")))
}

/// Directories an image is looked up in
fn image_dirs() -> impl Iterator<Item = PathBuf> {
    local_dirs().chain(download_dir())
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
        .map(|a| a.maps.into_iter().map(|m| m.map).collect())
        .unwrap_or_default()
}

pub fn has_index() -> bool {
    read_index().is_some()
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Whether the file hashes to `sha256`. Each file is hashed once until it changes.
fn file_matches(path: &Path, sha256: &str) -> bool {
    static HASHED: LazyLock<Mutex<HashMap<PathBuf, (SystemTime, String)>>> = LazyLock::new(Default::default);
    let Ok(modified) = std::fs::metadata(path).and_then(|meta| meta.modified()) else {
        return false;
    };
    let mut hashed = HASHED.lock().unwrap_or_else(|p| p.into_inner());
    if let Some((at, hash)) = hashed.get(path)
        && *at == modified
    {
        return hash.eq_ignore_ascii_case(sha256);
    }
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    let hash = sha256_hex(&bytes);
    let matches = hash.eq_ignore_ascii_case(sha256);
    hashed.insert(path.to_path_buf(), (modified, hash));
    matches
}

/// The images of `maps` the repository has (with their SHA-256) that are not on
/// disk, or whose download no longer matches the index: changed or tampered with
pub fn missing_images(maps: &[MapDefinition]) -> Vec<(String, String)> {
    let published: HashMap<String, String> = read_index()
        .into_iter()
        .flat_map(|index| index.area)
        .flat_map(|area| area.maps)
        .filter_map(|m| Some((m.map.image, m.sha256?)))
        .collect();
    let Some(downloads) = download_dir() else {
        return Vec::new();
    };
    let mut missing: Vec<(String, String)> = maps
        .iter()
        .filter_map(|map| Some((map.image.clone(), published.get(&map.image)?.clone())))
        .filter(|(image, sha256)| {
            image_mime(image).is_ok()
                && !local_dirs().any(|dir| dir.join(image).is_file())
                && !file_matches(&downloads.join(image), sha256)
        })
        .collect();
    missing.sort();
    missing.dedup();
    missing
}

/// Bytes of downloaded images and index on disk
pub fn stored_bytes() -> u64 {
    download_dir()
        .and_then(|dir| std::fs::read_dir(dir).ok())
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok()?.metadata().ok())
        .filter(|meta| meta.is_file())
        .map(|meta| meta.len())
        .sum()
}

/// Delete everything downloaded; the user's own maps are elsewhere and stay
pub fn clear() -> Result<(), String> {
    match download_dir() {
        Some(dir) if dir.is_dir() => {
            std::fs::remove_dir_all(&dir).map_err(|e| format!("Failed to delete {}: {e}", dir.display()))
        }
        _ => Ok(()),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Downloads
// ─────────────────────────────────────────────────────────────────────────────

/// Download `path`, refusing bodies over `max_bytes` before they are read in full
async fn fetch(path: &str, max_bytes: usize) -> Result<Vec<u8>, String> {
    static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
        reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .build()
            .unwrap_or_default()
    });
    let failed = |e: reqwest::Error| format!("Failed to download {path}: {e}");
    let too_big = || format!("{path} is over {max_bytes} bytes");
    let mut response = CLIENT
        .get(format!("{REPO_URL}/{path}"))
        .header("User-Agent", USER_AGENT)
        .timeout(FETCH_TIMEOUT)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(failed)?;
    if response.content_length().is_some_and(|len| len > max_bytes as u64) {
        return Err(too_big());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(failed)? {
        if bytes.len() + chunk.len() > max_bytes {
            return Err(too_big());
        }
        bytes.extend_from_slice(&chunk);
    }
    if bytes.is_empty() {
        return Err(format!("{path} is empty"));
    }
    Ok(bytes)
}

/// Write through a sibling file, so an interrupted download never leaves a half-written one
/// behind. Each write gets its own sibling: two pulls of one boss may download an image at once.
fn store(path: &Path, bytes: &[u8]) -> Result<(), String> {
    static NEXT_PART: AtomicU32 = AtomicU32::new(0);
    let failed = |e: std::io::Error| format!("Failed to write {}: {e}", path.display());
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(failed)?;
    }
    let part = path.with_extension(format!("{}.part", NEXT_PART.fetch_add(1, Ordering::Relaxed)));
    std::fs::write(&part, bytes).map_err(failed)?;
    std::fs::rename(&part, path).map_err(|e| {
        let _ = std::fs::remove_file(&part);
        failed(e)
    })
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
    // After a failed refresh, keep using the stored index for a while instead of
    // waiting on the network again for every fight opened
    static RETRY_AT: Mutex<Option<Instant>> = Mutex::new(None);
    let backing_off = || {
        RETRY_AT
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .is_some_and(|at| Instant::now() < at)
    };
    if is_fresh || (path.is_file() && backing_off()) {
        return Ok(());
    }
    let downloaded = async {
        let bytes = fetch(INDEX_FILE, MAX_INDEX_BYTES).await?;
        let text = std::str::from_utf8(&bytes).map_err(|e| format!("Map index is not text: {e}"))?;
        toml::from_str::<MapIndex>(text).map_err(|e| format!("Map index is not valid: {e}"))?;
        store(&path, &bytes)
    }
    .await;
    *RETRY_AT.lock().unwrap_or_else(|p| p.into_inner()) = downloaded.is_err().then(|| Instant::now() + INDEX_RETRY);
    match downloaded {
        Err(e) if !path.is_file() => Err(e),
        Err(e) => {
            tracing::warn!(error = %e, "Keeping the stored map index");
            Ok(())
        }
        Ok(()) => Ok(()),
    }
}

/// Download images (name, SHA-256) into the download directory, a few at a time
pub async fn download_images(images: Vec<(String, String)>) -> Result<(), String> {
    let dir = download_dir().ok_or("No config directory")?;
    stream::iter(images)
        .map(|(image, sha256)| {
            let path = dir.join(&image);
            async move {
                let bytes = fetch(&format!("maps/{image}"), MAX_IMAGE_BYTES).await?;
                if !sha256_hex(&bytes).eq_ignore_ascii_case(&sha256) {
                    return Err(format!("{image} does not match the map index, not stored"));
                }
                store(&path, &bytes)
            }
        })
        .buffer_unordered(PARALLEL_DOWNLOADS)
        .try_collect::<()>()
        .await
}
