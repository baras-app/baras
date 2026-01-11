//! Shared audio configuration for timers, effects, and alerts

use serde::{Deserialize, Serialize};

/// Audio configuration shared by timers, effects, and alerts
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AudioConfig {
    /// Master toggle for audio on this item
    #[serde(default)]
    pub enabled: bool,

    /// Audio file to play (relative to sounds directory)
    pub file: Option<String>,

    /// Seconds before expiration to play audio (0 = on expiration)
    #[serde(default)]
    pub offset: u8,

    /// Start countdown audio at N seconds remaining (0 = disabled)
    #[serde(default)]
    pub countdown_start: u8,

    /// Voice pack for countdown (None = default)
    #[serde(default)]
    pub countdown_voice: Option<String>,

    /// Alert text to display on alert overlay when effect triggers.
    /// If non-empty, sends this text to the alert overlay.
    #[serde(default)]
    pub alert_text: Option<String>,
}

impl AudioConfig {
    /// Check if any audio is configured
    pub fn has_audio(&self) -> bool {
        self.enabled && (self.file.is_some() || self.countdown_start > 0)
    }

    /// Check if countdown audio is enabled
    pub fn has_countdown(&self) -> bool {
        self.enabled && self.countdown_start > 0
    }

    /// Check if alert text is configured
    pub fn has_alert_text(&self) -> bool {
        self.alert_text.as_ref().is_some_and(|t| !t.is_empty())
    }
}
