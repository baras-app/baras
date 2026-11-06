//! Cross-session pull history types served to the History tab.
//!
//! The backend keeps the full per-log summaries; only these compact rows and
//! aggregates cross IPC.

mod library;
mod meta;
mod progress;
mod runs;

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

pub use library::{
    ArchiveReport, CleanupCandidate, CleanupPlan, CleanupReport, ContentTag, DeleteReport,
    HistoryStatus, LOG_NOTE_MAX, LibraryContent, LibraryRow, LogCleanup, LogStatus,
};
pub use meta::{LogMeta, ParselyUpload, PullMeta};
pub use progress::{BossHpRecord, PhaseRef, PullDeath, PullEnd, PullProgress};
pub use runs::{RunBoss, RunDetail, RunList, RunMeta, RunPlayer, RunSummary, RunTimer, TimerStart, iso_to_ms};

/// One recorded pull, reduced to what the History table shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PullRow {
    pub filename: String,
    /// Log path, present only when the file still exists on disk
    pub path: Option<String>,
    /// No path because the log is compressed in the archive; restoring it
    /// brings the path back
    #[serde(default)]
    pub archived: bool,
    pub encounter_id: u64,
    /// Stable pull key within the log (combat start, ms); see `PullRecord`
    pub start_ms: i64,
    pub character: String,
    /// ISO 8601 start time
    pub timestamp: String,
    pub operation: String,
    /// Definition area id; 0 in files written before it was recorded
    #[serde(default)]
    pub area_id: i64,
    /// Server display name ("Star Forge"); empty when unknown
    #[serde(default)]
    pub server: String,
    /// Patch live on the pull's date ("7.9"), from the date table
    #[serde(default)]
    pub game_version: String,
    /// Tags of the operation run this pull belongs to
    #[serde(default)]
    pub tags: Vec<String>,
    pub boss: String,
    pub difficulty: String,
    pub success: bool,
    pub duration_seconds: i64,
    /// Raid content (operations); false for flashpoints, world bosses, dummies
    pub is_operation: bool,
    #[serde(default)]
    pub is_flashpoint: bool,
    // Logging character's own numbers on this pull
    pub discipline: Option<String>,
    /// "Tank" | "Healer" | "Dps"
    pub role: Option<String>,
    pub role_icon: Option<String>,
    /// Discipline icon filename, resolved by the frontend's icon registry
    pub discipline_icon: Option<String>,
    pub dps: Option<i64>,
    pub hps: Option<i64>,
    #[serde(default)]
    pub ehps: Option<i64>,
    /// Parsely link: this fight's own upload, else the whole log's
    #[serde(default)]
    pub parsely: Option<String>,
    // Pull progress
    pub end_reason: PullEnd,
    /// Lowest boss HP %, only when exactly one boss NPC was tracked (multi-boss
    /// fights have no single honest number yet)
    pub boss_hp_pct: Option<f32>,
    /// Furthest phase name reached
    pub phase: Option<String>,
    pub deaths: u16,
    /// Logging character's own deaths
    #[serde(default)]
    pub my_deaths: u16,
    pub raid_size: u8,
    pub partial: bool,
}

/// Pull/kill counts for one boss, used to build the operation → boss tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BossNode {
    pub operation: String,
    pub boss: String,
    pub pulls: u32,
    pub kills: u32,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PullHistoryOverview {
    /// Every character with recorded pulls (unfiltered)
    pub characters: Vec<String>,
    /// Every discipline the logging characters have played (unfiltered)
    pub disciplines: Vec<String>,
    /// Sorted by operation then boss; training dummy parses last
    pub bosses: Vec<BossNode>,
    /// Every run tag in use (unfiltered), for the toolbar
    #[serde(default)]
    pub tags: Vec<String>,
    /// Servers and patches seen in any pull (unfiltered)
    #[serde(default)]
    pub servers: Vec<String>,
    #[serde(default)]
    pub versions: Vec<String>,
}

/// Row filter applied server-side to both the tree and the pull list.
/// Empty strings mean "any".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PullFilter {
    pub character: String,
    /// "Tank" | "Healer" | "Dps"
    pub role: String,
    pub discipline: String,
    /// "Story" | "Veteran" | "Master", matched against the difficulty name
    pub tier: String,
    /// "8" | "16", matched against the leading player count of the difficulty name
    #[serde(default)]
    pub group_size: String,
    /// Flashpoint pulls and runs alongside operations (off: operations only)
    #[serde(default)]
    pub flashpoints: bool,
    pub kills_only: bool,
    /// Inclusive day bounds as `YYYY-MM-DD`; empty = open-ended
    pub date_from: String,
    pub date_to: String,
    /// Only pulls / runs carrying this run tag
    #[serde(default)]
    pub tag: String,
    /// Server display name and patch, as shown on rows; empty = any
    #[serde(default)]
    pub server: String,
    #[serde(default)]
    pub game_version: String,
}

impl Default for PullFilter {
    fn default() -> Self {
        Self {
            character: String::new(),
            role: String::new(),
            discipline: String::new(),
            tier: "Master".into(),
            group_size: "8".into(),
            flashpoints: false,
            kills_only: false,
            date_from: String::new(),
            date_to: String::new(),
            tag: String::new(),
            server: String::new(),
            game_version: String::new(),
        }
    }
}

/// Column the History pull table is sorted by.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum PullSortKey {
    #[default]
    Date,
    Duration,
    Dps,
    Hps,
}

/// The history views, each its own main tab (see `MainTab::history_view`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HistorySubTab {
    #[default]
    Overview,
    Operations,
    Library,
}

/// History tab selections kept across tab switches (see `UiSessionState`).
#[derive(Debug, Clone, PartialEq)]
pub struct HistoryState {
    pub filter: PullFilter,
    /// Selected (operation, boss)
    pub selected: Option<(String, String)>,
    /// Operation sections the user has opened in the sidebar tree
    pub expanded: HashSet<String>,
    /// Sidebar boss/operation name filter
    pub boss_search: String,
    pub sort_key: PullSortKey,
    pub sort_desc: bool,
    /// Operations rail selection: (filename, first pull start_ms)
    pub selected_run: Option<(String, i64)>,
    /// Operations rail: opened sections and the search box
    pub run_expanded: HashSet<String>,
    pub run_search: String,
    /// Library filters
    pub library_search: String,
    pub library_content: LibraryContent,
    pub library_favorites: bool,
    /// Library: the cleanup/scan settings panel is unfolded
    pub library_settings_open: bool,
    /// Boss Statistics and Raid Summaries: the selection sidebar is folded
    /// to its toggle
    pub sidebar_collapsed: bool,
}

impl Default for HistoryState {
    fn default() -> Self {
        Self {
            filter: PullFilter::default(),
            selected: None,
            expanded: HashSet::new(),
            boss_search: String::new(),
            sort_key: PullSortKey::Date,
            sort_desc: true,
            selected_run: None,
            run_expanded: HashSet::new(),
            run_search: String::new(),
            library_search: String::new(),
            library_content: LibraryContent::default(),
            library_favorites: false,
            library_settings_open: false,
            sidebar_collapsed: false,
        }
    }
}

/// Progress lines emitted by `baras-parse-worker --backfill` (one JSON per
/// line on stdout) and relayed to the History tab as Tauri events.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum BackfillEvent {
    Progress {
        current: u32,
        total: u32,
        file: String,
        pulls: u32,
        /// History was already current for this log; no worker was spawned
        #[serde(default)]
        skipped: bool,
    },
    Done {
        pulls: u32,
        logs: u32,
        failed: u32,
        scanned: u32,
        #[serde(default)]
        skipped: u32,
        elapsed_secs: f32,
        /// Stopped by the user before every log was scanned
        #[serde(default)]
        cancelled: bool,
    },
}

/// Progress of a cleanup run (startup or "Run cleanup now") on the
/// `log-cleanup` event: one `Progress` per file, then `Done`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum CleanupEvent {
    Progress {
        current: u32,
        total: u32,
        file: String,
    },
    Done {
        report: CleanupReport,
    },
}
