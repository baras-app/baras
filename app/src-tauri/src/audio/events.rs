//! Audio event types for timer system integration

/// Events that can trigger audio playback
#[derive(Debug, Clone)]
pub enum AudioEvent {
    /// Countdown: speak timer name + seconds remaining
    /// e.g., "Shield 3", "Shield 2", "Shield 1"
    Countdown {
        timer_name: String,
        seconds: u8,
    },

    /// Alert fired: speak the alert text
    Alert {
        text: String,
        /// Optional custom sound file path (relative to sounds dir)
        custom_sound: Option<String>,
    },

    /// Speak arbitrary text
    Speak {
        text: String,
    },
}
