//! Effect tracking system
//!
//! This module provides:
//! - **Definitions**: Templates that describe what effects to track (loaded from TOML)
//! - **Active instances**: Runtime state of currently active effects
//! - **Tracker**: Signal handler that manages effect lifecycle
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                     Definition (TOML config)                     │
//! │  "Track effect ID 814832605462528 as 'Kolto Probe', green, 20s" │
//! └─────────────────────────────────────────────────────────────────┘
//!                              │
//!                    GameSignal::EffectApplied
//!                              │
//!                              ▼
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                   ActiveEffect (runtime state)                   │
//! │  "Player 'Tank' has Kolto Probe, applied 3s ago, 2 stacks"      │
//! └─────────────────────────────────────────────────────────────────┘
//!                              │
//!                              ▼
//!                     Overlay Renderer
//! ```

mod active;
mod alacrity;
mod alerts;
mod audio;
mod definition;
mod lifecycle;
mod modifier_match;
pub mod tracker;

#[cfg(test)]
mod tracker_tests;

pub use active::{ActiveEffect, EffectKey};
pub use audio::EffectAudio;
pub use definition::{
    AbilitySelector, AlertTrigger, DefinitionConfig, DisplayTarget, EFFECTS_DSL_VERSION, EffectDefinition,
    EffectSelector, EntityFilter, RefreshAbility, RefreshScope, RefreshTrigger,
};
pub use lifecycle::Lifecycle;
pub use tracker::{DefinitionSet, EffectTracker, NewTargetInfo};
