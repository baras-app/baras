//! Boss encounter system
//!
//! This module provides:
//! - **BossEncounterDefinition**: Static config loaded from TOML (phases, counters, timers, challenges)
//! - **ChallengeDefinition**: Flexible metric tracking for raid challenges
//!
//! Note: Runtime boss state (phases, counters, HP) is now tracked in `CombatEncounter`
//! which consolidates all encounter-scoped state.

mod challenge;
mod counter;
mod definition;
mod loader;
mod phase;

pub use challenge::*;
pub use counter::*;
pub use definition::*;
pub use loader::*;
pub use phase::*;
