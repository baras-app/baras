//! Boss encounter system
//!
//! This module provides:
//! - **BossEncounterDefinition**: Static config loaded from TOML (phases, counters, timers, challenges)
//! - **BossEncounterState**: Runtime state during combat (current phase, HP, counter values)
//! - **Challenge definitions**: Flexible metric tracking for raid challenges

mod definition;
mod loader;
mod state;

pub use definition::*;
pub use loader::*;
pub use state::*;
