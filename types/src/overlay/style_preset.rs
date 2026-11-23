//! One-click looks: the global window style and metric bar shape together.
//! Colors and per-overlay overrides are left untouched.

use super::settings::{OverlaySettings, default_gradient_intensity};
use crate::appearance::{default_bar_gap, default_bar_radius};
use crate::frame_style::FrameStyle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StylePreset {
    /// The app defaults
    Simple,
    /// No background or border, default shapes
    Transparent,
    Compact,
    Rounded,
    Flat,
}

impl StylePreset {
    pub const ALL: [Self; 5] = [
        Self::Simple,
        Self::Transparent,
        Self::Compact,
        Self::Rounded,
        Self::Flat,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Simple => "Simple",
            Self::Transparent => "Transparent",
            Self::Compact => "Compact",
            Self::Rounded => "Rounded",
            Self::Flat => "Flat",
        }
    }

    pub fn apply(self, s: &mut OverlaySettings) {
        let d = FrameStyle::default();
        // (opacity, border, window radius, element radius, metric radius, metric gap, gradient)
        let (opacity, border, window, element, bar_radius, bar_gap, gradient) = match self {
            Self::Simple => (
                d.opacity,
                d.border_width,
                d.corner_radius,
                d.element_radius,
                default_bar_radius(),
                default_bar_gap(),
                default_gradient_intensity(),
            ),
            Self::Transparent => (
                0,
                0.0,
                d.corner_radius,
                d.element_radius,
                default_bar_radius(),
                default_bar_gap(),
                default_gradient_intensity(),
            ),
            Self::Compact => (200, 0.0, 2.0, 2.0, 2.0, 1.0, default_gradient_intensity()),
            Self::Rounded => (d.opacity, 0.0, 10.0, 6.0, 6.0, 3.0, default_gradient_intensity()),
            Self::Flat => (d.opacity, 1.0, 0.0, 0.0, 0.0, 2.0, 0.0),
        };
        let f = &mut s.frame_style;
        f.opacity = opacity;
        f.border_width = border;
        f.corner_radius = window;
        f.element_radius = element;
        s.metric_bar_radius = bar_radius;
        s.metric_bar_gap = bar_gap;
        s.metric_gradient_intensity = gradient;
    }
}
