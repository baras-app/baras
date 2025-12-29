//! Boss encounter system
//!
//! This module provides:
//! - **BossEncounterDefinition**: Static config loaded from TOML (phases, counters, timers, challenges)
//! - **BossEncounterState**: Runtime state during combat (current phase, HP, counter values)
//! - **ChallengeDefinition**: Flexible metric tracking for raid challenges
//!
//! Note: ChallengeTracker instances live on Encounter, but the type is defined here
//! alongside the ChallengeDefinition it consumes.

mod challenge;
mod counter;
mod definition;
mod loader;
mod phase;
mod state;

pub use challenge::*;
pub use counter::*;
pub use definition::*;
pub use loader::*;
pub use phase::*;
pub use state::*;
