//! Timer management handler
//!
//! Manages ability cooldown and buff timers.
//! Reacts to signals to start, pause, and reset timers.

use crate::events::{GameSignal, SignalHandler};

/// Manages ability cooldown and buff timers.
/// Reacts to signals to start, pause, and reset timers.
#[derive(Debug, Default)]
pub struct TimerManager {
    // TODO: Add timer definitions and active timers
}

impl TimerManager {
    pub fn new() -> Self {
        Self::default()
    }
}

impl SignalHandler for TimerManager {
    fn handle_signal(&mut self, signal: &GameSignal) {
        match signal {
            GameSignal::CombatEnded { .. } => {
                // TODO: Reset combat timers
            }
            GameSignal::EntityDeath { entity_id, .. } => {
                // TODO: Pause timers for dead entity
                let _ = entity_id;
            }
            GameSignal::EntityRevived { entity_id, .. } => {
                // TODO: Resume timers for revived entity
                let _ = entity_id;
            }
            GameSignal::AbilityActivated {
                ability_id,
                source_id,
                timestamp,
                ..
            } => {
                // TODO: Start timer if ability has an associated timer
                let _ = (ability_id, source_id, timestamp);
            }
            _ => {}
        }
    }

    fn on_encounter_start(&mut self, _encounter_id: u64) {
        // TODO: Reset encounter-scoped timers
    }

    fn on_encounter_end(&mut self, _encounter_id: u64) {
        // TODO: Cleanup encounter-scoped timers
    }
}
