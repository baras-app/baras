//! One-shot sound file playback.

use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

use rodio::{Decoder, DeviceSinkBuilder, Player};

/// Play a sound file on its own thread at `volume` percent. Failures to open
/// the output device or decode the file are silent: a missing alert sound must
/// never interrupt the caller.
pub fn play_file(path: PathBuf, volume: u8) {
    std::thread::spawn(move || {
        let Ok(mut device) = DeviceSinkBuilder::open_default_sink() else {
            return;
        };
        // The sink is dropped on purpose once the sound has finished.
        device.log_on_drop(false);
        let Ok(file) = File::open(&path) else { return };
        let Ok(source) = Decoder::new(BufReader::new(file)) else {
            return;
        };

        let player = Player::connect_new(device.mixer());
        player.set_volume(volume as f32 / 100.0);
        player.append(source);
        player.sleep_until_end();
    });
}
