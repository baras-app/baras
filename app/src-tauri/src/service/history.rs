//! Persists the active session's recordable pulls to the cross-session
//! history store (`baras_core::history`). Called after a historical parse
//! completes, after every live `CombatEnded`, and when a Parsely link lands.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::RwLock;
use tracing::{info, warn};

use baras_core::context::{ParsingSession, resolve};
use baras_core::history::{LogHistory, start_ms_of};
use baras_types::history::BackfillEvent;

use crate::state::SharedState;

pub const BACKFILL_EVENT: &str = "history-backfill";

/// Snapshot the session's recordable pulls. None when no file or cache is active.
pub fn build_pull_history(session: &ParsingSession) -> Option<LogHistory> {
    let cache = session.session_cache.as_ref()?;
    let path = session.active_file.as_ref()?;
    let character = cache
        .player_initialized
        .then(|| resolve(cache.player.name).to_string())
        .filter(|n| !n.is_empty());
    LogHistory::from_summaries(path, character, cache.encounter_history.summaries())
}

/// Write off the async runtime; failures are logged, never fatal.
pub fn save_in_background(history: LogHistory) {
    tokio::task::spawn_blocking(move || {
        if let Err(e) = history.save() {
            warn!(file = %history.filename, error = %e, "Failed to persist pull history");
        }
    });
}

pub async fn persist_pull_history(session: &Arc<RwLock<ParsingSession>>) {
    let history = build_pull_history(&*session.read().await);
    if let Some(history) = history {
        save_in_background(history);
    }
}

/// Put the per-fight Parsely links from `library.json` back onto the freshly
/// parsed summaries, so the Data Explorer offers the link rather than another
/// upload. Links key by the pull's start time, which survives a re-parse.
pub async fn restore_parsely_links(session: &Arc<RwLock<ParsingSession>>, shared: &SharedState) {
    let mut session = session.write().await;
    let Some(filename) = session.active_file.as_ref().and_then(|p| p.file_name()).and_then(|f| f.to_str()) else {
        return;
    };
    let links: HashMap<i64, String> = {
        let meta = shared.library_meta.lock().unwrap_or_else(|p| p.into_inner());
        meta.pull_parsely_links(filename).map(|(ms, l)| (ms, l.to_string())).collect()
    };
    if links.is_empty() {
        return;
    }
    if let Some(cache) = session.session_cache.as_mut() {
        let restored = cache.encounter_history.restore_parsely_links(|s| {
            s.start_time.as_deref().and_then(start_ms_of).and_then(|ms| links.get(&ms).cloned())
        });
        info!(restored, known = links.len(), "Restored Parsely links onto parsed encounters");
    }
}

/// Sidecar location: Tauri bundle name with target triple, plain name next to
/// the exe, else PATH.
pub fn worker_binary_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| {
            let dir = exe.parent()?;
            let candidates = [
                dir.join(format!(
                    "baras-parse-worker-{}-unknown-linux-gnu",
                    std::env::consts::ARCH
                )),
                dir.join("baras-parse-worker"),
            ];
            candidates.into_iter().find(|p| p.exists())
        })
        .unwrap_or_else(|| PathBuf::from("baras-parse-worker"))
}

/// Run `baras-parse-worker --backfill` over the configured log directory,
/// relaying its JSON progress lines to the frontend as `history-backfill`
/// events. Only one backfill runs at a time.
pub async fn run_backfill(shared: Arc<SharedState>, app: AppHandle, stale_only: bool) -> Result<(), String> {
    if shared
        .history_backfill_running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("A backfill is already running".into());
    }

    let log_dir = PathBuf::from(shared.config.read().await.log_directory.clone());
    let definitions_dir = app
        .path()
        .resolve("definitions/encounters", tauri::path::BaseDirectory::Resource)
        .ok();

    let mut cmd = tokio::process::Command::new(worker_binary_path());
    cmd.arg("--backfill").arg(&log_dir);
    if let Some(defs) = definitions_dir {
        cmd.arg(defs);
    }
    if stale_only {
        cmd.arg("--stale-only");
    }
    if let Some(log_path) = dirs::config_dir().map(|p| p.join("baras").join("baras.log")) {
        cmd.env("BARAS_LOG_PATH", log_path);
    }
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            shared.history_backfill_running.store(false, Ordering::SeqCst);
            return Err(format!("Failed to start parse worker: {e}"));
        }
    };
    let stdout = child.stdout.take().ok_or("Worker stdout unavailable")?;
    shared.history_backfill_cancel.store(false, Ordering::SeqCst);
    info!(log_dir = %log_dir.display(), "History backfill started");

    tokio::spawn(async move {
        let started = std::time::Instant::now();
        let mut lines = BufReader::new(stdout).lines();
        let mut cancelled = false;
        loop {
            // `next_line` is cancel-safe, so polling the flag between lines loses nothing
            tokio::select! {
                line = lines.next_line() => match line {
                    Ok(Some(line)) => match serde_json::from_str::<BackfillEvent>(&line) {
                        Ok(event) => {
                            let _ = app.emit(BACKFILL_EVENT, &event);
                        }
                        Err(e) => warn!(error = %e, "Unparseable backfill line"),
                    },
                    _ => break,
                },
                _ = tokio::time::sleep(std::time::Duration::from_millis(250)) => {
                    if shared.history_backfill_cancel.load(Ordering::SeqCst) {
                        cancelled = true;
                        let _ = child.start_kill();
                        break;
                    }
                }
            }
        }
        match child.wait().await {
            Ok(_) if cancelled => info!("History backfill cancelled"),
            Ok(status) if status.success() => info!("History backfill finished"),
            Ok(status) => warn!(?status, "History backfill exited abnormally"),
            Err(e) => warn!(error = %e, "History backfill wait failed"),
        }
        if cancelled {
            // The worker never got to report, so close the loop for the UI
            let _ = app.emit(
                BACKFILL_EVENT,
                &BackfillEvent::Done { pulls: 0, logs: 0, failed: 0, scanned: 0, skipped: 0, elapsed_secs: started.elapsed().as_secs_f32(), cancelled: true },
            );
        }
        shared.history_backfill_running.store(false, Ordering::SeqCst);
    });
    Ok(())
}
