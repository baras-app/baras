//! Audio event types for timer system integration

/// Events that can trigger audio playback
#[derive(Debug, Clone)]
pub enum AudioEvent {
    /// Countdown: play voice pack number for remaining seconds
    /// e.g., plays "Amy/3.mp3", "Amy/2.mp3", "Amy/1.mp3"
    Countdown {
        timer_name: String,
        seconds: u8,
        /// Voice pack name (Amy, Jim, Yolo, Nerevar)
        voice_pack: String,
    },

    /// Alert fired: speak the alert text
    Alert {
        text: String,
        /// Optional custom sound file path (relative to sounds dir)
        custom_sound: Option<String>,
    },

    /// Speak arbitrary text
    Speak { text: String },
}
