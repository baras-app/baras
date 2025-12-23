use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::NaiveDateTime;
use tokio::sync::RwLock;

use crate::combat_log::{CombatEvent, Reader};
use crate::context::{AppConfig, parse_log_filename};
use crate::effects::{DefinitionSet, EffectTracker};
use crate::events::{EventProcessor, GameSignal, SignalHandler};
use crate::state::SessionCache;

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
            .and_then(|f| parse_log_filename(f))
            .map(|(_, dt)| dt);

        Self {
            current_byte: None,
            active_file: Some(path),
            game_session_date: date_stamp,
            session_cache: Some(SessionCache::new()),
            processor: EventProcessor::new(),
            signal_handlers: Vec::new(),
            effect_tracker: Arc::new(Mutex::new(EffectTracker::new(definitions))),
        }
    }

    /// Register a signal handler to receive game signals
    pub fn add_signal_handler(&mut self, handler: Box<dyn SignalHandler + Send + Sync>) {
        self.signal_handlers.push(handler);
    }

    /// Process a single event through the processor and dispatch signals
    pub fn process_event(&mut self, event: CombatEvent) {
        if let Some(cache) = &mut self.session_cache {
            let signals = self.processor.process_event(event, cache);
            self.dispatch_signals(&signals);
        }
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
    }

    /// Get a shared reference to the effect tracker for overlay queries.
    ///
    /// The returned Arc can be cloned and held by overlay code for periodic queries.
    /// Lock the mutex to access `active_effects()` or `effects_for_target()`.
    pub fn effect_tracker(&self) -> Arc<Mutex<EffectTracker>> {
        Arc::clone(&self.effect_tracker)
    }

    /// Tick the effect tracker to update expiration state and remove stale effects.
    ///
    /// Call this periodically (e.g., at overlay refresh rate ~10fps) to ensure
    /// duration-expired effects are cleaned up even without new events.
    pub fn tick(&self) {
        if let Ok(mut tracker) = self.effect_tracker.lock() {
            tracker.tick();
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
pub async fn parse_file(state: Arc<RwLock<ParsingSession>>) -> Result<ParseResult, String> {
    let timer = std::time::Instant::now();

    let active_path = {
        let s = state.read().await;
        s.active_file.clone().ok_or("invalid file given")?
    };

    let reader = Reader::from(active_path, Arc::clone(&state));

    let (events, end_pos) = reader
        .read_log_file()
        .await
        .map_err(|e| format!("failed to parse log file: {}", e))?;

    let events_count = events.len();
    let elapsed_ms = timer.elapsed().as_millis();

    {
        let mut s = state.write().await;
        s.current_byte = Some(end_pos);
        s.process_events(events);
    }

    Ok(ParseResult {
        events_count,
        elapsed_ms,
        reader,
        end_pos,
    })
}
