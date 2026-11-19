//! Effects A/B, cooldown tracker and DOT tracker overlays

use crate::appearance::default_font_color;
use crate::overlay::timers::default_overlay_border_color;
use crate::serde_defaults::{default_scaling_factor, default_true};
use crate::{Color, overlay_colors};
use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// Effects A/B Overlay Config (consolidated personal effects)
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration for Effects A overlay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectsAConfig {
    /// Icon size in pixels
    #[serde(default = "default_icon_size")]
    pub icon_size: u8,
    /// Maximum effects to display
    #[serde(default = "default_max_buffs")]
    pub max_display: u8,
    /// Use vertical layout (true) or horizontal (false)
    #[serde(default)]
    pub layout_vertical: bool,
    /// Render as stacked progress bars — overrides layout_vertical
    #[serde(default)]
    pub layout_bar: bool,
    /// Show effect names below/beside icons
    #[serde(default)]
    pub show_effect_names: bool,
    /// Show countdown text on icons
    #[serde(default = "default_true")]
    pub show_countdown: bool,
    /// When true, stacks are shown large and centered; timer is secondary
    #[serde(default)]
    pub stack_priority: bool,
    /// Show header title above overlay
    #[serde(default)]
    pub show_header: bool,
    /// Font scale multiplier (0.3 - 3.0, default 1.0)
    #[serde(default = "default_scaling_factor")]
    pub font_scale: f32,
    /// When true, background shrinks to fit content instead of filling the window
    #[serde(default)]
    pub dynamic_background: bool,
    /// When true, entries stack from the bottom of the overlay window
    #[serde(default)]
    pub stack_from_bottom: bool,
    /// When true (and in bar layout), draw an outline around each entry
    #[serde(default = "default_true")]
    pub show_border: bool,
    /// Color of the per-entry border outline (bar layout only)
    #[serde(default = "default_overlay_border_color")]
    pub border_color: Color,
    /// Fade each bar's fill from its color (left) to a darkened version (right).
    /// Bar layout only.
    #[serde(default)]
    pub bar_gradient: bool,
}

fn default_icon_size() -> u8 {
    32
}
fn default_max_buffs() -> u8 {
    8
}

impl Default for EffectsAConfig {
    fn default() -> Self {
        Self {
            icon_size: 32,
            max_display: 8,
            layout_vertical: false,
            layout_bar: false,
            show_effect_names: false,
            show_countdown: true,
            stack_priority: false,
            show_header: false,
            font_scale: 1.0,
            dynamic_background: false,
            stack_from_bottom: false,
            show_border: true,
            border_color: default_overlay_border_color(),
            bar_gradient: false,
        }
    }
}

/// Effects B/C overlays share the Effects A config shape
pub type EffectsBConfig = EffectsAConfig;

// Legacy aliases for backwards compatibility
pub type PersonalBuffsConfig = EffectsAConfig;
pub type PersonalDebuffsConfig = EffectsBConfig;

// ─────────────────────────────────────────────────────────────────────────────
// Cooldown Tracker Overlay Config
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration for the cooldown tracker overlay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CooldownTrackerConfig {
    /// Icon size in pixels
    #[serde(default = "default_icon_size")]
    pub icon_size: u8,
    /// Maximum cooldowns to display
    #[serde(default = "default_max_cooldowns")]
    pub max_display: u8,
    /// Show ability names
    #[serde(default = "default_true")]
    pub show_ability_names: bool,
    /// Sort by remaining time
    #[serde(default = "default_true")]
    pub sort_by_remaining: bool,
    /// Show header title above overlay
    #[serde(default)]
    pub show_header: bool,
    /// Font scale multiplier (0.3 - 3.0, default 1.0)
    #[serde(default = "default_scaling_factor")]
    pub font_scale: f32,
    /// When true, background shrinks to fit content instead of filling the window
    #[serde(default)]
    pub dynamic_background: bool,
    /// Render cooldowns as stacked progress bars instead of icons
    #[serde(default)]
    pub layout_bar: bool,
    /// When true, entries stack from the bottom of the overlay window
    #[serde(default)]
    pub stack_from_bottom: bool,
    /// When true (and in bar layout), draw an outline around each entry
    #[serde(default = "default_true")]
    pub show_border: bool,
    /// Color of the per-entry border outline (bar layout only)
    #[serde(default = "default_overlay_border_color")]
    pub border_color: Color,
    /// Fade each bar's fill from its color (left) to a darkened version (right).
    /// Bar layout only.
    #[serde(default)]
    pub bar_gradient: bool,
}

fn default_max_cooldowns() -> u8 {
    10
}

impl Default for CooldownTrackerConfig {
    fn default() -> Self {
        Self {
            icon_size: 32,
            max_display: 10,
            show_ability_names: true,
            sort_by_remaining: true,
            show_header: false,
            font_scale: 1.0,
            dynamic_background: false,
            layout_bar: false,
            stack_from_bottom: false,
            show_border: true,
            border_color: default_overlay_border_color(),
            bar_gradient: false,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// DOT Tracker Overlay Config
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration for the multi-target DOT tracker overlay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DotTrackerConfig {
    /// Maximum targets to track simultaneously
    #[serde(default = "default_max_targets")]
    pub max_targets: u8,
    /// Icon size in pixels
    #[serde(default = "default_small_icon")]
    pub icon_size: u8,
    /// Font color for target names
    #[serde(default = "default_font_color")]
    pub font_color: Color,
    /// Show DOT names in bar labels (bar layout only)
    #[serde(default)]
    pub show_effect_names: bool,
    /// Show source name (who applied)
    #[serde(default)]
    pub show_source_name: bool,
    /// Show header title above overlay
    #[serde(default)]
    pub show_header: bool,
    /// Show countdown timers on icons
    #[serde(default = "default_true")]
    pub show_countdown: bool,
    /// Font scale multiplier (0.3 - 3.0, default 1.0)
    #[serde(default = "default_scaling_factor")]
    pub font_scale: f32,
    /// When true, background shrinks to fit content instead of filling the window
    #[serde(default)]
    pub dynamic_background: bool,
    /// When true, entries stack from the bottom of the overlay window
    #[serde(default)]
    pub stack_from_bottom: bool,
    /// Flush DOT icons against the right edge of the window instead of
    /// starting them after the name column (icon layout only)
    #[serde(default)]
    pub right_align_icons: bool,
    /// Render DOTs as stacked progress bars grouped under target names
    #[serde(default)]
    pub layout_bar: bool,
    /// When true (and in bar layout), draw an outline around each entry
    #[serde(default = "default_true")]
    pub show_border: bool,
    /// Color of the per-entry border outline (bar layout only)
    #[serde(default = "default_overlay_border_color")]
    pub border_color: Color,
    /// Fade each bar's fill from its color (left) to a darkened version (right).
    /// Bar layout only.
    #[serde(default)]
    pub bar_gradient: bool,
}

fn default_max_targets() -> u8 {
    6
}
fn default_small_icon() -> u8 {
    20
}

impl Default for DotTrackerConfig {
    fn default() -> Self {
        Self {
            max_targets: 6,
            icon_size: 20,
            font_color: overlay_colors::WHITE,
            show_effect_names: false,
            show_source_name: false,
            show_header: false,
            show_countdown: true,
            font_scale: 1.0,
            dynamic_background: false,
            stack_from_bottom: false,
            right_align_icons: false,
            layout_bar: false,
            show_border: true,
            border_color: default_overlay_border_color(),
            bar_gradient: false,
        }
    }
}
