//! Timer system
//!
//! This module provides:
//! - **Definitions**: Templates that describe timers (loaded from TOML)
//! - **Active instances**: Runtime state of currently running timers
//! - **Manager**: Signal handler that manages timer lifecycle
//!
//! # Timer Types
//!
//! Timers can be triggered by various game events:
//! - Combat start (boss enrage timers)
//! - Ability casts (cooldown tracking)
//! - Effect applications/removals
//! - Boss HP thresholds
//! - Other timers expiring (chaining)

mod active;
mod definition;
mod manager;

pub use active::{ActiveTimer, TimerKey};
pub use definition::{TimerDefinition, TimerTrigger};
pub use manager::TimerManager;
