//! Write the crops handed to recognition, and what came back, to disk.
//!
//! The text alone cannot say whether a misread came from the crop, the
//! preprocessing or the decoder. The images can.
//!
//! Off by default, and every failure here is swallowed: a diagnostic that
//! breaks detection is worse than no diagnostic.

use std::fmt::Write as _;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use baras_overlay::capture::CapturedImage;
use crate::analysis::{Band, BandKind, PreparedCrop};

/// Where dumps are written: `<config>/baras/ocr-debug/<timestamp>/`.
fn session_dir() -> Option<PathBuf> {
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S%.3f").to_string();
    let dir = dirs::config_dir()?
        .join("baras")
        .join("ocr-debug")
        .join(stamp);
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// One detection pass worth of images and readings.
pub struct DebugDump {
    dir: PathBuf,
    notes: String,
}

impl DebugDump {
    /// Create a dump directory for this pass, or `None` if it cannot be made.
    pub fn new(slot_count: usize) -> Option<Self> {
        let dir = session_dir()?;
        let mut notes = String::new();
        let _ = writeln!(notes, "BARAS raid OCR debug dump");
        let _ = writeln!(
            notes,
            "captured {}",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f")
        );
        let _ = writeln!(notes, "slots {slot_count}");
        Some(Self { dir, notes })
    }

    pub fn path(&self) -> &Path {
        &self.dir
    }

    /// Record the whole captured region, so slot alignment can be checked.
    pub fn capture(&mut self, image: &CapturedImage) {
        let _ = writeln!(
            self.notes,
            "capture {}x{} -> capture.png",
            image.width, image.height
        );
        write_rgba(&self.dir.join("capture.png"), image);
    }

    /// Record one slot's rectangle within the capture.
    pub fn slot(&mut self, slot: u8, rect: (i32, i32, u32, u32)) {
        let _ = writeln!(
            self.notes,
            "\nslot {slot:<2} rect x={} y={} w={} h={}",
            rect.0, rect.1, rect.2, rect.3
        );
    }

    /// Record where the slot's health bar was found, or how it was made up.
    pub fn bar(&mut self, slot: u8, bar: Option<(u32, u32)>, inferred: bool) {
        let _ = match bar {
            Some((top, height)) if inferred => writeln!(
                self.notes,
                "  (slot {slot}: bar borrowed from the grid: top={top} h={height})"
            ),
            Some((top, height)) => {
                writeln!(self.notes, "  bar top={top} h={height}")
            }
            None => writeln!(self.notes, "  (slot {slot}: no bar found anywhere in the grid)"),
        };
    }

    /// Record a slot that produced no bands at all.
    pub fn no_bands(&mut self, slot: u8) {
        let _ = writeln!(self.notes, "  (slot {slot}: no bands detected)");
    }

    /// Record one crop, its text, and what the caller made of it.
    pub fn band(
        &mut self,
        slot: u8,
        band: &Band,
        crop: &PreparedCrop,
        text: &str,
        outcome: &str,
    ) {
        let kind = match band.kind {
            BandKind::Name => "name",
            BandKind::Health => "health",
        };
        let file = format!("slot{slot:02}-{kind}.png");

        let _ = writeln!(
            self.notes,
            "  {kind:<6} band top={} h={}  crop {}x{}  -> {file}",
            band.top, band.height, crop.width, crop.height
        );
        let _ = writeln!(self.notes, "         text={text:?}");
        let _ = writeln!(self.notes, "         {outcome}");

        write_gray(&self.dir.join(file), crop);
    }

    /// Record a band whose crop could not be prepared.
    pub fn band_skipped(&mut self, band: &Band, reason: &str) {
        let _ = writeln!(
            self.notes,
            "  {:?} band top={} h={} skipped: {reason}",
            band.kind, band.top, band.height
        );
    }

    /// Flush the notes. Consumes the dump so it cannot be written twice.
    pub fn finish(self) {
        let path = self.dir.join("readings.txt");
        if let Err(e) = std::fs::write(&path, self.notes) {
            tracing::warn!("Could not write OCR debug notes to {path:?}: {e}");
        } else {
            tracing::info!("OCR debug dump written to {:?}", self.dir);
        }
    }
}

/// Write an 8-bit grayscale PNG. Failures are logged, never propagated.
fn write_gray(path: &Path, crop: &PreparedCrop) {
    if crop.gray.len() != (crop.width as usize * crop.height as usize) {
        tracing::warn!("Skipping malformed debug crop for {path:?}");
        return;
    }
    let result = File::create(path).map_err(|e| e.to_string()).and_then(|file| {
        let mut encoder = png::Encoder::new(BufWriter::new(file), crop.width, crop.height);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .and_then(|mut writer| writer.write_image_data(&crop.gray))
            .map_err(|e| e.to_string())
    });
    if let Err(e) = result {
        tracing::warn!("Could not write debug crop {path:?}: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_a_readable_grayscale_png() {
        let dir = std::env::temp_dir().join("baras-ocr-dump-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("crop.png");
        let _ = std::fs::remove_file(&path);

        let crop = PreparedCrop {
            width: 4,
            height: 2,
            gray: vec![0, 64, 128, 255, 255, 128, 64, 0],
        };
        write_gray(&path, &crop);

        let decoder = png::Decoder::new(File::open(&path).unwrap());
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).unwrap();

        assert_eq!((info.width, info.height), (4, 2));
        assert_eq!(&buf[..info.buffer_size()], &crop.gray[..]);
    }

    #[test]
    fn malformed_crop_is_skipped_rather_than_panicking() {
        let dir = std::env::temp_dir().join("baras-ocr-dump-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("malformed.png");
        let _ = std::fs::remove_file(&path);

        // Claims 4x2 but carries two pixels.
        write_gray(
            &path,
            &PreparedCrop {
                width: 4,
                height: 2,
                gray: vec![1, 2],
            },
        );

        assert!(!path.exists(), "a malformed crop must not produce a file");
    }
}

/// Write the captured region as RGBA.
fn write_rgba(path: &Path, image: &CapturedImage) {
    let expected = image.width as usize * image.height as usize * 4;
    if image.rgba.len() < expected {
        tracing::warn!("Skipping malformed debug capture for {path:?}");
        return;
    }
    let result = File::create(path).map_err(|e| e.to_string()).and_then(|file| {
        let mut encoder = png::Encoder::new(BufWriter::new(file), image.width, image.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .and_then(|mut writer| writer.write_image_data(&image.rgba[..expected]))
            .map_err(|e| e.to_string())
    });
    if let Err(e) = result {
        tracing::warn!("Could not write debug capture {path:?}: {e}");
    }
}
