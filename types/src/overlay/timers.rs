//! Timer and alerts overlays

use crate::appearance::default_font_color;
use crate::serde_defaults::{default_scaling_factor, default_true};
use crate::{Color, overlay_colors};
use serde::{Deserialize, Serialize};

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
    /// Font scale multiplier (0.3 - 3.0, default 1.0)
    #[serde(default = "default_scaling_factor")]
    pub font_scale: f32,
    /// When true, background shrinks to fit content instead of filling the window
    #[serde(default)]
    pub dynamic_background: bool,
    /// When true, entries stack from the bottom of the overlay window
    #[serde(default)]
    pub stack_from_bottom: bool,
    /// When true, draw an outline around each timer entry
    #[serde(default = "default_true")]
    pub show_border: bool,
    /// Color of the per-entry border outline
    #[serde(default = "default_timer_border_color")]
    pub border_color: Color,
    /// Fade each bar's fill from its color (left) to a darkened version (right).
    #[serde(default)]
    pub bar_gradient: bool,
}

fn default_timer_bar_color() -> Color {
    [100, 180, 220, 255]
}
fn default_max_timers() -> u8 {
    10
}
fn default_timer_border_color() -> Color {
    default_overlay_border_color()
}

pub(crate) fn default_overlay_border_color() -> Color {
    [128, 128, 128, 255]
}

impl Default for TimerOverlayConfig {
    fn default() -> Self {
        Self {
            default_bar_color: default_timer_bar_color(),
            font_color: overlay_colors::WHITE,
            max_display: 10,
            sort_by_remaining: true,
            font_scale: 1.0,
            dynamic_background: false,
            stack_from_bottom: false,
            show_border: true,
            border_color: default_timer_border_color(),
            bar_gradient: false,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Alerts Overlay Configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration for the alerts text overlay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertsOverlayConfig {
    /// Font size for alert text (default 12)
    #[serde(default = "default_alerts_font_size")]
    pub font_size: u8,
    /// Maximum number of alerts to display at once
    #[serde(default = "default_alerts_max_display")]
    pub max_display: u8,
    /// Seconds to show each alert at full opacity
    #[serde(default = "default_alerts_duration")]
    pub default_duration: f32,
    /// Seconds for fade-out effect after duration expires
    #[serde(default = "default_alerts_fade_duration")]
    pub fade_duration: f32,
    /// Show ability icon to the left of alert text (default true)
    #[serde(default = "default_true")]
    pub show_icons: bool,
    /// When true, background shrinks to fit content instead of filling the window
    #[serde(default)]
    pub dynamic_background: bool,
}

fn default_alerts_font_size() -> u8 {
    12
}
fn default_alerts_max_display() -> u8 {
    5
}
fn default_alerts_duration() -> f32 {
    5.0
}
fn default_alerts_fade_duration() -> f32 {
    1.0
}

impl Default for AlertsOverlayConfig {
    fn default() -> Self {
        Self {
            font_size: default_alerts_font_size(),
            max_display: default_alerts_max_display(),
            default_duration: default_alerts_duration(),
            fade_duration: default_alerts_fade_duration(),
            show_icons: true,
            dynamic_background: false,
        }
    }
}
