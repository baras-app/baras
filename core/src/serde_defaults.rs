//! Common serde default value functions
//!
//! Used across timer, effect, and boss definitions to avoid duplication.

/// Default for enabled fields
pub fn default_true() -> bool {
    true
}

/// Default timer/effect color (light gray with full opacity)
pub fn default_timer_color() -> [u8; 4] {
    [200, 200, 200, 255]
}
