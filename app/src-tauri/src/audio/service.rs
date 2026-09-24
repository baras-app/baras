//! Audio playback service using TTS and optional custom sounds
//!
//! Runs in a background task, receiving AudioEvents via channel.
//! TTS is only available on Windows/macOS - Linux requires speech-dispatcher.

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::mpsc;

use crate::state::SharedState;

use super::events::AudioEvent;

/// Audio service that handles TTS and sound playback
pub struct AudioService {
    /// Channel to receive audio events
    event_rx: mpsc::Receiver<AudioEvent>,

    /// Shared app state — audio settings are read fresh per event so config
    /// changes apply immediately
    shared: Arc<SharedState>,

    /// Path to user custom sounds directory (overrides bundled for the General category)
    user_sounds_dir: PathBuf,

    /// Path to bundled `core/definitions/` directory — parent of `sounds/`,
    /// `mechanic-sounds/`, etc.
    bundled_definitions_dir: PathBuf,

    /// TTS engine (None if initialization failed or unavailable on platform)
    #[cfg(not(target_os = "linux"))]
    tts: Option<tts::Tts>,
}

impl AudioService {
    /// Create a new audio service.
    ///
    /// `bundled_definitions_dir` is the `core/definitions/` root (parent of
    /// `sounds/` and `mechanic-sounds/`).
    pub fn new(
        event_rx: mpsc::Receiver<AudioEvent>,
        shared: Arc<SharedState>,
        user_sounds_dir: PathBuf,
        bundled_definitions_dir: PathBuf,
    ) -> Self {
        #[cfg(not(target_os = "linux"))]
        let tts = {
            // Try to initialize TTS, gracefully handle failure
            match tts::Tts::default() {
                Ok(mut engine) => {
                    let _ = engine.set_rate(engine.normal_rate());
                    Some(engine)
                }
                Err(_) => None,
            }
        };

        Self {
            event_rx,
            shared,
            user_sounds_dir,
            bundled_definitions_dir,
            #[cfg(not(target_os = "linux"))]
            tts,
        }
    }

    /// Run the audio service (blocking async loop)
    pub async fn run(mut self) {
        while let Some(event) = self.event_rx.recv().await {
            // Read settings and extract what we need, then drop the guard
            let (enabled, tts_enabled, volume, normalize) = {
                let config = self.shared.config.read().await;
                (
                    config.audio.enabled,
                    config.audio.tts_enabled,
                    config.audio.volume,
                    config.audio.normalize_loudness,
                )
            };

            // Master audio toggle
            if !enabled {
                continue;
            }

            match &event {
                AudioEvent::Countdown {
                    timer_name: _,
                    seconds,
                    voice_pack,
                } => {
                    if !self.play_countdown_voice(voice_pack, *seconds, volume, normalize)
                        && tts_enabled
                    {
                        self.speak(&format!("{}", seconds), volume);
                    }
                }

                AudioEvent::Alert { text, custom_sound } => {
                    if let Some(sound_file) = custom_sound {
                        self.play_custom_sound(sound_file, volume, normalize);
                    } else if tts_enabled {
                        self.speak(text, volume);
                    }
                }

                AudioEvent::Speak { text } => {
                    if tts_enabled {
                        self.speak(text, volume);
                    }
                }
            }
        }
    }

    /// Speak text using TTS at the given volume (0-200). TTS engines can't
    /// amplify past their max, so 100-200 clamps to full engine volume.
    #[cfg(not(target_os = "linux"))]
    fn speak(&mut self, text: &str, volume: u8) {
        if let Some(ref mut tts) = self.tts {
            let (min, max) = (tts.min_volume(), tts.max_volume());
            let pct = (volume.min(100)) as f32 / 100.0;
            let _ = tts.set_volume(min + (max - min) * pct);
            let _ = tts.speak(text, false);
        }
    }

    /// espeak amplitude natively ranges 0-200 with 100 as normal loudness,
    /// matching the slider directly.
    #[cfg(target_os = "linux")]
    fn speak(&mut self, text: &str, volume: u8) {
        use std::process::Command;
        let text = text.to_string();
        std::thread::spawn(move || {
            let _ = Command::new("espeak")
                .arg("-a")
                .arg(volume.to_string())
                .arg(&text)
                .output();
        });
    }

    /// Play a countdown number using a voice pack (returns false if not found).
    ///
    /// Voice packs are subfolders under the `sounds/` directory containing
    /// per-second `1.mp3`, `2.mp3`, ... files.
    fn play_countdown_voice(&self, voice: &str, seconds: u8, volume: u8, normalize: bool) -> bool {
        let filename = format!("{}.mp3", seconds);
        let user_path = self.user_sounds_dir.join(voice).join(&filename);
        let bundled_path = self
            .bundled_definitions_dir
            .join("sounds")
            .join(voice)
            .join(&filename);

        let path = if user_path.exists() {
            user_path
        } else if bundled_path.exists() {
            bundled_path
        } else {
            return false;
        };

        super::play(path, volume, normalize);
        true
    }

    /// Play a custom sound file. Accepts folder-relative refs, legacy bare
    /// filenames, or absolute paths — see [`super::resolve_sound_path`].
    fn play_custom_sound(&self, filename: &str, volume: u8, normalize: bool) {
        let Some(path) = super::resolve_sound_path(
            filename,
            &self.user_sounds_dir,
            &self.bundled_definitions_dir,
        ) else {
            return;
        };

        super::play(path, volume, normalize);
    }
}

/// Sender handle for sending audio events
pub type AudioSender = mpsc::Sender<AudioEvent>;

/// Create a new audio channel
pub fn create_audio_channel() -> (AudioSender, mpsc::Receiver<AudioEvent>) {
    // Buffer size of 64 should be plenty for audio events
    mpsc::channel(64)
}
