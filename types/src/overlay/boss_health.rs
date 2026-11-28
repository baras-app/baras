//! Boss health overlay

use crate::appearance::default_font_color;
use crate::overlay::timers::default_overlay_border_color;
use crate::serde_defaults::{default_scaling_factor, default_true};
use crate::{Color, overlay_colors};
use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// Boss Health Settings
// ─────────────────────────────────────────────────────────────────────────────

/// Which side of each boss HP bar the effect-icon strip occupies. Space for
/// the strip is always reserved while icons are enabled, so effects landing
/// mid-fight never resize the layout. Beside the bar (left/right) horizontal
/// space is scarce, so only a few icon slots are reserved there.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IconPosition {
    Top,
    #[default]
    Bottom,
    Left,
    Right,
}

/// Configuration for the boss health bar overlay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BossHealthConfig {
    #[serde(default = "default_boss_bar_color")]
    pub bar_color: Color,
    #[serde(default = "default_font_color")]
    pub font_color: Color,
    #[serde(default = "default_true")]
    pub show_percent: bool,
    #[serde(default = "default_true")]
    pub show_target: bool,
    /// When true, show the current HP value (e.g. "1.90M") inline on the bar
    #[serde(default = "default_true")]
    pub show_hp_value: bool,
    /// When true, show phase HP markers (line through the bar + gutter label)
    #[serde(default = "default_true")]
    pub show_hp_markers: bool,
    /// When true, show active shields (gutter fill + remaining amount)
    #[serde(default = "default_true")]
    pub show_shield: bool,
    /// When true, show boss-effect icons; their strip's space is always
    /// reserved so entries never resize when effects land
    #[serde(default)]
    pub show_icons: bool,
    /// Which side of the bar the effect-icon strip occupies
    #[serde(default)]
    pub icon_position: IconPosition,
    /// Font scale multiplier (0.3 - 3.0, default 1.0)
    #[serde(default = "default_scaling_factor")]
    pub font_scale: f32,
    /// Scale multiplier for the lower (gutter) bar: scales its text and,
    /// more gently, the bar height itself (0.5 - 2.0)
    #[serde(default = "default_scaling_factor", alias = "target_scale")]
    pub lower_bar_scale: f32,
    /// Scale multiplier for the boss-effect icons (0.5 - 2.0)
    #[serde(default = "default_scaling_factor")]
    pub icon_scale: f32,
    /// How many bosses the window is sized for (1 - 7): entries render at
    /// window_height / visible_bosses and only compress when more bosses
    /// than this are alive
    #[serde(default = "default_visible_bosses")]
    pub visible_bosses: u32,
    /// When true, background shrinks to fit content instead of filling the window
    #[serde(default)]
    pub dynamic_background: bool,
    /// When true (default), boss health clears after combat ends
    #[serde(default = "default_true")]
    pub clear_after_combat: bool,
    /// When true, draw an outline around each boss HP bar
    #[serde(default = "default_true")]
    pub show_border: bool,
    /// Color of the per-HP-bar border outline
    #[serde(default = "default_overlay_border_color")]
    pub border_color: Color,
    /// Fade each bar's fill from its color (left) to a darkened version (right).
    #[serde(default)]
    pub bar_gradient: bool,
}

fn default_boss_bar_color() -> Color {
    overlay_colors::BOSS_BAR
}

fn default_visible_bosses() -> u32 {
    4
}

impl Default for BossHealthConfig {
    fn default() -> Self {
        Self {
            bar_color: overlay_colors::BOSS_BAR,
            font_color: overlay_colors::WHITE,
            show_percent: true,
            show_target: true,
            show_hp_value: true,
            show_hp_markers: true,
            show_shield: true,
            show_icons: false,
            icon_position: IconPosition::Bottom,
            font_scale: 1.0,
            lower_bar_scale: 1.0,
            icon_scale: 1.0,
            visible_bosses: 4,
            dynamic_background: false,
            clear_after_combat: true,
            show_border: true,
            border_color: default_overlay_border_color(),
            bar_gradient: false,
        }
    }
}
