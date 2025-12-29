//! Audio playback service using TTS and optional custom sounds
//!
//! Runs in a background task, receiving AudioEvents via channel.
//! TTS is only available on Windows/macOS - Linux requires speech-dispatcher.

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::{mpsc, RwLock};

use baras_types::AudioSettings;

use super::events::AudioEvent;

/// Audio service that handles TTS and sound playback
pub struct AudioService {
    /// Channel to receive audio events
    event_rx: mpsc::Receiver<AudioEvent>,

    /// Shared audio settings (can be updated at runtime)
    settings: Arc<RwLock<AudioSettings>>,

    /// Path to user custom sounds directory
    user_sounds_dir: PathBuf,

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
    ) -> Self {
        #[cfg(not(target_os = "linux"))]
        let tts = {
            // Try to initialize TTS, gracefully handle failure
            match tts::Tts::default() {
                Ok(mut engine) => {
                    // Set a reasonable speech rate
                    let _ = engine.set_rate(engine.normal_rate());
                    eprintln!("[AUDIO] TTS initialized successfully");
                    Some(engine)
                }
                Err(e) => {
                    eprintln!("[AUDIO] TTS initialization failed: {}. TTS disabled.", e);
                    None
                }
            }
        };

        #[cfg(target_os = "linux")]
        eprintln!("[AUDIO] TTS not available on Linux (requires speech-dispatcher). Custom sounds only.");

        Self {
            event_rx,
            settings,
            user_sounds_dir,
            #[cfg(not(target_os = "linux"))]
            tts,
        }
    }

    /// Run the audio service (blocking async loop)
    pub async fn run(mut self) {
        while let Some(event) = self.event_rx.recv().await {
            // Read settings and extract what we need, then drop the guard
            let (enabled, countdown_enabled, countdown_seconds, alerts_enabled, volume) = {
                let settings = self.settings.read().await;
                (
                    settings.enabled,
                    settings.countdown_enabled,
                    settings.countdown_seconds,
                    settings.alerts_enabled,
                    settings.volume,
                )
            };

            // Master audio toggle
            if !enabled {
                continue;
            }

            match &event {
                AudioEvent::Countdown { timer_name, seconds } => {
                    eprintln!("[AUDIO] Countdown: {} {}", timer_name, seconds);
                    if countdown_enabled && *seconds <= countdown_seconds {
                        self.speak(&format!("{} {}", timer_name, seconds));
                    }
                }

                AudioEvent::Alert { text, custom_sound } => {
                    eprintln!("[AUDIO] Alert: {}", text);
                    if alerts_enabled {
                        if let Some(sound_file) = custom_sound {
                            self.play_custom_sound(sound_file, volume);
                        } else {
                            self.speak(text);
                        }
                    }
                }

                AudioEvent::Speak { text } => {
                    eprintln!("[AUDIO] Speak: {}", text);
                    self.speak(text);
                }
            }
        }

        eprintln!("[AUDIO] Service stopped");
    }

    /// Speak text using TTS (no-op on Linux)
    #[cfg(not(target_os = "linux"))]
    fn speak(&mut self, text: &str) {
        if let Some(ref mut tts) = self.tts {
            // Use non-blocking speech (interrupt = false means queue)
            if let Err(e) = tts.speak(text, false) {
                eprintln!("[AUDIO] TTS speak failed: {}", e);
            }
        }
    }

    #[cfg(target_os = "linux")]
    fn speak(&mut self, _text: &str) {
        // TTS not available on Linux without speech-dispatcher
    }

    /// Play a custom sound file
    fn play_custom_sound(&self, filename: &str, volume: u8) {
        let path = self.user_sounds_dir.join(filename);

        if !path.exists() {
            eprintln!("[AUDIO] Sound file not found: {:?}", path);
            return;
        }

        // Use rodio for custom sounds
        use rodio::{Decoder, OutputStream, Sink};
        use std::fs::File;
        use std::io::BufReader;

        let Ok((_stream, stream_handle)) = OutputStream::try_default() else {
            eprintln!("[AUDIO] Failed to get audio output stream");
            return;
        };

        let Ok(file) = File::open(&path) else {
            eprintln!("[AUDIO] Failed to open sound file: {:?}", path);
            return;
        };

        let Ok(source) = Decoder::new(BufReader::new(file)) else {
            eprintln!("[AUDIO] Failed to decode sound file: {:?}", path);
            return;
        };

        let Ok(sink) = Sink::try_new(&stream_handle) else {
            eprintln!("[AUDIO] Failed to create audio sink");
            return;
        };

        sink.set_volume(volume as f32 / 100.0);
        sink.append(source);
        sink.detach(); // Non-blocking playback
    }
}

/// Sender handle for sending audio events
pub type AudioSender = mpsc::Sender<AudioEvent>;

/// Create a new audio channel
pub fn create_audio_channel() -> (AudioSender, mpsc::Receiver<AudioEvent>) {
    // Buffer size of 64 should be plenty for audio events
    mpsc::channel(64)
}
