//! Log library management.
//!
//! A combat log owns several artifacts: the file itself, its `DirectoryIndex`
//! entry, its `LogAreaCache` entry, a parquet session dir under the data
//! cache, a history JSON, and user metadata in `library.json`. This module is
//! the one place that knows all of them, so deleting a log leaves nothing
//! dangling and nothing precious (favorites, the live log) is ever touched.
//!
//! Removal is two-step: `preview` is a pure dry run over the indexes, then
//! `delete` or `archive` acts on the exact paths the user confirmed.

mod archive;

use std::path::{Path, PathBuf};

use chrono::NaiveDate;

use baras_types::history::{
    ContentTag, DeleteCandidate, DeleteCriteria, DeletePreview, DeleteReport, PullRow,
};

pub use archive::{ArchivedLog, archive, archive_dir, archived_logs, restore};

use crate::context::{DirectoryIndex, FileAreaEntry, FileAreaIndex, LogAreaCache};
use crate::game_data::{Difficulty, is_pvp_area};
use crate::history::{DUMMY_GROUP, LibraryMeta, PullIndex, history_file_for};
use crate::storage::data_dir;

/// What a log holds, from the areas it visited plus its recorded pulls.
/// Sorted and deduplicated; `Empty` when neither says anything, so every log
/// carries at least one tag and retention is purely tag-based.
pub fn content_tags(areas: Option<&FileAreaIndex>, rows: &[PullRow]) -> Vec<ContentTag> {
    let mut tags: Vec<ContentTag> = areas
        .map(|a| a.areas.iter().map(area_tag).collect())
        .unwrap_or_default();
    if rows.iter().any(|r| r.operation == DUMMY_GROUP) {
        tags.push(ContentTag::Dummy);
    }
    if tags.is_empty() {
        tags.push(ContentTag::Empty);
    }
    tags.sort();
    tags.dedup();
    tags
}

fn area_tag(area: &FileAreaEntry) -> ContentTag {
    if is_pvp_area(area.area_id) {
        return ContentTag::Pvp;
    }
    match Difficulty::from_difficulty_id(area.difficulty_id) {
        Some(Difficulty::Master4) => ContentTag::MasterFlashpoint,
        Some(Difficulty::Veteran4) => ContentTag::VeteranFlashpoint,
        Some(Difficulty::Story8 | Difficulty::Story16) => ContentTag::StoryOps,
        Some(Difficulty::Veteran8 | Difficulty::Veteran16) => ContentTag::VeteranOps,
        Some(Difficulty::Master8 | Difficulty::Master16) => ContentTag::MasterOps,
        None => ContentTag::Other,
    }
}

/// True when the log must be kept under `retain`. A non-empty log that has
/// not been area-indexed yet is unknown, so it is kept until it has been.
pub fn is_retained(tags: &[ContentTag], indexed: bool, size: u64, retain: &[ContentTag]) -> bool {
    if retain.is_empty() {
        return false;
    }
    if !indexed && size > 0 {
        return true;
    }
    tags.iter().any(|t| retain.contains(t))
}

/// Every log the automatic cleanup must leave alone: the live log, favorites,
/// and logs holding retained content (or not yet classified).
pub fn protected_paths(view: &LibraryView<'_>) -> std::collections::HashSet<PathBuf> {
    let retain = &view.meta.retain;
    view.directory
        .entries()
        .into_iter()
        .filter(|e| {
            if view.live.is_some_and(|l| l == e.path) || view.meta.is_favorite(&e.filename) {
                return true;
            }
            let area_index = view.areas.get(&e.path);
            let tags = content_tags(area_index, view.pulls.rows_for(&e.filename));
            is_retained(&tags, area_index.is_some(), e.file_size, retain)
        })
        .map(|e| e.path.clone())
        .collect()
}

/// Read-only view of every index a preview joins.
pub struct LibraryView<'a> {
    pub directory: &'a DirectoryIndex,
    pub areas: &'a LogAreaCache,
    pub pulls: &'a PullIndex,
    pub meta: &'a LibraryMeta,
    /// The log being tailed, never a candidate
    pub live: Option<&'a Path>,
}

/// Dry run: which logs `criteria` would remove. A log is a candidate when it
/// holds no kept content and passes every enabled narrowing filter.
pub fn preview(
    view: &LibraryView<'_>,
    criteria: &DeleteCriteria,
    today: NaiveDate,
) -> DeletePreview {
    let mut out = DeletePreview::default();
    for entry in view.directory.entries() {
        let area_index = view.areas.get(&entry.path);
        let content = content_tags(area_index, view.pulls.rows_for(&entry.filename));
        if is_retained(&content, area_index.is_some(), entry.file_size, &criteria.keep) {
            out.kept += 1;
            continue;
        }
        if criteria.smaller_than.is_some_and(|n| entry.file_size >= n) {
            continue;
        }
        if let Some(days) = criteria.older_than_days
            && today.signed_duration_since(entry.date).num_days() <= i64::from(days)
        {
            continue;
        }
        if view.live.is_some_and(|l| l == entry.path) {
            out.skipped_live += 1;
            continue;
        }
        if view.meta.is_favorite(&entry.filename) {
            out.skipped_favorites += 1;
            continue;
        }
        out.total_bytes += entry.file_size;
        out.files.push(DeleteCandidate {
            path: entry.path.to_string_lossy().into_owned(),
            filename: entry.filename.clone(),
            date: entry.date.to_string(),
            size: entry.file_size,
            content,
        });
    }
    out.files.sort_by(|a, b| b.size.cmp(&a.size));
    out
}

/// Everything that must be touched to delete logs. Mutable so the indexes
/// stay consistent with disk in one place.
pub struct LibraryMut<'a> {
    pub directory: &'a mut DirectoryIndex,
    pub areas: &'a mut LogAreaCache,
    pub meta: &'a mut LibraryMeta,
    pub live: Option<&'a Path>,
}

impl LibraryMut<'_> {
    /// The filename of a log that may be removed, or `None` for the live log,
    /// a favorite, or a path without a usable name.
    fn removable<'p>(&self, path: &'p Path) -> Option<&'p str> {
        let filename = path.file_name()?.to_str()?;
        (!(self.live.is_some_and(|l| l == path) || self.meta.is_favorite(filename)))
            .then_some(filename)
    }

    /// Forget a log that has left the directory, keeping its history and notes.
    fn forget(&mut self, path: &Path, filename: &str) {
        self.directory.remove_file(path);
        self.areas.remove(path);
        remove_parquet_session(filename);
    }
}

/// Delete the given logs and every artifact they own. `keep_history` leaves
/// the history JSON and pull notes so the pulls stay in the dashboard with
/// "log missing". The live log and favorites are refused, not deleted.
pub fn delete(lib: &mut LibraryMut<'_>, paths: &[PathBuf], keep_history: bool) -> DeleteReport {
    let mut report = DeleteReport::default();
    for path in paths {
        let Some(filename) = lib.removable(path) else {
            report.failed.push(path.to_string_lossy().into_owned());
            continue;
        };
        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "Failed to delete log");
                report.failed.push(path.to_string_lossy().into_owned());
                continue;
            }
        }
        report.deleted += 1;
        report.bytes_freed += size;
        lib.forget(path, filename);
        if !keep_history {
            if let Ok(hf) = history_file_for(filename) {
                let _ = std::fs::remove_file(hf);
            }
            lib.meta.remove_log(filename);
        }
    }
    report
}

/// Parquet session dirs are keyed by the log filename (see the historical
/// open path in the service). Missing is fine; it is a cache.
fn remove_parquet_session(filename: &str) {
    if let Ok(dir) = data_dir() {
        let session = dir.join(filename);
        if session.is_dir() {
            let _ = std::fs::remove_dir_all(session);
        }
    }
}
