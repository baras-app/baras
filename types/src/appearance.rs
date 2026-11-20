//! Colours, class colours and shared overlay appearance

use crate::serde_defaults::default_true;
use serde::{Deserialize, Serialize};

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
    pub const DPS: Color = [180, 50, 50, 255]; // Red
    pub const HPS: Color = [50, 180, 50, 255]; // Green
    pub const TPS: Color = [50, 100, 180, 255]; // Blue
    pub const DTPS: Color = [180, 80, 80, 255]; // Dark red
    pub const ABS: Color = [100, 150, 200, 255]; // Light blue
    pub const APM: Color = [140, 140, 140, 255]; // Grey
    pub const INTERRUPTS: Color = [180, 130, 50, 255]; // Amber
    pub const BOSS_BAR: Color = [200, 50, 50, 255]; // Boss health red
    pub const FRAME_BG: Color = [40, 40, 40, 200]; // Raid frame background

    /// Get the default bar color for an overlay type by its config key
    pub fn for_key(key: &str) -> Color {
        match key {
            "dps" | "edps" | "bossdps" => DPS,
            "hps" | "ehps" => HPS,
            "tps" => TPS,
            "dtps" | "edtps" | "incoming_damage" => DTPS,
            "abs" => HPS,
            "apm" => APM,
            "interrupts" => INTERRUPTS,
            _ => DPS,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Appearance Config
// ─────────────────────────────────────────────────────────────────────────────

/// Which icon to show next to player names in metric overlays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ClassIconMode {
    /// No icon
    None,
    /// Class silhouette icon (role-tinted)
    #[default]
    Class,
    /// Discipline-specific icon (full color)
    Discipline,
}

// ─────────────────────────────────────────────────────────────────────────────
// Class Color Config
// ─────────────────────────────────────────────────────────────────────────────

/// Per-archetype bar colors for metric overlays.
///
/// Each field covers a mirror-class pair (Imperial / Republic).
/// Used when `use_class_color` is enabled on a metric overlay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassColorConfig {
    /// Sorcerer / Sage
    #[serde(default = "default_class_color_sorcerer")]
    pub sorcerer_sage: Color,
    /// Assassin / Shadow
    #[serde(default = "default_class_color_assassin")]
    pub assassin_shadow: Color,
    /// Juggernaut / Guardian
    #[serde(default = "default_class_color_juggernaut")]
    pub juggernaut_guardian: Color,
    /// Marauder / Sentinel
    #[serde(default = "default_class_color_marauder")]
    pub marauder_sentinel: Color,
    /// Mercenary / Commando
    #[serde(default = "default_class_color_mercenary")]
    pub mercenary_commando: Color,
    /// Powertech / Vanguard
    #[serde(default = "default_class_color_powertech")]
    pub powertech_vanguard: Color,
    /// Operative / Scoundrel
    #[serde(default = "default_class_color_operative")]
    pub operative_scoundrel: Color,
    /// Sniper / Gunslinger
    #[serde(default = "default_class_color_sniper")]
    pub sniper_gunslinger: Color,
}

fn default_class_color_sorcerer() -> Color {
    [128, 64, 192, 255]
} // violet
fn default_class_color_assassin() -> Color {
    [90, 48, 128, 255]
} // dark purple
fn default_class_color_juggernaut() -> Color {
    [192, 48, 48, 255]
} // crimson
fn default_class_color_marauder() -> Color {
    [208, 80, 32, 255]
} // red-orange
fn default_class_color_mercenary() -> Color {
    [64, 128, 64, 255]
} // green
fn default_class_color_powertech() -> Color {
    [192, 104, 32, 255]
} // orange
fn default_class_color_operative() -> Color {
    [96, 120, 48, 255]
} // olive
fn default_class_color_sniper() -> Color {
    [192, 160, 32, 255]
} // gold

impl Default for ClassColorConfig {
    fn default() -> Self {
        Self {
            sorcerer_sage: default_class_color_sorcerer(),
            assassin_shadow: default_class_color_assassin(),
            juggernaut_guardian: default_class_color_juggernaut(),
            marauder_sentinel: default_class_color_marauder(),
            mercenary_commando: default_class_color_mercenary(),
            powertech_vanguard: default_class_color_powertech(),
            operative_scoundrel: default_class_color_operative(),
            sniper_gunslinger: default_class_color_sniper(),
        }
    }
}

impl ClassColorConfig {
    /// Look up the bar color for a class by its display name.
    ///
    /// Accepts both Imperial and Republic class names (e.g., "Sorcerer" or "Sage").
    /// Returns `None` if the name is unrecognized — callers should fall back to
    /// the configured bar color.
    pub fn for_class_name(&self, name: &str) -> Option<Color> {
        match name {
            "Sorcerer" | "Sage" => Some(self.sorcerer_sage),
            "Assassin" | "Shadow" => Some(self.assassin_shadow),
            "Juggernaut" | "Guardian" => Some(self.juggernaut_guardian),
            "Marauder" | "Sentinel" => Some(self.marauder_sentinel),
            "Mercenary" | "Commando" => Some(self.mercenary_commando),
            "Powertech" | "Vanguard" => Some(self.powertech_vanguard),
            "Operative" | "Scoundrel" => Some(self.operative_scoundrel),
            "Sniper" | "Gunslinger" => Some(self.sniper_gunslinger),
            _ => None,
        }
    }
}

/// Per-overlay appearance configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayAppearanceConfig {
    #[serde(default = "default_true")]
    pub show_header: bool,
    #[serde(default = "default_true")]
    pub show_footer: bool,
    #[serde(default = "default_true")]
    pub show_class_icons: bool,
    #[serde(default = "default_font_color")]
    pub font_color: Color,
    #[serde(default = "default_bar_color")]
    pub bar_color: Color,
    #[serde(default = "default_max_entries")]
    pub max_entries: u8,
    /// Text beside each player's name; see `metric_template`
    #[serde(default = "default_metric_template")]
    pub template: String,
    /// Pre-`template` column toggles; read once by `migrate_legacy`
    #[serde(default, skip_serializing, rename = "show_total")]
    pub legacy_show_total: Option<bool>,
    #[serde(default, skip_serializing, rename = "show_per_second")]
    pub legacy_show_per_second: Option<bool>,
    #[serde(default = "default_true")]
    pub show_percent: bool,
    #[serde(default = "default_true")]
    pub show_duration: bool,
    /// Color each player's bar using their class color (from global ClassColorConfig).
    /// Falls back to `bar_color` when class is unknown.
    #[serde(default)]
    pub use_class_color: bool,
    /// Fade each bar's fill from its base color (left) to a darkened version
    /// (right), spanning the filled portion. Details-style single-color gradient.
    /// Enabled by default for metric overlays.
    #[serde(default = "default_true")]
    pub bar_gradient: bool,
    /// Vertical gap between bars as a fraction of bar height (0.0 = connected).
    /// `None` follows `OverlaySettings::metric_bar_spacing`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bar_spacing: Option<f32>,
    /// Corner radius of each bar in pixels at base size (0 = square).
    /// `None` follows `OverlaySettings::metric_bar_radius`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bar_radius: Option<f32>,
    /// Pre-`bar_spacing` per-overlay value; read once by `migrate_legacy`
    #[serde(default, skip_serializing, rename = "bar_spacing_ratio")]
    pub legacy_bar_spacing: Option<f32>,
    #[serde(default, skip_serializing_if = "MetricOverrides::is_empty")]
    pub overrides: MetricOverrides,
}

/// One metric overlay's overrides of the global `OverlaySettings::metric_*`
/// options; `None` inherits. Resolved by `OverlaySettings::metric_options_for`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MetricOverrides {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_empty_bars: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_from_bottom: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scaling_factor: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class_icon_mode: Option<ClassIconMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_scale: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dynamic_background: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_background_bar: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gradient_intensity: Option<f32>,
}

impl MetricOverrides {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Fully resolved options one metric overlay draws with
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MetricOptions {
    pub show_empty_bars: bool,
    pub stack_from_bottom: bool,
    pub scaling_factor: f32,
    pub class_icon_mode: ClassIconMode,
    pub font_scale: f32,
    pub dynamic_background: bool,
    pub show_background_bar: bool,
    pub gradient_intensity: f32,
}

fn default_metric_template() -> String {
    "{rate}".to_string()
}

pub(crate) fn default_bar_spacing_ratio() -> f32 {
    0.2
}

pub const MAX_BAR_RADIUS: f32 = 16.0;

pub(crate) fn default_bar_radius() -> f32 {
    4.0
}

pub(crate) fn default_font_color() -> Color {
    overlay_colors::WHITE
}
fn default_bar_color() -> Color {
    overlay_colors::DPS
}
fn default_max_entries() -> u8 {
    16
}

impl Default for OverlayAppearanceConfig {
    fn default() -> Self {
        Self {
            show_header: true,
            show_footer: true,
            show_class_icons: true,
            font_color: overlay_colors::WHITE,
            bar_color: overlay_colors::DPS,
            max_entries: 16,
            template: default_metric_template(),
            legacy_show_total: None,
            legacy_show_per_second: None,
            show_percent: true,
            show_duration: true,
            use_class_color: false,
            bar_gradient: true,
            bar_spacing: None,
            bar_radius: None,
            legacy_bar_spacing: None,
            overrides: MetricOverrides::default(),
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
// Overlay Position
// ─────────────────────────────────────────────────────────────────────────────

/// Position configuration for an overlay
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
