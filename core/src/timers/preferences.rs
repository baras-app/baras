//! Timer preferences - user-specific overrides for timer presentation
//!
//! Preferences are stored separately from definitions so users can:
//! - Toggle timers on/off without modifying definition files
//! - Customize colors and sounds
//! - Share definition files without personal settings mixed in

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

// ═══════════════════════════════════════════════════════════════════════════
// Preference Types
// ═══════════════════════════════════════════════════════════════════════════

/// Individual timer preference overrides.
/// All fields are optional - only set fields override the definition.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TimerPreference {
    /// Override enabled state
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,

    /// Override audio enabled state
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_enabled: Option<bool>,

    /// Override audio file path
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_file: Option<String>,

    /// Override display color [R, G, B, A]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<[u8; 4]>,
}

impl TimerPreference {
    /// Check if this preference has any overrides set
    pub fn is_empty(&self) -> bool {
        self.enabled.is_none()
            && self.audio_enabled.is_none()
            && self.audio_file.is_none()
            && self.color.is_none()
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
            toml::to_string_pretty(&cleaned).map_err(|e| PreferencesError::Serialize(e))?;

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

    /// Update audio file for a timer
    pub fn update_audio_file(&mut self, key: &str, file: Option<String>) {
        let pref = self.timers.entry(key.to_string()).or_default();
        pref.audio_file = file;
    }

    /// Update color for a timer
    pub fn update_color(&mut self, key: &str, color: [u8; 4]) {
        let pref = self.timers.entry(key.to_string()).or_default();
        pref.color = Some(color);
    }

    /// Remove all overrides for a timer (reset to defaults)
    pub fn clear(&mut self, key: &str) {
        self.timers.remove(key);
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
        let key = Self::key_for_definition(def);
        self.timers
            .get(&key)
            .and_then(|p| p.enabled)
            .unwrap_or(def.enabled)
    }

    /// Get effective color for a timer (preference overrides definition)
    pub fn get_color(&self, def: &TimerDefinition) -> [u8; 4] {
        let key = Self::key_for_definition(def);
        self.timers
            .get(&key)
            .and_then(|p| p.color)
            .unwrap_or(def.color)
    }

    /// Get effective audio enabled state (preference overrides definition)
    pub fn is_audio_enabled(&self, def: &TimerDefinition) -> bool {
        let key = Self::key_for_definition(def);
        self.timers
            .get(&key)
            .and_then(|p| p.audio_enabled)
            .unwrap_or(def.audio.enabled)
    }

    /// Get effective audio file (preference overrides definition)
    pub fn get_audio_file(&self, def: &TimerDefinition) -> Option<String> {
        let key = Self::key_for_definition(def);
        self.timers
            .get(&key)
            .and_then(|p| p.audio_file.clone())
            .or_else(|| def.audio.file.clone())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Error Types
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug)]
pub enum PreferencesError {
    Io(std::path::PathBuf, std::io::Error),
    Parse(std::path::PathBuf, toml::de::Error),
    Serialize(toml::ser::Error),
}

impl std::fmt::Display for PreferencesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(path, e) => write!(f, "IO error at {}: {}", path.display(), e),
            Self::Parse(path, e) => write!(f, "Parse error in {}: {}", path.display(), e),
            Self::Serialize(e) => write!(f, "Serialization error: {}", e),
        }
    }
}

impl std::error::Error for PreferencesError {}

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

        prefs.update_color("test.timer", [255, 0, 0, 255]);
        assert_eq!(
            prefs.get("test.timer").unwrap().color,
            Some([255, 0, 0, 255])
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
