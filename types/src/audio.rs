//! Sound picker and audio settings

use crate::serde_defaults::default_true;
use serde::{Deserialize, Serialize};

/// Audio `file` value meaning "play nothing" (countdown voice only), as opposed to an
/// unset file, which falls back to speaking the entry's name
pub const NO_SOUND: &str = "none";

// ─────────────────────────────────────────────────────────────────────────────
// Sound Picker
// ─────────────────────────────────────────────────────────────────────────────

/// A grouped set of available sound files for the audio picker UI.
/// `folder` is the bundled subdirectory under `core/definitions/`, also used
/// as the persisted prefix (e.g. `"sounds/Alert.mp3"`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoundCategory {
    pub name: String,
    pub folder: String,
    pub files: Vec<String>,
}

/// Audio settings for timer alerts and countdowns
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioSettings {
    /// Master enable for all audio
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Volume level (0-200; 100 = natural loudness, above amplifies)
    #[serde(default = "default_audio_volume")]
    pub volume: u8,

    /// Enable TTS fallback for alerts without a sound file and countdowns
    /// without a voice pack
    #[serde(default = "default_true", alias = "alerts_enabled")]
    pub tts_enabled: bool,
}

fn default_audio_volume() -> u8 {
    80
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            volume: 80,
            tts_enabled: true,
        }
    }
}
