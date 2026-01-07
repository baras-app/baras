//! Audio playback service using TTS and optional custom sounds
//!
//! Runs in a background task, receiving AudioEvents via channel.
//! TTS is only available on Windows/macOS - Linux requires speech-dispatcher.

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::{RwLock, mpsc};

use baras_types::AudioSettings;

use super::events::AudioEvent;

/// Audio service that handles TTS and sound playback
pub struct AudioService {
    /// Channel to receive audio events
    event_rx: mpsc::Receiver<AudioEvent>,

    /// Shared audio settings (can be updated at runtime)
    settings: Arc<RwLock<AudioSettings>>,

    /// Path to user custom sounds directory (overrides bundled)
    user_sounds_dir: PathBuf,

    /// Path to bundled sounds directory (fallback)
    bundled_sounds_dir: PathBuf,

    /// TTS engine (None if initialization failed or unavailable on platform)
    #[cfg(not(target_os = "linux"))]
    tts: Option<tts::Tts>,
}

impl AudioService {
    /// Create a new audio service
    pub fn new(
        event_rx: mpsc::Receiver<AudioEvent>,
        settings: Arc<RwLock<AudioSettings>>,
        user_sounds_dir: PathBuf,
        bundled_sounds_dir: PathBuf,
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
            settings,
            user_sounds_dir,
            bundled_sounds_dir,
            #[cfg(not(target_os = "linux"))]
            tts,
        }
    }

    /// Run the audio service (blocking async loop)
    pub async fn run(mut self) {
        while let Some(event) = self.event_rx.recv().await {
            // Read settings and extract what we need, then drop the guard
            let (enabled, countdown_enabled, alerts_enabled, volume) = {
                let settings = self.settings.read().await;
                (
                    settings.enabled,
                    settings.countdown_enabled,
                    settings.alerts_enabled,
                    settings.volume,
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
                    if countdown_enabled && !self.play_countdown_voice(voice_pack, *seconds, volume)
                    {
                        self.speak(&format!("{}", seconds));
                    }
                }

                AudioEvent::Alert { text, custom_sound } => {
                    if alerts_enabled {
                        if let Some(sound_file) = custom_sound {
                            self.play_custom_sound(sound_file, volume);
                        } else {
                            self.speak(text);
                        }
                    }
                }

                AudioEvent::Speak { text } => {
                    self.speak(text);
                }
            }
        }
    }

    /// Speak text using TTS (no-op on Linux)
    #[cfg(not(target_os = "linux"))]
    fn speak(&mut self, text: &str) {
        if let Some(ref mut tts) = self.tts {
            let _ = tts.speak(text, false);
        }
    }

    #[cfg(target_os = "linux")]
    fn speak(&mut self, text: &str) {
        use std::process::Command;
        let text = text.to_string();
        std::thread::spawn(move || {
            let _ = Command::new("espeak").arg(&text).output();
        });
    }

    /// Play a countdown number using a voice pack (returns false if not found)
    fn play_countdown_voice(&self, voice: &str, seconds: u8, volume: u8) -> bool {
        let filename = format!("{}.mp3", seconds);
        let user_path = self.user_sounds_dir.join(voice).join(&filename);
        let bundled_path = self.bundled_sounds_dir.join(voice).join(&filename);

        let path = if user_path.exists() {
            user_path
        } else if bundled_path.exists() {
            bundled_path
        } else {
            return false;
        };

        let vol = volume;
        std::thread::spawn(move || {
            use rodio::{Decoder, OutputStream, Sink};
            use std::fs::File;
            use std::io::BufReader;

            let Ok((_stream, stream_handle)) = OutputStream::try_default() else {
                return;
            };
            let Ok(file) = File::open(&path) else { return };
            let Ok(source) = Decoder::new(BufReader::new(file)) else {
                return;
            };
            let Ok(sink) = Sink::try_new(&stream_handle) else {
                return;
            };

            sink.set_volume(vol as f32 / 100.0);
            sink.append(source);
            sink.sleep_until_end();
        });
        true
    }

    /// Play a custom sound file
    fn play_custom_sound(&self, filename: &str, volume: u8) {
        let user_path = self.user_sounds_dir.join(filename);
        let bundled_path = self.bundled_sounds_dir.join(filename);

        let path = if user_path.exists() {
            user_path
        } else if bundled_path.exists() {
            bundled_path
        } else {
            return;
        };

        let vol = volume;
        std::thread::spawn(move || {
            use rodio::{Decoder, OutputStream, Sink};
            use std::fs::File;
            use std::io::BufReader;

            let Ok((_stream, stream_handle)) = OutputStream::try_default() else {
                return;
            };
            let Ok(file) = File::open(&path) else { return };
            let Ok(source) = Decoder::new(BufReader::new(file)) else {
                return;
            };
            let Ok(sink) = Sink::try_new(&stream_handle) else {
                return;
            };

            sink.set_volume(vol as f32 / 100.0);
            sink.append(source);
            sink.sleep_until_end();
        });
    }
}

/// Sender handle for sending audio events
pub type AudioSender = mpsc::Sender<AudioEvent>;

/// Create a new audio channel
pub fn create_audio_channel() -> (AudioSender, mpsc::Receiver<AudioEvent>) {
    // Buffer size of 64 should be plenty for audio events
    mpsc::channel(64)
}
