//! Log library management: criteria-based deletion or archival with a
//! dry-run preview, and the history store health line.

use serde::{Deserialize, Serialize};

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
    pub const ALL: [ContentTag; 9] = [
        ContentTag::MasterOps,
        ContentTag::VeteranOps,
        ContentTag::StoryOps,
        ContentTag::MasterFlashpoint,
        ContentTag::VeteranFlashpoint,
        ContentTag::Pvp,
        ContentTag::Dummy,
        ContentTag::Other,
        ContentTag::Empty,
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

/// Which logs a bulk delete or archive targets. A log is a candidate when
/// none of its content is in `keep` and it passes every enabled narrowing
/// filter. Favorites and the live log are always skipped.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct DeleteCriteria {
    /// Logs holding any of these are never candidates. Starts from the saved
    /// retention list; the user unticks what they want to remove.
    #[serde(default)]
    pub keep: Vec<ContentTag>,
    /// Only files smaller than this many bytes
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub smaller_than: Option<u64>,
    /// Only files whose log date is older than this many days
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub older_than_days: Option<u32>,
    /// Delete only: keep the per-log history JSON so pulls stay in the dashboard
    pub keep_history: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeleteCandidate {
    pub path: String,
    pub filename: String,
    /// `YYYY-MM-DD`
    pub date: String,
    pub size: u64,
    pub content: Vec<ContentTag>,
}

/// Dry-run result shown before the user confirms.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct DeletePreview {
    pub files: Vec<DeleteCandidate>,
    pub total_bytes: u64,
    /// Logs left alone because they hold kept content (or are not yet classified)
    pub kept: u32,
    pub skipped_favorites: u32,
    pub skipped_live: u32,
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
    pub tags: Vec<String>,
    pub has_note: bool,
    /// Whole-file Parsely link
    pub parsely: Option<String>,
    /// Pulls with their own Parsely link
    pub parsely_fights: u32,
}
