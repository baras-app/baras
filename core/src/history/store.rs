//! One JSON file per combat log under `~/.config/baras/history/`.
//!
//! Every file is a pure function of (log bytes, `HISTORY_VERSION`,
//! definitions): live tailing and backfill both rewrite it in full through
//! `LogHistory::from_summaries`, so re-running either is harmless. The header
//! records which log bytes produced it; `is_current` is the "already done"
//! check that lets a backfill skip a file without spawning a worker.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

use super::record::PullRecord;
use super::{history_dir, is_recordable};
use crate::context::parse_log_filename;
use crate::encounter::summary::EncounterSummary;

/// Bump whenever `PullRecord` gains a field that old files cannot default
/// sensibly. Files below this version are "stale" and need a re-scan.
pub const HISTORY_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogHistory {
    #[serde(default)]
    pub version: u32,
    /// Combat log filename (e.g. `combat_2026-09-29_02_20_23_294905.txt`)
    pub filename: String,
    /// Full path the log was parsed from. Cleared on load if the file is gone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    /// Local player for this log (None if never initialized)
    pub character_name: Option<String>,
    /// Log start time parsed from the filename
    pub started_at: Option<NaiveDateTime>,
    /// Length and mtime (unix seconds) of the log when this file was written
    #[serde(default)]
    pub source_len: u64,
    #[serde(default)]
    pub source_mtime: i64,
    pub pulls: Vec<PullRecord>,
}

/// Just enough of a history file to decide whether it needs a re-scan,
/// without deserializing the pulls.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct HistoryHeader {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub source_len: u64,
    #[serde(default)]
    pub source_mtime: i64,
}

impl HistoryHeader {
    pub fn read(history_file: &Path) -> std::io::Result<Self> {
        let bytes = std::fs::read(history_file)?;
        serde_json::from_slice(&bytes).map_err(std::io::Error::other)
    }

    /// True when this file was written by the current store version from the
    /// log exactly as it is on disk now.
    pub fn is_current(&self, log_meta: &std::fs::Metadata) -> bool {
        self.version == HISTORY_VERSION
            && self.source_len == log_meta.len()
            && self.source_mtime == mtime_secs(log_meta)
    }
}

pub fn mtime_secs(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// `<log stem>.json`: the history file name for a log, also the index key.
pub fn history_file_name(log_filename: &str) -> String {
    let stem = log_filename.strip_suffix(".txt").unwrap_or(log_filename);
    format!("{stem}.json")
}

/// `history/<log stem>.json`
pub fn history_file_for(log_filename: &str) -> std::io::Result<PathBuf> {
    Ok(history_dir()?.join(history_file_name(log_filename)))
}

/// Pre-versioning layout: pulls stored as full `EncounterSummary` values.
/// Loaded once to migrate; never written.
#[derive(Deserialize)]
pub struct LegacyLogHistory {
    pub filename: String,
    #[serde(default)]
    pub path: Option<PathBuf>,
    pub character_name: Option<String>,
    pub started_at: Option<NaiveDateTime>,
    pub pulls: Vec<EncounterSummary>,
}

/// A loaded history plus anything the legacy format carried that now lives
/// in the user metadata store.
pub struct Loaded {
    pub history: LogHistory,
    /// `(pull start_ms, parsely link)` from legacy per-fight uploads
    pub legacy_parsely: Vec<(i64, String)>,
}

impl LogHistory {
    /// Build from a session's encounter history, keeping only recordable pulls.
    /// Returns None if `path` has no usable filename. The source header is
    /// taken from the log on disk right now; a log that is still being written
    /// will simply read as stale until its next save.
    pub fn from_summaries(
        path: &Path,
        character_name: Option<String>,
        summaries: &[EncounterSummary],
    ) -> Option<Self> {
        let filename = path.file_name()?.to_str()?;
        let (source_len, source_mtime) = std::fs::metadata(path)
            .map(|m| (m.len(), mtime_secs(&m)))
            .unwrap_or((0, 0));
        Some(Self {
            version: HISTORY_VERSION,
            filename: filename.to_string(),
            path: Some(path.to_path_buf()),
            character_name,
            started_at: parse_log_filename(filename).map(|(_, dt)| dt),
            source_len,
            source_mtime,
            pulls: summaries
                .iter()
                .filter(|s| is_recordable(s))
                .map(PullRecord::from)
                .collect(),
        })
    }

    /// Read a history file in either layout. Legacy files are converted in
    /// memory and reported as version 0 so the index marks them stale.
    pub fn load(history_file: &Path) -> std::io::Result<Loaded> {
        let bytes = std::fs::read(history_file)?;
        let header: HistoryHeader =
            serde_json::from_slice(&bytes).map_err(std::io::Error::other)?;
        if header.version >= 1 {
            let history = serde_json::from_slice(&bytes).map_err(std::io::Error::other)?;
            return Ok(Loaded {
                history,
                legacy_parsely: Vec::new(),
            });
        }
        let legacy: LegacyLogHistory =
            serde_json::from_slice(&bytes).map_err(std::io::Error::other)?;
        let pulls: Vec<PullRecord> = legacy.pulls.iter().map(PullRecord::from).collect();
        let legacy_parsely = legacy
            .pulls
            .iter()
            .zip(&pulls)
            .filter_map(|(s, r)| s.parsely_link.clone().map(|l| (r.start_ms, l)))
            .collect();
        Ok(Loaded {
            history: LogHistory {
                version: 0,
                filename: legacy.filename,
                path: legacy.path,
                character_name: legacy.character_name,
                started_at: legacy.started_at,
                source_len: 0,
                source_mtime: 0,
                pulls,
            },
            legacy_parsely,
        })
    }

    /// Where the log should be: the stored path, or `log_dir/filename` for
    /// files recorded before paths were stored.
    pub fn log_path(&self, log_dir: &Path) -> PathBuf {
        self.path
            .clone()
            .unwrap_or_else(|| log_dir.join(&self.filename))
    }

    pub fn file(&self) -> std::io::Result<PathBuf> {
        history_file_for(&self.filename)
    }

    /// Write atomically (tmp + rename). A log with no recordable pulls
    /// removes any stale file left from an earlier parse.
    pub fn save(&self) -> std::io::Result<()> {
        let path = self.file()?;
        if self.pulls.is_empty() {
            return match std::fs::remove_file(&path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                other => other,
            };
        }
        let tmp = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec(self).map_err(std::io::Error::other)?;
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(&tmp, &path)
    }

    pub fn modified(history_file: &Path) -> std::io::Result<SystemTime> {
        std::fs::metadata(history_file)?.modified()
    }
}
