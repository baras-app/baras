//! Backfill mode: record pull history for every log in a directory and in
//! the archive.
//!
//! Each log is parsed by a child copy of this binary in `--summary-only`
//! mode (no parquet output), so memory stays flat across hundreds of files.
//! The child's JSON summary is turned into a `LogHistory` file exactly as the
//! app does after a historical open.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use baras_core::history::{HistoryHeader, LogHistory, history_dir, history_file_for};
use baras_core::library::{ArchivedLog, archived_logs};
use baras_core::state::ParseWorkerOutput;
use baras_types::history::BackfillEvent;

/// Machine-readable progress for the app; humans get stderr.
fn emit(event: &BackfillEvent) {
    if let Ok(json) = serde_json::to_string(event) {
        println!("{json}");
    }
}

/// `stale_only` skips logs whose history file was written by the current
/// store version from the log exactly as it is on disk (see `HistoryHeader`).
pub fn run(log_dir: &Path, definitions_dir: Option<&Path>, stale_only: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let tmp_out = history_dir().map_err(|e| e.to_string())?.join(".backfill-tmp");

    let mut files: Vec<Source> = std::fs::read_dir(log_dir)
        .map_err(|e| format!("Cannot read {}: {e}", log_dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|f| f.to_str())
                .is_some_and(|f| f.starts_with("combat_") && f.ends_with(".txt"))
                && p.metadata().is_ok_and(|m| m.len() > 0)
        })
        .map(Source::Raw)
        .collect();
    files.extend(archived_logs().into_iter().map(Source::Archived));
    files.sort_by(|a, b| a.name().cmp(b.name()));

    let total = files.len();
    let (mut recorded_logs, mut recorded_pulls, mut failed, mut skipped) = (0usize, 0usize, 0usize, 0usize);
    let started = std::time::Instant::now();

    for (i, source) in files.iter().enumerate() {
        let name = source.name();
        if stale_only && source.is_current() {
            skipped += 1;
            emit(&BackfillEvent::Progress {
                current: (i + 1) as u32,
                total: total as u32,
                file: name.to_string(),
                pulls: 0,
                skipped: true,
            });
            continue;
        }
        // An archive is inflated next to the worker's scratch output and
        // parsed from there; the copy carries the original mtime, so the
        // history header matches the archive afterwards
        let extracted = match source {
            Source::Raw(_) => None,
            Source::Archived(a) => match std::fs::create_dir_all(&tmp_out).and_then(|()| a.extract(&tmp_out)) {
                Ok(p) => Some(p),
                Err(e) => {
                    failed += 1;
                    eprintln!("[{}/{total}] {name}: cannot extract archive ({e})", i + 1);
                    continue;
                }
            },
        };
        let path = extracted.as_deref().unwrap_or_else(|| source.path());
        let mut cmd = Command::new(&exe);
        cmd.arg(path).arg("backfill").arg(&tmp_out);
        if let Some(defs) = definitions_dir {
            cmd.arg(defs);
        }
        cmd.arg("--summary-only")
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        let output = match cmd.output().and_then(|o| {
            if o.status.success() {
                Ok(o.stdout)
            } else {
                Err(std::io::Error::other(format!("exit {:?}", o.status.code())))
            }
        }) {
            Ok(stdout) => stdout,
            Err(e) => {
                failed += 1;
                eprintln!("[{}/{total}] {name}: worker failed ({e})", i + 1);
                continue;
            }
        };

        let parsed: ParseWorkerOutput = match serde_json::from_slice(&output) {
            Ok(p) => p,
            Err(e) => {
                failed += 1;
                eprintln!("[{}/{total}] {name}: bad worker output ({e})", i + 1);
                continue;
            }
        };

        let character = (!parsed.player.name.is_empty()).then(|| parsed.player.name.clone());
        let history = LogHistory::from_summaries(path, character, &parsed.encounters, &parsed.timer_starts);
        if let Some(tmp) = &extracted {
            let _ = std::fs::remove_file(tmp);
        }
        let Some(mut history) = history else {
            continue;
        };
        if extracted.is_some() {
            // The scratch copy is gone; the index finds the archive by filename
            history.path = None;
        }
        let pulls = history.pulls.len();
        match history.save() {
            Ok(()) => {
                if pulls > 0 {
                    recorded_logs += 1;
                    recorded_pulls += pulls;
                }
                eprintln!("[{}/{total}] {name}: {pulls} pulls ({} ms)", i + 1, parsed.elapsed_ms);
                emit(&BackfillEvent::Progress {
                    current: (i + 1) as u32,
                    total: total as u32,
                    file: name.to_string(),
                    pulls: pulls as u32,
                    skipped: false,
                });
            }
            Err(e) => {
                failed += 1;
                eprintln!("[{}/{total}] {name}: save failed ({e})", i + 1);
            }
        }
    }

    let _ = std::fs::remove_dir_all(&tmp_out);
    let elapsed_secs = started.elapsed().as_secs_f32();
    eprintln!(
        "Backfill done: {recorded_pulls} pulls across {recorded_logs} logs, {failed} failed, {skipped} skipped, {total} scanned in {elapsed_secs:.1}s"
    );
    emit(&BackfillEvent::Done {
        pulls: recorded_pulls as u32,
        logs: recorded_logs as u32,
        failed: failed as u32,
        scanned: total as u32,
        skipped: skipped as u32,
        elapsed_secs,
        cancelled: false,
    });
    Ok(())
}

/// A log to scan: raw in the directory, or compressed in the archive.
enum Source {
    Raw(PathBuf),
    Archived(ArchivedLog),
}

impl Source {
    fn name(&self) -> &str {
        match self {
            Source::Raw(p) => p.file_name().and_then(|f| f.to_str()).unwrap_or("?"),
            Source::Archived(a) => &a.filename,
        }
    }

    fn path(&self) -> &Path {
        match self {
            Source::Raw(p) => p,
            Source::Archived(a) => &a.path,
        }
    }

    /// History file exists, is at the current version, and matches the log's
    /// length and mtime.
    fn is_current(&self) -> bool {
        let Some(header) = history_file_for(self.name()).ok().and_then(|hf| HistoryHeader::read(&hf).ok()) else {
            return false;
        };
        match self {
            Source::Raw(p) => p.metadata().is_ok_and(|m| header.is_current(&m)),
            Source::Archived(a) => a.history_current(&header),
        }
    }
}
