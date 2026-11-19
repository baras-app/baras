//! Timer preferences - user-specific overrides for timer presentation
//!
//! Preferences cover only personal toggles (visibility, role filter, audio
//! on/off). Material changes — color, audio file, anything that alters what
//! the timer *is* — live on the definition itself so they sync via
//! `_custom.toml` and round-trip through encounter export.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::game_data::Role;

// ═══════════════════════════════════════════════════════════════════════════
// Preference Types
// ═══════════════════════════════════════════════════════════════════════════

/// Individual phase preference overrides.
/// All fields are optional - only set fields override the definition.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PhasePreference {
    /// Override enabled state
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

impl PhasePreference {
    /// Check if this preference has any overrides set
    pub fn is_empty(&self) -> bool {
        self.enabled.is_none()
    }
}

/// Individual counter preference overrides.
/// All fields are optional - only set fields override the definition.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CounterPreference {
    /// Override enabled state
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

impl CounterPreference {
    /// Check if this preference has any overrides set
    pub fn is_empty(&self) -> bool {
        self.enabled.is_none()
    }
}

/// Individual timer preference overrides.
/// All fields are optional - only set fields override the definition.
///
/// Material fields (color, audio file, etc.) live on the definition, not here.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TimerPreference {
    /// Override enabled state
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,

    /// Override audio enabled state
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_enabled: Option<bool>,

    /// Role filter: which roles should see this timer
    /// None = all roles (default), Some([Tank]) = tank-only, etc.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roles: Option<Vec<Role>>,
}

impl TimerPreference {
    /// Check if this preference has any overrides set
    pub fn is_empty(&self) -> bool {
        self.enabled.is_none() && self.audio_enabled.is_none() && self.roles.is_none()
    }
}

/// Collection of timer preferences keyed by timer path.
///
/// Key format:
/// - Boss timers: `{area_name}.{boss_id}.{timer_id}` (e.g., `dxun.red.packmaster_leap`)
/// - Standalone timers: `{timer_id}` (e.g., `my_custom_timer`)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TimerPreferences {
    /// Timer preferences by key
    #[serde(default)]
    pub timers: HashMap<String, TimerPreference>,

    /// Phase preferences by key (same key format as timers: `area.boss.phase_id`)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub phases: HashMap<String, PhasePreference>,

    /// Counter preferences by key (same key format as timers: `area.boss.counter_id`)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub counters: HashMap<String, CounterPreference>,
}

impl TimerPreferences {
    /// Create empty preferences
    pub fn new() -> Self {
        Self::default()
    }

    /// Load preferences from a TOML file
    pub fn load(path: &Path) -> Result<Self, PreferencesError> {
        if !path.exists() {
            return Ok(Self::new());
        }

        let content = std::fs::read_to_string(path)
            .map_err(|e| PreferencesError::Io(path.to_path_buf(), e))?;

        toml::from_str(&content).map_err(|e| PreferencesError::Parse(path.to_path_buf(), e))
    }

    /// Save preferences to a TOML file
    pub fn save(&self, path: &Path) -> Result<(), PreferencesError> {
        // Clean up empty preferences before saving
        let cleaned = self.without_empty();

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| PreferencesError::Io(path.to_path_buf(), e))?;
        }

        let content =
            toml::to_string_pretty(&cleaned).map_err(PreferencesError::Serialize)?;

        std::fs::write(path, content).map_err(|e| PreferencesError::Io(path.to_path_buf(), e))
    }

    /// Get preference for a timer by key
    pub fn get(&self, key: &str) -> Option<&TimerPreference> {
        self.timers.get(key)
    }

    /// Set preference for a timer
    pub fn set(&mut self, key: String, pref: TimerPreference) {
        if pref.is_empty() {
            self.timers.remove(&key);
        } else {
            self.timers.insert(key, pref);
        }
    }

    /// Update a single field for a timer preference
    pub fn update_enabled(&mut self, key: &str, enabled: bool) {
        let pref = self.timers.entry(key.to_string()).or_default();
        pref.enabled = Some(enabled);
    }

    /// Update audio enabled for a timer
    pub fn update_audio_enabled(&mut self, key: &str, enabled: bool) {
        let pref = self.timers.entry(key.to_string()).or_default();
        pref.audio_enabled = Some(enabled);
    }

    /// Update role filter for a timer (None = all roles)
    pub fn update_roles(&mut self, key: &str, roles: Option<Vec<Role>>) {
        let pref = self.timers.entry(key.to_string()).or_default();
        pref.roles = roles;
    }

    /// Remove all overrides for a timer (reset to defaults)
    pub fn clear(&mut self, key: &str) {
        self.timers.remove(key);
    }

    // ─── Phase preference helpers ─────────────────────────────────────

    /// Get preference for a phase by key
    pub fn get_phase(&self, key: &str) -> Option<&PhasePreference> {
        self.phases.get(key)
    }

    /// Update enabled state for a phase
    pub fn update_phase_enabled(&mut self, key: &str, enabled: bool) {
        let pref = self.phases.entry(key.to_string()).or_default();
        pref.enabled = Some(enabled);
    }

    /// Remove all overrides for a phase (reset to defaults)
    pub fn clear_phase(&mut self, key: &str) {
        self.phases.remove(key);
    }

    // ─── Counter preference helpers ───────────────────────────────────

    /// Get preference for a counter by key
    pub fn get_counter(&self, key: &str) -> Option<&CounterPreference> {
        self.counters.get(key)
    }

    /// Update enabled state for a counter
    pub fn update_counter_enabled(&mut self, key: &str, enabled: bool) {
        let pref = self.counters.entry(key.to_string()).or_default();
        pref.enabled = Some(enabled);
    }

    /// Remove all overrides for a counter (reset to defaults)
    pub fn clear_counter(&mut self, key: &str) {
        self.counters.remove(key);
    }

    /// Return a copy with empty preferences removed
    fn without_empty(&self) -> Self {
        Self {
            timers: self
                .timers
                .iter()
                .filter(|(_, v)| !v.is_empty())
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            phases: self
                .phases
                .iter()
                .filter(|(_, v)| !v.is_empty())
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            counters: self
                .counters
                .iter()
                .filter(|(_, v)| !v.is_empty())
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Key Generation
// ═══════════════════════════════════════════════════════════════════════════

/// Generate a preference key for a boss timer
pub fn boss_timer_key(area_name: &str, boss_name: &str, timer_id: &str) -> String {
    format!(
        "{}.{}.{}",
        normalize_key_part(area_name),
        normalize_key_part(boss_name),
        normalize_key_part(timer_id)
    )
}

/// Generate a preference key for a standalone timer
pub fn standalone_timer_key(timer_id: &str) -> String {
    normalize_key_part(timer_id)
}

/// Generate a preference key for a boss phase
pub fn boss_phase_key(area_name: &str, boss_name: &str, phase_id: &str) -> String {
    format!(
        "{}.{}.{}",
        normalize_key_part(area_name),
        normalize_key_part(boss_name),
        normalize_key_part(phase_id)
    )
}

/// Generate a preference key for a boss counter
pub fn boss_counter_key(area_name: &str, boss_name: &str, counter_id: &str) -> String {
    format!(
        "{}.{}.{}",
        normalize_key_part(area_name),
        normalize_key_part(boss_name),
        normalize_key_part(counter_id)
    )
}

/// Normalize a key part: lowercase, replace spaces with underscores
fn normalize_key_part(s: &str) -> String {
    s.to_lowercase().replace(' ', "_")
}

use super::TimerDefinition;

impl TimerPreferences {
    /// Generate the preference key for a timer definition
    pub fn key_for_definition(def: &TimerDefinition) -> String {
        if let Some(ref boss_name) = def.boss {
            // Boss timer: use first encounter name (area_name) + boss + timer_id
            let area_name = def
                .encounters
                .first()
                .map(|s| s.as_str())
                .unwrap_or("unknown");
            boss_timer_key(area_name, boss_name, &def.id)
        } else {
            // Standalone timer: just timer_id
            standalone_timer_key(&def.id)
        }
    }

    /// Get effective enabled state for a timer (preference overrides definition)
    pub fn is_enabled(&self, def: &TimerDefinition) -> bool {
        // Checked per definition per signal: skip building the key when there is nothing to look up
        if self.timers.is_empty() {
            return def.enabled;
        }
        let key = Self::key_for_definition(def);
        self.timers
            .get(&key)
            .and_then(|p| p.enabled)
            .unwrap_or(def.enabled)
    }

    /// Get effective audio enabled state (preference overrides definition)
    pub fn is_audio_enabled(&self, def: &TimerDefinition) -> bool {
        let key = Self::key_for_definition(def);
        self.timers
            .get(&key)
            .and_then(|p| p.audio_enabled)
            .unwrap_or(def.audio.enabled)
    }

    /// Check if timer should be visible for the given role.
    /// None current_role = unknown role, shows all timers.
    /// None roles pref = all roles, always visible.
    pub fn is_role_visible(&self, def: &TimerDefinition, current_role: Option<Role>) -> bool {
        let Some(role) = current_role else {
            return true;
        };
        let key = Self::key_for_definition(def);
        match self.timers.get(&key).and_then(|p| p.roles.as_ref()) {
            None => true,
            Some(roles) => roles.contains(&role),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Error Types
// ═══════════════════════════════════════════════════════════════════════════

/// Errors during timer preferences operations
#[derive(Debug, Error)]
pub enum PreferencesError {
    #[error("IO error at {0}")]
    Io(PathBuf, #[source] std::io::Error),

    #[error("parse error in {0}")]
    Parse(PathBuf, #[source] toml::de::Error),

    #[error("serialization error")]
    Serialize(#[from] toml::ser::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boss_timer_key_format() {
        let key = boss_timer_key("Dxun", "red", "packmaster_leap");
        assert_eq!(key, "dxun.red.packmaster_leap");
    }

    #[test]
    fn boss_timer_key_normalizes_spaces() {
        let key = boss_timer_key("The Ravagers", "Master Blaster", "Ion Cutter");
        assert_eq!(key, "the_ravagers.master_blaster.ion_cutter");
    }

    #[test]
    fn standalone_timer_key_format() {
        let key = standalone_timer_key("my_custom_timer");
        assert_eq!(key, "my_custom_timer");
    }

    #[test]
    fn empty_preference_detection() {
        let pref = TimerPreference::default();
        assert!(pref.is_empty());

        let pref = TimerPreference {
            enabled: Some(false),
            ..Default::default()
        };
        assert!(!pref.is_empty());
    }

    #[test]
    fn preference_update_methods() {
        let mut prefs = TimerPreferences::new();

        prefs.update_enabled("test.timer", false);
        assert_eq!(prefs.get("test.timer").unwrap().enabled, Some(false));

        prefs.update_audio_enabled("test.timer", false);
        assert_eq!(
            prefs.get("test.timer").unwrap().audio_enabled,
            Some(false)
        );
    }

    #[test]
    fn clear_removes_preference() {
        let mut prefs = TimerPreferences::new();
        prefs.update_enabled("test.timer", false);
        assert!(prefs.get("test.timer").is_some());

        prefs.clear("test.timer");
        assert!(prefs.get("test.timer").is_none());
    }
}
