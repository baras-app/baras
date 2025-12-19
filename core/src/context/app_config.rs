use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Position Config
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayPositionConfig {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    /// Monitor identifier - falls back to primary if unavailable
    pub monitor_id: Option<String>,
}

impl Default for OverlayPositionConfig {
    fn default() -> Self {
        Self {
            x: 50,
            y: 50,
            width: 280,
            height: 200,
            monitor_id: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlaySettings {
    /// Position configs keyed by overlay type name (e.g., "dps", "hps", "tps")
    #[serde(default)]
    pub positions: HashMap<String, OverlayPositionConfig>,
    /// Enabled state keyed by overlay type name (user preference, persisted)
    #[serde(default, alias = "visibility")]
    pub enabled: HashMap<String, bool>,
    /// Global visibility toggle - when false, all overlays are hidden
    #[serde(default = "default_visible")]
    pub overlays_visible: bool,
}

fn default_visible() -> bool {
    true
}

impl Default for OverlaySettings {
    fn default() -> Self {
        Self {
            positions: HashMap::new(),
            enabled: HashMap::new(),
            overlays_visible: true,
        }
    }
}

impl OverlaySettings {
    /// Get position for an overlay type, returning default if not set
    pub fn get_position(&self, overlay_type: &str) -> OverlayPositionConfig {
        self.positions.get(overlay_type).cloned().unwrap_or_default()
    }

    /// Set position for an overlay type
    pub fn set_position(&mut self, overlay_type: &str, config: OverlayPositionConfig) {
        self.positions.insert(overlay_type.to_string(), config);
    }

    /// Check if an overlay is enabled (defaults to false)
    pub fn is_enabled(&self, overlay_type: &str) -> bool {
        self.enabled.get(overlay_type).copied().unwrap_or(false)
    }

    /// Set enabled state for an overlay type
    pub fn set_enabled(&mut self, overlay_type: &str, enabled: bool) {
        self.enabled.insert(overlay_type.to_string(), enabled);
    }

    /// Get list of all enabled overlay type keys
    pub fn enabled_types(&self) -> Vec<String> {
        self.enabled
            .iter()
            .filter_map(|(k, &v)| if v { Some(k.clone()) } else { None })
            .collect()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// App Config
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone)]
pub struct AppConfig {
    pub log_directory: String,
    #[serde(default)]
    pub auto_delete_empty_files: bool,
    #[serde(default)]
    pub log_retention_days: u32,
    #[serde(default)]
    pub overlay_settings: OverlaySettings,
}

impl ::std::default::Default for AppConfig {
    fn default() -> Self {
        Self {
            log_directory: "/home/prescott/baras/test-log-files/".to_string(),
            auto_delete_empty_files: false,
            log_retention_days: 21,
            overlay_settings: OverlaySettings::default(),
        }
    }
}

impl AppConfig {
    pub fn load() -> Self {
        confy::load("baras", None).unwrap_or_default()
    }

    pub fn save(self) {
        confy::store("baras", None, self).expect("Failed to save configuration");
    }
}
