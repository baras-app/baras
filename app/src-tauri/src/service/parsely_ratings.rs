//! Parsely percentile tables (`/api/rating/{boss}/{mode}/{size}`).
//!
//! Parsely rebuilds its tables once a day and stamps each with when
//! (`_meta.updated_at`). A table is final until a day after its stamp, so it
//! is downloaded once per rebuild; only when that rebuild is overdue is it
//! asked for again, at most once per `RECHECK_INTERVAL`. One JSON file per
//! table under `parsely_ratings/`. The tables of one area (the one entered live, or
//! the one being viewed) stay in memory so tabbing between its bosses touches
//! neither disk nor network.
//!
//! Nothing here is allowed to fail loudly: a missing table only means no
//! percentiles. See `FetchError` for how failures are retried.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Deserializer, Serialize};
use tracing::{debug, warn};

use baras_core::game_data::Role;
use baras_types::{DummyParse, DummyPercentiles, EncounterRatings, RatingPercentiles, RatingSource, TtkRating};

use super::ServiceHandle;
use crate::commands::USER_AGENT;
use crate::state::MutexExt;

const RATING_URL: &str = "https://parsely.io/api/rating";
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);
/// A table is superseded this long after Parsely built it: the daily rebuild
/// plus slack for one that runs a little late
const REBUILD_PERIOD_SECS: i64 = 24 * 60 * 60 + 30 * 60;
/// How often a table whose rebuild is overdue is asked for again
const RECHECK_INTERVAL_SECS: i64 = 60 * 60;
/// No requests at all for this long after the site couldn't be reached
const UNREACHABLE_PAUSE: Duration = Duration::from_secs(15 * 60);
/// A table is ~350 KB; anything far beyond that isn't one
const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;

// ─────────────────────────────────────────────────────────────────────────
// Table
// ─────────────────────────────────────────────────────────────────────────

/// Discipline key → thresholds, best percentile first. A key is `all` or a
/// mirror pair of discipline slugs (`annihilation,watchman`).
type Thresholds = HashMap<String, Vec<f32>>;

/// `_meta`: when Parsely built the table and how many parses it rates
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Meta {
    updated_at: String,
    counts: Counts,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Counts {
    all: u32,
    /// Per single discipline slug; mirrors are summed in `Meta::count`
    disciplines: HashMap<String, u32>,
}

impl Meta {
    /// Parses behind a discipline's percentiles: its own plus its mirror's
    fn count(&self, key: &str) -> u32 {
        key.split(',')
            .filter_map(|slug| self.counts.disciplines.get(slug))
            .sum()
    }
}

/// The rated metrics we show; doubles as the API response and the on-disk
/// format. Parsing is lenient so additions to the API don't break it: unknown
/// metrics and fields are ignored, and a metric or discipline that is missing
/// or not a number list is simply unrated.
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct RatingTable {
    /// UTC day (`YYYY-MM-DD`) this table was fetched
    fetched: String,
    /// Unix seconds of that fetch
    checked: i64,
    #[serde(deserialize_with = "lenient")]
    dps: Thresholds,
    #[serde(deserialize_with = "lenient")]
    boss_dps: Thresholds,
    #[serde(deserialize_with = "lenient")]
    hps: Thresholds,
    #[serde(deserialize_with = "lenient")]
    ehps: Thresholds,
    #[serde(deserialize_with = "lenient")]
    dtps: Thresholds,
    #[serde(deserialize_with = "lenient")]
    apm: Thresholds,
    /// Crit rate and share of damage from crits; dummy parses only
    #[serde(deserialize_with = "lenient")]
    crit_percent: Thresholds,
    #[serde(deserialize_with = "lenient")]
    crit_damage_percent: Thresholds,
    /// Fight length in seconds, under `all` only
    #[serde(deserialize_with = "lenient")]
    ttk: Thresholds,
    #[serde(rename = "_meta", deserialize_with = "lenient_meta")]
    meta: Meta,
}

impl RatingTable {
    fn is_empty(&self) -> bool {
        [&self.dps, &self.boss_dps, &self.hps, &self.ehps, &self.dtps, &self.apm]
            .iter()
            .all(|m| m.is_empty())
    }
}

fn lenient<'de, D: Deserializer<'de>>(d: D) -> Result<Thresholds, D::Error> {
    let raw = serde_json::Value::deserialize(d)?;
    Ok(serde_json::from_value::<HashMap<String, serde_json::Value>>(raw)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(k, v)| Some((k, serde_json::from_value::<Vec<f32>>(v).ok()?)))
        .filter(|(_, v)| !v.is_empty())
        .collect())
}

impl RatingTable {
    /// Nothing newer to fetch: the next rebuild isn't due, or we asked
    /// recently and it hadn't happened yet
    fn is_current(&self) -> bool {
        let now = chrono::Utc::now().timestamp();
        let built = chrono::DateTime::parse_from_rfc3339(&self.meta.updated_at).map_or(0, |t| t.timestamp());
        now - built < REBUILD_PERIOD_SECS || now - self.checked < RECHECK_INTERVAL_SECS
    }

    fn source(&self) -> RatingSource {
        RatingSource {
            total: self.meta.counts.all,
            // `2026-11-22T06:08:14+00:00` → the day
            updated: self.meta.updated_at.split('T').next().unwrap_or_default().to_string(),
        }
    }
}

fn lenient_meta<'de, D: Deserializer<'de>>(d: D) -> Result<Meta, D::Error> {
    Ok(serde_json::from_value(serde_json::Value::deserialize(d)?).unwrap_or_default())
}

/// The key rating `slug` (a discipline or `all`): the mirror pair naming it
fn rating_key<'a>(thresholds: &'a Thresholds, slug: &str) -> Option<&'a str> {
    thresholds
        .keys()
        .map(String::as_str)
        .find(|key| key.split(',').any(|s| s == slug))
}

/// A discipline's slug in the rating tables (`advanced-prototype`)
fn disc_slug(discipline: baras_core::game_data::Discipline) -> String {
    discipline.name().to_ascii_lowercase().replace(' ', "-")
}

/// Percentile (1-100) of `value` among a discipline's thresholds; 0 when it
/// reaches none or the discipline isn't rated
fn percentile(thresholds: &Thresholds, slug: &str, value: f32) -> u8 {
    let Some(t) = rating_key(thresholds, slug)
        .and_then(|key| thresholds.get(key))
        .filter(|_| value > 0.0)
    else {
        return 0;
    };
    // Damage taken and kill time are listed ascending (lower is better), the
    // rest descending
    let ascending = t.first() < t.last();
    let missed = t.partition_point(|&x| if ascending { x < value } else { x > value });
    // 100 entries today, one per percentile; scaled in case that changes
    ((t.len() - missed) * 100 / t.len()) as u8
}

// ─────────────────────────────────────────────────────────────────────────
// Fetching
// ─────────────────────────────────────────────────────────────────────────

enum FetchError {
    /// Offline, timeout, or a non-success status (site down, endpoint moved):
    /// nothing is requested again until `UNREACHABLE_PAUSE` has passed
    Unreachable(String),
    /// The site answered but not with a table for this path (unknown boss, a
    /// response we can't read): that path isn't requested again this session
    Rejected(String),
}

/// Errors come back as HTTP 200 with this body
#[derive(Deserialize)]
struct ApiError {
    status: String,
    #[serde(default)]
    message: String,
}

async fn fetch(path: &str) -> Result<RatingTable, FetchError> {
    let unreachable = |e: reqwest::Error| FetchError::Unreachable(e.to_string());
    // One client for every table, so a prefetch reuses its connection
    static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(reqwest::Client::new);
    let bytes = CLIENT
        .get(format!("{RATING_URL}/{path}"))
        .header("User-Agent", USER_AGENT)
        .timeout(FETCH_TIMEOUT)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(unreachable)?
        .bytes()
        .await
        .map_err(unreachable)?;

    if bytes.len() > MAX_BODY_BYTES {
        return Err(FetchError::Rejected(format!(
            "response too large ({} bytes)",
            bytes.len()
        )));
    }
    if let Ok(e) = serde_json::from_slice::<ApiError>(&bytes)
        && e.status == "error"
    {
        return Err(FetchError::Rejected(e.message));
    }
    match serde_json::from_slice::<RatingTable>(&bytes) {
        Ok(t) if !t.is_empty() => Ok(t),
        Ok(_) => Err(FetchError::Rejected("no known metrics in response".into())),
        Err(e) => Err(FetchError::Rejected(format!("unreadable response: {e}"))),
    }
}

fn cache_file(path: &str) -> Option<PathBuf> {
    Some(
        baras_core::paths::config_dir()?
            .join("parsely_ratings")
            .join(format!("{}.json", path.replace('/', "_"))),
    )
}

/// The cached file whatever its age; None when missing or unreadable (a
/// corrupt file, or one from before the API carried `_meta`, is just fetched
/// again)
fn read_cache_file(path: &str) -> Option<RatingTable> {
    let bytes = std::fs::read(cache_file(path)?).ok()?;
    serde_json::from_slice::<RatingTable>(&bytes)
        .ok()
        .filter(|t| !t.is_empty() && !t.meta.updated_at.is_empty())
}

fn write_cache_file(path: &str, table: &RatingTable) {
    let Some(file) = cache_file(path) else { return };
    let written = file
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|_| serde_json::to_vec(table).map_err(std::io::Error::other))
        // Whole file or nothing, so a crash can't leave half a table behind
        .and_then(|json| {
            let tmp = file.with_extension("tmp");
            std::fs::write(&tmp, json).and_then(|_| std::fs::rename(&tmp, &file))
        });
    // The table still serves from memory; it is fetched again next launch
    if let Err(e) = written {
        warn!("Could not cache Parsely ratings at {}: {e}", file.display());
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Cache
// ─────────────────────────────────────────────────────────────────────────

#[derive(Default)]
struct CacheState {
    /// Tables of the current area, by path
    tables: HashMap<String, Arc<RatingTable>>,
    rejected: HashSet<String>,
    unreachable_until: Option<Instant>,
}

impl CacheState {
    /// No request may be made for `path` right now
    fn blocked(&self, path: &str) -> bool {
        self.rejected.contains(path) || self.unreachable_until.is_some_and(|t| Instant::now() < t)
    }
}

#[derive(Default)]
pub struct RatingsCache {
    state: Mutex<CacheState>,
    /// One request at a time, so a view and a prefetch never download the
    /// same table twice and the site is never hit in parallel
    fetching: tokio::sync::Mutex<()>,
}

fn today() -> String {
    chrono::Utc::now().date_naive().to_string()
}

impl RatingsCache {
    /// Hold only `paths` (one area's bosses) in memory
    fn keep_only(&self, paths: &[String]) {
        self.state.locked().tables.retain(|p, _| paths.contains(p));
    }

    /// The table for `path` from memory or disk, never the network. A stale
    /// table is good enough only while a fresh one can't be requested.
    fn local(&self, path: &str) -> Option<Arc<RatingTable>> {
        let mut state = self.state.locked();
        let usable = |t: &RatingTable, state: &CacheState| t.is_current() || state.blocked(path);
        if let Some(t) = state.tables.get(path)
            && usable(t, &state)
        {
            return Some(Arc::clone(t));
        }
        let table = Arc::new(read_cache_file(path).filter(|t| usable(t, &state))?);
        state.tables.insert(path.to_string(), Arc::clone(&table));
        Some(table)
    }

    /// The table for `path`, downloading it when the cached one isn't current. A
    /// failed download falls back to the stale file, if there is one.
    async fn get(&self, path: &str) -> Option<Arc<RatingTable>> {
        if let Some(t) = self.local(path) {
            return Some(t);
        }
        let _one_at_a_time = self.fetching.lock().await;
        // Whoever held the lock may have fetched this table, or found the site down
        if let Some(t) = self.local(path) {
            return Some(t);
        }
        if self.state.locked().blocked(path) {
            return None;
        }

        let fetched = fetch(path).await;
        let mut state = self.state.locked();
        let table = match fetched {
            Ok(mut t) => {
                t.fetched = today();
                t.checked = chrono::Utc::now().timestamp();
                state.unreachable_until = None;
                write_cache_file(path, &t);
                t
            }
            Err(e) => {
                match e {
                    FetchError::Unreachable(e) => {
                        warn!("Parsely ratings unreachable, pausing requests ({path}): {e}");
                        state.unreachable_until = Some(Instant::now() + UNREACHABLE_PAUSE);
                    }
                    FetchError::Rejected(e) => {
                        warn!("Parsely has no usable ratings for {path}: {e}");
                        state.rejected.insert(path.to_string());
                    }
                }
                read_cache_file(path)?
            }
        };
        let table = Arc::new(table);
        state.tables.insert(path.to_string(), Arc::clone(&table));
        Some(table)
    }

    /// Make `paths` the set held in memory, downloading the ones not
    /// current. Gives up at the first failure when the site is unreachable.
    pub async fn load_set(&self, paths: Vec<String>) {
        self.keep_only(&paths);
        for path in &paths {
            if self.get(path).await.is_none() && self.state.locked().unreachable_until.is_some() {
                debug!("Parsely ratings prefetch stopped at {path}");
                break;
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Query
// ─────────────────────────────────────────────────────────────────────────

impl ServiceHandle {
    /// Kill time percentile of every finished kill of a rated boss in the
    /// session, by encounter id. Downloads the tables not cached today, one
    /// boss at a time; kills whose table can't be had are left out.
    pub async fn parsely_ttk_ratings(&self) -> Vec<(u64, TtkRating)> {
        // Copy what the lookup needs so the session lock isn't held over a fetch
        let kills: Vec<(u64, String, i64)> = {
            let session_guard = self.shared.session.read().await;
            let Some(session) = session_guard.as_ref() else {
                return Vec::new();
            };
            let session = session.read().await;
            let Some(cache) = session.session_cache.as_ref() else {
                return Vec::new();
            };
            cache
                .encounter_history
                .summaries()
                .iter()
                .filter(|s| s.success && !s.partial)
                .filter_map(|s| Some((s.encounter_id, s.parsely_rating.clone()?, s.duration_seconds)))
                .collect()
        };

        let mut ratings = Vec::with_capacity(kills.len());
        for (id, path, duration) in kills {
            let Some(table) = self.shared.parsely_ratings.get(&path).await else {
                continue;
            };
            let pct = percentile(&table.ttk, "all", duration as f32);
            if pct > 0 {
                // `_meta` counts player parses, one per player per kill; the
                // path ends in the group size (`sparky/hm/16`)
                let size = path.rsplit('/').next().and_then(|s| s.parse().ok()).unwrap_or(1).max(1);
                let mut source = table.source();
                source.total = source.total.div_ceil(size);
                ratings.push((id, TtkRating { pct, source }));
            }
        }
        ratings
    }

    /// Fill each row's DPS percentile from the dummy table; rows stay
    /// unrated when the table can't be had.
    pub(super) async fn rate_dummy_rows(&self, rows: &mut [baras_types::history::DummyRow]) {
        use baras_core::encounter::summary::DUMMY_RATING_PATH;
        if rows.is_empty() {
            return;
        }
        let Some(table) = self.shared.parsely_ratings.get(DUMMY_RATING_PATH).await else {
            return;
        };
        for row in rows {
            row.dps_pct = row
                .discipline
                .as_deref()
                .and_then(baras_core::game_data::Discipline::from_name)
                .map(|d| percentile(&table.dps, &disc_slug(d), row.dps as f32))
                .filter(|pct| *pct > 0);
        }
    }

    /// Percentiles of a killed dummy parse on the table at `path`; None when
    /// the table can't be had.
    pub(super) async fn dummy_percentiles(
        &self,
        path: &str,
        discipline: baras_core::game_data::Discipline,
        parse: &DummyParse,
    ) -> Option<DummyPercentiles> {
        let table = self.shared.parsely_ratings.get(path).await?;
        let slug = disc_slug(discipline);
        let rate = |t: &Thresholds, v: f32| percentile(t, &slug, v);
        Some(DummyPercentiles {
            dps: rate(&table.dps, parse.dps as f32),
            crit: rate(&table.crit_percent, parse.crit_pct),
            crit_damage: rate(&table.crit_damage_percent, parse.crit_damage_pct),
            apm: rate(&table.apm, parse.apm),
            ttk: percentile(&table.ttk, "all", parse.duration_secs as f32),
            count: rating_key(&table.dps, &slug).map_or(0, |key| table.meta.count(key)),
            source: table.source(),
        })
    }

    /// Percentiles for a finished kill of a rated boss, from the encounter's
    /// final metrics; empty when there is nothing to rate. Healing is rated
    /// for healers only.
    ///
    /// With `local_only` the network is never touched and None means the
    /// table has to be downloaded first.
    pub async fn parsely_percentiles(&self, encounter_idx: u32, local_only: bool) -> Option<EncounterRatings> {
        // Copy what the lookup needs so the session lock isn't held over a fetch
        let (path, area_paths, players) = {
            let session_guard = self.shared.session.read().await;
            let session = session_guard.as_ref()?.read().await;
            let summaries = session.session_cache.as_ref()?.encounter_history.summaries();
            let summary = summaries.iter().find(|s| s.encounter_id == encounter_idx as u64)?;
            let Some(path) = summary
                .parsely_rating
                .clone()
                .filter(|_| summary.success && !summary.partial)
            else {
                return Some(EncounterRatings::default());
            };
            // Every rated boss pulled in this area, so tabbing between them stays in memory
            let mut area_paths: Vec<String> = summaries
                .iter()
                .filter(|s| s.area_id == summary.area_id)
                .filter_map(|s| s.parsely_rating.clone())
                .collect();
            area_paths.sort_unstable();
            area_paths.dedup();
            (path, area_paths, summary.player_metrics.clone())
        };

        let cache = &self.shared.parsely_ratings;
        let table = if local_only {
            cache.keep_only(&area_paths);
            cache.local(&path)?
        } else {
            let table = cache.get(&path).await;
            let shared = Arc::clone(&self.shared);
            tokio::spawn(async move { shared.parsely_ratings.load_set(area_paths).await });
            match table {
                Some(t) => t,
                None => {
                    return Some(EncounterRatings {
                        unavailable: true,
                        ..Default::default()
                    });
                }
            }
        };

        Some(EncounterRatings {
            stale_as_of: (!table.is_current()).then(|| table.fetched.clone()),
            unavailable: false,
            source: table.source(),
            players: players
                .into_iter()
                .filter_map(|pm| {
                    let disc = pm.discipline?;
                    let slug = disc_slug(disc);
                    let rate = |t: &Thresholds, v: f32| percentile(t, &slug, v);
                    let healer = disc.role() == Role::Healer;
                    let count = rating_key(&table.dps, &slug).map_or(0, |key| table.meta.count(key));
                    Some((
                        pm.name,
                        RatingPercentiles {
                            dps: rate(&table.dps, pm.dps as f32),
                            boss_dps: rate(&table.boss_dps, pm.bossdps as f32),
                            hps: if healer { rate(&table.hps, pm.hps as f32) } else { 0 },
                            ehps: if healer { rate(&table.ehps, pm.ehps as f32) } else { 0 },
                            dtps: rate(&table.dtps, pm.dtps as f32),
                            apm: rate(&table.apm, pm.apm),
                            count,
                        },
                    ))
                })
                .collect(),
        })
    }
}
