//! Sound-file playback and loudness normalization.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

use ebur128::{EbuR128, Mode};
use rodio::{Decoder, OutputStream, Sink, Source};

// Target integrated loudness.
const TARGET_LUFS: f64 = -16.0;

// Normalization leaves 1 dB of headroom at 100% volume.
const PEAK_CEILING_DBFS: f64 = -1.0;

#[derive(Hash, Eq, PartialEq)]
struct GainKey {
    path: PathBuf,
    len: Option<u64>,
    modified: Option<SystemTime>,
}

impl GainKey {
    fn new(path: &Path) -> Self {
        let metadata = path.metadata().ok();
        Self {
            path: path.to_owned(),
            len: metadata.as_ref().map(|m| m.len()),
            modified: metadata.and_then(|m| m.modified().ok()),
        }
    }
}

fn gain_cache() -> &'static Mutex<HashMap<GainKey, Arc<OnceLock<f32>>>> {
    static CACHE: OnceLock<Mutex<HashMap<GainKey, Arc<OnceLock<f32>>>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// Play a sound file on a background thread.
pub fn play(path: PathBuf, volume: u8, normalize: bool) {
    std::thread::spawn(move || {
        let gain = if normalize { gain_for(&path) } else { 1.0 };

        let Ok((_stream, stream_handle)) = OutputStream::try_default() else {
            return;
        };
        let Ok(file) = std::fs::File::open(&path) else {
            return;
        };
        let Ok(source) = Decoder::new(std::io::BufReader::new(file)) else {
            return;
        };
        let Ok(sink) = Sink::try_new(&stream_handle) else {
            return;
        };

        sink.set_volume(volume as f32 / 100.0 * gain);
        sink.append(source);
        sink.sleep_until_end();
    });
}

/// Get the cached normalization gain for a file.
fn gain_for(path: &Path) -> f32 {
    let key = GainKey::new(path);
    let cached = {
        let mut cache = gain_cache().lock().unwrap_or_else(|e| e.into_inner());
        cache.retain(|entry, _| entry.path != path || entry == &key);
        Arc::clone(cache.entry(key).or_default())
    };
    *cached.get_or_init(|| measure_gain(path).unwrap_or(1.0))
}

/// Measure a file's normalization gain.
fn measure_gain(path: &Path) -> Option<f32> {
    let file = std::fs::File::open(path).ok()?;
    let source = Decoder::new(std::io::BufReader::new(file)).ok()?;
    let (channels, sample_rate) = (source.channels(), source.sample_rate());
    let mut meter = EbuR128::new(channels as u32, sample_rate, Mode::I | Mode::SAMPLE_PEAK).ok()?;

    let chunk_size = sample_rate as usize * channels as usize;
    let mut samples = source.convert_samples::<f32>();
    let mut chunk = Vec::with_capacity(chunk_size);
    loop {
        chunk.extend(samples.by_ref().take(chunk_size));
        if chunk.is_empty() {
            break;
        }
        meter.add_frames_f32(&chunk).ok()?;
        chunk.clear();
    }

    let peak = (0..channels as u32)
        .filter_map(|ch| meter.sample_peak(ch).ok())
        .fold(0.0f64, f64::max);

    // Short clips may not have an integrated loudness value.
    let loudness = meter.loudness_global().ok().filter(|l| l.is_finite());
    compute_gain(loudness, peak)
}

/// Compute gain without exceeding the peak ceiling.
fn compute_gain(loudness_lufs: Option<f64>, peak: f64) -> Option<f32> {
    if peak <= 0.0 || !peak.is_finite() {
        return None;
    }
    let headroom = from_db(PEAK_CEILING_DBFS) / peak;
    let gain = match loudness_lufs {
        Some(lufs) => headroom.min(from_db(TARGET_LUFS - lufs)),
        None => headroom,
    };
    Some(gain as f32)
}

fn from_db(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn to_db(gain: f32) -> f64 {
        20.0 * (gain as f64).log10()
    }

    #[test]
    fn quiet_clip_with_headroom_reaches_target() {
        // +10 dB fits below the ceiling.
        let gain = compute_gain(Some(-26.0), from_db(-12.0)).unwrap();
        assert!((to_db(gain) - 10.0).abs() < 0.01);
    }

    #[test]
    fn loud_clip_is_attenuated() {
        let gain = compute_gain(Some(-9.6), from_db(-3.0)).unwrap();
        assert!((to_db(gain) - -6.4).abs() < 0.01);
    }

    #[test]
    fn gain_is_clamped_so_peaks_stay_under_ceiling() {
        // Only 1 dB of headroom is available.
        let peak = from_db(-2.0);
        let gain = compute_gain(Some(-26.0), peak).unwrap();
        assert!((to_db(gain) - 1.0).abs() < 0.01);
        assert!(peak * gain as f64 <= from_db(PEAK_CEILING_DBFS) + 1e-6);
    }

    #[test]
    fn missing_loudness_falls_back_to_peak_normalization() {
        let gain = compute_gain(None, from_db(-7.0)).unwrap();
        assert!((to_db(gain) - 6.0).abs() < 0.01);
    }

    #[test]
    fn silent_clip_has_no_gain() {
        assert!(compute_gain(Some(f64::NEG_INFINITY), 0.0).is_none());
        assert!(compute_gain(None, 0.0).is_none());
    }
}
