pub mod handler;
pub mod processor;
pub mod signal;

// Refactored modules for processor logic
mod challenge;
mod combat_state;
mod counter;
mod phase;

#[cfg(test)]
mod processor_tests;

pub use counter::check_counter_timer_triggers;
pub use handler::SignalHandler;
pub use processor::EventProcessor;
pub use signal::GameSignal;
