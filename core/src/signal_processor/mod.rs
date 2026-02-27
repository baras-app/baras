pub mod handler;
pub mod processor;
pub mod signal;
pub mod trigger_eval;

// Refactored modules for processor logic
mod challenge;
mod combat_state;
mod counter;
mod phase;

#[cfg(test)]
mod processor_tests;

pub use combat_state::tick_combat_state;
pub use counter::check_counter_timer_triggers;
pub use handler::SignalHandler;
pub use phase::check_timer_phase_transitions;
pub use processor::EventProcessor;
pub use signal::GameSignal;
pub use trigger_eval::FilterContext;
