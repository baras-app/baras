//! Operation runs derived from one log's pulls: consecutive pulls in the same
//! area at the same difficulty, split after the final boss dies. Nothing is
//! stored; runs are rebuilt from `PullRow`s on every query.

use hashbrown::{HashMap, HashSet};

use baras_types::history::{
    PullFilter, PullRow, RunBoss, RunDetail, RunMeta, RunPlayer, RunSummary, RunTimer, TimerStart,
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

/// Runs of one log, chronological. `path` is the log's on-disk path when it exists.
pub(super) fn runs_of_log(
    rows: &[PullRow],
    timer_starts: &[TimerStart],
    path: Option<&str>,
    catalog: &AreaCatalog,
) -> Vec<RunSummary> {
    let mut sorted: Vec<&PullRow> = rows.iter().filter(|r| r.is_operation).collect();
    sorted.sort_by_key(|r| r.start_ms);

    let mut groups: Vec<Vec<&PullRow>> = Vec::new();
    let mut closed = true;
    for r in sorted {
        let same = groups.last().is_some_and(|g| {
            let f = g[0];
            f.area_id == r.area_id && f.operation == r.operation && f.difficulty == r.difficulty
        });
        if closed || !same {
            groups.push(Vec::new());
        }
        groups.last_mut().unwrap().push(r);
        closed = r.success && catalog.area(r.area_id).is_some_and(|a| a.is_final(&r.boss));
    }

    let mut prev_end: Option<i64> = None;
    groups
        .into_iter()
        .map(|g| {
            let run = summarize(&g, prev_end, timer_starts, path, catalog);
            prev_end = Some(g.iter().map(|r| end_ms(r)).max().unwrap_or(run.start_ms));
            run
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

    let timer = def.and_then(|a| a.timer_at(&tier_key, size)).map(|rule| {
        let start = timer_starts
            .iter()
            .filter(|t| t.area_id == first.area_id)
            .filter_map(|t| start_ms_of(&t.time))
            .filter(|&ms| ms <= first.start_ms && prev_end.is_none_or(|p| ms > p))
            .max();
        let started = start.unwrap_or(first.start_ms);
        let elapsed_secs = final_kill.map(|k| (end_ms(k) - started) / 1000);
        RunTimer {
            started: iso_of(started),
            from_first_pull: start.is_none(),
            elapsed_secs,
            limit_secs: rule.limit_secs,
            achieved: elapsed_secs.zip(rule.limit_secs).map(|(e, l)| e <= i64::from(l)),
        }
    });

    let played = g.iter().find(|r| r.discipline.is_some()).unwrap_or(&first);
    RunSummary {
        filename: first.filename.clone(),
        path: path.map(str::to_string),
        start_ms: first.start_ms,
        character: first.character.clone(),
        discipline: played.discipline.clone(),
        discipline_icon: played.discipline_icon.clone(),
        role_icon: played.role_icon.clone(),
        server: first.server.clone(),
        game_version: first.game_version.clone(),
        operation: def.map_or_else(|| first.operation.clone(), |a| a.name.clone()),
        area_id: first.area_id,
        difficulty: first.difficulty.clone(),
        tier: tier.to_string(),
        group_size: size,
        started: first.timestamp.clone(),
        ended: iso_of(last_end),
        pulls: g.len() as u32,
        kills: g.iter().filter(|r| r.success).count() as u32,
        bosses_total: def.map_or(0, |a| a.bosses_at(&tier_key, size).count() as u32),
        cleared: final_kill.is_some(),
        deaths: g.iter().map(|r| u32::from(r.deaths)).sum(),
        my_deaths: g.iter().map(|r| u32::from(r.my_deaths)).sum(),
        pull_secs: g.iter().map(|r| r.duration_seconds).sum(),
        span_secs: (last_end - first.start_ms) / 1000,
        timer,
        tags: Vec::new(),
        note: String::new(),
    }
}

/// Toolbar filter applied to a run: role, discipline and "operations only"
/// do not apply; "kills only" means cleared runs.
pub(super) fn run_matches(run: &RunSummary, f: &PullFilter) -> bool {
    let day = run.started.get(..10).unwrap_or(&run.started);
    (f.character.is_empty() || run.character == f.character)
        && (f.tier.is_empty() || run.tier == f.tier)
        && (f.group_size.is_empty() || run.group_size.to_string() == f.group_size)
        && (!f.kills_only || run.cleared)
        && (f.tag.is_empty() || run.tags.contains(&f.tag))
        && (f.server.is_empty() || run.server == f.server)
        && (f.game_version.is_empty() || run.game_version == f.game_version)
        && (f.date_from.is_empty() || day >= f.date_from.as_str())
        && (f.date_to.is_empty() || day <= f.date_to.as_str())
}

pub(super) fn apply_meta(run: &mut RunSummary, meta: Option<RunMeta>) {
    if let Some(m) = meta {
        run.tags = m.tags;
        run.note = m.note;
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
        discipline: Option<crate::game_data::Discipline>,
        discipline_name: Option<String>,
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
            if line.discipline.is_some() {
                a.discipline = line.discipline;
                a.discipline_name = line.discipline_name.clone();
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
                discipline: a.discipline_name,
                discipline_icon: a.discipline.map(|d| d.icon_name().to_string()),
                role_icon: a.discipline.map(|d| d.role().icon_name().to_string()),
                pulls: a.pulls,
                deaths: a.deaths,
                first_deaths: a.first_deaths,
                avg_dps: a.dps / n,
                avg_hps: a.hps / n,
                avg_ehps: a.ehps / n,
            }
        })
        .collect();
    players.sort_by(|a, b| (&a.role_icon, b.avg_dps).cmp(&(&b.role_icon, a.avg_dps)));

    RunDetail { run, bosses, players, pulls: rows }
}
