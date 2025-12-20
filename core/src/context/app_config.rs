use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Config Types
// ─────────────────────────────────────────────────────────────────────────────

/// RGBA color as [r, g, b, a] bytes
pub type Color = [u8; 4];

/// Per-overlay appearance configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayAppearanceConfig {
    #[serde(default = "default_true")]
    pub show_header: bool,
    #[serde(default = "default_true")]
    pub show_footer: bool,
    #[serde(default)]
    pub show_class_icons: bool,  // Reserved for future use
    #[serde(default = "default_font_color")]
    pub font_color: Color,
    #[serde(default = "default_bar_color")]
    pub bar_color: Color,
    #[serde(default = "default_max_entries")]
    pub max_entries: u8,
}

fn default_true() -> bool { true }
fn default_font_color() -> Color { [255, 255, 255, 255] }  // White
fn default_bar_color() -> Color { [180, 50, 50, 255] }     // Red (DPS default)
fn default_max_entries() -> u8 { 8 }

impl Default for OverlayAppearanceConfig {
    fn default() -> Self {
        Self {
            show_header: true,
            show_footer: true,
            show_class_icons: false,
            font_color: default_font_color(),
            bar_color: default_bar_color(),
            max_entries: 8,
        }
    }
}

/// Stats that can be displayed on the personal overlay
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PersonalStat {
    EncounterTime,
    EncounterCount,
    Apm,
    Dps,
    EDps,
    TotalDamage,
    Hps,
    EHps,
    TotalHealing,
    Dtps,
    EDtps,
    Tps,
    TotalThreat,
    DamageCritPct,
    HealCritPct,
    EffectiveHealPct,
    ClassDiscipline,
}

/// Configuration for the personal stats overlay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonalOverlayConfig {
    #[serde(default = "default_personal_stats")]
    pub visible_stats: Vec<PersonalStat>,
    #[serde(default = "default_font_color")]
    pub font_color: Color,
}

fn default_personal_stats() -> Vec<PersonalStat> {
    vec![
        PersonalStat::EncounterTime,
        PersonalStat::Dps,
        PersonalStat::Hps,
        PersonalStat::Dtps,
        PersonalStat::Apm,
    ]
}

impl Default for PersonalOverlayConfig {
    fn default() -> Self {
        Self {
            visible_stats: default_personal_stats(),
            font_color: default_font_color(),
        }
    }
}

/// Position configuration for an overlay
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
    /// Appearance configs keyed by overlay type name
    #[serde(default)]
    pub appearances: HashMap<String, OverlayAppearanceConfig>,
    /// Enabled state keyed by overlay type name (user preference, persisted)
    #[serde(default, alias = "visibility")]
    pub enabled: HashMap<String, bool>,
    /// Global visibility toggle - when false, all overlays are hidden
    #[serde(default = "default_visible")]
    pub overlays_visible: bool,

    // --- Personal overlay ---

    #[serde(default)]
    pub personal_overlay: PersonalOverlayConfig,

    // --- Global settings (apply to all overlays) ---

    /// Background transparency (0 = fully transparent, 255 = fully opaque)
    #[serde(default = "default_background_alpha")]
    pub background_alpha: u8,
    /// Global toggle for class icons (future use)
    #[serde(default)]
    pub class_icons_enabled: bool,
}

fn default_visible() -> bool { true }
fn default_background_alpha() -> u8 { 180 }

impl Default for OverlaySettings {
    fn default() -> Self {
        Self {
            positions: HashMap::new(),
            appearances: HashMap::new(),
            enabled: HashMap::new(),
            overlays_visible: true,
            personal_overlay: PersonalOverlayConfig::default(),
            background_alpha: default_background_alpha(),
            class_icons_enabled: false,
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

    /// Get appearance for an overlay type, returning default if not set
    pub fn get_appearance(&self, overlay_type: &str) -> OverlayAppearanceConfig {
        self.appearances.get(overlay_type).cloned().unwrap_or_default()
    }

    /// Set appearance for an overlay type
    pub fn set_appearance(&mut self, overlay_type: &str, config: OverlayAppearanceConfig) {
        self.appearances.insert(overlay_type.to_string(), config);
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

fn default_log_directory() -> String {
    #[cfg(target_os = "windows")]
    {
        dirs::document_dir()
            .map(|p| p.join("Star Wars - The Old Republic/CombatLogs"))
            .and_then(|p| p.to_str().map(String::from))
            .unwrap_or_default()
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        dirs::home_dir()
            .map(|p| {
                p.join(".local/share/Steam/steamapps/compatdata/1286830/pfx/drive_c/users/steamuser/Documents/Star Wars - The Old Republic/CombatLogs")
            })
            .and_then(|p| p.to_str().map(String::from))
            .unwrap_or_default()
    }
    #[cfg(target_os = "macos")]
    {
        String::new()
    }
}

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

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            log_directory: default_log_directory(),
            auto_delete_empty_files: false,
            log_retention_days: 21,
            overlay_settings: OverlaySettings::default(),
        }
    }
}

impl AppConfig {
    pub fn load() -> Self {
        confy::load("baras", "config").unwrap_or_default()
    }

    pub fn save(self) {
        confy::store("baras", "config", self).expect("Failed to save configuration");
    }
}
