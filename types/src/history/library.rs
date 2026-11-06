//! Log library management: the automatic cleanup rules with their dry-run
//! plan, and the history store health line.

use serde::{Deserialize, Serialize};

/// Longest log or run note kept: a one-line summary, not a document.
pub const LOG_NOTE_MAX: usize = 140;

/// What kind of content a log holds, derived from the areas it visited and
/// the pulls recorded in it. A log usually carries several.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentTag {
    MasterFlashpoint,
    VeteranFlashpoint,
    StoryOps,
    VeteranOps,
    MasterOps,
    Pvp,
    Dummy,
    /// Open world, world bosses, unknown difficulty
    Other,
    /// No areas visited and nothing recorded
    Empty,
}

impl ContentTag {
    /// The tags offered as filters and retention choices; `Other` and
    /// `Empty` exist only so classification always yields something
    pub const ALL: [ContentTag; 7] = [
        ContentTag::MasterOps,
        ContentTag::VeteranOps,
        ContentTag::StoryOps,
        ContentTag::MasterFlashpoint,
        ContentTag::VeteranFlashpoint,
        ContentTag::Pvp,
        ContentTag::Dummy,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ContentTag::MasterFlashpoint => "MM FP",
            ContentTag::VeteranFlashpoint => "VM FP",
            ContentTag::StoryOps => "SM Ops",
            ContentTag::VeteranOps => "VM Ops",
            ContentTag::MasterOps => "NiM Ops",
            ContentTag::Pvp => "PvP",
            ContentTag::Dummy => "Dummy",
            ContentTag::Other => "Other",
            ContentTag::Empty => "Empty",
        }
    }
}

/// Automatic log housekeeping, run once the directory index is loaded at
/// startup and on demand from Log Management. Favorites and the live log are
/// never touched; logs holding retained content (`LibraryMeta::retain`) are
/// archived but never deleted. The delete-after rule also covers archives.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogCleanup {
    /// Delete logs smaller than this many bytes (not from today); 0 = off
    #[serde(default = "default_small_log_bytes")]
    pub delete_smaller_than: u64,
    /// Archive logs whose date is older than this many days; None = off
    #[serde(default)]
    pub archive_after_days: Option<u32>,
    /// Delete logs whose date is older than this many days; None = off
    #[serde(default)]
    pub delete_after_days: Option<u32>,
}

fn default_small_log_bytes() -> u64 {
    1_048_576
}

impl Default for LogCleanup {
    fn default() -> Self {
        Self {
            delete_smaller_than: default_small_log_bytes(),
            archive_after_days: None,
            delete_after_days: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CleanupCandidate {
    pub path: String,
    pub filename: String,
    /// `YYYY-MM-DD`
    pub date: String,
    pub size: u64,
    pub content: Vec<ContentTag>,
}

/// Dry run of the cleanup rules: what would be archived and deleted right now.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CleanupPlan {
    pub archive: Vec<CleanupCandidate>,
    pub delete: Vec<CleanupCandidate>,
    pub archive_bytes: u64,
    pub delete_bytes: u64,
    /// Logs a delete rule matched but that hold retained content with no
    /// archive rule to fall back on, or that are not yet classified
    pub kept: u32,
    pub skipped_favorites: u32,
    pub skipped_live: u32,
}

impl CleanupPlan {
    pub fn is_empty(&self) -> bool {
        self.archive.is_empty() && self.delete.is_empty()
    }
}

/// What a cleanup run actually did.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CleanupReport {
    pub archived: u32,
    pub deleted: u32,
    pub bytes_freed: u64,
    /// Compressed size of what was archived
    pub bytes_archived: u64,
    pub failed: Vec<String>,
}

/// What a confirmed delete actually did.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct DeleteReport {
    pub deleted: u32,
    pub failed: Vec<String>,
    pub bytes_freed: u64,
}

/// What a confirmed archive actually did.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ArchiveReport {
    pub archived: u32,
    pub failed: Vec<String>,
    pub bytes_before: u64,
    pub bytes_after: u64,
}

/// History store health for the Library header and the re-scan button.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct HistoryStatus {
    pub logs: u32,
    pub pulls: u32,
    /// History files older than the current store version or whose source
    /// log changed since they were written
    pub stale: u32,
    /// Logs in the directory with no history file yet
    #[serde(default)]
    pub unscanned: u32,
}

/// Where a log stands in the history store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogStatus {
    /// Being tailed right now
    Live,
    /// History file current
    Indexed,
    /// History file from an older store version or older log bytes
    Stale,
    /// Never scanned (or scanned and found nothing recordable)
    NoHistory,
    /// Compressed into the archive directory; restore to open
    Archived,
    /// Archived, and its history is from an older store version
    ArchivedStale,
}

impl LogStatus {
    pub fn is_archived(self) -> bool {
        matches!(self, LogStatus::Archived | LogStatus::ArchivedStale)
    }
}

/// One log in the Library table: directory index + area cache + history
/// index + user metadata, joined in the service. Archived logs point at the
/// `.zst` file and report its compressed size.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LibraryRow {
    pub path: String,
    pub filename: String,
    /// `YYYY-MM-DD HH:MM` local
    pub date: String,
    pub character: Option<String>,
    /// Server the log was recorded on (from its pulls); empty when unknown
    #[serde(default)]
    pub server: String,
    pub size: u64,
    /// Archived rows only: the log's size before compression, when known
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_size: Option<u64>,
    /// Area display names in visit order (e.g. "Dxun NiM 8")
    pub areas: Vec<String>,
    #[serde(default)]
    pub content: Vec<ContentTag>,
    pub pulls: u32,
    pub kills: u32,
    /// Every recorded pull is a training dummy parse
    pub dummy_only: bool,
    pub status: LogStatus,
    pub favorite: bool,
    /// The user's own tags on this log
    pub tags: Vec<String>,
    /// Tags on runs recorded in this log (read-only here)
    #[serde(default)]
    pub run_tags: Vec<String>,
    /// One-line note, at most `LOG_NOTE_MAX` characters
    #[serde(default)]
    pub note: String,
    /// Whole-file Parsely link
    pub parsely: Option<String>,
    /// Pulls with their own Parsely link
    pub parsely_fights: u32,
}

/// Library content filter: any log, logs with kills, or logs carrying one tag
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum LibraryContent {
    #[default]
    Any,
    Kills,
    Tag(ContentTag),
}

impl LibraryContent {
    pub fn label(self) -> &'static str {
        match self {
            LibraryContent::Any => "Any content",
            LibraryContent::Kills => "Kills only",
            LibraryContent::Tag(t) => t.label(),
        }
    }
}
