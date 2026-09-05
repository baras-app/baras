//! Audio subsystem for timer alerts and countdowns
//!
//! Provides TTS-based audio for timer countdowns and alerts,
//! with optional support for custom sound files.

mod events;
mod path;
mod playback;
mod service;

pub use events::AudioEvent;
pub use path::{resolve_bundled_definitions_dir, resolve_sound_path};
pub use playback::play;
pub use service::{AudioSender, AudioService, create_audio_channel};
