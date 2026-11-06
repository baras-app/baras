//! `~/.config/baras/library.json`: favorites, tags, notes and Parsely links.
//!
//! The one history file a re-parse never touches. Logs key by filename, pulls
//! by `filename#start_ms` (see `PullRecord::start_ms`), so metadata follows
//! the pull through definition changes and backfills.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use baras_types::history::{ContentTag, LOG_NOTE_MAX, LogMeta, ParselyUpload, PullMeta, RunMeta};

pub const META_VERSION: u32 = 1;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LibraryMeta {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub logs: BTreeMap<String, LogMeta>,
    #[serde(default)]
    pub pulls: BTreeMap<String, PullMeta>,
    /// Operation runs, keyed by `filename#start_ms` of the run's first pull
    #[serde(default)]
    pub runs: BTreeMap<String, RunMeta>,
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
        self.runs.retain(|_, m| !m.is_empty());
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

    pub fn run(&self, filename: &str, start_ms: i64) -> Option<&RunMeta> {
        self.runs.get(&pull_key(filename, start_ms))
    }

    pub fn set_run(&mut self, filename: &str, start_ms: i64, mut meta: RunMeta) {
        meta.note = meta.note.chars().take(LOG_NOTE_MAX).collect();
        let key = pull_key(filename, start_ms);
        if meta.is_empty() {
            self.runs.remove(&key);
        } else {
            self.runs.insert(key, meta);
        }
    }

    /// Tags on any run of one log, sorted and deduplicated
    pub fn run_tags_for_log(&self, filename: &str) -> Vec<String> {
        let prefix = format!("{filename}#");
        let mut tags: Vec<String> = self
            .runs
            .range(prefix.clone()..)
            .take_while(|(k, _)| k.starts_with(&prefix))
            .flat_map(|(_, m)| m.tags.iter().cloned())
            .collect();
        tags.sort();
        tags.dedup();
        tags
    }

    /// Every tag used on any run, sorted, for the tag picker
    pub fn run_tags(&self) -> Vec<String> {
        let mut tags: Vec<String> = self.runs.values().flat_map(|m| m.tags.iter().cloned()).collect();
        tags.sort();
        tags.dedup();
        tags
    }

    /// The user's tags and one-line note on a log. Tags are deduplicated and
    /// the note capped; `save` drops the entry once nothing is left.
    pub fn set_log_meta(&mut self, filename: &str, mut tags: Vec<String>, note: &str) {
        tags.retain(|t| !t.trim().is_empty());
        tags.dedup();
        let log = self.log_mut(filename);
        log.tags = tags;
        log.note = note.trim().chars().take(LOG_NOTE_MAX).collect();
    }

    pub fn is_favorite(&self, filename: &str) -> bool {
        self.logs.get(filename).is_some_and(|m| m.favorite)
    }

    pub fn set_favorite(&mut self, filename: &str, favorite: bool) {
        self.log_mut(filename).favorite = favorite;
    }

    /// `(start_ms, meta)` for every pull of one log
    pub fn pulls_of(&self, filename: &str) -> impl Iterator<Item = (i64, &PullMeta)> {
        let prefix = format!("{filename}#");
        self.pulls
            .range(prefix.clone()..)
            .take_while(move |(k, _)| k.starts_with(&prefix))
            .filter_map(|(k, m)| k.rsplit_once('#')?.1.parse().ok().map(|ms| (ms, m)))
    }

    /// Pulls of this log that carry their own Parsely link
    pub fn pull_parsely_count(&self, filename: &str) -> u32 {
        self.pull_parsely_links(filename).count() as u32
    }

    /// `(start_ms, link)` for every pull of one log that was uploaded on its own
    pub fn pull_parsely_links(&self, filename: &str) -> impl Iterator<Item = (i64, &str)> {
        self.pulls_of(filename)
            .filter_map(|(ms, m)| m.parsely.as_ref().map(|p| (ms, p.link.as_str())))
    }

    /// Link for one pull: its own upload first, else the whole-log upload
    pub fn parsely_link_for(&self, filename: &str, start_ms: i64) -> Option<&str> {
        self.pull(filename, start_ms)
            .and_then(|m| m.parsely.as_ref())
            .or_else(|| self.log(filename).and_then(|m| m.parsely.as_ref()))
            .map(|p| p.link.as_str())
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
        self.runs.retain(|k, _| !k.starts_with(&prefix));
    }
}

pub fn now_iso() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}
