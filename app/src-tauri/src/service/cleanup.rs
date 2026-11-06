//! Automatic log housekeeping: the one place that joins every index into a
//! `LibraryView`, plans what the configured rules would remove, and carries
//! the plan out. Runs once the directory index has finished loading at
//! startup, and on demand from Log Management.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use baras_core::context::{DirectoryIndex, LogAreaCache};
use baras_core::history::PullIndex;
use baras_core::library::{LibraryMut, LibraryView};
use baras_types::history::{ArchiveReport, CleanupPlan, CleanupReport};
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

/// Plan, then archive and delete what the plan listed. Each log is
/// compressed with no lock held and swapped out under the locks afterwards,
/// so the Library and the directory watcher keep working through a long run.
pub async fn run(shared: &Arc<SharedState>, app: &AppHandle) -> CleanupReport {
    let live = live_path(shared).await;
    let plan = plan(shared, live.as_deref()).await;
    if plan.is_empty() {
        return CleanupReport::default();
    }
    let mut archived = ArchiveReport::default();
    for candidate in &plan.archive {
        let path = PathBuf::from(&candidate.path);
        let compressed = match tokio::task::block_in_place(|| baras_core::library::compress_log(&path)) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "Failed to archive log");
                archived.failed.push(candidate.path.clone());
                continue;
            }
        };
        let live = live_path(shared).await;
        with_library(shared, live.as_deref(), |lib| baras_core::library::commit(lib, compressed, &mut archived)).await;
    }
    let delete: Vec<PathBuf> = plan.delete.iter().map(|f| PathBuf::from(&f.path)).collect();
    let deleted = with_library(shared, live.as_deref(), |lib| baras_core::library::delete(lib, &delete, true)).await;
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
    report
}

/// Run `f` with every index locked for writing, briefly.
async fn with_library<T>(shared: &SharedState, live: Option<&Path>, f: impl FnOnce(&mut LibraryMut<'_>) -> T) -> T {
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
