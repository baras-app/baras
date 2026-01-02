use super::signal::GameSignal;
use crate::encounter::CombatEncounter;

/// Trait for systems that react to game signals.
/// Implement this for timers, effect trackers, overlays, etc.
pub trait SignalHandler {
    /// Handle a single signal with optional encounter context.
    ///
    /// The encounter provides read access to phase, counters, HP, and combat time
    /// so handlers don't need to maintain duplicate state.
    fn handle_signal(&mut self, signal: &GameSignal, encounter: Option<&CombatEncounter>);

    /// Handle multiple signals (default implementation calls handle_signal for each)
    fn handle_signals(&mut self, signals: &[GameSignal], encounter: Option<&CombatEncounter>) {
        for signal in signals {
            self.handle_signal(signal, encounter);
        }
    }

    /// Called when a new encounter starts (optional hook for reset logic)
    fn on_encounter_start(&mut self, _encounter_id: u64) {}

    /// Called when an encounter ends (optional hook for cleanup)
    fn on_encounter_end(&mut self, _encounter_id: u64) {}
}
