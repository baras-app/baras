//! User-owned metadata about logs and pulls (favorites, tags, notes, Parsely
//! links). Lives in `~/.config/baras/library.json`, never in the regenerated
//! per-log history files, so a re-parse cannot clobber it.

use serde::{Deserialize, Serialize};

/// A successful Parsely upload of a whole log or one fight.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParselyUpload {
    pub link: String,
    /// ISO 8601 local time of the upload
    pub uploaded_at: String,
    /// Log file length at upload time; whole-file uploads only. Lets the UI
    /// offer "upload again" when the log grew afterwards.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_len: Option<u64>,
    #[serde(default)]
    pub visibility: u8,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LogMeta {
    #[serde(default)]
    pub favorite: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parsely: Option<ParselyUpload>,
}

impl LogMeta {
    pub fn is_empty(&self) -> bool {
        !self.favorite && self.tags.is_empty() && self.note.is_empty() && self.parsely.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PullMeta {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    /// `(combat_secs, label)`
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bookmarks: Vec<(f32, String)>,
    /// User-hidden pull (practice pull, bugged fight)
    #[serde(default)]
    pub excluded: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parsely: Option<ParselyUpload>,
}

impl PullMeta {
    pub fn is_empty(&self) -> bool {
        self.note.is_empty()
            && self.bookmarks.is_empty()
            && !self.excluded
            && self.parsely.is_none()
    }
}
