//! In-memory index over the per-log history files.
//!
//! Built lazily and refreshed by mtime: each call to `refresh` stats the
//! history directory and re-parses only files that are new or changed, so the
//! app's own writes and external backfills are both picked up without any
//! invalidation plumbing. Rows are compact `PullRow`s; everything else is a
//! linear scan over them (< 10k rows for a heavy raider).

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use hashbrown::{HashMap, HashSet};

use baras_types::history::{BossNode, HistoryStatus, PullFilter, PullHistoryOverview, PullRow};

use super::record::PullRecord;
use super::store::{HISTORY_VERSION, HistoryHeader, LogHistory, history_file_name};
use super::{DUMMY_GROUP, history_dir};
use crate::encounter::PhaseType;

struct IndexedLog {
    modified: SystemTime,
    /// Where the log should be; existence is checked at query time
    log_path: PathBuf,
    /// Written by an older store version, or from log bytes that have since
    /// changed. Decided at load time from the file header.
    stale: bool,
    rows: Vec<PullRow>,
}

/// What a refresh found that the caller may want to act on.
#[derive(Debug, Default)]
pub struct RefreshReport {
    /// Per-fight Parsely links from pre-versioning files:
    /// `(log filename, pull start_ms, link)`. Adopt into `LibraryMeta`.
    pub legacy_parsely: Vec<(String, i64, String)>,
}

#[derive(Default)]
pub struct PullIndex {
    logs: HashMap<String, IndexedLog>,
}

impl PullIndex {
    /// Sync with the history directory. `log_dir` is the fallback location
    /// for logs recorded before paths were stored.
    pub fn refresh(&mut self, log_dir: &Path) -> std::io::Result<RefreshReport> {
        let mut report = RefreshReport::default();
        let mut seen = HashSet::new();
        for entry in std::fs::read_dir(history_dir()?)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "json") {
                continue;
            }
            let Some(name) = path
                .file_name()
                .and_then(|f| f.to_str())
                .map(str::to_string)
            else {
                continue;
            };
            let modified = entry.metadata()?.modified()?;
            seen.insert(name.clone());
            if self.logs.get(&name).is_some_and(|l| l.modified == modified) {
                continue;
            }
            match LogHistory::load(&path) {
                Ok(loaded) => {
                    let history = loaded.history;
                    for (start_ms, link) in loaded.legacy_parsely {
                        report
                            .legacy_parsely
                            .push((history.filename.clone(), start_ms, link));
                    }
                    let log_path = history.log_path(log_dir);
                    let stale = is_stale(&history, &log_path);
                    let rows = history
                        .pulls
                        .iter()
                        .map(|p| row_from(&history, p))
                        .collect();
                    self.logs.insert(
                        name,
                        IndexedLog {
                            modified,
                            log_path,
                            stale,
                            rows,
                        },
                    );
                }
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "Skipping unreadable history file")
                }
            }
        }
        self.logs.retain(|name, _| seen.contains(name));
        Ok(report)
    }

    /// Health line for the Library header. `live` is the log being tailed,
    /// which is always behind its history file and must not count as stale.
    pub fn status(&self, live: Option<&Path>) -> HistoryStatus {
        let mut status = HistoryStatus::default();
        for log in self.logs.values() {
            status.logs += 1;
            status.pulls += log.rows.len() as u32;
            if log.stale && live.is_none_or(|l| l != log.log_path) {
                status.stale += 1;
            }
        }
        status
    }

    /// Filter option lists (unfiltered) plus per-boss pull/kill counts under `filter`.
    pub fn overview(&self, filter: &PullFilter) -> PullHistoryOverview {
        let characters = distinct(self.rows().map(|r| r.character.as_str()));
        let disciplines = distinct(self.rows().filter_map(|r| r.discipline.as_deref()));

        let mut counts: HashMap<(&str, &str), (u32, u32)> = HashMap::new();
        for r in self.rows().filter(|r| matches(r, filter)) {
            let e = counts.entry((&r.operation, &r.boss)).or_default();
            e.0 += 1;
            e.1 += u32::from(r.success);
        }
        let mut bosses: Vec<BossNode> = counts
            .into_iter()
            .map(|((operation, boss), (pulls, kills))| BossNode {
                operation: operation.to_string(),
                boss: boss.to_string(),
                pulls,
                kills,
            })
            .collect();
        bosses.sort_by(|a, b| {
            (a.operation == DUMMY_GROUP, &a.operation, &a.boss).cmp(&(
                b.operation == DUMMY_GROUP,
                &b.operation,
                &b.boss,
            ))
        });
        PullHistoryOverview {
            characters,
            disciplines,
            bosses,
        }
    }

    /// Every pull of one boss under `filter`, newest first, with `path` set
    /// when the log exists.
    pub fn pulls(&self, operation: &str, boss: &str, filter: &PullFilter) -> Vec<PullRow> {
        let mut out = Vec::new();
        for log in self.logs.values() {
            let mut exists = None;
            for r in log
                .rows
                .iter()
                .filter(|r| r.operation == operation && r.boss == boss && matches(r, filter))
            {
                let exists = *exists.get_or_insert_with(|| log.log_path.is_file());
                let mut row = r.clone();
                row.path = exists.then(|| log.log_path.to_string_lossy().into_owned());
                out.push(row);
            }
        }
        out.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        out
    }

    /// Rows of one log, for library joins (pull/kill counts, dummy-only).
    pub fn rows_for(&self, log_filename: &str) -> &[PullRow] {
        self.log_info(log_filename).map_or(&[], |(rows, _)| rows)
    }

    /// `(rows, stale)` for one log; None when it has no history file.
    pub fn log_info(&self, log_filename: &str) -> Option<(&[PullRow], bool)> {
        self.logs
            .get(history_file_name(log_filename).as_str())
            .map(|l| (l.rows.as_slice(), l.stale))
    }

    fn rows(&self) -> impl Iterator<Item = &PullRow> {
        self.logs.values().flat_map(|l| l.rows.iter())
    }
}

/// Old store version, or the log on disk no longer matches the header. A
/// missing log is not stale: there is nothing to re-scan from.
fn is_stale(history: &LogHistory, log_path: &Path) -> bool {
    if history.version < HISTORY_VERSION {
        return true;
    }
    match std::fs::metadata(log_path) {
        Ok(meta) => !HistoryHeader {
            version: history.version,
            source_len: history.source_len,
            source_mtime: history.source_mtime,
        }
        .is_current(&meta),
        Err(_) => false,
    }
}

fn matches(r: &PullRow, f: &PullFilter) -> bool {
    (f.character.is_empty() || r.character == f.character)
        && (f.role.is_empty() || r.role.as_deref() == Some(f.role.as_str()))
        && (f.discipline.is_empty() || r.discipline.as_deref() == Some(f.discipline.as_str()))
        && (f.tier.is_empty() || r.difficulty.contains(f.tier.as_str()))
        && (f.group_size.is_empty() || r.difficulty.split(' ').next() == Some(f.group_size.as_str()))
        && (!f.operations_only || r.is_operation)
        && (!f.kills_only || r.success)
        && in_date_range(&r.timestamp, f)
}

/// ISO timestamps compare lexicographically, so day bounds need no parsing
fn in_date_range(timestamp: &str, f: &PullFilter) -> bool {
    let day = timestamp.get(..10).unwrap_or(timestamp);
    (f.date_from.is_empty() || day >= f.date_from.as_str())
        && (f.date_to.is_empty() || day <= f.date_to.as_str())
}

fn distinct<'a>(it: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut v: Vec<String> = it
        .collect::<HashSet<_>>()
        .into_iter()
        .map(str::to_string)
        .collect();
    v.sort();
    v
}

fn row_from(log: &LogHistory, p: &PullRecord) -> PullRow {
    let character = log
        .character_name
        .clone()
        .unwrap_or_else(|| "Unknown".into());
    let me = p.roster.iter().find(|m| m.name == character);
    let (operation, boss) = if p.kind == PhaseType::DummyParse {
        (DUMMY_GROUP.to_string(), DUMMY_GROUP.to_string())
    } else {
        (
            p.area_name.clone(),
            p.boss_name
                .clone()
                .unwrap_or_else(|| p.display_name.clone()),
        )
    };
    let progress = p.progress.as_ref();
    PullRow {
        filename: log.filename.clone(),
        path: None,
        encounter_id: p.encounter_id,
        start_ms: p.start_ms,
        timestamp: if p.start_time.is_empty() {
            log.started_at
                .map(|t| t.format("%Y-%m-%dT%H:%M:%S").to_string())
                .unwrap_or_default()
        } else {
            p.start_time.clone()
        },
        operation,
        boss,
        difficulty: p.difficulty.clone().unwrap_or_else(|| "-".into()),
        success: p.success,
        duration_seconds: p.duration_secs,
        is_operation: p.kind == PhaseType::Raid,
        discipline: me.and_then(|m| m.discipline_name.clone()),
        role: me.and_then(|m| m.discipline.map(|d| format!("{:?}", d.role()))),
        role_icon: me.and_then(|m| m.discipline.map(|d| d.role().icon_name().to_string())),
        discipline_icon: me.and_then(|m| m.discipline.map(|d| d.icon_name().to_string())),
        dps: me.map(|m| m.dps),
        hps: me.map(|m| m.hps),
        ehps: me.map(|m| m.ehps),
        parsely: None,
        end_reason: p.end_reason,
        boss_hp_pct: progress
            .filter(|pr| pr.bosses.len() == 1)
            .map(|pr| pr.bosses[0].lowest_pct),
        phase: progress.and_then(|pr| pr.phase_reached.as_ref().map(|ph| ph.name.clone())),
        deaths: p.deaths.len() as u16,
        raid_size: p.raid_size,
        partial: p.partial,
        character,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Dev benchmark against the real history store (skipped in CI).
    /// `cargo test -p baras-core --lib history::index -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn refresh_timing_on_real_store() {
        let mut index = PullIndex::default();
        let t = std::time::Instant::now();
        index.refresh(Path::new(".")).unwrap();
        let cold = t.elapsed();
        let t = std::time::Instant::now();
        index.refresh(Path::new(".")).unwrap();
        let warm = t.elapsed();
        let t = std::time::Instant::now();
        let filter = PullFilter::default();
        let overview = index.overview(&filter);
        let pulls = index.pulls(
            &overview.bosses[0].operation,
            &overview.bosses[0].boss,
            &filter,
        );
        let query = t.elapsed();
        println!(
            "logs={} bosses={} first_boss_pulls={} cold={cold:?} warm={warm:?} query={query:?}",
            index.logs.len(),
            overview.bosses.len(),
            pulls.len()
        );
    }
}
