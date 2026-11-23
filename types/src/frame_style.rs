//! Window chrome every overlay shares: background, border, corner radius.
//!
//! `OverlaySettings` holds one global `FrameStyle` plus per-overlay
//! `FrameStyleOverride`s; `OverlaySettings::frame_style_for` merges them.

use serde::{Deserialize, Serialize};

use crate::Color;

pub const MAX_CORNER_RADIUS: f32 = 16.0;
pub const MAX_BORDER_WIDTH: f32 = 4.0;

/// Fully resolved style an overlay frame draws with
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FrameStyle {
    /// Background fill; its alpha is ignored in favour of `opacity`
    pub background_color: Color,
    /// Background alpha (0-255)
    pub opacity: u8,
    pub border_color: Color,
    /// Border thickness in pixels; 0 = no border
    pub border_width: f32,
    /// Corner radius at the overlay's base size (scales with the window)
    pub corner_radius: f32,
    /// Corner radius of the bars inside element overlays (boss HP, timers,
    /// effects/cooldowns/DoTs bar mode) at base size
    pub element_radius: f32,
}

impl Default for FrameStyle {
    fn default() -> Self {
        Self {
            background_color: [30, 30, 30, 255],
            opacity: 180,
            border_color: [128, 128, 128, 255],
            border_width: 0.0,
            corner_radius: 6.0,
            element_radius: 3.0,
        }
    }
}

/// Per-overlay overrides; `None` inherits the global value
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FrameStyleOverride {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_color: Option<Color>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opacity: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_color: Option<Color>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_width: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub corner_radius: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub element_radius: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
}

impl FrameStyleOverride {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

impl FrameStyle {
    pub fn with_override(self, o: &FrameStyleOverride) -> Self {
        Self {
            background_color: o.background_color.unwrap_or(self.background_color),
            opacity: o.opacity.unwrap_or(self.opacity),
            border_color: o.border_color.unwrap_or(self.border_color),
            border_width: o.border_width.unwrap_or(self.border_width),
            corner_radius: o.corner_radius.unwrap_or(self.corner_radius),
            element_radius: o.element_radius.unwrap_or(self.element_radius),
        }
    }
}
