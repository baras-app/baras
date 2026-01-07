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
mod matching;
mod preferences;
mod signal_handlers;

#[cfg(test)]
mod manager_tests;

pub use active::{ActiveTimer, TimerKey};
pub use definition::{TimerConfig, TimerDefinition, TimerTrigger};
pub use manager::{FiredAlert, TimerManager};
pub use preferences::{
    PreferencesError, TimerPreference, TimerPreferences, boss_timer_key, standalone_timer_key,
};

use std::path::Path;

/// Load timer definitions from a TOML file
pub fn load_timers_from_file(path: &Path) -> Result<Vec<TimerDefinition>, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read timer file {:?}: {}", path, e))?;

    let config: TimerConfig = toml::from_str(&content)
        .map_err(|e| format!("Failed to parse timer file {:?}: {}", path, e))?;

    Ok(config.timers)
}

/// Load all timer definitions from a directory (recursively)
pub fn load_timers_from_dir(dir: &Path) -> Result<Vec<TimerDefinition>, String> {
    let mut all_timers = Vec::new();

    if !dir.exists() {
        return Ok(all_timers); // Empty if directory doesn't exist
    }

    let entries = std::fs::read_dir(dir)
        .map_err(|e| format!("Failed to read timer directory {:?}: {}", dir, e))?;

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() {
            // Recurse into subdirectories
            match load_timers_from_dir(&path) {
                Ok(timers) => all_timers.extend(timers),
                Err(e) => eprintln!("Warning: {}", e),
            }
        } else if path.extension().is_some_and(|ext| ext == "toml") {
            match load_timers_from_file(&path) {
                Ok(timers) => {
                    eprintln!("Loaded {} timers from {:?}", timers.len(), path.file_name());
                    all_timers.extend(timers);
                }
                Err(e) => eprintln!("Warning: {}", e),
            }
        }
    }

    Ok(all_timers)
}
