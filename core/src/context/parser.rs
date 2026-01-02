use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::NaiveDateTime;
use tokio::sync::RwLock;

use crate::combat_log::{CombatEvent, Reader};
use crate::context::{AppConfig, parse_log_filename};
use crate::effects::{DefinitionSet, EffectTracker};
use crate::signal_processor::{EventProcessor, GameSignal, SignalHandler};
use crate::state::SessionCache;
use crate::dsl::BossEncounterDefinition;
use crate::timers::{TimerDefinition, TimerManager};
use crate::storage::{encounter_filename, EncounterWriter, EventMetadata};

/// A parsing session that processes combat events and tracks game state.
///
/// The session maintains:
/// - Event processing pipeline (encounters, metrics)
/// - Effect tracking (HoTs, debuffs, shields for overlay display) - Live mode only
/// - Timer tracking (boss mechanics countdown timers) - Live mode only
/// - Signal handlers for cross-cutting concerns
///
/// In Live mode, effect and timer tracking are enabled for overlay display.
/// In Historical mode, these components are not created to save memory.
pub struct ParsingSession {
    pub current_byte: Option<u64>,
    pub active_file: Option<PathBuf>,
    pub game_session_date: Option<NaiveDateTime>,
    pub session_cache: Option<SessionCache>,
    processor: EventProcessor,
    signal_handlers: Vec<Box<dyn SignalHandler + Send + Sync>>,
    /// Effect tracker for HoT/debuff/shield overlay display.
    /// Only created in Live mode. None in Historical mode.
    effect_tracker: Option<Arc<Mutex<EffectTracker>>>,
    /// Timer manager for boss/mechanic countdown timers.
    /// Only created in Live mode. None in Historical mode.
    timer_manager: Option<Arc<Mutex<TimerManager>>>,

    // Live parquet writing (for streaming mode)
    /// Directory where encounter parquet files are written
    encounters_dir: Option<PathBuf>,
    /// Current encounter index (continues from subprocess)
    encounter_idx: u32,
    /// Event buffer for current encounter
    encounter_writer: Option<EncounterWriter>,
}

impl Default for ParsingSession {
    /// Creates a Live mode session with effect and timer tracking enabled.
    fn default() -> Self {
        Self::live()
    }
}

impl ParsingSession {
    /// Create a Live mode session with effect and timer tracking.
    pub fn live() -> Self {
        Self {
            current_byte: None,
            active_file: None,
            game_session_date: None,
            session_cache: Some(SessionCache::new()),
            processor: EventProcessor::new(),
            signal_handlers: Vec::new(),
            effect_tracker: Some(Arc::new(Mutex::new(EffectTracker::default()))),
            timer_manager: Some(Arc::new(Mutex::new(TimerManager::default()))),
            encounters_dir: None,
            encounter_idx: 0,
            encounter_writer: None,
        }
    }

    /// Create a Historical mode session without effect/timer tracking.
    pub fn historical() -> Self {
        Self {
            current_byte: None,
            active_file: None,
            game_session_date: None,
            session_cache: Some(SessionCache::new()),
            processor: EventProcessor::new(),
            signal_handlers: Vec::new(),
            effect_tracker: None,
            timer_manager: None,
            encounters_dir: None,
            encounter_idx: 0,
            encounter_writer: None,
        }
    }

    /// Create a new Live mode parsing session for a log file.
    ///
    /// This is the primary constructor for live file tailing with effect and timer tracking.
    pub fn new(path: PathBuf, definitions: DefinitionSet) -> Self {
        let date_stamp = path
            .file_name()
            .and_then(|f| f.to_str())
            .and_then(parse_log_filename)
            .map(|(_, dt)| dt);

        Self {
            current_byte: None,
            active_file: Some(path),
            game_session_date: date_stamp,
            session_cache: Some(SessionCache::new()),
            processor: EventProcessor::new(),
            signal_handlers: Vec::new(),
            effect_tracker: Some(Arc::new(Mutex::new(EffectTracker::new(definitions)))),
            timer_manager: Some(Arc::new(Mutex::new(TimerManager::default()))),
            encounters_dir: None,
            encounter_idx: 0,
            encounter_writer: None,
        }
    }

    /// Register a signal handler to receive game signals
    pub fn add_signal_handler(&mut self, handler: Box<dyn SignalHandler + Send + Sync>) {
        self.signal_handlers.push(handler);
    }

    /// Process a single event through the processor and dispatch signals
    pub fn process_event(&mut self, event: CombatEvent) {
        if let Some(cache) = &mut self.session_cache {
            // Write event to parquet buffer if live writing is enabled
            if let Some(writer) = &mut self.encounter_writer {
                let metadata = EventMetadata::from_cache(cache, self.encounter_idx, event.timestamp);
                writer.push_event(&event, &metadata);
            }

            let signals = self.processor.process_event(event, cache);

            // Flush parquet on combat end
            let should_flush = signals.iter().any(|s| matches!(s, GameSignal::CombatEnded { .. }));

            self.dispatch_signals(&signals);

            if should_flush {
                self.flush_encounter_parquet();
            }
        }
    }

    /// Flush current encounter buffer to parquet file
    fn flush_encounter_parquet(&mut self) {
        let Some(writer) = &mut self.encounter_writer else { return };
        if writer.is_empty() { return; }

        let Some(dir) = &self.encounters_dir else { return };

        let filename = encounter_filename(self.encounter_idx);
        let path = dir.join(&filename);

        if let Err(e) = writer.write_to_file(&path) {
            eprintln!("[PARQUET] Failed to write encounter {}: {}", self.encounter_idx, e);
        } else {
            eprintln!("[PARQUET] Wrote encounter {} ({} events)", self.encounter_idx, writer.len());
        }

        writer.clear();
        self.encounter_idx += 1;
    }

    /// Enable live parquet writing for streaming mode.
    /// Call after subprocess completes to continue writing encounters.
    pub fn enable_live_parquet(&mut self, encounters_dir: PathBuf, starting_idx: u32) {
        self.encounters_dir = Some(encounters_dir);
        self.encounter_idx = starting_idx;
        self.encounter_writer = Some(EncounterWriter::with_capacity(10_000));
    }

    /// Process multiple events
    pub fn process_events(&mut self, events: Vec<CombatEvent>) {
        let mut all_signals = Vec::new();

        if let Some(cache) = &mut self.session_cache {
            for event in events {
                let signals = self.processor.process_event(event, cache);
                all_signals.extend(signals);
            }
        }

        self.dispatch_signals(&all_signals);
    }

    fn dispatch_signals(&mut self, signals: &[GameSignal]) {
        let Some(cache) = &self.session_cache else { return };

        // Get current encounter and ensure it has local_player_id from cache
        let encounter = cache.current_encounter();
        let local_player_id = if cache.player_initialized {
            Some(cache.player.id)
        } else {
            None
        };

        // Forward to registered signal handlers
        for handler in &mut self.signal_handlers {
            handler.handle_signals(signals, encounter);
        }

        // Forward to effect tracker (Live mode only)
        if let Some(tracker) = &self.effect_tracker {
            if let Ok(mut tracker) = tracker.lock() {
                tracker.handle_signals_with_player(signals, encounter, local_player_id);
            }
        }

        // Forward to timer manager (Live mode only)
        if let Some(timer_mgr) = &self.timer_manager {
            if let Ok(mut timer_mgr) = timer_mgr.lock() {
                timer_mgr.handle_signals(signals, encounter);
            }
        }
    }

    /// Get a shared reference to the effect tracker for overlay queries.
    /// Returns None in Historical mode.
    pub fn effect_tracker(&self) -> Option<Arc<Mutex<EffectTracker>>> {
        self.effect_tracker.as_ref().map(Arc::clone)
    }

    /// Get a shared reference to the timer manager for overlay queries.
    /// Returns None in Historical mode.
    pub fn timer_manager(&self) -> Option<Arc<Mutex<TimerManager>>> {
        self.timer_manager.as_ref().map(Arc::clone)
    }

    /// Tick the effect tracker and timer manager to update expiration state.
    ///
    /// Call this periodically (e.g., at overlay refresh rate ~10fps) to ensure
    /// duration-expired effects and timers are updated. No-op in Historical mode.
    pub fn tick(&self) {
        if let Some(tracker) = &self.effect_tracker {
            if let Ok(mut tracker) = tracker.lock() {
                tracker.tick();
            }
        }
        if let Some(timer_mgr) = &self.timer_manager {
            if let Ok(mut timer_mgr) = timer_mgr.lock() {
                timer_mgr.tick();
            }
        }
    }

    /// Update effect definitions (e.g., after config reload). No-op in Historical mode.
    pub fn set_definitions(&self, definitions: DefinitionSet) {
        if let Some(tracker) = &self.effect_tracker {
            if let Ok(mut tracker) = tracker.lock() {
                tracker.set_definitions(definitions);
            }
        }
    }

    /// Enable/disable live mode for effect tracking.
    /// Call with `true` after initial file load to start tracking effects.
    /// No-op in Historical mode (session has no effect tracker).
    pub fn set_effect_live_mode(&self, enabled: bool) {
        if let Some(tracker) = &self.effect_tracker {
            if let Ok(mut tracker) = tracker.lock() {
                tracker.set_live_mode(enabled);
            }
        }
    }

    /// Enable/disable live mode for timer tracking.
    /// Call with `true` after initial file load to filter stale events.
    /// No-op in Historical mode (session has no timer manager).
    pub fn set_timer_live_mode(&self, enabled: bool) {
        if let Some(timer_mgr) = &self.timer_manager {
            if let Ok(mut timer_mgr) = timer_mgr.lock() {
                timer_mgr.set_live_mode(enabled);
            }
        }
    }

    /// Update timer definitions (e.g., after config reload). No-op in Historical mode.
    pub fn set_timer_definitions(&self, definitions: Vec<TimerDefinition>) {
        if let Some(timer_mgr) = &self.timer_manager {
            if let Ok(mut timer_mgr) = timer_mgr.lock() {
                timer_mgr.set_definitions(definitions);
            }
        }
    }

    /// Update boss definitions (for boss detection and phase tracking).
    /// NOTE: This only updates TimerManager. For full support, use `load_boss_definitions`.
    pub fn set_boss_definitions(&self, bosses: Vec<BossEncounterDefinition>) {
        if let Some(timer_mgr) = &self.timer_manager {
            if let Ok(mut timer_mgr) = timer_mgr.lock() {
                timer_mgr.load_boss_definitions(bosses);
            }
        }
    }

    /// Load boss definitions into both SessionCache and TimerManager.
    /// Requires mutable access - use this when entering a new area.
    pub fn load_boss_definitions(&mut self, bosses: Vec<BossEncounterDefinition>) {
        // Update SessionCache (for boss encounter detection and state tracking)
        if let Some(cache) = &mut self.session_cache {
            cache.load_boss_definitions(bosses.clone());
        }

        // Update TimerManager (for timer activation) - Live mode only
        if let Some(timer_mgr) = &self.timer_manager {
            if let Ok(mut timer_mgr) = timer_mgr.lock() {
                timer_mgr.load_boss_definitions(bosses);
            }
        }
    }

    /// Finalize the current session after parsing completes.
    ///
    /// Call this after processing all events from a historical file to ensure
    /// the final encounter is added to the encounter history.
    pub fn finalize_session(&mut self) {
        if let Some(cache) = &mut self.session_cache {
            cache.finalize_current_encounter();
        }
    }

    /// Get the encounters directory path (for querying historical parquet files).
    pub fn encounters_dir(&self) -> Option<&std::path::PathBuf> {
        self.encounters_dir.as_ref()
    }

    /// Get the current encounter writer (for querying live data).
    pub fn encounter_writer(&self) -> Option<&crate::storage::EncounterWriter> {
        self.encounter_writer.as_ref()
    }

    /// Sync timer context from session cache (call after initial file parse).
    ///
    /// This ensures the TimerManager knows the current area even if parsing
    /// started mid-session (no AreaEntered signal was received).
    /// No-op in Historical mode (session has no timer manager).
    pub fn sync_timer_context(&self) {
        let Some(cache) = &self.session_cache else {
            return;
        };
        let Some(timer_mgr) = &self.timer_manager else {
            return;
        };

        let area = &cache.current_area;
        if area.area_name.is_empty() {
            return;
        }

        let difficulty = crate::game_data::Difficulty::from_game_string(&area.difficulty_name);
        let area_id = if area.area_id != 0 { Some(area.area_id) } else { None };

        if let Ok(mut timer_mgr) = timer_mgr.lock() {
            timer_mgr.set_context(
                area_id,
                Some(area.area_name.clone()),
                None, // Boss will be detected on target change
                difficulty,
            );
            eprintln!("[TIMER] Synced initial context from cache: area={} (id={:?}), difficulty={:?}",
                area.area_name, area_id, difficulty);
        }
    }
}

/// Resolve a log file path, joining with log_directory if relative.
pub fn resolve_log_path(config: &AppConfig, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        Path::new(&config.log_directory).join(path)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// File Parsing Helper
// ─────────────────────────────────────────────────────────────────────────────

/// Result of parsing a log file
pub struct ParseResult {
    pub events_count: usize,
    pub elapsed_ms: u128,
    pub reader: Reader,
    pub end_pos: u64,
}

/// Parse an entire log file, processing events through the session.
/// Uses streaming to avoid allocating all events at once.
pub async fn parse_file(state: Arc<RwLock<ParsingSession>>) -> Result<ParseResult, String> {
    let timer = std::time::Instant::now();

    let active_path = {
        let s = state.read().await;
        s.active_file.clone().ok_or("invalid file given")?
    };

    let reader = Reader::from(active_path, Arc::clone(&state));

    // Stream-parse: process events one at a time without collecting
    let mut s = state.write().await;
    let session_date = s.game_session_date.unwrap_or_default();
    let (end_pos, events_count) = reader
        .read_log_file_streaming(session_date, |event| {
            s.process_event(event);
        })
        .map_err(|e| format!("failed to parse log file: {}", e))?;

    s.current_byte = Some(end_pos);
    // Sync area context to timer manager (handles mid-session starts)
    s.sync_timer_context();
    drop(s);

    let elapsed_ms = timer.elapsed().as_millis();

    Ok(ParseResult {
        events_count,
        elapsed_ms,
        reader,
        end_pos,
    })
}
