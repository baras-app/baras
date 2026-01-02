//! Domain Specific Language definitions
//!
//! This module contains stateless definitions parsed from TOML config files.
//! These are the building blocks for defining boss encounters, timers, effects, etc.
//!
//! - **audio**: Audio configuration for timers/alerts
//! - **definition**: Boss encounter definitions (entities, phases, counters, timers, challenges)
//! - **challenge**: Challenge metric tracking definitions
//! - **counter**: Counter definitions for tracking occurrences
//! - **entity_filter**: Entity matching/filtering
//! - **loader**: TOML loading and saving
//! - **phase**: Phase definitions for boss encounters
//! - **triggers**: Unified trigger system
//!
//! Note: Runtime state (phases, counters, HP) is tracked in `CombatEncounter`
//! which consolidates all encounter-scoped state.

mod audio;
mod challenge;
mod counter;
mod definition;
mod entity_filter;
mod loader;
mod phase;
pub mod triggers;

pub use audio::*;
pub use challenge::*;
pub use counter::*;
pub use definition::*;
pub use entity_filter::*;
pub use loader::*;
pub use phase::*;
pub use triggers::*;
