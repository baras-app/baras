//! Raid and enemy frames overlays

use crate::serde_defaults::{default_scaling_factor, default_true};
use crate::{Color, overlay_colors};
use serde::{Deserialize, Serialize};

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
    #[serde(default = "default_effect_offset")]
    pub effect_horizontal_offset: f32,
    #[serde(default = "default_frame_bg")]
    pub frame_bg_color: Color,
    #[serde(default = "default_true")]
    pub show_role_icons: bool,
    #[serde(default)]
    pub show_class_icons: bool,
    #[serde(default = "default_effect_fill_opacity")]
    pub effect_fill_opacity: u8,
    #[serde(default)]
    pub show_effect_icons: bool,
    /// Outline colored effect squares with the effect's own color
    #[serde(default)]
    pub effect_colored_borders: bool,
    #[serde(default = "default_frame_spacing")]
    pub frame_spacing: f32,
}

fn default_grid_columns() -> u8 {
    2
}
fn default_grid_rows() -> u8 {
    4
}
fn default_max_effects() -> u8 {
    4
}
fn default_effect_size() -> f32 {
    14.0
}
fn default_effect_offset() -> f32 {
    3.0
}
fn default_frame_bg() -> Color {
    overlay_colors::FRAME_BG
}
fn default_effect_fill_opacity() -> u8 {
    255
}
fn default_frame_spacing() -> f32 {
    4.0
}

impl Default for RaidOverlaySettings {
    fn default() -> Self {
        Self {
            grid_columns: 2,
            grid_rows: 4,
            max_effects_per_frame: 4,
            effect_size: 14.0,
            effect_vertical_offset: 3.0,
            effect_horizontal_offset: 3.0,
            frame_bg_color: overlay_colors::FRAME_BG,
            show_role_icons: true,
            show_class_icons: false,
            effect_fill_opacity: 255,
            show_effect_icons: false,
            effect_colored_borders: false,
            frame_spacing: 4.0,
        }
    }
}

impl RaidOverlaySettings {
    /// Get total number of slots
    pub fn total_slots(&self) -> u8 {
        self.grid_columns * self.grid_rows
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Enemy Frames Settings
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration for the enemy HP frames overlay (PvP).
/// Layout is fixed at 4 rows: arena teams fill one column, warzone teams two.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnemyFramesConfig {
    /// Show each enemy's current target inside the frame
    #[serde(default = "default_true")]
    pub show_target: bool,
    /// Frame scale multiplier (0.5 - 2.0)
    #[serde(default = "default_scaling_factor")]
    pub scale: f32,
}

impl Default for EnemyFramesConfig {
    fn default() -> Self {
        Self {
            show_target: true,
            scale: 1.0,
        }
    }
}
