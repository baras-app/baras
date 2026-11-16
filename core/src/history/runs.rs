//! Operation runs derived from recorded pulls: the pulls made in one server
//! instance of an area, split after the final boss dies. Nothing is stored;
//! runs are rebuilt from `PullRow`s and `InstanceVisit`s on every query.

use hashbrown::{HashMap, HashSet};

use baras_types::history::{
    InstanceVisit, PlayerSpec, PullFilter, PullRow, RunBoss, RunDetail, RunMeta, RunPlayer, RunSummary, RunTimer,
    TimerStart, keeps,
};

use super::catalog::AreaCatalog;
use super::record::{PullRecord, start_ms_of};

/// `("Master", 8)` from `"8 Player Master"`. Tier words cover the English,
/// German and French clients; anything else reads as `("", size)`.
pub fn difficulty_parts(difficulty: &str) -> (&'static str, u8) {
    let size = difficulty
        .split(|c: char| !c.is_ascii_digit())
        .find_map(|t| t.parse().ok())
        .unwrap_or(0);
    let lower = difficulty.to_lowercase();
    let tier = if ["master", "meister", "maître"].iter().any(|w| lower.contains(w)) {
        "Master"
    } else if ["veteran", "vétéran"].iter().any(|w| lower.contains(w)) {
        "Veteran"
    } else if ["story", "geschichte", "histoire"].iter().any(|w| lower.contains(w)) {
        "Story"
    } else {
        ""
    };
    (tier, size)
}

fn iso_of(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|t| t.naive_utc().format("%Y-%m-%dT%H:%M:%S").to_string())
        .unwrap_or_default()
}

fn end_ms(r: &PullRow) -> i64 {
    r.start_ms + r.duration_seconds * 1000
}

/// How long an instance may sit between two visits and still be the same
/// run. Process numbers are reused after server restarts, so counters from
/// far-apart visits prove nothing.
const MAX_VISIT_GAP_MS: i64 = 12 * 3600 * 1000;

/// One log's contribution to run building.
pub(super) struct RunSource<'a> {
    pub rows: &'a [PullRow],
    pub visits: &'a [InstanceVisit],
    pub timer_starts: &'a [TimerStart],
    pub path: Option<String>,
    pub archived: bool,
}

pub(super) struct BuiltRun<'a> {
    pub summary: RunSummary,
    /// Chronological; may span logs (disconnects, character swaps)
    pub rows: Vec<&'a PullRow>,
}

/// One instanced visit of one log with the pulls made during it.
struct Segment<'a> {
    src: usize,
    entered_ms: i64,
    visit: &'a InstanceVisit,
    rows: Vec<&'a PullRow>,
}

/// A run being assembled, with what its visits say about the instance.
#[derive(Default)]
struct OpenRun<'a> {
    rows: Vec<(usize, &'a PullRow)>,
    /// Final boss killed: later pulls start a new run
    closed: bool,
    src: usize,
    prefix: i64,
    max_ctr: i64,
    trash: HashSet<i64>,
    /// A boss died: the group is saved to this instance's lockout
    has_kill: bool,
    last_ms: i64,
}

impl<'a> OpenRun<'a> {
    /// Whether `seg` is another visit of this run.
    fn continues_into(&self, seg: &Segment) -> bool {
        let v = seg.visit;
        if v.prefix == 0 || self.prefix == 0 {
            // No NPC seen on one side: nothing to compare, so a log's
            // consecutive visits stay together as they always have
            return seg.src == self.src;
        }
        if seg.entered_ms - self.last_ms > MAX_VISIT_GAP_MS {
            return false;
        }
        if v.prefix != self.prefix {
            // An emptied instance is torn down. Coming back under the lockout
            // lands in a new process with the kills intact; without kills (or
            // on another character) a new process is a fresh instance.
            let character = self.rows.last().map(|(_, r)| r.character.as_str());
            return self.has_kill && seg.rows.first().map(|r| r.character.as_str()) == character;
        }
        // An NPC that already existed when the earlier visit ended
        if v.min_ctr <= self.max_ctr {
            return true;
        }
        // Everything is newer. A kill locks the group to the instance, so
        // only before the first one can this be a reset, which shows as the
        // opening trash dying a second time.
        self.has_kill || !v.trash.iter().any(|c| self.trash.contains(c))
    }

    fn absorb(&mut self, seg: &Segment) {
        let v = seg.visit;
        if v.prefix != 0 {
            self.max_ctr = if self.prefix == v.prefix { self.max_ctr.max(v.max_ctr) } else { v.max_ctr };
            self.prefix = v.prefix;
        }
        self.trash.extend(v.trash.iter().copied());
        self.last_ms = self.last_ms.max(seg.entered_ms);
        self.src = seg.src;
    }

    fn push(&mut self, src: usize, r: &'a PullRow, catalog: &AreaCatalog) {
        self.rows.push((src, r));
        self.last_ms = self.last_ms.max(end_ms(r));
        self.has_kill |= r.success;
        self.closed = is_final_kill(r, catalog);
    }
}

fn is_final_kill(r: &PullRow, catalog: &AreaCatalog) -> bool {
    r.success && catalog.area(r.area_id).is_some_and(|a| a.is_final(&r.boss))
}

/// Every run across `sources`, chronological. Visits chain into one run
/// while their NPC ids and boss kills say it is the same instance or lockout
/// (see `OpenRun::continues_into`), across logs when it is. Logs recorded
/// before visits existed fall back to consecutive same-area pulls.
pub(super) fn build_runs<'a>(sources: &[RunSource<'a>], catalog: &AreaCatalog) -> Vec<BuiltRun<'a>> {
    let mut segments: Vec<Segment<'a>> = Vec::new();
    let mut runs: Vec<OpenRun<'a>> = Vec::new();

    for (src, source) in sources.iter().enumerate() {
        let mut sorted: Vec<&PullRow> = source.rows.iter().filter(|r| r.is_operation || r.is_flashpoint).collect();
        sorted.sort_by_key(|r| r.start_ms);
        let first = segments.len();
        segments.extend(source.visits.iter().filter_map(|visit| {
            Some(Segment { src, entered_ms: start_ms_of(&visit.entered)?, visit, rows: Vec::new() })
        }));

        // Pulls outside any recorded visit: the pre-visit grouping
        let mut closed = true;
        let mut legacy: Option<usize> = None;
        for r in sorted {
            let seg = segments[first..]
                .iter_mut()
                .rev()
                .find(|s| s.entered_ms <= r.start_ms && s.visit.area_id == r.area_id);
            if let Some(seg) = seg {
                seg.rows.push(r);
                continue;
            }
            let same = legacy.is_some_and(|i| {
                let f = runs[i].rows[0].1;
                f.area_id == r.area_id && f.operation == r.operation && f.difficulty == r.difficulty
            });
            if closed || !same {
                runs.push(OpenRun { src, ..Default::default() });
                legacy = Some(runs.len() - 1);
            }
            runs[legacy.unwrap()].push(src, r, catalog);
            closed = runs[legacy.unwrap()].closed;
        }
    }

    // A visit without pulls only matters for what its NPC ids prove
    segments.retain(|s| s.visit.prefix != 0 || !s.rows.is_empty());
    segments.sort_by_key(|s| s.entered_ms);

    let mut latest: HashMap<(i64, i64), usize> = HashMap::new();
    for seg in &segments {
        let key = (seg.visit.area_id, seg.visit.difficulty_id);
        let open = latest.get(&key).copied().filter(|&i| !runs[i].closed && runs[i].continues_into(seg));
        if open.is_none() && seg.rows.is_empty() {
            // Nothing ties this visit to the run and it starts none itself
            continue;
        }
        let mut cur = open.unwrap_or_else(|| {
            runs.push(OpenRun::default());
            runs.len() - 1
        });
        runs[cur].absorb(seg);
        for r in &seg.rows {
            if runs[cur].closed {
                runs.push(OpenRun::default());
                cur = runs.len() - 1;
                runs[cur].absorb(seg);
            }
            runs[cur].push(seg.src, r, catalog);
        }
        latest.insert(key, cur);
    }

    runs.retain(|r| !r.rows.is_empty());
    runs.sort_by_key(|r| r.rows[0].1.start_ms);

    // The clock start must fall after the previous run of the same log
    let mut prev_end: HashMap<usize, i64> = HashMap::new();
    runs.into_iter()
        .map(|run| {
            let source = &sources[run.rows[0].0];
            let rows: Vec<&PullRow> = run.rows.iter().map(|(_, r)| *r).collect();
            let mut summary =
                summarize(&rows, prev_end.get(&run.rows[0].0).copied(), source.timer_starts, source.path.as_deref(), catalog);
            summary.archived = source.archived;
            for (src, r) in &run.rows {
                let e = prev_end.entry(*src).or_insert(i64::MIN);
                *e = (*e).max(end_ms(r));
            }
            BuiltRun { summary, rows }
        })
        .collect()
}

fn summarize(
    g: &[&PullRow],
    prev_end: Option<i64>,
    timer_starts: &[TimerStart],
    path: Option<&str>,
    catalog: &AreaCatalog,
) -> RunSummary {
    let first = g[0];
    let (tier, size) = difficulty_parts(&first.difficulty);
    let tier_key = tier.to_lowercase();
    let def = catalog.area(first.area_id);
    let final_kill = g
        .iter()
        .find(|r| r.success && def.is_some_and(|a| a.is_final(&r.boss)));
    let last_end = g.iter().map(|r| end_ms(r)).max().unwrap_or(first.start_ms);
    // The in-game timed run needs every required boss down, not just the last one
    let full_clear = def.is_some_and(|a| {
        a.bosses_at(&tier_key, size)
            .all(|b| g.iter().any(|r| r.success && r.boss == b.name))
    });

    // Every run gets a clock: from the timer start line (else the first pull)
    // to the final kill, or to the last pull's end when the run was not cleared
    let start = timer_starts
        .iter()
        .filter(|t| t.area_id == first.area_id)
        .filter_map(|t| start_ms_of(&t.time))
        .filter(|&ms| ms <= first.start_ms && prev_end.is_none_or(|p| ms > p))
        .max();
    let started = start.unwrap_or(first.start_ms);
    let elapsed_secs = (final_kill.map_or(last_end, |k| end_ms(k)) - started) / 1000;
    let limit_secs = def.and_then(|a| a.timer_at(&tier_key, size)).and_then(|r| r.limit_secs);
    let timer = RunTimer {
        started: iso_of(started),
        from_first_pull: start.is_none(),
        elapsed_secs,
        limit_secs,
        achieved: limit_secs.map(|l| full_clear && elapsed_secs <= i64::from(l)),
    };

    let played = g.iter().find(|r| r.discipline.is_some()).unwrap_or(&first);
    RunSummary {
        filename: first.filename.clone(),
        path: path.map(str::to_string),
        archived: false,
        start_ms: first.start_ms,
        character: first.character.clone(),
        discipline: played.discipline.clone(),
        discipline_icon: played.discipline_icon.clone(),
        role: played.role.clone(),
        role_icon: played.role_icon.clone(),
        server: first.server.clone(),
        game_version: first.game_version.clone(),
        operation: def.map_or_else(|| first.operation.clone(), |a| a.name.clone()),
        area_id: first.area_id,
        is_flashpoint: first.is_flashpoint,
        difficulty: first.difficulty.clone(),
        tier: tier.to_string(),
        group_size: size,
        started: first.timestamp.clone(),
        ended: iso_of(last_end),
        pulls: g.len() as u32,
        // Matches `bosses_total`: optional encounters never count as a boss kill
        kills: g.iter().filter(|r| r.success && !def.is_some_and(|a| a.is_optional(&r.boss))).count() as u32,
        bosses_total: def.map_or(0, |a| a.bosses_at(&tier_key, size).count() as u32),
        cleared: final_kill.is_some(),
        deaths: g.iter().map(|r| u32::from(r.deaths)).sum(),
        my_deaths: g.iter().map(|r| u32::from(r.my_deaths)).sum(),
        pull_secs: g.iter().map(|r| r.duration_seconds).sum(),
        span_secs: (last_end - first.start_ms) / 1000,
        timer,
        tags: Vec::new(),
        note: String::new(),
        hidden: false,
    }
}

/// Toolbar filter applied to a run: role and discipline are the ones the
/// logging character played; "kills only" means cleared runs; group size
/// is an operations thing.
pub(super) fn run_matches(run: &RunSummary, f: &PullFilter) -> bool {
    let day = run.started.get(..10).unwrap_or(&run.started);
    keeps(&f.characters, &run.character)
        && (f.role.is_empty() || run.role.as_deref() == Some(f.role.as_str()))
        && (f.disciplines.is_empty() || run.discipline.as_ref().is_some_and(|d| f.disciplines.contains(d)))
        && (f.flashpoints || !run.is_flashpoint)
        && (f.tier.is_empty() || run.tier == f.tier)
        && (f.group_size.is_empty() || run.is_flashpoint || run.group_size.to_string() == f.group_size)
        && (!f.kills_only || run.cleared)
        && (!f.timed_only || run.timer.achieved == Some(true))
        && (f.show_hidden || !run.hidden)
        && (f.tags.is_empty() || f.tags.iter().any(|t| run.tags.contains(t)))
        && keeps(&f.servers, &run.server)
        && keeps(&f.game_versions, &run.game_version)
        && (f.date_from.is_empty() || day >= f.date_from.as_str())
        && (f.date_to.is_empty() || day <= f.date_to.as_str())
}

pub(super) fn apply_meta(run: &mut RunSummary, meta: Option<RunMeta>) {
    if let Some(m) = meta {
        run.tags = m.tags;
        run.note = m.note;
        run.hidden = m.hidden;
    }
}

/// Boss order, per-player deaths and averages for one run. `records` are the
/// log's full pull records (only those belonging to the run are read).
pub fn run_detail(run: RunSummary, rows: Vec<PullRow>, records: &[PullRecord], catalog: &AreaCatalog) -> RunDetail {
    let (tier, size) = difficulty_parts(&run.difficulty);
    let tier_key = tier.to_lowercase();
    let def = catalog.area(run.area_id);

    // Definition order first, then anything pulled that the definition lacks
    let mut names: Vec<String> = def
        .map(|a| a.bosses_at(&tier_key, size).map(|b| b.name.clone()).collect())
        .unwrap_or_default();
    for r in &rows {
        if !names.contains(&r.boss) {
            names.push(r.boss.clone());
        }
    }
    let bosses = names
        .into_iter()
        .map(|name| {
            let pulls: Vec<&PullRow> = rows.iter().filter(|r| r.boss == name).collect();
            let kill = pulls.iter().find(|r| r.success);
            let best = pulls
                .iter()
                .filter(|r| !r.success)
                .filter_map(|r| r.boss_hp_pct.map(|p| (p, *r)))
                .min_by(|a, b| a.0.total_cmp(&b.0));
            RunBoss {
                pulls: pulls.len() as u32,
                killed: kill.is_some(),
                kill_time: kill.map(|k| iso_of(end_ms(k))),
                kill_secs: kill.map(|k| (end_ms(k) - run.start_ms) / 1000),
                best_hp_pct: best.map(|(p, _)| p),
                phase: best
                    .and_then(|(_, r)| r.phase.clone())
                    .or_else(|| pulls.last().and_then(|r| r.phase.clone())),
                deaths: pulls.iter().map(|r| u32::from(r.deaths)).sum(),
                skipped: pulls.is_empty(),
                name,
            }
        })
        .collect();

    #[derive(Default)]
    struct Acc {
        /// Disciplines in order of first appearance
        specs: Vec<(crate::game_data::Discipline, String)>,
        pulls: u32,
        deaths: u32,
        first_deaths: u32,
        dps: i64,
        hps: i64,
        ehps: i64,
    }
    let in_run: HashSet<i64> = rows.iter().map(|r| r.start_ms).collect();
    let mut acc: HashMap<&str, Acc> = HashMap::new();
    for p in records.iter().filter(|p| in_run.contains(&p.start_ms)) {
        for line in &p.roster {
            let a = acc.entry(line.name.as_str()).or_default();
            a.pulls += 1;
            a.dps += line.dps;
            a.hps += line.hps;
            a.ehps += line.ehps;
            if let Some(d) = line.discipline
                && !a.specs.iter().any(|(x, _)| *x == d)
            {
                a.specs.push((d, line.discipline_name.clone().unwrap_or_default()));
            }
        }
        for (i, d) in p.deaths.iter().enumerate() {
            let a = acc.entry(d.name.as_str()).or_default();
            a.deaths += 1;
            a.first_deaths += u32::from(i == 0);
        }
    }
    let mut players: Vec<RunPlayer> = acc
        .into_iter()
        .map(|(name, a)| {
            let n = i64::from(a.pulls.max(1));
            RunPlayer {
                name: name.to_string(),
                specs: a
                    .specs
                    .into_iter()
                    .map(|(d, name)| PlayerSpec {
                        discipline: name,
                        discipline_icon: d.icon_name().to_string(),
                        role_icon: d.role().icon_name().to_string(),
                    })
                    .collect(),
                pulls: a.pulls,
                deaths: a.deaths,
                first_deaths: a.first_deaths,
                avg_dps: a.dps / n,
                avg_hps: a.hps / n,
                avg_ehps: a.ehps / n,
            }
        })
        .collect();
    // Grouped by the role first played, highest DPS first within it
    let role = |p: &RunPlayer| p.specs.first().map(|s| s.role_icon.clone());
    players.sort_by(|a, b| (role(a), b.avg_dps).cmp(&(role(b), a.avg_dps)));

    let first_deaths = rows
        .iter()
        .map(|r| {
            records
                .iter()
                .find(|p| p.start_ms == r.start_ms)
                .and_then(|p| p.deaths.first())
                .map(|d| d.name.clone())
        })
        .collect();

    RunDetail { run, bosses, players, pulls: rows, first_deaths }
}
