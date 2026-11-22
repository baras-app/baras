//! Log directory watching and tailing lifecycle

use super::effect_data::clear_uptime_tracking;
use super::{CombatService, OverlayUpdate, cleanup, directory, history, process_monitor};
use baras_core::context::DirectoryIndex;
use baras_core::directory_watcher;
use baras_core::directory_watcher::DirectoryWatcher;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use tauri::Emitter;
use tracing::{debug, error, info, warn};

impl CombatService {
    pub(super) async fn on_directory_changed(&mut self) {
        // Stop existing watcher
        if let Some(handle) = self.directory_handle.take() {
            self.shared.watching.store(false, Ordering::SeqCst);
            handle.abort();
            let _ = handle.await;
        }

        // Stop any active tailing
        self.stop_tailing().await;

        // Resume live tailing mode (restart means we want to watch for new files)
        self.shared.is_live_tailing.store(true, Ordering::SeqCst);
        let _ = self
            .overlay_tx
            .try_send(OverlayUpdate::NotLiveStateChanged { is_live: true });

        // Start new watcher (reads directory from config)
        self.start_watcher().await;
    }
    pub(super) async fn file_detected(&mut self, path: PathBuf) {
        // Always update the index
        {
            let mut index = self.shared.directory_index.write().await;
            index.add_file(&path);
        }

        // Trigger area indexer to scan any files that aren't indexed yet
        // (the previous "newest" file is now complete and should be indexed)
        Self::spawn_area_indexer(self.shared.clone(), self.area_index.clone(), self.app_handle.clone());

        // Notify frontend that file list changed
        let _ = self.app_handle.emit("log-files-changed", ());

        // Only auto-switch if in live tailing mode
        if !self.shared.is_live_tailing.load(Ordering::SeqCst) {
            return;
        }

        let (is_newest, is_empty) = {
            let index = self.shared.directory_index.read().await;
            match index.newest_file() {
                Some(f) if f.path == path => (true, f.is_empty),
                _ => (false, false),
            }
        };

        if !is_newest {
            return;
        }

        // Check if current session has player initialized
        let has_player = {
            let session_guard = self.shared.session.read().await;
            if let Some(session) = session_guard.as_ref() {
                let s = session.read().await;
                s.session_cache.as_ref().map(|c| c.player_initialized).unwrap_or(false)
            } else {
                false
            }
        };

        // If new file is empty and we have existing player data, defer the switch
        // This preserves the current session for viewing/uploading until new data arrives
        if is_empty && has_player {
            info!(
                new_file = %path.display(),
                "Empty log file detected, deferring switch until content arrives"
            );
            self.pending_file = Some(path);
            // Start polling for content — the OS may not deliver a separate
            // Modify event after the Create (Windows coalesces them).
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
            interval.tick().await; // consume the immediate first tick
            self.pending_file_interval = Some(interval);
            // The old character logged out — drop its "show when absent" placeholders
            clear_uptime_tracking(&self.shared).await;
            // Emit event so frontend can show "session ended" indicator
            let _ = self.app_handle.emit("session-ended", ());
            let _ = self
                .overlay_tx
                .try_send(OverlayUpdate::NotLiveStateChanged { is_live: false });
            return;
        }

        info!(
            new_file = %path.display(),
            "Log file rotation detected, switching to new file"
        );
        self.pending_file = None;
        self.pending_file_interval = None;
        self.start_tailing(path).await;
    }

    /// Handle file modification - re-check character data for files that were missing it
    pub(super) async fn file_modified(&mut self, path: PathBuf) {
        let updated = {
            let mut index = self.shared.directory_index.write().await;
            // Check if this specific file needs character re-extraction
            if index.is_missing_character(&path) {
                // Re-run character extraction since file may now have content
                index.refresh_missing_characters()
            } else {
                0
            }
        };

        if updated > 0 {
            debug!(updated, "Re-read character names from modified files");
            // Notify frontend that file list changed (display names may have updated)
            let _ = self.app_handle.emit("log-files-changed", ());
        }

        // If this is the pending file, delegate to the shared resolution logic
        if self.pending_file.as_ref() == Some(&path) {
            self.check_pending_file().await;
        }
    }

    /// Check if the pending file now has content and switch to it if so.
    /// Called both from `file_modified()` (on OS events) and from the poll
    /// timer (as a fallback when the OS coalesces Create+Modify events).
    pub(super) async fn check_pending_file(&mut self) {
        let path = match self.pending_file.clone() {
            Some(p) => p,
            None => {
                // No pending file — disable the poll timer if it's running
                self.pending_file_interval = None;
                return;
            }
        };

        // Re-stat the file and try to extract character data
        {
            let mut index = self.shared.directory_index.write().await;
            if index.is_missing_character(&path) {
                index.refresh_missing_characters();
            }
        }

        let has_content = {
            let index = self.shared.directory_index.read().await;
            index
                .newest_file()
                .map(|f| f.path == path && !f.is_empty)
                .unwrap_or(false)
        };

        if has_content {
            info!(
                file = %path.display(),
                "Pending file now has content, switching"
            );
            self.pending_file = None;
            self.pending_file_interval = None;
            let _ = self
                .overlay_tx
                .try_send(OverlayUpdate::NotLiveStateChanged { is_live: true });
            self.start_tailing(path).await;
        }
    }

    pub(super) async fn file_removed(&mut self, path: PathBuf) {
        let was_active = {
            let session_guard = self.shared.session.read().await;
            if let Some(session) = session_guard.as_ref() {
                let s = session.read().await;
                s.active_file.as_ref().map(|p| p == &path).unwrap_or(false)
            } else {
                false
            }
        };
        // Update index
        {
            let mut index = self.shared.directory_index.write().await;
            index.remove_file(&path);
        }

        // Notify frontend that file list changed
        let _ = self.app_handle.emit("log-files-changed", ());
        // Check if we need to switch files

        if was_active {
            self.stop_tailing().await;
            // Optionally switch to next newest
            let next = {
                let index = self.shared.directory_index.read().await;
                index.newest_file().map(|f| f.path.clone())
            };
            if let Some(next_path) = next {
                self.start_tailing(next_path).await;
            }
        }
    }

    pub(super) async fn start_watcher(&mut self) {
        // Only read from what is stored in config
        let dir = {
            let config = self.shared.config.read().await;
            PathBuf::from(&config.log_directory)
        };

        // Guard against invalid input
        if !dir.exists() {
            warn!(directory = %dir.display(), "Log directory does not exist");
            return;
        }
        if !dir.is_dir() {
            warn!(directory = %dir.display(), "Log directory path is not a directory");
            return;
        }

        // Phase 1: Quick-index only the newest file so the user can start working immediately
        match directory_watcher::build_index_newest(&dir) {
            Ok((quick_index, newest)) => {
                {
                    let mut index_guard = self.shared.directory_index.write().await;
                    *index_guard = quick_index;
                }
                let _ = self.app_handle.emit("log-files-changed", ());

                // Auto-load newest file if available
                if let Some(ref newest_path) = newest {
                    self.start_tailing(newest_path.clone()).await;
                }
            }
            Err(e) => {
                error!(directory = %dir.display(), error = %e, "Failed to quick-index newest file");
                let _ = self.app_handle.emit("directory-error", e);
            }
        }

        // Phase 2: Backfill the full index in the background using the disk cache.
        // This avoids re-opening thousands of files on every startup.
        let backfill_dir = dir.clone();
        let backfill_shared = self.shared.clone();
        let backfill_app_handle = self.app_handle.clone();
        tokio::task::spawn_blocking(move || {
            let cache_path =
                DirectoryIndex::default_cache_path().unwrap_or_else(|| PathBuf::from("directory_cache.json"));
            match directory_watcher::build_index_cached(&backfill_dir, &cache_path) {
                Ok((full_index, _)) => {
                    let shared = backfill_shared;
                    let app_handle = backfill_app_handle;
                    // Use a blocking approach to write into the async RwLock since
                    // we're on a spawn_blocking thread
                    let rt = tokio::runtime::Handle::current();
                    rt.block_on(async {
                        let mut index_guard = shared.directory_index.write().await;
                        index_guard.merge_from(full_index);
                    });
                    let _ = app_handle.emit("log-files-changed", ());
                    info!("Background index backfill complete");
                    // The index is complete and the live log known: apply the cleanup rules
                    baras_core::library::sweep_partial(&backfill_dir);
                    rt.block_on(cleanup::run(&shared, &app_handle));
                }
                Err(e) => {
                    error!(error = %e, "Background index backfill failed");
                }
            }
        });

        let mut watcher = match DirectoryWatcher::new(&dir) {
            Ok(w) => w,
            Err(e) => {
                error!(directory = %dir.display(), error = %e, "Failed to create directory watcher");
                self.shared.watching.store(false, Ordering::SeqCst);
                return;
            }
        };

        // Clone the command sender so watcher can send back to service
        let cmd_tx = self.cmd_tx.clone();
        let shared = self.shared.clone();

        let handle = tokio::spawn(async move {
            while let Some(event) = watcher.next_event().await {
                if let Some(cmd) = directory::translate_event(event)
                    && cmd_tx.send(cmd).await.is_err()
                {
                    break; // Service shut down
                }
            }
            // Watcher stopped
            shared.watching.store(false, Ordering::SeqCst);
        });

        self.directory_handle = Some(handle);
        self.shared.watching.store(true, Ordering::SeqCst);
        let _ = self.app_handle.emit("session-updated", "WatcherStarted");
    }
}

impl CombatService {
    pub(super) async fn stop_tailing(&mut self) {
        if self.tail_handle.is_some() {
            debug!("Stopping active tail task");
        }

        // Reset combat state
        self.shared.in_combat.store(false, Ordering::SeqCst);

        // Stop process monitor
        self.stop_process_monitor();

        // Cancel effects task
        if let Some(handle) = self.effects_handle.take() {
            handle.abort();
            let _ = handle.await;
        }

        // Cancel metrics task
        if let Some(handle) = self.metrics_handle.take() {
            handle.abort();
            let _ = handle.await;
        }

        // Cancel tail task
        if let Some(handle) = self.tail_handle.take() {
            handle.abort();
            let _ = handle.await;
        }

        let ended = self.shared.session.write().await.take();
        if let Some(session) = ended
            && self.shared.is_live_tailing.load(Ordering::SeqCst)
        {
            history::persist_final(&session).await;
        }
    }

    /// Start monitoring the game process continuously.
    ///
    /// Updates `shared.game_running` on every poll and emits `NotLiveStateChanged`
    /// events on transitions so the overlay auto-hide system can react.
    /// Runs for the lifetime of the tailing session (stopped by `stop_tailing`).
    /// Only one monitor runs at a time; calling this while one is active is a no-op.
    pub(super) fn start_process_monitor(&mut self) {
        if self.process_monitor_handle.is_some() {
            return;
        }

        let overlay_tx = self.overlay_tx.clone();
        let shared = self.shared.clone();
        let handle = tokio::spawn(async move {
            let mut was_running = shared.game_running.load(Ordering::SeqCst);

            loop {
                match process_monitor::is_game_running().await {
                    Some(is_running) => {
                        shared.game_running.store(is_running, Ordering::SeqCst);

                        if was_running && !is_running {
                            // Game just closed
                            info!("Game process no longer detected");
                            clear_uptime_tracking(&shared).await;
                            let _ = overlay_tx.try_send(OverlayUpdate::NotLiveStateChanged { is_live: false });
                        } else if !was_running && is_running {
                            // Game just launched — set the starting flag so stale-recovery
                            // stays blocked until a fresh session confirms a new character.
                            info!("Game process detected; waiting for fresh log session before restoring overlays");
                            shared.auto_hide.set_game_starting(true);
                        }

                        was_running = is_running;
                    }
                    None => {
                        // Process check failed — safe default: assume running,
                        // keep current state, and try again next poll
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(2500)).await;
            }
        });

        self.process_monitor_handle = Some(handle);
    }

    /// Stop the game process monitor if running.
    fn stop_process_monitor(&mut self) {
        if let Some(handle) = self.process_monitor_handle.take() {
            handle.abort();
        }
    }

    pub(super) async fn refresh_index(&mut self) {
        let log_dir = self.shared.config.read().await.log_directory.clone();
        let cache_path = DirectoryIndex::default_cache_path().unwrap_or_else(|| PathBuf::from("directory_cache.json"));
        if let Ok(index) = DirectoryIndex::build_index_cached(&PathBuf::from(&log_dir), &cache_path) {
            *self.shared.directory_index.write().await = index;
        }
    }
}
