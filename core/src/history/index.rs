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

use baras_types::history::{
    RunLink,
    AreaSeen, BossNode, HistoryStatus, InstanceVisit, LogContinuity, LogLink, PullFilter, PullHistoryOverview, PullRow,
    DummyRow, PvpMatchRow, RunSummary, TimerStart, keeps,
};

use super::catalog::AreaCatalog;
use super::meta::LibraryMeta;
use super::dummy::dummy_rows;
use super::pvp::pvp_rows;
use super::record::PullRecord;
use super::runs::{BuiltRun, RunSource, apply_meta, build_runs, run_matches};
use super::store::{HISTORY_VERSION, HistoryHeader, LogHistory, history_file_name};
use super::{DUMMY_GROUP, difficulty_parts, history_dir};
use crate::encounter::PhaseType;
use crate::game_data::{game_version_at, server_name};

struct IndexedLog {
    modified: SystemTime,
    /// Where the log should be; existence is checked at query time
    log_path: PathBuf,
    /// Logging character ("Unknown" when the log never named one)
    character: String,
    /// Server display name from the first pull that recorded one; empty otherwise
    server: String,
    /// Log start from the filename, local time taken as UTC millis (see `PullRow::start_ms`)
    started_ms: i64,
    /// Written by an older store version, or from log bytes that have since
    /// changed. Decided at load time from the file header.
    stale: bool,
    rows: Vec<PullRow>,
    /// Warzone matches and arena rounds, kept out of `rows` so the boss views
    /// never see them
    pvp: Vec<PvpMatchRow>,
    /// Training dummy kills; the dummy parses in `rows` are there for the
    /// library only and never reach the boss views
    dummy: Vec<DummyRow>,
    timer_starts: Vec<TimerStart>,
    visits: Vec<InstanceVisit>,
    continuity: LogContinuity,
    areas: Vec<AreaSeen>,
}

/// What one log's history says about its content and its neighbours.
#[derive(Clone, Copy)]
pub struct LogFacts<'a> {
    /// Every area entered; empty for files written before areas were recorded
    pub areas: &'a [AreaSeen],
    /// Instanced visits, including one a resumed log never named with an
    /// `AreaEntered` line
    pub visits: &'a [InstanceVisit],
    pub continuity: &'a LogContinuity,
    /// Filename of the log that resumed this one after a crash or disconnect
    pub continued_in: Option<&'a str>,
}

impl LogFacts<'_> {
    pub fn link(&self) -> LogLink {
        LogLink {
            mid_start: self.continuity.mid_start,
            resumed_from: self.continuity.resumed_from.clone(),
            continued_in: self.continued_in.map(str::to_owned),
        }
    }
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
    /// Resumed log filename -> the log that resumed it (see `LogContinuity::resumed_from`)
    continued_in: HashMap<String, String>,
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
            let Some(name) = path.file_name().and_then(|f| f.to_str()).map(str::to_string) else {
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
                        report.legacy_parsely.push((history.filename.clone(), start_ms, link));
                    }
                    let log_path = history.log_path(log_dir);
                    let stale = is_stale(&history, &log_path);
                    let rows: Vec<PullRow> = history
                        .pulls
                        .iter()
                        .filter(|p| p.kind != PhaseType::PvP)
                        .map(|p| row_from(&history, p))
                        .collect();
                    let pvp = pvp_rows(&history);
                    let dummy = dummy_rows(&history);
                    let server = rows
                        .iter()
                        .map(|r| r.server.clone())
                        .find(|s| !s.is_empty())
                        .unwrap_or_default();
                    let started_ms = history.started_at.map(|t| t.and_utc().timestamp_millis()).unwrap_or(0);
                    self.logs.insert(
                        name,
                        IndexedLog {
                            modified,
                            log_path,
                            character: history.character_name.clone().unwrap_or_else(|| "Unknown".into()),
                            server,
                            started_ms,
                            stale,
                            rows,
                            pvp,
                            dummy,
                            timer_starts: history.timer_starts,
                            visits: history.visits,
                            continuity: history.continuity,
                            areas: history.areas,
                        },
                    );
                }
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "Skipping unreadable history file")
                }
            }
        }
        self.logs.retain(|name, _| seen.contains(name));
        self.continued_in = self
            .logs
            .iter()
            .filter_map(|(name, l)| Some((l.continuity.resumed_from.clone()?, log_file_name(name))))
            .collect();
        self.backfill_area_ids();
        Ok(report)
    }

    /// Files written before area ids were recorded carry 0. The same area
    /// name in any re-scanned log tells us the id, which gives those rows
    /// definition order and run detection without waiting for their re-scan.
    fn backfill_area_ids(&mut self) {
        let known: HashMap<String, i64> = self
            .rows()
            .filter(|r| r.area_id != 0)
            .map(|r| (r.operation.clone(), r.area_id))
            .collect();
        if known.is_empty() {
            return;
        }
        for row in self
            .logs
            .values_mut()
            .flat_map(|l| l.rows.iter_mut())
            .filter(|r| r.area_id == 0)
        {
            if let Some(id) = known.get(&row.operation) {
                row.area_id = *id;
            }
        }
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
    pub fn overview(&self, filter: &PullFilter, catalog: &AreaCatalog, meta: &LibraryMeta) -> PullHistoryOverview {
        let characters = distinct(self.rows().map(|r| r.character.as_str()));
        let disciplines = distinct(self.rows().filter_map(|r| r.discipline.as_deref()));
        let servers = distinct(self.rows().map(|r| r.server.as_str()).filter(|s| !s.is_empty()));
        let mut versions = distinct(self.rows().map(|r| r.game_version.as_str()).filter(|s| !s.is_empty()));
        versions.reverse();

        // (pulls, kills, area_id, is_operation, is_flashpoint): the id is for definition order below
        let mut counts: HashMap<(&str, &str), (u32, u32, i64, bool, bool)> = HashMap::new();
        for r in self.filtered(filter, catalog, meta) {
            let e = counts.entry((&r.operation, &r.boss)).or_default();
            e.0 += 1;
            e.1 += u32::from(r.success);
            e.2 = e.2.max(r.area_id);
            e.3 |= r.is_operation;
            e.4 |= r.is_flashpoint;
        }
        // Operation order from the definition; bosses it does not know go last, by name
        let order = |area_id: i64, boss: &str| {
            catalog
                .area(area_id)
                .and_then(|a| a.bosses.iter().position(|b| b.name == boss))
                .unwrap_or(usize::MAX)
        };
        let mut keyed: Vec<(usize, BossNode)> = counts
            .into_iter()
            .map(
                |((operation, boss), (pulls, kills, area_id, is_operation, is_flashpoint))| {
                    (
                        order(area_id, boss),
                        BossNode {
                            operation: operation.to_string(),
                            boss: boss.to_string(),
                            pulls,
                            kills,
                            is_operation,
                            is_flashpoint,
                        },
                    )
                },
            )
            .collect();
        keyed.sort_by(|(oa, a), (ob, b)| (&a.operation, oa, &a.boss).cmp(&(&b.operation, ob, &b.boss)));
        let bosses: Vec<BossNode> = keyed.into_iter().map(|(_, n)| n).collect();
        PullHistoryOverview {
            characters,
            disciplines,
            bosses,
            tags: meta.run_tags(),
            servers,
            versions,
        }
    }

    /// Every pull of one boss under `filter`, newest first, with `path` set
    /// when the log exists.
    pub fn pulls(
        &self,
        operation: &str,
        boss: &str,
        filter: &PullFilter,
        catalog: &AreaCatalog,
        meta: &LibraryMeta,
    ) -> Vec<PullRow> {
        let mut out = Vec::new();
        let pull_tags = self.pull_tags(catalog, meta);
        for log in self.logs.values() {
            let mut exists = None;
            for r in log
                .rows
                .iter()
                .filter(|r| r.operation == operation && r.boss == boss && matches(r, filter))
            {
                let hidden = meta.is_pull_hidden(&r.filename, r.start_ms);
                if hidden && !filter.show_hidden {
                    continue;
                }
                let tags = tags_of(&pull_tags, r);
                if !filter.tags.is_empty() && !filter.tags.iter().any(|t| tags.contains(t)) {
                    continue;
                }
                // One stat per log: on disk, else in the archive
                let (exists, archived) = *exists.get_or_insert_with(|| {
                    let on_disk = log.log_path.is_file();
                    (on_disk, !on_disk && crate::library::archived_log(&r.filename).is_some())
                });
                let mut row = r.clone();
                row.path = exists.then(|| log.log_path.to_string_lossy().into_owned());
                row.archived = archived;
                row.tags = tags.to_vec();
                row.hidden = hidden;
                out.push(row);
            }
        }
        out.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        out
    }

    /// Operation runs under `filter`, newest first, with the user's tags and
    /// note attached by `meta`.
    pub fn runs(&self, catalog: &AreaCatalog, filter: &PullFilter, meta: &LibraryMeta) -> Vec<RunSummary> {
        let mut out: Vec<RunSummary> = self
            .built_runs(catalog, meta)
            .into_iter()
            .map(|r| r.summary)
            .filter(|r| run_matches(r, filter))
            .collect();
        out.sort_by(|a, b| b.started.cmp(&a.started));
        out
    }

    /// One run by its key, with its rows in order (for the detail view).
    pub fn run(
        &self,
        log_filename: &str,
        start_ms: i64,
        catalog: &AreaCatalog,
        meta: &LibraryMeta,
    ) -> Option<(RunSummary, Vec<PullRow>)> {
        let run = self
            .built_runs(catalog, meta)
            .into_iter()
            .find(|r| r.summary.filename == log_filename && r.summary.start_ms == start_ms)?;
        // A run can span logs, so each pull opens in its own
        let rows = run
            .rows
            .iter()
            .map(|r| {
                let mut row = (*r).clone();
                if let Some(log) = self.logs.get(history_file_name(&r.filename).as_str()) {
                    (row.path, row.archived) = log.location();
                }
                row.tags = run.summary.tags.clone();
                row
            })
            .collect();
        Some((run.summary, rows))
    }

    /// The run each pull of one log belongs to.
    pub fn run_links(&self, log_filename: &str, catalog: &AreaCatalog, meta: &LibraryMeta) -> Vec<RunLink> {
        self.built_runs(catalog, meta)
            .iter()
            .flat_map(|run| {
                run.rows.iter().filter(|r| r.filename == log_filename).map(|r| RunLink {
                    encounter_id: r.encounter_id,
                    filename: run.summary.filename.clone(),
                    start_ms: run.summary.start_ms,
                    is_flashpoint: run.summary.is_flashpoint,
                })
            })
            .collect()
    }

    /// Runs across every log, with the user's tags and note attached. A run
    /// merged from several logs keeps whichever of its old keys carries them.
    fn built_runs(&self, catalog: &AreaCatalog, meta: &LibraryMeta) -> Vec<BuiltRun<'_>> {
        // Sorted so equal-time visits chain the same way on every query
        let mut logs: Vec<(&String, &IndexedLog)> = self.logs.iter().collect();
        logs.sort_by_key(|(name, _)| *name);
        let sources: Vec<RunSource> = logs
            .into_iter()
            .map(|(_, log)| {
                let (path, archived) = log.location();
                RunSource {
                    rows: &log.rows,
                    visits: &log.visits,
                    timer_starts: &log.timer_starts,
                    path,
                    archived,
                }
            })
            .collect();
        let mut runs = build_runs(&sources, catalog);
        for run in &mut runs {
            let found = run.rows.iter().find_map(|r| meta.run(&r.filename, r.start_ms));
            apply_meta(&mut run.summary, found.cloned());
        }
        runs
    }

    /// Run tags per pull, keyed by `(filename, start_ms)`
    fn pull_tags(&self, catalog: &AreaCatalog, meta: &LibraryMeta) -> HashMap<(&str, i64), Vec<String>> {
        let mut out = HashMap::new();
        if meta.run_tags().is_empty() {
            return out;
        }
        for run in self
            .built_runs(catalog, meta)
            .into_iter()
            .filter(|r| !r.summary.tags.is_empty())
        {
            for r in run.rows {
                out.insert((r.filename.as_str(), r.start_ms), run.summary.tags.clone());
            }
        }
        out
    }

    /// Every row passing `filter`, across logs. Run tags are only resolved
    /// when the filter asks for one.
    fn filtered(&self, filter: &PullFilter, catalog: &AreaCatalog, meta: &LibraryMeta) -> Vec<&PullRow> {
        let mut out = Vec::new();
        let pull_tags = (!filter.tags.is_empty()).then(|| self.pull_tags(catalog, meta));
        for log in self.logs.values() {
            for r in log.rows.iter().filter(|r| r.operation != DUMMY_GROUP && matches(r, filter)) {
                // Hidden pulls stay out of the boss counts too
                if !filter.show_hidden && meta.is_pull_hidden(&r.filename, r.start_ms) {
                    continue;
                }
                if let Some(pull_tags) = &pull_tags
                    && !filter.tags.iter().any(|t| tags_of(pull_tags, r).contains(t))
                {
                    continue;
                }
                out.push(r);
            }
        }
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

    /// None when the log has no history file.
    pub fn facts_for(&self, log_filename: &str) -> Option<LogFacts<'_>> {
        let log = self.logs.get(history_file_name(log_filename).as_str())?;
        Some(LogFacts {
            areas: &log.areas,
            visits: &log.visits,
            continuity: &log.continuity,
            continued_in: self.continued_in.get(log_filename).map(String::as_str),
        })
    }

    /// Boss pulls across logs; dummy parses have their own view
    fn rows(&self) -> impl Iterator<Item = &PullRow> {
        self.logs
            .values()
            .flat_map(|l| l.rows.iter())
            .filter(|r| r.operation != DUMMY_GROUP)
    }

    /// Every training dummy kill, newest first, with `path` set when the
    /// log exists.
    pub fn dummy_parses(&self) -> Vec<DummyRow> {
        let mut out = Vec::new();
        for log in self.logs.values().filter(|l| !l.dummy.is_empty()) {
            let path = log
                .log_path
                .is_file()
                .then(|| log.log_path.to_string_lossy().into_owned());
            out.extend(log.dummy.iter().cloned().map(|mut d| {
                d.path = path.clone();
                d
            }));
        }
        out.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        out
    }

    /// Every PvP match and arena round, newest first, with `path` set when
    /// the log exists.
    pub fn pvp_matches(&self) -> Vec<PvpMatchRow> {
        let mut out = Vec::new();
        for log in self.logs.values().filter(|l| !l.pvp.is_empty()) {
            let path = log
                .log_path
                .is_file()
                .then(|| log.log_path.to_string_lossy().into_owned());
            out.extend(log.pvp.iter().cloned().map(|mut m| {
                m.path = path.clone();
                m
            }));
        }
        out.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        out
    }

    /// Every indexed log with its character, for the roster
    pub fn logs(&self) -> impl Iterator<Item = LogRef<'_>> {
        self.logs.iter().map(|(filename, l)| LogRef {
            filename,
            character: &l.character,
            server: &l.server,
            started_ms: l.started_ms,
            rows: &l.rows,
        })
    }
}

/// One log as the roster sees it
pub struct LogRef<'a> {
    pub filename: &'a str,
    pub character: &'a str,
    pub server: &'a str,
    pub started_ms: i64,
    pub rows: &'a [PullRow],
}

impl IndexedLog {
    /// `(path, archived)`: the log on disk, else whether the archive has it
    fn location(&self) -> (Option<String>, bool) {
        let path = self
            .log_path
            .is_file()
            .then(|| self.log_path.to_string_lossy().into_owned());
        let archived = path.is_none()
            && self
                .rows
                .first()
                .is_some_and(|r| crate::library::archived_log(&r.filename).is_some());
        (path, archived)
    }
}

fn tags_of<'a>(pull_tags: &'a HashMap<(&'a str, i64), Vec<String>>, r: &'a PullRow) -> &'a [String] {
    pull_tags
        .get(&(r.filename.as_str(), r.start_ms))
        .map_or(&[], Vec::as_slice)
}

/// Old store version, or the log (in the directory or the archive) no longer
/// matches the header. A log that is gone from both is not stale: there is
/// nothing to re-scan from.
/// The log filename an index key (`<log stem>.json`) stands for.
fn log_file_name(history_name: &str) -> String {
    format!("{}.txt", history_name.strip_suffix(".json").unwrap_or(history_name))
}

fn is_stale(history: &LogHistory, log_path: &Path) -> bool {
    let header = HistoryHeader {
        character_name: None,
        version: history.version,
        source_len: history.source_len,
        source_mtime: history.source_mtime,
    };
    // Gone from both comes first: a deleted log's old-version history must
    // not count toward "need a scan"
    match std::fs::metadata(log_path) {
        Ok(meta) => history.version < HISTORY_VERSION || !header.is_current(&meta),
        Err(_) => crate::library::archived_log(&history.filename)
            .is_some_and(|a| history.version < HISTORY_VERSION || !a.history_current(&header)),
    }
}

fn matches(r: &PullRow, f: &PullFilter) -> bool {
    let (tier, size) = difficulty_parts(&r.difficulty);
    keeps(&f.characters, &r.character)
        && (f.role.is_empty() || r.role.as_deref() == Some(f.role.as_str()))
        && (f.disciplines.is_empty() || r.discipline.as_ref().is_some_and(|d| f.disciplines.contains(d)))
        && (f.tier.is_empty() || tier == f.tier)
        && (f.group_size.is_empty() || r.is_flashpoint || size.to_string() == f.group_size)
        && (!f.kills_only || r.success)
        && keeps(&f.servers, &r.server)
        && keeps(&f.game_versions, &r.game_version)
        && in_date_range(&r.timestamp, f)
}

/// ISO timestamps compare lexicographically, so day bounds need no parsing
fn in_date_range(timestamp: &str, f: &PullFilter) -> bool {
    let day = timestamp.get(..10).unwrap_or(timestamp);
    (f.date_from.is_empty() || day >= f.date_from.as_str()) && (f.date_to.is_empty() || day <= f.date_to.as_str())
}

fn distinct<'a>(it: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut v: Vec<String> = it.collect::<HashSet<_>>().into_iter().map(str::to_string).collect();
    v.sort();
    v
}

fn row_from(log: &LogHistory, p: &PullRecord) -> PullRow {
    let character = log.character_name.clone().unwrap_or_else(|| "Unknown".into());
    let me = p.roster.iter().find(|m| m.name == character);
    let (operation, boss) = if p.kind == PhaseType::DummyParse {
        (DUMMY_GROUP.to_string(), DUMMY_GROUP.to_string())
    } else {
        (
            p.area_name.clone(),
            p.boss_name.clone().unwrap_or_else(|| p.display_name.clone()),
        )
    };
    let progress = p.progress.as_ref();
    PullRow {
        filename: log.filename.clone(),
        path: None,
        archived: false,
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
        area_id: p.area_id,
        server: server_name(&p.server).to_string(),
        game_version: game_version_at(p.start_time.get(..10).unwrap_or("")).to_string(),
        tags: Vec::new(),
        hidden: false,
        boss,
        difficulty: p.difficulty.clone().unwrap_or_else(|| "-".into()),
        success: p.success,
        duration_seconds: p.duration_secs,
        is_operation: p.kind == PhaseType::Raid,
        is_flashpoint: p.kind == PhaseType::Flashpoint,
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
        my_deaths: p.deaths.iter().filter(|d| d.name == character).count() as u16,
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
        let (catalog, meta) = (AreaCatalog::default(), LibraryMeta::default());
        let overview = index.overview(&filter, &catalog, &meta);
        let pulls = index.pulls(
            &overview.bosses[0].operation,
            &overview.bosses[0].boss,
            &filter,
            &catalog,
            &meta,
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
