//! Audio subsystem for timer alerts and countdowns
//!
//! Provides TTS-based audio for timer countdowns and alerts,
//! with optional support for custom sound files.

mod events;
mod service;

pub use events::AudioEvent;
pub use service::{create_audio_channel, AudioSender, AudioService};
