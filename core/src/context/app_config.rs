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
    /// Show cumulative total value (e.g., total damage dealt)
    #[serde(default)]
    pub show_total: bool,
    /// Show per-second rate (e.g., DPS) - when both are enabled, rate is rightmost
    #[serde(default = "default_true")]
    pub show_per_second: bool,
}

fn default_true() -> bool { true }
fn default_font_color() -> Color { [255, 255, 255, 255] }  // White
fn default_bar_color() -> Color { [180, 50, 50, 255] }     // Red (DPS default)
fn default_max_entries() -> u8 { 16 }

// ─────────────────────────────────────────────────────────────────────────────
// Per-Overlay-Type Default Colors (Single Source of Truth)
// ─────────────────────────────────────────────────────────────────────────────

/// Default bar colors for each overlay type
pub mod overlay_colors {
    use super::Color;

    pub const DPS: Color = [180, 50, 50, 255];      // Red
    pub const EDPS: Color = [180, 50, 50, 255];     // Red (same as DPS)
    pub const BOSSDPS: Color = [180, 50, 50, 255];     // Red (same as DPS)
    pub const HPS: Color = [50, 180, 50, 255];      // Green
    pub const EHPS: Color = [50, 180, 50, 255];     // Green (same as HPS)
    pub const TPS: Color = [50, 100, 180, 255];     // Blue
    pub const DTPS: Color = [180, 80, 80, 255];     // Dark red
    pub const EDTPS: Color = [180, 80, 80, 255];    // Dark red (same as DTPS)
    pub const ABS: Color = [100, 150, 200, 255];    // Light blue

    /// Get the default bar color for an overlay type by its config key
    pub fn for_key(key: &str) -> Color {
        match key {
            "dps" => DPS,
            "edps" => EDPS,
            "bossdps" => EDPS,
            "hps" => HPS,
            "ehps" => EHPS,
            "tps" => TPS,
            "dtps" => DTPS,
            "edtps" => EDTPS,
            "abs" => ABS,
            _ => DPS, // Fallback to DPS color
        }
    }
}

impl OverlayAppearanceConfig {
    /// Get default appearance for an overlay type by its config key.
    /// Uses the correct bar color for each overlay type.
    pub fn default_for_type(key: &str) -> Self {
        Self {
            bar_color: overlay_colors::for_key(key),
            ..Self::default()
        }
    }
}

impl Default for OverlayAppearanceConfig {
    fn default() -> Self {
        Self {
            show_header: true,
            show_footer: true,
            show_class_icons: false,
            font_color: default_font_color(),
            bar_color: default_bar_color(),
            max_entries: 16,
            show_total: false,
            show_per_second: true,
        }
    }
}

/// Stats that can be displayed on the personal overlay
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PersonalStat {
    EncounterName,
    Difficulty,
    EncounterTime,
    EncounterCount,
    Apm,
    Dps,
    EDps,
    BossDps,
    TotalDamage,
    BossDamage,
    Hps,
    EHps,
    TotalHealing,
    Dtps,
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
    #[serde(default = "default_font_color")]
    pub label_color: Color,
}

fn default_personal_stats() -> Vec<PersonalStat> {
    vec![
        PersonalStat::EncounterName,
        PersonalStat::Difficulty,
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
            label_color: default_font_color(),
        }
    }
}

/// Position configuration for an overlay
///
/// Positions are stored relative to the monitor's top-left corner, allowing
/// overlays to appear in the same position on their target monitor even if
/// the monitor layout changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayPositionConfig {
    /// X offset from the monitor's left edge (relative position)
    pub x: i32,
    /// Y offset from the monitor's top edge (relative position)
    pub y: i32,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Monitor identifier - overlay will be placed on this monitor if available,
    /// otherwise falls back to primary monitor
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

// ─────────────────────────────────────────────────────────────────────────────
// Raid Overlay Settings
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration for the raid frame overlay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaidOverlaySettings {
    /// Grid columns (1, 2, or 4 - max 4)
    #[serde(default = "default_grid_columns")]
    pub grid_columns: u8,

    /// Grid rows (1, 2, 4, or 8 - max 8)
    #[serde(default = "default_grid_rows")]
    pub grid_rows: u8,

    /// Maximum effects shown per player frame
    #[serde(default = "default_max_effects")]
    pub max_effects_per_frame: u8,

    /// Size of effect indicators in pixels (8-24)
    #[serde(default = "default_effect_size")]
    pub effect_size: f32,

    /// Vertical offset for effect indicators
    #[serde(default = "default_effect_offset")]
    pub effect_vertical_offset: f32,

    /// Frame background color [r, g, b, a]
    #[serde(default = "default_frame_bg")]
    pub frame_bg_color: Color,

    /// Show role icons on frames
    #[serde(default = "default_true")]
    pub show_role_icons: bool,

    /// Effect indicator fill opacity (0-255)
    #[serde(default = "default_effect_fill_opacity")]
    pub effect_fill_opacity: u8,
}

fn default_grid_columns() -> u8 { 2 }
fn default_grid_rows() -> u8 { 4 }
fn default_max_effects() -> u8 { 4 }
fn default_effect_size() -> f32 { 14.0 }
fn default_effect_offset() -> f32 { 3.0 }
fn default_frame_bg() -> Color { [40, 40, 40, 200] }
fn default_effect_fill_opacity() -> u8 { 255 }

impl Default for RaidOverlaySettings {
    fn default() -> Self {
        Self {
            grid_columns: default_grid_columns(),
            grid_rows: default_grid_rows(),
            max_effects_per_frame: default_max_effects(),
            effect_size: default_effect_size(),
            effect_vertical_offset: default_effect_offset(),
            frame_bg_color: default_frame_bg(),
            show_role_icons: true,
            effect_fill_opacity: default_effect_fill_opacity(),
        }
    }
}

impl RaidOverlaySettings {
    /// Validate that grid dimensions result in 4, 8, or 16 total slots
    pub fn is_valid_grid(&self) -> bool {
        let total = self.grid_columns as u16 * self.grid_rows as u16;
        matches!(total, 4 | 8 | 16)
    }

    /// Get total number of slots
    pub fn total_slots(&self) -> u8 {
        self.grid_columns * self.grid_rows
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Settings (combined)
// ─────────────────────────────────────────────────────────────────────────────

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

    // --- Category opacity settings ---

    /// Background opacity for metric overlays (DPS, HPS, TPS, etc.)
    /// 0 = fully transparent, 255 = fully opaque
    #[serde(default = "default_opacity")]
    pub metric_opacity: u8,
    /// Background opacity for personal overlay
    /// 0 = fully transparent, 255 = fully opaque
    #[serde(default = "default_opacity")]
    pub personal_opacity: u8,

    /// Global toggle for class icons (future use)
    #[serde(default)]
    pub class_icons_enabled: bool,

    /// Lag compensation offset in milliseconds for effect countdowns.
    /// Adjusts for log I/O and processing latency.
    /// Positive = countdown ends earlier, Negative = countdown ends later.
    #[serde(default = "default_lag_offset")]
    pub effect_lag_offset_ms: i32,

    /// Default appearance configs per overlay type (not persisted, populated by backend)
    /// Used by frontend for "Reset to Default" functionality
    #[serde(default, skip_deserializing)]
    pub default_appearances: HashMap<String, OverlayAppearanceConfig>,

    // --- Raid overlay ---

    /// Raid frame overlay configuration
    #[serde(default)]
    pub raid_overlay: RaidOverlaySettings,

    /// Background opacity for raid frame overlay (0-255)
    #[serde(default = "default_opacity")]
    pub raid_opacity: u8,
}

fn default_visible() -> bool { true }
fn default_opacity() -> u8 { 180 }
fn default_lag_offset() -> i32 { 750 }

impl Default for OverlaySettings {
    fn default() -> Self {
        Self {
            positions: HashMap::new(),
            appearances: HashMap::new(),
            enabled: HashMap::new(),
            overlays_visible: true,
            personal_overlay: PersonalOverlayConfig::default(),
            metric_opacity: default_opacity(),
            personal_opacity: default_opacity(),
            class_icons_enabled: false,
            effect_lag_offset_ms: default_lag_offset(),
            default_appearances: HashMap::new(),
            raid_overlay: RaidOverlaySettings::default(),
            raid_opacity: default_opacity(),
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
// Hotkey Settings
// ─────────────────────────────────────────────────────────────────────────────

/// Global hotkey configuration
/// Hotkeys use Tauri's shortcut format (e.g., "CommandOrControl+Shift+O")
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeySettings {
    /// Toggle all overlays visible/hidden
    #[serde(default)]
    pub toggle_visibility: Option<String>,
    /// Toggle move/resize mode
    #[serde(default)]
    pub toggle_move_mode: Option<String>,
    /// Toggle raid frame rearrange mode
    #[serde(default)]
    pub toggle_rearrange_mode: Option<String>,
}

impl Default for HotkeySettings {
    fn default() -> Self {
        Self {
            toggle_visibility: None,
            toggle_move_mode: None,
            toggle_rearrange_mode: None,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Profiles
// ─────────────────────────────────────────────────────────────────────────────

/// Maximum number of profiles a user can create
pub const MAX_PROFILES: usize = 12;

/// A named snapshot of all overlay settings.
/// Profiles allow users to quickly switch between different configurations
/// (e.g., healer setup vs DPS setup, 8-man vs 16-man raids).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayProfile {
    /// User-defined name for this profile
    pub name: String,
    /// Complete snapshot of overlay settings at time of save
    pub settings: OverlaySettings,
}

impl OverlayProfile {
    /// Create a new profile with the given name and settings
    pub fn new(name: String, settings: OverlaySettings) -> Self {
        Self { name, settings }
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
    /// Global hotkey bindings
    #[serde(default)]
    pub hotkeys: HotkeySettings,
    /// Saved overlay profiles (max 12)
    #[serde(default)]
    pub profiles: Vec<OverlayProfile>,
    /// Name of the currently active profile, if any.
    /// When None, the user is working with unsaved settings.
    #[serde(default)]
    pub active_profile_name: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            log_directory: default_log_directory(),
            auto_delete_empty_files: false,
            log_retention_days: 21,
            overlay_settings: OverlaySettings::default(),
            hotkeys: HotkeySettings::default(),
            profiles: Vec::new(),
            active_profile_name: None,
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

    // ─────────────────────────────────────────────────────────────────────────
    // Profile Management
    // ─────────────────────────────────────────────────────────────────────────

    /// Save current overlay settings as a new profile with the given name.
    /// If a profile with that name exists, it will be updated.
    /// Returns Err if max profiles reached and name doesn't exist.
    pub fn save_profile(&mut self, name: String) -> Result<(), &'static str> {
        // Check if profile already exists (update case)
        if let Some(profile) = self.profiles.iter_mut().find(|p| p.name == name) {
            profile.settings = self.overlay_settings.clone();
            self.active_profile_name = Some(name);
            return Ok(());
        }

        // New profile - check limit
        if self.profiles.len() >= MAX_PROFILES {
            return Err("Maximum number of profiles reached (12)");
        }

        self.profiles.push(OverlayProfile::new(name.clone(), self.overlay_settings.clone()));
        self.active_profile_name = Some(name);
        Ok(())
    }

    /// Load a profile by name, replacing current overlay settings.
    /// Returns Err if profile not found.
    pub fn load_profile(&mut self, name: &str) -> Result<(), &'static str> {
        let profile = self.profiles.iter().find(|p| p.name == name)
            .ok_or("Profile not found")?;
        self.overlay_settings = profile.settings.clone();
        self.active_profile_name = Some(name.to_string());
        Ok(())
    }

    /// Delete a profile by name. Returns Err if profile not found.
    /// If the deleted profile was active, active_profile_name becomes None.
    pub fn delete_profile(&mut self, name: &str) -> Result<(), &'static str> {
        let len_before = self.profiles.len();
        self.profiles.retain(|p| p.name != name);
        if self.profiles.len() == len_before {
            return Err("Profile not found");
        }
        if self.active_profile_name.as_deref() == Some(name) {
            self.active_profile_name = None;
        }
        Ok(())
    }

    /// Rename a profile. Returns Err if old name not found or new name already exists.
    pub fn rename_profile(&mut self, old_name: &str, new_name: String) -> Result<(), &'static str> {
        // Check new name doesn't already exist
        if self.profiles.iter().any(|p| p.name == new_name) {
            return Err("A profile with that name already exists");
        }

        let profile = self.profiles.iter_mut().find(|p| p.name == old_name)
            .ok_or("Profile not found")?;
        profile.name = new_name.clone();

        if self.active_profile_name.as_deref() == Some(old_name) {
            self.active_profile_name = Some(new_name);
        }
        Ok(())
    }

    /// Get list of profile names
    pub fn profile_names(&self) -> Vec<String> {
        self.profiles.iter().map(|p| p.name.clone()).collect()
    }

    /// Check if a profile name is available
    pub fn is_profile_name_available(&self, name: &str) -> bool {
        !self.profiles.iter().any(|p| p.name == name)
    }
}
