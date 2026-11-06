//! Automatic log housekeeping: the one place that joins every index into a
//! `LibraryView`, plans what the configured rules would remove, and carries
//! the plan out. Runs once the directory index has finished loading at
//! startup, and on demand from Log Management.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use baras_core::context::{DirectoryIndex, LogAreaCache};
use baras_core::history::PullIndex;
use baras_core::library::{LibraryMut, LibraryView};
use baras_types::history::{ArchiveReport, CleanupEvent, CleanupPlan, CleanupReport};
use tauri::{AppHandle, Emitter};

use crate::state::SharedState;

/// The pull index, brought up to date with the history directory. Links
/// found in pre-metadata history files are moved into library.json on the
/// way, so the one pass that loads a file never drops them. Locks the index,
/// then briefly the metadata: every caller takes them in that order.
pub fn pull_index<'a>(shared: &'a SharedState, log_dir: &Path) -> std::sync::MutexGuard<'a, PullIndex> {
    let mut index = shared.pull_index.lock().unwrap_or_else(|p| p.into_inner());
    match index.refresh(log_dir) {
        Ok(report) if !report.legacy_parsely.is_empty() => {
            let mut meta = shared.library_meta.lock().unwrap_or_else(|p| p.into_inner());
            let links = report.legacy_parsely.iter().map(|(f, ms, l)| (f.as_str(), *ms, l.as_str()));
            if meta.merge_legacy_parsely(links) && let Err(e) = meta.save() {
                tracing::warn!(error = %e, "Failed to save migrated Parsely links");
            }
        }
        Ok(_) => {}
        Err(e) => tracing::warn!(error = %e, "Failed to refresh pull history index"),
    }
    index
}

/// The log being tailed, if any.
pub async fn live_path(shared: &SharedState) -> Option<PathBuf> {
    let guard = shared.session.read().await;
    let session = guard.as_ref()?;
    session.read().await.active_file.clone()
}

/// Dry run under the saved rules. Nothing is touched.
pub async fn plan(shared: &Arc<SharedState>, live: Option<&Path>) -> CleanupPlan {
    let (rules, log_dir) = {
        let config = shared.config.read().await;
        (config.log_cleanup.clone(), PathBuf::from(&config.log_directory))
    };
    let directory = shared.directory_index.read().await;
    let areas = shared.area_cache.read().await;
    // The area indexer may still be running at startup; the on-disk copy is
    // enough to classify logs for the retention list.
    let disk_areas = areas.is_empty().then(|| {
        baras_core::context::default_cache_path()
            .map(|p| LogAreaCache::load_from_disk(&p))
            .unwrap_or_default()
    });
    let shared = shared.clone();
    tokio::task::block_in_place(|| {
        let archived = baras_core::library::archived_logs();
        let pulls = pull_index(&shared, &log_dir);
        let meta = shared.library_meta.lock().unwrap_or_else(|p| p.into_inner());
        let view = LibraryView {
            directory: &directory,
            areas: disk_areas.as_ref().unwrap_or(&areas),
            archived: &archived,
            pulls: &pulls,
            meta: &meta,
            live,
        };
        baras_core::library::plan(&view, &rules, chrono::Local::now().date_naive())
    })
}

pub const CLEANUP_EVENT: &str = "log-cleanup";

/// Plan, then archive and delete what the plan listed, reporting each file
/// on the `log-cleanup` event.
pub async fn run(shared: &Arc<SharedState>, app: &AppHandle) -> CleanupReport {
    let live = live_path(shared).await;
    let plan = plan(shared, live.as_deref()).await;
    if plan.is_empty() {
        return CleanupReport::default();
    }
    let archive: Vec<PathBuf> = plan.archive.iter().map(|f| PathBuf::from(&f.path)).collect();
    let delete: Vec<PathBuf> = plan.delete.iter().map(|f| PathBuf::from(&f.path)).collect();
    apply(shared, app, &archive, &delete).await
}

/// Archive then delete the given logs. Each log is compressed with no lock
/// held and swapped out under the locks afterwards, so the Library and the
/// directory watcher keep working through a long run.
pub async fn apply(shared: &Arc<SharedState>, app: &AppHandle, archive: &[PathBuf], delete: &[PathBuf]) -> CleanupReport {
    let total = (archive.len() + delete.len()) as u32;
    let progress = |current: u32, path: &Path| {
        let file = path.file_name().and_then(|f| f.to_str()).unwrap_or("").to_string();
        let _ = app.emit(CLEANUP_EVENT, &CleanupEvent::Progress { current, total, file });
    };
    let mut archived = ArchiveReport::default();
    for (i, path) in archive.iter().enumerate() {
        progress(i as u32 + 1, path);
        let compressed = match tokio::task::block_in_place(|| baras_core::library::compress_log(path)) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "Failed to archive log");
                archived.failed.push(path.to_string_lossy().into_owned());
                continue;
            }
        };
        let live = live_path(shared).await;
        with_library(shared, live.as_deref(), |lib| baras_core::library::commit(lib, compressed, &mut archived)).await;
        let _ = app.emit("log-files-changed", ());
    }
    if let Some(path) = delete.first() {
        progress(archive.len() as u32 + 1, path);
    }
    let live = live_path(shared).await;
    let deleted = with_library(shared, live.as_deref(), |lib| baras_core::library::delete(lib, delete, true)).await;
    let mut failed = archived.failed;
    failed.extend(deleted.failed);
    let report = CleanupReport {
        archived: archived.archived,
        deleted: deleted.deleted,
        bytes_freed: deleted.bytes_freed + archived.bytes_before.saturating_sub(archived.bytes_after),
        bytes_archived: archived.bytes_after,
        failed,
    };
    if report.archived > 0 || report.deleted > 0 {
        tracing::info!(archived = report.archived, deleted = report.deleted, bytes_freed = report.bytes_freed, "Log cleanup");
        let directory = shared.directory_index.read().await;
        let areas = shared.area_cache.read().await;
        save_log_caches(app, &directory, &areas);
    }
    let _ = app.emit(CLEANUP_EVENT, &CleanupEvent::Done { report: report.clone() });
    report
}

/// Run `f` with every index locked for writing, briefly.
pub async fn with_library<T>(shared: &SharedState, live: Option<&Path>, f: impl FnOnce(&mut LibraryMut<'_>) -> T) -> T {
    let mut directory = shared.directory_index.write().await;
    let mut areas = shared.area_cache.write().await;
    let mut meta = shared.library_meta.lock().unwrap_or_else(|p| p.into_inner());
    let mut lib = LibraryMut { directory: &mut directory, areas: &mut areas, meta: &mut meta, live };
    f(&mut lib)
}

/// Persist the directory and area caches after the log set changed, and
/// tell the frontend.
pub fn save_log_caches(app: &AppHandle, directory: &DirectoryIndex, areas: &LogAreaCache) {
    if let Some(path) = DirectoryIndex::default_cache_path() {
        directory.save_cache(&path);
    }
    if let Some(path) = baras_core::context::default_cache_path()
        && let Err(e) = areas.save_to_disk(&path)
    {
        tracing::warn!(error = %e, "Failed to save area cache");
    }
    let _ = app.emit("log-files-changed", ());
}
