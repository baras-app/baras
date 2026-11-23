//! Challenge overlay

use crate::appearance::{default_font_color, unmigrated_bar_gap};
use crate::serde_defaults::{default_scaling_factor, default_true};
use crate::{Color, overlay_colors};
use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// Challenge Overlay Configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Layout direction for challenge cards
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeLayout {
    /// Stack challenges vertically (default)
    #[default]
    Vertical,
    /// Arrange challenges horizontally
    Horizontal,
}

/// Column display mode for individual challenges
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeColumns {
    /// Show total value and percent
    TotalPercent,
    /// Show total value and per-second rate
    TotalPerSecond,
    /// Show per-second rate and percent (default)
    #[default]
    PerSecondPercent,
    /// Show only total value
    TotalOnly,
    /// Show only per-second rate
    PerSecondOnly,
    /// Show only percent
    PercentOnly,
}

/// Configuration for the challenge overlay (global settings)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeOverlayConfig {
    /// Font color for challenge text
    #[serde(default = "default_font_color")]
    pub font_color: Color,
    /// Default bar color for challenges (individual challenges may override)
    #[serde(default = "default_challenge_bar_color")]
    pub default_bar_color: Color,
    /// Show footer with totals
    #[serde(default = "default_true")]
    pub show_footer: bool,
    /// Show duration in header
    #[serde(default = "default_true")]
    pub show_duration: bool,
    /// Maximum challenges to display
    #[serde(default = "default_max_challenges")]
    pub max_display: u8,
    /// Layout direction for challenge cards
    #[serde(default)]
    pub layout: ChallengeLayout,
    /// When true, show grey background bar behind each player's fill bar
    #[serde(default)]
    pub show_background_bar: bool,
    /// Font scale multiplier (0.3 - 3.0, default 1.0)
    #[serde(default = "default_scaling_factor")]
    pub font_scale: f32,
    /// When true, background shrinks to fit content instead of filling the window
    #[serde(default)]
    pub dynamic_background: bool,
    /// Fade each bar's fill from its color (left) to a darkened version (right).
    /// Enabled by default for the challenges overlay.
    #[serde(default = "default_true")]
    pub bar_gradient: bool,
    /// Vertical gap between player bars in pixels, unscaled (0 = connected)
    #[serde(default = "unmigrated_bar_gap")]
    pub bar_gap: f32,
    /// Pre-`bar_gap` gap as a fraction of bar height; read once by `migrate_legacy`
    #[serde(default, skip_serializing, rename = "bar_spacing_ratio")]
    pub legacy_bar_spacing_ratio: Option<f32>,
    /// How many challenge cards the window is sized for. 0 = auto (cards
    /// fill the window). When set, each card takes 1/N of the window width
    /// (horizontal layout) or height (vertical) and only compresses when
    /// more than N challenges are visible.
    #[serde(default)]
    pub visible_challenges: u8,
    /// Anchor cards to the far edge of the stacking axis: right edge in
    /// horizontal layout, bottom edge in vertical.
    #[serde(default)]
    pub stack_from_end: bool,
}

fn default_challenge_bar_color() -> Color {
    overlay_colors::DPS
}
fn default_max_challenges() -> u8 {
    4
}

impl Default for ChallengeOverlayConfig {
    fn default() -> Self {
        Self {
            font_color: overlay_colors::WHITE,
            default_bar_color: overlay_colors::DPS,
            show_footer: true,
            show_duration: true,
            max_display: 4,
            layout: ChallengeLayout::Vertical,
            show_background_bar: false,
            font_scale: 1.0,
            dynamic_background: false,
            bar_gradient: true,
            bar_gap: 4.0,
            legacy_bar_spacing_ratio: None,
            visible_challenges: 0,
            stack_from_end: false,
        }
    }
}
