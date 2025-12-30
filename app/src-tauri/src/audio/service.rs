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
        eprintln!("[AUDIO] Using espeak for TTS on Linux");

        eprintln!("[AUDIO] User sounds: {:?}", user_sounds_dir);
        eprintln!("[AUDIO] Bundled sounds: {:?}", bundled_sounds_dir);

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
                AudioEvent::Countdown { timer_name, seconds, voice_pack } => {
                    eprintln!("[AUDIO] Countdown: {} {} (voice: {})", timer_name, seconds, voice_pack);
                    if countdown_enabled {
                        // Timer manager already filtered by countdown_start, just play it
                        if !self.play_countdown_voice(voice_pack, *seconds, volume) {
                            self.speak(&format!("{}", seconds));
                        }
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
    fn speak(&mut self, text: &str) {
        // Use espeak directly on Linux (non-blocking)
        use std::process::Command;

        let text = text.to_string();
        std::thread::spawn(move || {
            if let Err(e) = Command::new("espeak")
                .arg(&text)
                .output()
            {
                eprintln!("[AUDIO] espeak failed: {}. Install with: sudo pacman -S espeak-ng", e);
            }
        });
    }

    /// Play a countdown number using a voice pack (returns false if not found)
    fn play_countdown_voice(&self, voice: &str, seconds: u8, volume: u8) -> bool {
        // Voice packs are in {sounds_dir}/{voice}/{number}.mp3
        let filename = format!("{}.mp3", seconds);
        let user_path = self.user_sounds_dir.join(voice).join(&filename);
        let bundled_path = self.bundled_sounds_dir.join(voice).join(&filename);

        let path = if user_path.exists() {
            user_path
        } else if bundled_path.exists() {
            bundled_path
        } else {
            eprintln!("[AUDIO] Voice pack not found: {}/{} (tried {:?} and {:?})",
                voice, filename, user_path, bundled_path);
            return false;
        };

        eprintln!("[AUDIO] Playing voice: {:?}", path);

        // Spawn thread to keep OutputStream alive until playback completes
        let vol = volume;
        let path_for_log = path.clone();
        std::thread::spawn(move || {
            use rodio::{Decoder, OutputStream, Sink};
            use std::fs::File;
            use std::io::BufReader;

            let (_stream, stream_handle) = match OutputStream::try_default() {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("[AUDIO] Failed to get output stream: {}", e);
                    return;
                }
            };
            let file = match File::open(&path) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("[AUDIO] Failed to open file {:?}: {}", path, e);
                    return;
                }
            };
            let source = match Decoder::new(BufReader::new(file)) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("[AUDIO] Failed to decode {:?}: {}", path, e);
                    return;
                }
            };
            let sink = match Sink::try_new(&stream_handle) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("[AUDIO] Failed to create sink: {}", e);
                    return;
                }
            };

            eprintln!("[AUDIO] Actually playing {:?} at volume {}", path_for_log, vol);
            sink.set_volume(vol as f32 / 100.0);
            sink.append(source);
            sink.sleep_until_end(); // Block thread until audio finishes
            eprintln!("[AUDIO] Finished playing {:?}", path_for_log);
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
            eprintln!("[AUDIO] Sound file not found: {:?} or {:?}", user_path, bundled_path);
            return;
        };

        eprintln!("[AUDIO] Playing custom sound: {:?}", path);

        // Spawn thread to keep OutputStream alive until playback completes
        let vol = volume;
        std::thread::spawn(move || {
            use rodio::{Decoder, OutputStream, Sink};
            use std::fs::File;
            use std::io::BufReader;

            let (_stream, stream_handle) = match OutputStream::try_default() {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("[AUDIO] Failed to get output stream: {}", e);
                    return;
                }
            };
            let file = match File::open(&path) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("[AUDIO] Failed to open file {:?}: {}", path, e);
                    return;
                }
            };
            let source = match Decoder::new(BufReader::new(file)) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("[AUDIO] Failed to decode {:?}: {}", path, e);
                    return;
                }
            };
            let sink = match Sink::try_new(&stream_handle) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("[AUDIO] Failed to create sink: {}", e);
                    return;
                }
            };

            sink.set_volume(vol as f32 / 100.0);
            sink.append(source);
            sink.sleep_until_end(); // Block thread until audio finishes
            eprintln!("[AUDIO] Finished playing custom sound");
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
