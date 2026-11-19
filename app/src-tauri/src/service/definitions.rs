//! Area, boss, timer and effect definition loading

use super::{CombatService, OverlayUpdate};
use crate::state::SharedState;
use baras_core::context::{AppConfigExt, ParsingSession, resolve};
use baras_core::{BossEncounterDefinition, DefinitionConfig, DefinitionSet, EFFECTS_DSL_VERSION};
use baras_overlay::NotesData;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager};
use tracing::{debug, error, info, warn};

impl CombatService {
    /// Build area index from encounter definition files (lightweight - only reads headers)
    pub(super) fn build_area_index(app_handle: &AppHandle) -> baras_core::boss::AreaIndex {
        use baras_core::boss::build_area_index;

        // Bundled definitions: shipped with the app in resources
        let bundled_dir = app_handle
            .path()
            .resolve("definitions/encounters", tauri::path::BaseDirectory::Resource)
            .ok();

        // Custom definitions: user's config directory
        let custom_dir = baras_core::paths::config_dir().map(|p| p.join("definitions").join("encounters"));

        let mut index = baras_core::boss::AreaIndex::new();

        // Build index from bundled directory
        if let Some(ref path) = bundled_dir
            && path.exists()
        {
            match build_area_index(path) {
                Ok(area_index) => index.extend(area_index),
                Err(e) => warn!(path = %path.display(), error = %e, "Failed to build bundled area index"),
            }
        }

        // Build index from custom directory (can override bundled)
        if let Some(ref path) = custom_dir
            && path.exists()
        {
            match build_area_index(path) {
                Ok(area_index) => index.extend(area_index),
                Err(e) => warn!(path = %path.display(), error = %e, "Failed to build custom area index"),
            }
        }

        index
    }

    /// Spawn a background task to index areas in log files.
    /// This scans files that aren't already in the cache (except the newest file).
    /// Runs silently in background without blocking normal app operations.
    pub(super) fn spawn_area_indexer(
        shared: Arc<SharedState>,
        area_index: Arc<baras_core::boss::AreaIndex>,
        app_handle: AppHandle,
    ) {
        use baras_core::context::{
            FileAreaIndex, LogAreaCache, SCAN_VERSION, default_cache_path, extract_areas_from_file,
        };
        use std::collections::HashSet;
        const NEWEST_FILE_SETTLE: std::time::Duration = std::time::Duration::from_secs(600);

        tauri::async_runtime::spawn(async move {
            // Small delay to let app finish initializing - don't block startup
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;

            // Load existing cache from disk
            let cache_path = match default_cache_path() {
                Some(p) => p,
                None => {
                    warn!("Could not determine area cache path");
                    return;
                }
            };

            let mut cache = LogAreaCache::load_from_disk(&cache_path);
            debug!(cached_files = cache.len(), "Loaded area cache from disk");

            // Update shared state with loaded cache (brief write lock)
            // Don't prune here - stale entries are harmless and pruning is slow
            {
                let mut area_cache = shared.area_cache.write().await;
                *area_cache = cache.clone();
            }
            // Don't emit event here - let frontend use its initial load

            // Get known area IDs from boss definitions
            let mut known_area_ids: HashSet<i64> = area_index.keys().copied().collect();
            if known_area_ids.is_empty() {
                debug!("No area definitions found, skipping area indexing");
                return;
            }
            // Warzones and arenas have no definitions but are content too
            known_area_ids.extend(baras_core::game_data::pvp_area_ids());

            // Collect file paths to scan quickly, then release lock
            let files_to_scan: Vec<(PathBuf, std::time::SystemTime)> = {
                let index = shared.directory_index.read().await;
                let newest_path = index.newest_file().map(|f| f.path.clone());
                index
                    .entries()
                    .iter()
                    .filter_map(|e| {
                        // Skip empty files
                        if e.is_empty {
                            return None;
                        }
                        // Get modification time
                        let modified = std::fs::metadata(&e.path)
                            .and_then(|m| m.modified())
                            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                        // Skip the newest file only while the game is still
                        // writing it; a finished one indexes like any other
                        if Some(&e.path) == newest_path.as_ref()
                            && modified.elapsed().is_ok_and(|age| age < NEWEST_FILE_SETTLE)
                        {
                            return None;
                        }
                        // Check if needs update
                        if cache.needs_update(&e.path, modified) {
                            Some((e.path.clone(), modified))
                        } else {
                            None
                        }
                    })
                    .collect()
            };
            // Lock released here

            if files_to_scan.is_empty() {
                debug!("All files already indexed, nothing to scan");
                return;
            }

            debug!(files_to_scan = files_to_scan.len(), "Starting background area indexing");

            // Scan files without holding any locks
            let mut scanned = 0;
            let mut last_saved = 0;
            for (path, modified) in files_to_scan {
                let modified_secs = modified
                    .duration_since(std::time::SystemTime::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);

                match extract_areas_from_file(&path, &known_area_ids) {
                    Ok(areas) => {
                        cache.insert(
                            path.clone(),
                            FileAreaIndex {
                                modified_secs,
                                areas,
                                scanned_with: SCAN_VERSION,
                            },
                        );
                        scanned += 1;
                    }
                    Err(e) => {
                        debug!(path = %path.display(), error = %e, "Failed to extract areas from file");
                    }
                }

                // Persist progress now and then: a restart resumes instead of
                // starting over, and the Library sees areas as they land
                if scanned - last_saved >= 25 {
                    last_saved = scanned;
                    *shared.area_cache.write().await = cache.clone();
                    if let Err(e) = cache.save_to_disk(&cache_path) {
                        warn!(error = %e, "Failed to save area cache to disk");
                    }
                    let _ = app_handle.emit("log-files-changed", ());
                }
                // Yield periodically to avoid starving other tasks
                if scanned % 10 == 0 {
                    tokio::task::yield_now().await;
                }
            }

            // Prune entries for deleted files (lazy cleanup)
            let cache_size_before = cache.len();
            cache.prune_missing();
            let pruned = cache_size_before - cache.len();
            if pruned > 0 {
                debug!(pruned, "Pruned stale entries from area cache");
            }

            // Save updated cache and notify frontend if anything changed
            if scanned > 0 || pruned > 0 {
                // Brief write lock to update shared state
                {
                    let mut area_cache = shared.area_cache.write().await;
                    *area_cache = cache.clone();
                }

                if let Err(e) = cache.save_to_disk(&cache_path) {
                    warn!(error = %e, "Failed to save area cache to disk");
                } else {
                    info!(scanned, pruned, "Area indexing complete, cache saved");
                }

                // Notify frontend that area data is now available
                let _ = app_handle.emit("log-files-changed", ());
            }
        });
    }

    /// Load boss definitions for a specific area, merging with custom overlays
    pub(super) fn load_area_definitions(&self, area_id: i64) -> Option<Vec<BossEncounterDefinition>> {
        use baras_core::boss::load_bosses_with_custom;

        let entry = self.area_index.get(&area_id)?;

        // User custom directory for overlay files
        let user_dir = baras_core::paths::config_dir().map(|p| p.join("definitions").join("encounters"));

        match load_bosses_with_custom(&entry.file_path, user_dir.as_deref()) {
            Ok(bosses) => Some(bosses),
            Err(e) => {
                warn!(
                    area_id,
                    path = %entry.file_path.display(),
                    error = %e,
                    "Failed to load boss definitions"
                );
                None
            }
        }
    }

    /// Switch to the alacrity saved for the local player's discipline. Live
    /// only: opening an old log must not change the user's settings.
    pub(super) async fn apply_alacrity_preset(&self, session: &ParsingSession) {
        if !self.shared.is_live_tailing.load(Ordering::SeqCst) {
            return;
        }
        let Some(cache) = session.session_cache.as_ref().filter(|c| c.player_initialized) else {
            return;
        };
        let mut config = self.shared.config.write().await;
        let Some(value) = baras_types::AlacrityPreset::find(
            &config.alacrity_presets,
            &cache.player.discipline_name,
            resolve(cache.player.name),
        )
        .map(|p| p.alacrity_percent)
        .filter(|v| *v != config.alacrity_percent) else {
            return;
        };
        config.alacrity_percent = value;
        if let Err(e) = config.clone().save() {
            error!(error = %e, "Failed to save configuration");
        }
        drop(config);
        session.set_effect_alacrity(value);
        info!(value, discipline = %cache.player.discipline_name, "Applied saved alacrity");
        let _ = self.app_handle.emit("alacrity-auto-set", value);
    }

    /// The area's `[area.timer_start]` region, if the definition file declares one.
    pub(super) fn load_area_timer_start(&self, area_id: i64) -> Option<baras_core::OperationTimerStart> {
        let entry = self.area_index.get(&area_id)?;
        baras_core::boss::load_area_config(&entry.file_path)
            .ok()
            .flatten()
            .and_then(|area| area.timer_start)
    }

    /// Get the path to the timer preferences file
    pub(super) fn timer_preferences_path() -> Option<std::path::PathBuf> {
        baras_core::paths::config_dir().map(|p| p.join("timer_preferences.toml"))
    }

    /// Initialize the icon cache for ability icons
    pub(super) fn init_icon_cache(app_handle: &AppHandle) -> Option<Arc<baras_overlay::icons::IconCache>> {
        use baras_overlay::icons::IconCache;

        debug!("Initializing icon cache");

        // Try bundled resources first, fall back to dev path
        let icons_dir = app_handle
            .path()
            .resolve("icons", tauri::path::BaseDirectory::Resource)
            .ok()
            .filter(|p| p.exists())
            .unwrap_or_else(|| {
                // Dev fallback: relative to project root
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .ancestors()
                    .nth(2)
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join("icons")
            });

        debug!(path = ?icons_dir, "Looking for icons");

        let csv_path = icons_dir.join("icons.csv");
        let zip_path = icons_dir.join("icons.zip");

        if !csv_path.exists() || !zip_path.exists() {
            debug!(
                path = ?icons_dir,
                csv_exists = csv_path.exists(),
                zip_exists = zip_path.exists(),
                "Icon files not found"
            );
            return None;
        }

        match IconCache::new(&csv_path, &zip_path, 200) {
            Ok(cache) => {
                info!(path = ?icons_dir, "Loaded icon cache");
                Some(Arc::new(cache))
            }
            Err(e) => {
                error!(error = %e, "Failed to load icon cache");
                None
            }
        }
    }

    /// Get the user effects config file path
    fn get_user_effects_path() -> Option<PathBuf> {
        baras_core::paths::config_dir().map(|p| p.join("definitions").join("effects.toml"))
    }

    /// Clean up old user effects directory structure (pre-delta architecture)
    fn cleanup_old_effects_dir() {
        let Some(old_dir) = baras_core::paths::config_dir().map(|p| p.join("definitions").join("effects")) else {
            return;
        };

        if old_dir.is_dir() {
            debug!(path = ?old_dir, "Removing old effects directory");
            if let Err(e) = std::fs::remove_dir_all(&old_dir) {
                error!(error = %e, path = ?old_dir, "Failed to remove old effects directory");
            }
        }
    }

    /// Load effect definitions from bundled resources and user config file.
    ///
    /// Architecture (delta-based):
    /// 1. Load bundled definitions from app resources (base layer)
    /// 2. Load user overrides from single file: ~/.config/baras/definitions/effects.toml
    /// 3. User effects with matching IDs replace bundled effects entirely
    ///
    /// Version checking:
    /// - User file must have `version = N` matching EFFECTS_DSL_VERSION
    /// - Mismatched versions cause user file to be deleted (breaking DSL change)
    pub(super) fn load_effect_definitions(app_handle: &AppHandle) -> DefinitionSet {
        // Clean up old directory structure on first run after update
        Self::cleanup_old_effects_dir();

        let mut set = DefinitionSet::new();

        // 1. Load bundled definitions from app resources
        if let Some(bundled_dir) = app_handle
            .path()
            .resolve("definitions/effects", tauri::path::BaseDirectory::Resource)
            .ok()
            .filter(|p| p.exists())
        {
            Self::load_bundled_definitions(&mut set, &bundled_dir);
        }

        // 2. Load user overrides from single config file
        if let Some(user_path) = Self::get_user_effects_path()
            && user_path.exists()
        {
            Self::load_user_effects(&mut set, &user_path);
        }

        set
    }

    /// Load bundled effect definitions from a directory
    fn load_bundled_definitions(set: &mut DefinitionSet, dir: &std::path::Path) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            debug!(path = ?dir, "Failed to read bundled effects directory");
            return;
        };

        let files: Vec<_> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.extension().is_some_and(|ext| ext == "toml") && p.file_name().is_none_or(|n| n != "custom.toml") // Skip template
            })
            .collect();

        debug!(count = files.len(), path = ?dir, "Loading bundled effect files");

        for path in files {
            if let Ok(contents) = std::fs::read_to_string(&path)
                && let Ok(config) = toml::from_str::<DefinitionConfig>(&contents)
            {
                let count = config.effects.len();
                set.add_definitions(config.effects, false);
                debug!(
                    file = ?path.file_name().unwrap_or_default(),
                    count,
                    "Loaded effect definitions"
                );
            }
        }
    }

    /// Load user effect overrides from single config file
    fn load_user_effects(set: &mut DefinitionSet, path: &std::path::Path) {
        let Ok(contents) = std::fs::read_to_string(path) else {
            error!(path = ?path, "Failed to read user effects file");
            return;
        };

        let Ok(config) = toml::from_str::<DefinitionConfig>(&contents) else {
            error!(path = ?path, "Failed to parse user effects file, backing up");
            let _ = std::fs::rename(path, path.with_extension("toml.bak"));
            return;
        };

        // Version check - backup file if version mismatch
        if config.version != EFFECTS_DSL_VERSION {
            warn!(
                file_version = config.version,
                expected_version = EFFECTS_DSL_VERSION,
                path = ?path,
                "User effects version mismatch, backing up file"
            );
            let _ = std::fs::rename(path, path.with_extension("toml.bak"));
            return;
        }

        if !config.effects.is_empty() {
            debug!(count = config.effects.len(), path = ?path, "Loading user effect overrides");
            set.add_definitions(config.effects, true); // Overwrite bundled
        }
    }
}

impl CombatService {
    /// Reload effect definitions from disk and update the active session
    pub(super) async fn reload_effect_definitions(&mut self) {
        self.definitions = Self::load_effect_definitions(&self.app_handle);

        if let Some(session) = self.shared.session.read().await.as_ref() {
            let session = session.read().await;
            session.set_definitions(self.definitions.clone());
        }
    }

    /// Reload timer and boss definitions from disk and update the active session.
    /// Invalidates the timer cache first to ensure definitions actually reload.
    pub(super) async fn reload_timer_definitions(&mut self) {
        self.area_index = Arc::new(Self::build_area_index(&self.app_handle));

        let current_area = self.shared.current_area_id.load(Ordering::SeqCst);
        if current_area == 0 {
            return;
        }

        let Some(bosses) = self.load_area_definitions(current_area) else {
            return;
        };

        // Send notes from boss definitions to overlay (first boss with notes)
        self.send_notes_from_bosses(&bosses);

        let session_guard = self.shared.session.read().await;
        let Some(session) = session_guard.as_ref() else {
            return;
        };

        let mut session = session.write().await;
        // Invalidate timer cache to ensure definitions actually reload
        // (bypasses fingerprint optimization for user-triggered reloads)
        if let Some(timer_mgr) = session.timer_manager()
            && let Ok(mut mgr) = timer_mgr.lock()
        {
            mgr.invalidate_definitions_cache();
        }
        session.load_boss_definitions(bosses, true);
    }

    /// Send notes from boss definitions to the notes overlay (only in live mode)
    pub(super) fn send_notes_from_bosses(&self, bosses: &[BossEncounterDefinition]) {
        // Only send notes in live tailing mode (not for historical files)
        if !self.shared.is_live_tailing.load(Ordering::SeqCst) {
            return;
        }

        // Find the first boss with notes (or aggregate all notes)
        // For now, send notes from first boss that has them
        for boss in bosses {
            if let Some(notes) = &boss.notes
                && !notes.is_empty()
            {
                let notes_data = NotesData {
                    text: notes.clone(),
                    boss_name: boss.name.clone(),
                };
                let _ = self.overlay_tx.try_send(OverlayUpdate::NotesUpdated(notes_data));
                return;
            }
        }

        // No notes found - send empty to clear the overlay
        let _ = self
            .overlay_tx
            .try_send(OverlayUpdate::NotesUpdated(NotesData::default()));
    }
}
