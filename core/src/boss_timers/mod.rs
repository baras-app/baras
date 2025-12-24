//! Boss encounter definitions with phases, counters, and timers
//!
//! This module provides a clean separation of concerns:
//! - **Encounter definitions**: Boss-specific phases and counters
//! - **Runtime state**: Tracks current phase, counter values, boss HP
//! - **Timer conditions**: Timers can filter by phase/counter state

mod definition;
mod loader;
mod state;

pub use definition::*;
pub use loader::*;
pub use state::*;
