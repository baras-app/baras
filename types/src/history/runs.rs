//! Operation runs: the pulls of one log in one area at one difficulty, split
//! after the final boss dies. Derived from recorded pulls at query time;
//! only the user's tags and note are stored (see `RunMeta`).

use serde::{Deserialize, Serialize};

use super::PullRow;

/// Milliseconds for a `%Y-%m-%dT%H:%M:%S` local timestamp taken at face value
/// (the convention `PullRow::start_ms` uses). No calendar crate: days since
/// the epoch via the civil-from-days inverse.
pub fn iso_to_ms(iso: &str) -> Option<i64> {
    let (date, time) = iso.split_once('T')?;
    let mut d = date.split('-').map(|s| s.parse::<i64>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    let mut t = time.split(':').map(|s| s.parse::<i64>().ok());
    let (h, mi, s) = (t.next()??, t.next()??, t.next()??);
    // Howard Hinnant's days_from_civil
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(((days * 86_400) + h * 3600 + mi * 60 + s) * 1000)
}

/// A player crossed the area's `[area.timer_start]` region. One per area
/// visit, recorded with the log's pulls so timed runs survive a re-parse.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimerStart {
    pub area_id: i64,
    pub difficulty_id: i64,
    /// ISO 8601 local time
    pub time: String,
}

/// User-owned per-run metadata, keyed like a pull by `filename#start_ms` of
/// the run's first pull.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RunMeta {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

impl RunMeta {
    pub fn is_empty(&self) -> bool {
        self.tags.is_empty() && self.note.is_empty()
    }
}

/// Timed-run clock for one run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunTimer {
    /// ISO 8601 start of the clock
    pub started: String,
    /// No start region was crossed (or recorded); the clock runs from the first pull
    pub from_first_pull: bool,
    /// Clock at the final-boss kill, or at the last pull's end while the run
    /// is not cleared
    pub elapsed_secs: i64,
    /// Achievement limit from the area definition; None when the area has no
    /// timed run
    pub limit_secs: Option<u32>,
    /// Full clear within the limit; None when there is no limit
    pub achieved: Option<bool>,
}

/// One run as listed in the rail.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunSummary {
    pub filename: String,
    /// Log path, present only when the file still exists on disk
    pub path: Option<String>,
    /// `start_ms` of the first pull: the run's stable key
    pub start_ms: i64,
    pub character: String,
    /// Logging character's discipline on this run, with icon names
    pub discipline: Option<String>,
    pub discipline_icon: Option<String>,
    pub role_icon: Option<String>,
    /// Server display name and patch live at the time
    #[serde(default)]
    pub server: String,
    #[serde(default)]
    pub game_version: String,
    /// Area display name (definition name when known, else the log's area name)
    pub operation: String,
    pub area_id: i64,
    #[serde(default)]
    pub is_flashpoint: bool,
    /// Difficulty as logged, e.g. "8 Player Master"
    pub difficulty: String,
    /// "Story" | "Veteran" | "Master" | ""
    pub tier: String,
    pub group_size: u8,
    /// ISO 8601 first pull start / last pull end
    pub started: String,
    pub ended: String,
    pub pulls: u32,
    pub kills: u32,
    /// Bosses in the definition for this difficulty; 0 when the area is unknown
    pub bosses_total: u32,
    /// Final boss killed
    pub cleared: bool,
    pub deaths: u32,
    /// Logging character's deaths
    pub my_deaths: u32,
    /// Seconds in boss combat / from first pull start to last pull end
    pub pull_secs: i64,
    pub span_secs: i64,
    pub timer: RunTimer,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RunList {
    pub runs: Vec<RunSummary>,
    /// Every tag used on any run, for the tag picker
    pub tags: Vec<String>,
}

/// One boss of a run, in definition order (unknown bosses after).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunBoss {
    pub name: String,
    pub pulls: u32,
    pub killed: bool,
    /// ISO 8601 kill time and seconds since the run started
    pub kill_time: Option<String>,
    pub kill_secs: Option<i64>,
    /// Best wipe: lowest boss HP % and furthest phase
    pub best_hp_pct: Option<f32>,
    pub phase: Option<String>,
    pub deaths: u32,
    /// Not pulled in this run
    pub skipped: bool,
}

/// One player across the run's pulls.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunPlayer {
    pub name: String,
    pub discipline: Option<String>,
    pub discipline_icon: Option<String>,
    pub role_icon: Option<String>,
    pub pulls: u32,
    pub deaths: u32,
    /// Deaths that were the first of their pull
    pub first_deaths: u32,
    pub avg_dps: i64,
    pub avg_hps: i64,
    pub avg_ehps: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunDetail {
    pub run: RunSummary,
    pub bosses: Vec<RunBoss>,
    pub players: Vec<RunPlayer>,
    /// The run's pulls, chronological
    pub pulls: Vec<PullRow>,
}
