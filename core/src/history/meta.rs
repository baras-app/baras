//! `~/.config/baras/library.json`: favorites, tags, notes and Parsely links.
//!
//! The one history file a re-parse never touches. Logs key by filename, pulls
//! by `filename#start_ms` (see `PullRecord::start_ms`), so metadata follows
//! the pull through definition changes and backfills.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use baras_types::history::{ContentTag, LogMeta, ParselyUpload, PullMeta};

pub const META_VERSION: u32 = 1;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LibraryMeta {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub logs: BTreeMap<String, LogMeta>,
    #[serde(default)]
    pub pulls: BTreeMap<String, PullMeta>,
    /// Content the user wants kept: a log holding any of these is never
    /// auto-deleted or bulk-deleted (favorites are the per-log equivalent)
    #[serde(default)]
    pub retain: Vec<ContentTag>,
}

pub fn pull_key(filename: &str, start_ms: i64) -> String {
    format!("{filename}#{start_ms}")
}

/// `~/.config/baras/library.json`
pub fn meta_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("baras")
        .join("library.json")
}

impl LibraryMeta {
    /// Missing or unreadable file → empty store (and a warning, never a panic).
    pub fn load() -> Self {
        let path = meta_path();
        match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|e| {
                tracing::warn!(path = %path.display(), error = %e, "Unreadable library.json; starting empty");
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    /// Atomic write (tmp + rename). Empty entries are dropped first so the
    /// file only ever holds real user data.
    pub fn save(&mut self) -> std::io::Result<()> {
        self.version = META_VERSION;
        self.logs.retain(|_, m| !m.is_empty());
        self.pulls.retain(|_, m| !m.is_empty());
        let path = meta_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(
            &tmp,
            serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?,
        )?;
        std::fs::rename(&tmp, &path)
    }

    pub fn log(&self, filename: &str) -> Option<&LogMeta> {
        self.logs.get(filename)
    }

    pub fn log_mut(&mut self, filename: &str) -> &mut LogMeta {
        self.logs.entry(filename.to_string()).or_default()
    }

    pub fn pull(&self, filename: &str, start_ms: i64) -> Option<&PullMeta> {
        self.pulls.get(&pull_key(filename, start_ms))
    }

    pub fn pull_mut(&mut self, filename: &str, start_ms: i64) -> &mut PullMeta {
        self.pulls.entry(pull_key(filename, start_ms)).or_default()
    }

    pub fn is_favorite(&self, filename: &str) -> bool {
        self.logs.get(filename).is_some_and(|m| m.favorite)
    }

    pub fn set_favorite(&mut self, filename: &str, favorite: bool) {
        self.log_mut(filename).favorite = favorite;
    }

    /// Pulls of this log that carry their own Parsely link
    pub fn pull_parsely_count(&self, filename: &str) -> u32 {
        let prefix = format!("{filename}#");
        self.pulls
            .range(prefix.clone()..)
            .take_while(|(k, _)| k.starts_with(&prefix))
            .filter(|(_, m)| m.parsely.is_some())
            .count() as u32
    }

    pub fn set_log_parsely(&mut self, filename: &str, upload: ParselyUpload) {
        self.log_mut(filename).parsely = Some(upload);
    }

    pub fn set_pull_parsely(&mut self, filename: &str, start_ms: i64, upload: ParselyUpload) {
        self.pull_mut(filename, start_ms).parsely = Some(upload);
    }

    /// Adopt per-fight links found in pre-versioning history files. Existing
    /// entries win. Returns true if anything was added.
    pub fn merge_legacy_parsely<'a>(
        &mut self,
        links: impl IntoIterator<Item = (&'a str, i64, &'a str)>,
    ) -> bool {
        let mut changed = false;
        for (filename, start_ms, link) in links {
            let meta = self.pull_mut(filename, start_ms);
            if meta.parsely.is_none() {
                meta.parsely = Some(ParselyUpload {
                    link: link.to_string(),
                    uploaded_at: String::new(),
                    source_len: None,
                    visibility: 0,
                });
                changed = true;
            }
        }
        changed
    }

    /// Forget everything about a deleted log (favorites excepted: a favorite is
    /// never deleted, so this is only reached for non-favorites).
    pub fn remove_log(&mut self, filename: &str) {
        self.logs.remove(filename);
        let prefix = format!("{filename}#");
        self.pulls.retain(|k, _| !k.starts_with(&prefix));
    }
}

pub fn now_iso() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}
