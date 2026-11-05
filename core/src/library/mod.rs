//! Log library management.
//!
//! A combat log owns several artifacts: the file itself, its `DirectoryIndex`
//! entry, its `LogAreaCache` entry, a parquet session dir under the data
//! cache, a history JSON, and user metadata in `library.json`. This module is
//! the one place that knows all of them, so deleting a log leaves nothing
//! dangling and nothing precious (favorites, the live log) is ever touched.
//!
//! Removal is two-step: `plan` is a pure dry run of the cleanup rules over
//! the indexes, then `execute` acts on exactly what the plan listed.

mod archive;

use std::path::{Path, PathBuf};

use chrono::NaiveDate;

use baras_types::history::{
    CleanupCandidate, CleanupPlan, CleanupReport, ContentTag, DeleteReport, LogCleanup, PullRow,
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
fn is_retained(tags: &[ContentTag], indexed: bool, size: u64, retain: &[ContentTag]) -> bool {
    if retain.is_empty() {
        return false;
    }
    if !indexed && size > 0 {
        return true;
    }
    tags.iter().any(|t| retain.contains(t))
}

/// Read-only view of every index a plan joins.
pub struct LibraryView<'a> {
    pub directory: &'a DirectoryIndex,
    pub areas: &'a LogAreaCache,
    pub pulls: &'a PullIndex,
    pub meta: &'a LibraryMeta,
    /// The log being tailed, never a candidate
    pub live: Option<&'a Path>,
}

enum Action {
    Archive,
    Delete,
}

/// Which rule, if any, a log of this age and size falls under. Deletion wins
/// over archival; the size rule leaves today's logs alone since the game may
/// still come back to one.
fn rule_for(rules: &LogCleanup, age_days: i64, size: u64) -> Option<Action> {
    if rules.delete_after_days.is_some_and(|d| age_days > i64::from(d)) {
        return Some(Action::Delete);
    }
    if rules.delete_smaller_than > 0 && size < rules.delete_smaller_than && age_days >= 1 {
        return Some(Action::Delete);
    }
    if rules.archive_after_days.is_some_and(|d| age_days > i64::from(d)) {
        return Some(Action::Archive);
    }
    None
}

/// Dry run: what `rules` would archive and delete today. The live log,
/// favorites and retained content are counted, not listed.
pub fn plan(view: &LibraryView<'_>, rules: &LogCleanup, today: NaiveDate) -> CleanupPlan {
    let mut out = CleanupPlan::default();
    for entry in view.directory.entries() {
        let age = today.signed_duration_since(entry.date).num_days();
        let Some(action) = rule_for(rules, age, entry.file_size) else {
            continue;
        };
        if view.live.is_some_and(|l| l == entry.path) {
            out.skipped_live += 1;
            continue;
        }
        if view.meta.is_favorite(&entry.filename) {
            out.skipped_favorites += 1;
            continue;
        }
        let area_index = view.areas.get(&entry.path);
        let content = content_tags(area_index, view.pulls.rows_for(&entry.filename));
        if is_retained(&content, area_index.is_some(), entry.file_size, &view.meta.retain) {
            out.kept += 1;
            continue;
        }
        let candidate = CleanupCandidate {
            path: entry.path.to_string_lossy().into_owned(),
            filename: entry.filename.clone(),
            date: entry.date.to_string(),
            size: entry.file_size,
            content,
        };
        match action {
            Action::Archive => {
                out.archive_bytes += entry.file_size;
                out.archive.push(candidate);
            }
            Action::Delete => {
                out.delete_bytes += entry.file_size;
                out.delete.push(candidate);
            }
        }
    }
    out.archive.sort_by(|a, b| b.size.cmp(&a.size));
    out.delete.sort_by(|a, b| b.size.cmp(&a.size));
    out
}

/// Carry out a plan: archive first, then delete. History rows always stay,
/// so pulls from removed logs remain in the statistics as "log missing".
pub fn execute(lib: &mut LibraryMut<'_>, plan: &CleanupPlan) -> CleanupReport {
    let paths = |c: &[CleanupCandidate]| c.iter().map(|f| PathBuf::from(&f.path)).collect::<Vec<_>>();
    let archived = archive(lib, &paths(&plan.archive));
    let deleted = delete(lib, &paths(&plan.delete), true);
    let mut failed = archived.failed;
    failed.extend(deleted.failed);
    CleanupReport {
        archived: archived.archived,
        deleted: deleted.deleted,
        bytes_freed: deleted.bytes_freed + archived.bytes_before.saturating_sub(archived.bytes_after),
        bytes_archived: archived.bytes_after,
        failed,
    }
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
