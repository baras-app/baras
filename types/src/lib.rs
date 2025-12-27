//! Shared configuration types for BARAS
//!
//! This crate contains serializable configuration types that are shared between
//! the native backend (baras-core) and the WASM frontend (app-ui).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─────────────────────────────────────────────────────────────────────────────
// Color Type
// ─────────────────────────────────────────────────────────────────────────────

/// RGBA color as [r, g, b, a] bytes
pub type Color = [u8; 4];

// ─────────────────────────────────────────────────────────────────────────────
// Default Color Constants
// ─────────────────────────────────────────────────────────────────────────────

/// Default colors for overlay types
pub mod overlay_colors {
    use super::Color;

    pub const WHITE: Color = [255, 255, 255, 255];
    pub const DPS: Color = [180, 50, 50, 255];      // Red
    pub const HPS: Color = [50, 180, 50, 255];      // Green
    pub const TPS: Color = [50, 100, 180, 255];     // Blue
    pub const DTPS: Color = [180, 80, 80, 255];     // Dark red
    pub const ABS: Color = [100, 150, 200, 255];    // Light blue
    pub const BOSS_BAR: Color = [200, 50, 50, 255]; // Boss health red
    pub const FRAME_BG: Color = [40, 40, 40, 200];  // Raid frame background

    /// Get the default bar color for an overlay type by its config key
    pub fn for_key(key: &str) -> Color {
        match key {
            "dps" | "edps" | "bossdps" => DPS,
            "hps" | "ehps" => HPS,
            "tps" => TPS,
            "dtps" | "edtps" => DTPS,
            "abs" => ABS,
            _ => DPS,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Serde Default Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn default_true() -> bool { true }
fn default_opacity() -> u8 { 180 }
fn default_lag_offset() -> i32 { 750 }

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Appearance Config
// ─────────────────────────────────────────────────────────────────────────────

/// Per-overlay appearance configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayAppearanceConfig {
    #[serde(default = "default_true")]
    pub show_header: bool,
    #[serde(default = "default_true")]
    pub show_footer: bool,
    #[serde(default)]
    pub show_class_icons: bool,
    #[serde(default = "default_font_color")]
    pub font_color: Color,
    #[serde(default = "default_bar_color")]
    pub bar_color: Color,
    #[serde(default = "default_max_entries")]
    pub max_entries: u8,
    #[serde(default)]
    pub show_total: bool,
    #[serde(default = "default_true")]
    pub show_per_second: bool,
}

fn default_font_color() -> Color { overlay_colors::WHITE }
fn default_bar_color() -> Color { overlay_colors::DPS }
fn default_max_entries() -> u8 { 16 }

impl Default for OverlayAppearanceConfig {
    fn default() -> Self {
        Self {
            show_header: true,
            show_footer: true,
            show_class_icons: false,
            font_color: overlay_colors::WHITE,
            bar_color: overlay_colors::DPS,
            max_entries: 16,
            show_total: false,
            show_per_second: true,
        }
    }
}

impl OverlayAppearanceConfig {
    /// Get default appearance for an overlay type by its config key.
    pub fn default_for_type(key: &str) -> Self {
        Self {
            bar_color: overlay_colors::for_key(key),
            ..Self::default()
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Personal Stats
// ─────────────────────────────────────────────────────────────────────────────

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
    /// Current boss phase (if any)
    Phase,
    /// Time in current phase
    PhaseTime,
}

impl PersonalStat {
    /// Get the display label for this stat
    pub fn label(&self) -> &'static str {
        match self {
            Self::EncounterName => "Encounter Name",
            Self::Difficulty => "Difficulty",
            Self::EncounterTime => "Duration",
            Self::EncounterCount => "Encounter",
            Self::Apm => "APM",
            Self::Dps => "DPS",
            Self::EDps => "eDPS",
            Self::BossDps => "Boss DPS",
            Self::BossDamage => "Boss Damage",
            Self::TotalDamage => "Total Damage",
            Self::Hps => "HPS",
            Self::EHps => "eHPS",
            Self::TotalHealing => "Total Healing",
            Self::Dtps => "eDTPS",
            Self::Tps => "TPS",
            Self::TotalThreat => "Total Threat",
            Self::DamageCritPct => "Dmg Crit %",
            Self::HealCritPct => "Heal Crit %",
            Self::EffectiveHealPct => "Eff Heal %",
            Self::ClassDiscipline => "Spec",
            Self::Phase => "Phase",
            Self::PhaseTime => "Phase Time",
        }
    }

    /// Get all stats in display order
    pub fn all() -> &'static [PersonalStat] {
        &[
            Self::EncounterName,
            Self::Difficulty,
            Self::EncounterTime,
            Self::EncounterCount,
            Self::ClassDiscipline,
            Self::Apm,
            Self::Dps,
            Self::EDps,
            Self::BossDamage,
            Self::BossDps,
            Self::TotalDamage,
            Self::Hps,
            Self::EHps,
            Self::TotalHealing,
            Self::Dtps,
            Self::Tps,
            Self::TotalThreat,
            Self::DamageCritPct,
            Self::HealCritPct,
            Self::EffectiveHealPct,
            Self::Phase,
            Self::PhaseTime,
        ]
    }
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
            font_color: overlay_colors::WHITE,
            label_color: overlay_colors::WHITE,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Position
// ─────────────────────────────────────────────────────────────────────────────

/// Position configuration for an overlay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayPositionConfig {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
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
    #[serde(default = "default_grid_columns")]
    pub grid_columns: u8,
    #[serde(default = "default_grid_rows")]
    pub grid_rows: u8,
    #[serde(default = "default_max_effects")]
    pub max_effects_per_frame: u8,
    #[serde(default = "default_effect_size")]
    pub effect_size: f32,
    #[serde(default = "default_effect_offset")]
    pub effect_vertical_offset: f32,
    #[serde(default = "default_frame_bg")]
    pub frame_bg_color: Color,
    #[serde(default = "default_true")]
    pub show_role_icons: bool,
    #[serde(default = "default_effect_fill_opacity")]
    pub effect_fill_opacity: u8,
}

fn default_grid_columns() -> u8 { 2 }
fn default_grid_rows() -> u8 { 4 }
fn default_max_effects() -> u8 { 4 }
fn default_effect_size() -> f32 { 14.0 }
fn default_effect_offset() -> f32 { 3.0 }
fn default_frame_bg() -> Color { overlay_colors::FRAME_BG }
fn default_effect_fill_opacity() -> u8 { 255 }

impl Default for RaidOverlaySettings {
    fn default() -> Self {
        Self {
            grid_columns: 2,
            grid_rows: 4,
            max_effects_per_frame: 4,
            effect_size: 14.0,
            effect_vertical_offset: 3.0,
            frame_bg_color: overlay_colors::FRAME_BG,
            show_role_icons: true,
            effect_fill_opacity: 255,
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
// Boss Health Settings
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration for the boss health bar overlay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BossHealthConfig {
    #[serde(default = "default_boss_bar_color")]
    pub bar_color: Color,
    #[serde(default = "default_font_color")]
    pub font_color: Color,
    #[serde(default = "default_true")]
    pub show_percent: bool,
}

fn default_boss_bar_color() -> Color { overlay_colors::BOSS_BAR }

impl Default for BossHealthConfig {
    fn default() -> Self {
        Self {
            bar_color: overlay_colors::BOSS_BAR,
            font_color: overlay_colors::WHITE,
            show_percent: true,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Timer Overlay Configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration for the timer bar overlay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimerOverlayConfig {
    /// Default bar color for timers (individual timers may override)
    #[serde(default = "default_timer_bar_color")]
    pub default_bar_color: Color,
    /// Font color for timer text
    #[serde(default = "default_font_color")]
    pub font_color: Color,
    /// Maximum number of timers to display
    #[serde(default = "default_max_timers")]
    pub max_display: u8,
    /// Sort by remaining time (vs. activation order)
    #[serde(default = "default_true")]
    pub sort_by_remaining: bool,
}

fn default_timer_bar_color() -> Color { [100, 180, 220, 255] }
fn default_max_timers() -> u8 { 10 }

impl Default for TimerOverlayConfig {
    fn default() -> Self {
        Self {
            default_bar_color: default_timer_bar_color(),
            font_color: overlay_colors::WHITE,
            max_display: 10,
            sort_by_remaining: true,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Hotkey Settings
// ─────────────────────────────────────────────────────────────────────────────

/// Global hotkey configuration
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct HotkeySettings {
    #[serde(default)]
    pub toggle_visibility: Option<String>,
    #[serde(default)]
    pub toggle_move_mode: Option<String>,
    #[serde(default)]
    pub toggle_rearrange_mode: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Profiles
// ─────────────────────────────────────────────────────────────────────────────

/// Maximum number of profiles a user can create
pub const MAX_PROFILES: usize = 12;

/// A named snapshot of all overlay settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayProfile {
    pub name: String,
    pub settings: OverlaySettings,
}

impl OverlayProfile {
    pub fn new(name: String, settings: OverlaySettings) -> Self {
        Self { name, settings }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Settings (combined)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlaySettings {
    #[serde(default)]
    pub positions: HashMap<String, OverlayPositionConfig>,
    #[serde(default)]
    pub appearances: HashMap<String, OverlayAppearanceConfig>,
    #[serde(default, alias = "visibility")]
    pub enabled: HashMap<String, bool>,
    #[serde(default = "default_true")]
    pub overlays_visible: bool,
    #[serde(default)]
    pub personal_overlay: PersonalOverlayConfig,
    #[serde(default = "default_opacity")]
    pub metric_opacity: u8,
    #[serde(default = "default_opacity")]
    pub personal_opacity: u8,
    #[serde(default)]
    pub class_icons_enabled: bool,
    #[serde(default = "default_lag_offset")]
    pub effect_lag_offset_ms: i32,
    #[serde(default, skip_deserializing)]
    pub default_appearances: HashMap<String, OverlayAppearanceConfig>,
    #[serde(default)]
    pub raid_overlay: RaidOverlaySettings,
    #[serde(default = "default_opacity")]
    pub raid_opacity: u8,
    #[serde(default)]
    pub boss_health: BossHealthConfig,
    #[serde(default = "default_opacity")]
    pub boss_health_opacity: u8,
    #[serde(default)]
    pub timer_overlay: TimerOverlayConfig,
    #[serde(default = "default_opacity")]
    pub timer_opacity: u8,
    #[serde(default)]
    pub effects_overlay: TimerOverlayConfig,
    #[serde(default = "default_opacity")]
    pub effects_opacity: u8,
}

impl Default for OverlaySettings {
    fn default() -> Self {
        Self {
            positions: HashMap::new(),
            appearances: HashMap::new(),
            enabled: HashMap::new(),
            overlays_visible: true,
            personal_overlay: PersonalOverlayConfig::default(),
            metric_opacity: 180,
            personal_opacity: 180,
            class_icons_enabled: false,
            effect_lag_offset_ms: 750,
            default_appearances: HashMap::new(),
            raid_overlay: RaidOverlaySettings::default(),
            raid_opacity: 180,
            boss_health: BossHealthConfig::default(),
            boss_health_opacity: 180,
            timer_overlay: TimerOverlayConfig::default(),
            timer_opacity: 180,
            effects_overlay: TimerOverlayConfig::default(),
            effects_opacity: 180,
        }
    }
}

impl OverlaySettings {
    pub fn get_position(&self, overlay_type: &str) -> OverlayPositionConfig {
        self.positions.get(overlay_type).cloned().unwrap_or_default()
    }

    pub fn set_position(&mut self, overlay_type: &str, config: OverlayPositionConfig) {
        self.positions.insert(overlay_type.to_string(), config);
    }

    pub fn get_appearance(&self, overlay_type: &str) -> OverlayAppearanceConfig {
        self.appearances.get(overlay_type).cloned().unwrap_or_default()
    }

    pub fn set_appearance(&mut self, overlay_type: &str, config: OverlayAppearanceConfig) {
        self.appearances.insert(overlay_type.to_string(), config);
    }

    pub fn is_enabled(&self, overlay_type: &str) -> bool {
        self.enabled.get(overlay_type).copied().unwrap_or(false)
    }

    pub fn set_enabled(&mut self, overlay_type: &str, enabled: bool) {
        self.enabled.insert(overlay_type.to_string(), enabled);
    }

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

/// Application configuration.
/// Parsely.io upload settings
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ParselySettings {
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub guild: String,
}

///
/// Note: Persistence methods (load/save) are provided by baras-core via the
/// `AppConfigExt` trait, as they require platform-specific dependencies.
/// The frontend derives Default (getting empty values) which is fine for deserialization.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub log_directory: String,
    #[serde(default)]
    pub auto_delete_empty_files: bool,
    #[serde(default)]
    pub auto_delete_old_files: bool,
    #[serde(default = "default_retention_days")]
    pub log_retention_days: u32,
    #[serde(default = "default_true")]
    pub minimize_to_tray: bool,
    #[serde(default)]
    pub overlay_settings: OverlaySettings,
    #[serde(default)]
    pub hotkeys: HotkeySettings,
    #[serde(default)]
    pub profiles: Vec<OverlayProfile>,
    #[serde(default)]
    pub active_profile_name: Option<String>,
    #[serde(default)]
    pub parsely: ParselySettings,
}

fn default_retention_days() -> u32 { 21 }

impl AppConfig {
    /// Create a new AppConfig with the specified log directory.
    /// Other fields use their default values.
    pub fn with_log_directory(log_directory: String) -> Self {
        Self {
            log_directory,
            auto_delete_empty_files: false,
            auto_delete_old_files: false,
            log_retention_days: 21,
            minimize_to_tray: false,
            overlay_settings: OverlaySettings::default(),
            hotkeys: HotkeySettings::default(),
            profiles: Vec::new(),
            active_profile_name: None,
            parsely: ParselySettings::default(),
        }
    }
}
