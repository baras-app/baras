//! Log library management.
//!
//! A combat log owns several artifacts: the file itself, its `DirectoryIndex`
//! entry, its `LogAreaCache` entry, a parquet session dir under the data
//! cache, a history JSON, and user metadata in `library.json`. This module is
//! the one place that knows all of them, so deleting a log leaves nothing
//! dangling and nothing precious (favorites, the live log) is ever touched.
//!
//! Removal is two-step: `plan` is a pure dry run of the cleanup rules over
//! the indexes, then the service acts on exactly what the plan listed:
//! `compress_log` + `commit` per archive, `delete` for the rest.

mod archive;

use std::path::{Path, PathBuf};

use chrono::NaiveDate;

use baras_types::history::{CleanupCandidate, CleanupPlan, ContentTag, DeleteReport, LogCleanup, PullRow};

pub use archive::{ArchivedLog, Compressed, archive_dir, archived_log, archived_logs, commit, compress_log, restore, sweep_partial};

use crate::context::{DirectoryIndex, FileAreaEntry, FileAreaIndex, LogAreaCache, parse_log_filename};
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
    // Pulls are only recorded for defined bosses and dummies, so a recorded
    // pull that is neither raid, flashpoint nor dummy is a world/lair boss
    if rows.iter().any(|r| !r.is_operation && !r.is_flashpoint && r.operation != DUMMY_GROUP) {
        tags.push(ContentTag::WorldBoss);
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

/// Read-only view of every index a plan joins.
pub struct LibraryView<'a> {
    pub directory: &'a DirectoryIndex,
    pub areas: &'a LogAreaCache,
    pub archived: &'a [ArchivedLog],
    pub pulls: &'a PullIndex,
    pub meta: &'a LibraryMeta,
    /// The log being tailed, never a candidate
    pub live: Option<&'a Path>,
}

#[derive(PartialEq)]
enum Action {
    Archive,
    Delete,
}

/// Which rule, if any, a raw log of this age and size falls under. Deletion
/// wins over archival; the size rule leaves today's logs alone since the game
/// may still come back to one.
fn rule_for(rules: &LogCleanup, age_days: i64, size: u64) -> Option<Action> {
    if rules.delete_after_days.is_some_and(|d| age_days > i64::from(d)) {
        return Some(Action::Delete);
    }
    if rules.delete_smaller_than > 0 && size < rules.delete_smaller_than && age_days >= 1 {
        return Some(Action::Delete);
    }
    if archive_due(rules, age_days) {
        return Some(Action::Archive);
    }
    None
}

fn archive_due(rules: &LogCleanup, age_days: i64) -> bool {
    rules.archive_after_days.is_some_and(|d| age_days > i64::from(d))
}

/// Retention only shields a log from deletion: a retained log that is due
/// for deletion is archived instead when the archive rule covers it, and
/// kept raw otherwise. A non-empty log that has not been area-indexed yet is
/// unknown, so it is left alone until it has been (an archive can no longer
/// be indexed). Returns None when the log is kept.
fn retained_action(
    action: Action,
    tags: &[ContentTag],
    indexed: bool,
    size: u64,
    rules: &LogCleanup,
    age_days: i64,
    retain: &[ContentTag],
) -> Option<Action> {
    if retain.is_empty() {
        return Some(action);
    }
    if !indexed && size > 0 {
        return None;
    }
    if action == Action::Archive || !tags.iter().any(|t| retain.contains(t)) {
        return Some(action);
    }
    archive_due(rules, age_days).then_some(Action::Archive)
}

/// Dry run: what `rules` would archive and delete today. The live log,
/// favorites and kept content are counted, not listed. Archived logs fall
/// under the delete-after rule only; their size is already paid for.
pub fn plan(view: &LibraryView<'_>, rules: &LogCleanup, today: NaiveDate) -> CleanupPlan {
    let mut out = CleanupPlan::default();
    let mut consider = |path: &Path, filename: &str, date: NaiveDate, size: u64, action: Action, age: i64| {
        if view.live.is_some_and(|l| l == path) {
            out.skipped_live += 1;
            return;
        }
        if view.meta.is_favorite(filename) {
            out.skipped_favorites += 1;
            return;
        }
        let area_index = view.areas.get(path);
        let content = content_tags(area_index, view.pulls.rows_for(filename));
        let Some(action) = retained_action(action, &content, area_index.is_some(), size, rules, age, &view.meta.retain) else {
            out.kept += 1;
            return;
        };
        let candidate = CleanupCandidate {
            path: path.to_string_lossy().into_owned(),
            filename: filename.to_owned(),
            date: date.to_string(),
            size,
            content,
        };
        match action {
            Action::Archive => {
                out.archive_bytes += size;
                out.archive.push(candidate);
            }
            Action::Delete => {
                out.delete_bytes += size;
                out.delete.push(candidate);
            }
        }
    };
    for entry in view.directory.entries() {
        let age = today.signed_duration_since(entry.date).num_days();
        if let Some(action) = rule_for(rules, age, entry.file_size) {
            consider(&entry.path, &entry.filename, entry.date, entry.file_size, action, age);
        }
    }
    for a in view.archived {
        let Some((date, _)) = parse_log_filename(&a.filename) else { continue };
        let age = today.signed_duration_since(date).num_days();
        if rules.delete_after_days.is_some_and(|d| age > i64::from(d)) {
            consider(&a.path, &a.filename, date, a.compressed_size, Action::Delete, age);
        }
    }
    out.archive.sort_by(|a, b| b.size.cmp(&a.size));
    out.delete.sort_by(|a, b| b.size.cmp(&a.size));
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
        // An archived log is addressed by its archive path
        let filename = filename.strip_suffix(".zst").unwrap_or(filename);
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
