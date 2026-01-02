use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::NaiveDateTime;
use tokio::sync::RwLock;

use crate::combat_log::{CombatEvent, Reader};
use crate::context::{AppConfig, parse_log_filename};
use crate::effects::{DefinitionSet, EffectTracker};
use crate::signal_processor::{EventProcessor, GameSignal, SignalHandler};
use crate::state::SessionCache;
use crate::boss::BossEncounterDefinition;
use crate::timers::{TimerDefinition, TimerManager};
use crate::storage::{encounter_filename, EncounterWriter, EventMetadata};

/// Build metadata for parquet event row (standalone to avoid borrow conflicts)
fn build_event_metadata(cache: &SessionCache, encounter_idx: u32) -> EventMetadata {
    let enc = cache.current_encounter();
    let boss_def = enc.and_then(|e| e.active_boss_definition());
    let current_phase = enc.and_then(|e| e.current_phase.clone());

    EventMetadata {
        encounter_idx,
        phase_id: current_phase.clone(),
        phase_name: current_phase.as_ref().and_then(|phase_id| {
            boss_def.and_then(|def| {
                def.phases
                    .iter()
                    .find(|p| &p.id == phase_id)
                    .map(|p| p.name.clone())
            })
        }),
        area_name: cache.current_area.area_name.clone(),
        boss_name: boss_def.map(|d| d.name.clone()),
        difficulty: if cache.current_area.difficulty_name.is_empty() {
            None
        } else {
            Some(cache.current_area.difficulty_name.clone())
        },
    }
}

/// A live parsing session that processes combat events and tracks game state.
///
/// The session maintains:
/// - Event processing pipeline (encounters, metrics)
/// - Effect tracking (HoTs, debuffs, shields for overlay display)
/// - Signal handlers for cross-cutting concerns
///
/// Effect tracking is independent of encounter lifecycle - flushing encounters
/// does not affect active effects. Effects represent current game state snapshot.
pub struct ParsingSession {
    pub current_byte: Option<u64>,
    pub active_file: Option<PathBuf>,
    pub game_session_date: Option<NaiveDateTime>,
    pub session_cache: Option<SessionCache>,
    processor: EventProcessor,
    signal_handlers: Vec<Box<dyn SignalHandler + Send + Sync>>,
    /// Effect tracker for HoT/debuff/shield overlay display.
    /// Wrapped in Arc<Mutex> for shared access between signal dispatch and overlay queries.
    effect_tracker: Arc<Mutex<EffectTracker>>,
    /// Timer manager for boss/mechanic countdown timers.
    /// Wrapped in Arc<Mutex> for shared access between signal dispatch and overlay queries.
    timer_manager: Arc<Mutex<TimerManager>>,

    // Live parquet writing (for streaming mode)
    /// Directory where encounter parquet files are written
    encounters_dir: Option<PathBuf>,
    /// Current encounter index (continues from subprocess)
    encounter_idx: u32,
    /// Event buffer for current encounter
    encounter_writer: Option<EncounterWriter>,
}

impl Default for ParsingSession {
    fn default() -> Self {
        Self {
            current_byte: None,
            active_file: None,
            game_session_date: None,
            session_cache: Some(SessionCache::new()),
            processor: EventProcessor::new(),
            signal_handlers: Vec::new(),
            effect_tracker: Arc::new(Mutex::new(EffectTracker::default())),
            timer_manager: Arc::new(Mutex::new(TimerManager::default())),
            encounters_dir: None,
            encounter_idx: 0,
            encounter_writer: None,
        }
    }
}

impl ParsingSession {
    /// Create a new parsing session for a log file.
    ///
    /// Effect tracking is always enabled. Pass a `DefinitionSet` to configure
    /// which effects to track, or use `DefinitionSet::default()` for an empty set.
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
            effect_tracker: Arc::new(Mutex::new(EffectTracker::new(definitions))),
            timer_manager: Arc::new(Mutex::new(TimerManager::default())),
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
                let metadata = build_event_metadata(cache, self.encounter_idx);
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
        // Forward to registered signal handlers
        for handler in &mut self.signal_handlers {
            handler.handle_signals(signals);
        }

        // Forward to effect tracker (kept separate for query access)
        if let Ok(mut tracker) = self.effect_tracker.lock() {
            tracker.handle_signals(signals);
        }

        // Forward to timer manager
        if let Ok(mut timer_mgr) = self.timer_manager.lock() {
            timer_mgr.handle_signals(signals);
        }
    }

    /// Get a shared reference to the effect tracker for overlay queries.
    ///
    /// The returned Arc can be cloned and held by overlay code for periodic queries.
    /// Lock the mutex to access `active_effects()` or `effects_for_target()`.
    pub fn effect_tracker(&self) -> Arc<Mutex<EffectTracker>> {
        Arc::clone(&self.effect_tracker)
    }

    /// Get a shared reference to the timer manager for overlay queries.
    pub fn timer_manager(&self) -> Arc<Mutex<TimerManager>> {
        Arc::clone(&self.timer_manager)
    }

    /// Tick the effect tracker and timer manager to update expiration state.
    ///
    /// Call this periodically (e.g., at overlay refresh rate ~10fps) to ensure
    /// duration-expired effects and timers are updated.
    pub fn tick(&self) {
        if let Ok(mut tracker) = self.effect_tracker.lock() {
            tracker.tick();
        }
        if let Ok(mut timer_mgr) = self.timer_manager.lock() {
            timer_mgr.tick();
        }
    }

    /// Update effect definitions (e.g., after config reload).
    pub fn set_definitions(&self, definitions: DefinitionSet) {
        if let Ok(mut tracker) = self.effect_tracker.lock() {
            tracker.set_definitions(definitions);
        }
    }

    /// Enable/disable live mode for effect tracking.
    /// Call with `true` after initial file load to start tracking effects.
    pub fn set_effect_live_mode(&self, enabled: bool) {
        if let Ok(mut tracker) = self.effect_tracker.lock() {
            tracker.set_live_mode(enabled);
        }
    }

    /// Enable/disable live mode for timer tracking.
    /// Call with `true` after initial file load to filter stale events.
    pub fn set_timer_live_mode(&self, enabled: bool) {
        if let Ok(mut timer_mgr) = self.timer_manager.lock() {
            timer_mgr.set_live_mode(enabled);
        }
    }

    /// Update timer definitions (e.g., after config reload).
    pub fn set_timer_definitions(&self, definitions: Vec<TimerDefinition>) {
        if let Ok(mut timer_mgr) = self.timer_manager.lock() {
            timer_mgr.set_definitions(definitions);
        }
    }

    /// Update boss definitions (for boss detection and phase tracking).
    /// NOTE: This only updates TimerManager. For full support, use `load_boss_definitions`.
    pub fn set_boss_definitions(&self, bosses: Vec<BossEncounterDefinition>) {
        if let Ok(mut timer_mgr) = self.timer_manager.lock() {
            timer_mgr.load_boss_definitions(bosses);
        }
    }

    /// Load boss definitions into both SessionCache and TimerManager.
    /// Requires mutable access - use this when entering a new area.
    pub fn load_boss_definitions(&mut self, bosses: Vec<BossEncounterDefinition>) {
        // Update SessionCache (for boss encounter detection and state tracking)
        if let Some(cache) = &mut self.session_cache {
            cache.load_boss_definitions(bosses.clone());
        }

        // Update TimerManager (for timer activation)
        if let Ok(mut timer_mgr) = self.timer_manager.lock() {
            timer_mgr.load_boss_definitions(bosses);
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
    pub fn sync_timer_context(&self) {
        let Some(cache) = &self.session_cache else {
            return;
        };

        let area = &cache.current_area;
        if area.area_name.is_empty() {
            return;
        }

        let difficulty = crate::game_data::Difficulty::from_game_string(&area.difficulty_name);
        let area_id = if area.area_id != 0 { Some(area.area_id) } else { None };

        if let Ok(mut timer_mgr) = self.timer_manager.lock() {
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
