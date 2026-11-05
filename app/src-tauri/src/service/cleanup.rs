//! Automatic log housekeeping: the one place that joins every index into a
//! `LibraryView`, plans what the configured rules would remove, and carries
//! the plan out. Runs once the directory index has finished loading at
//! startup, and on demand from Log Management.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use baras_core::context::{DirectoryIndex, LogAreaCache};
use baras_core::library::{LibraryMut, LibraryView};
use baras_types::history::{CleanupPlan, CleanupReport};
use tauri::{AppHandle, Emitter};

use crate::state::SharedState;

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
        let mut pulls = shared.pull_index.lock().unwrap_or_else(|p| p.into_inner());
        if let Err(e) = pulls.refresh(&log_dir) {
            tracing::warn!(error = %e, "Failed to refresh pull history index");
        }
        let meta = shared.library_meta.lock().unwrap_or_else(|p| p.into_inner());
        let view = LibraryView {
            directory: &directory,
            areas: disk_areas.as_ref().unwrap_or(&areas),
            pulls: &pulls,
            meta: &meta,
            live,
        };
        baras_core::library::plan(&view, &rules, chrono::Local::now().date_naive())
    })
}

/// Plan, then archive and delete what the plan listed. The directory and
/// area caches are persisted and the frontend told when anything changed.
pub async fn run(shared: &Arc<SharedState>, app: &AppHandle) -> CleanupReport {
    let live = live_path(shared).await;
    let plan = plan(shared, live.as_deref()).await;
    if plan.is_empty() {
        return CleanupReport::default();
    }
    let mut directory = shared.directory_index.write().await;
    let mut areas = shared.area_cache.write().await;
    let report = {
        let mut meta = shared.library_meta.lock().unwrap_or_else(|p| p.into_inner());
        let mut lib = LibraryMut {
            directory: &mut directory,
            areas: &mut areas,
            meta: &mut meta,
            live: live.as_deref(),
        };
        baras_core::library::execute(&mut lib, &plan)
    };
    if report.archived > 0 || report.deleted > 0 {
        tracing::info!(archived = report.archived, deleted = report.deleted, bytes_freed = report.bytes_freed, "Log cleanup");
        save_log_caches(app, &directory, &areas);
    }
    report
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
