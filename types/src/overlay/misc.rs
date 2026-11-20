//! Notes, combat time, ability cast, operation timer and ability queue overlays

use crate::appearance::default_font_color;
use crate::serde_defaults::{default_scaling_factor, default_true};
use crate::{Color, overlay_colors};
use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// Notes Overlay Configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration for the encounter notes overlay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotesOverlayConfig {
    /// Font size for notes text (default 14)
    #[serde(default = "default_notes_font_size")]
    pub font_size: u8,
    /// Font color for notes text
    #[serde(default = "default_font_color")]
    pub font_color: Color,
    #[serde(default)]
    pub dynamic_background: bool,
}

fn default_notes_font_size() -> u8 {
    14
}

impl Default for NotesOverlayConfig {
    fn default() -> Self {
        Self {
            font_size: default_notes_font_size(),
            font_color: overlay_colors::WHITE,
            dynamic_background: false,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Combat Time Overlay Configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration for the standalone combat time overlay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombatTimeOverlayConfig {
    /// Whether to show the "Combat Time" title and separator
    #[serde(default = "default_true")]
    pub show_title: bool,
    /// Font scale multiplier (0.3 - 3.0, default 1.0)
    #[serde(default = "default_scaling_factor")]
    pub font_scale: f32,
    /// Font color (RGBA)
    #[serde(default = "default_font_color")]
    pub font_color: Color,
    /// When true, background shrinks to fit content
    #[serde(default)]
    pub dynamic_background: bool,
    /// When true, overlay clears when combat ends; otherwise keeps last time
    #[serde(default = "default_true")]
    pub clear_after_combat: bool,
}

impl Default for CombatTimeOverlayConfig {
    fn default() -> Self {
        Self {
            show_title: true,
            font_scale: 1.0,
            font_color: overlay_colors::WHITE,
            dynamic_background: false,
            clear_after_combat: true,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Ability Cast Overlay Configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration for the recently-cast abilities overlay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbilityCastOverlayConfig {
    /// Maximum number of casts to display
    #[serde(default = "default_ability_cast_max_display")]
    pub max_display: u8,
    /// Seconds after which a cast is removed from the list
    #[serde(default = "default_ability_cast_prune_secs")]
    pub prune_secs: f32,
    /// Icon edge size in pixels (16 - 128)
    #[serde(default = "default_ability_cast_icon_size")]
    pub icon_size: u8,
    /// Font scale multiplier (0.3 - 3.0, default 1.0)
    #[serde(default = "default_scaling_factor")]
    pub font_scale: f32,
    /// Font color (RGBA)
    #[serde(default = "default_font_color")]
    pub font_color: Color,
    /// When true, background shrinks to fit content
    #[serde(default = "default_true")]
    pub dynamic_background: bool,
    /// When true, rows anchor to the bottom edge with the newest cast lowest
    #[serde(default)]
    pub stack_from_bottom: bool,
}

fn default_ability_cast_max_display() -> u8 {
    8
}
fn default_ability_cast_prune_secs() -> f32 {
    10.0
}
fn default_ability_cast_icon_size() -> u8 {
    24
}

impl Default for AbilityCastOverlayConfig {
    fn default() -> Self {
        Self {
            max_display: default_ability_cast_max_display(),
            prune_secs: default_ability_cast_prune_secs(),
            icon_size: default_ability_cast_icon_size(),
            font_scale: 1.0,
            font_color: overlay_colors::WHITE,
            dynamic_background: true,
            stack_from_bottom: false,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Operation Timer Overlay Configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration for the operation timer overlay (persistent timer across encounters)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationTimerOverlayConfig {
    /// Whether to show the title (operation name or "Op Timer") and separator
    #[serde(default = "default_true")]
    pub show_title: bool,
    /// Font scale multiplier (0.3 - 3.0, default 1.0)
    #[serde(default = "default_scaling_factor")]
    pub font_scale: f32,
    /// Font color (RGBA)
    #[serde(default = "default_font_color")]
    pub font_color: Color,
    /// When true, background shrinks to fit content
    #[serde(default)]
    pub dynamic_background: bool,
}

impl Default for OperationTimerOverlayConfig {
    fn default() -> Self {
        Self {
            show_title: true,
            font_scale: 1.0,
            font_color: overlay_colors::WHITE,
            dynamic_background: false,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Ability Queue Overlay Configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration for the ability queue overlay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbilityQueueOverlayConfig {
    /// Maximum entries to display across all tiers
    #[serde(default = "default_max_display")]
    pub max_display: u8,
    /// Font scale multiplier (default 1.0)
    #[serde(default = "default_scaling_factor")]
    pub font_scale: f32,
    /// Font color (RGBA)
    #[serde(default = "default_font_color")]
    pub font_color: Color,
    /// GCD bar accent color (RGBA)
    #[serde(default = "default_gcd_color")]
    pub gcd_color: Color,
    /// When true, background shrinks to fit content
    #[serde(default = "default_true")]
    pub dynamic_background: bool,
}

fn default_max_display() -> u8 {
    12
}
fn default_gcd_color() -> Color {
    [120, 200, 255, 255]
}

impl Default for AbilityQueueOverlayConfig {
    fn default() -> Self {
        Self {
            max_display: 12,
            font_scale: 1.0,
            font_color: overlay_colors::WHITE,
            gcd_color: [120, 200, 255, 255],
            dynamic_background: true,
        }
    }
}
