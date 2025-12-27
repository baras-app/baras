pub mod handler;
pub mod processor;
pub mod signal;

#[cfg(test)]
mod processor_tests;

pub use handler::SignalHandler;
pub use processor::EventProcessor;
pub use signal::GameSignal;
