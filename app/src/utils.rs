//! Utility functions
//!
//! Helper functions used across the frontend.

use crate::types::Color;

/// Parse a hex color string (e.g., "#ff0000") to RGBA bytes
pub fn parse_hex_color(hex: &str) -> Option<Color> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some([r, g, b, 255])
}

/// Format a Color as a hex string for HTML color inputs
pub fn color_to_hex(color: &Color) -> String {
    format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2])
}

/// Default function for serde that returns true
pub fn default_true() -> bool {
    true
}
