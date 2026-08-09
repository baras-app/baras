//! Replay recorded debug dumps through the current detection pipeline.
//!
//! Each dump directory holds the `capture.png` a live pass saw and the
//! readings it produced at the time (`readings.txt`). Re-running the capture
//! through the current code and diffing the two shows exactly what a change
//! did across every recorded frame — an A/B test over real data.
//!
//! Usage:
//!   cargo run -p baras-raid-ocr --example replay --release -- ~/.config/baras/ocr-debug
//!
//! Arguments are dump directories or parents of them. Only name readings are
//! compared; slots whose reading did not change are counted but not printed.

use std::collections::BTreeMap;
use std::fs::File;
use std::path::{Path, PathBuf};

use baras_overlay::capture::CapturedImage;
use baras_raid_ocr::{SlotRect, observe_slots};

fn main() {
    let mut dumps: Vec<PathBuf> = Vec::new();
    for arg in std::env::args().skip(1) {
        let path = PathBuf::from(shellexpand(&arg));
        if path.join("readings.txt").exists() {
            dumps.push(path);
        } else if let Ok(entries) = std::fs::read_dir(&path) {
            for entry in entries.flatten() {
                let sub = entry.path();
                if sub.join("readings.txt").exists() {
                    dumps.push(sub);
                }
            }
        }
    }
    dumps.sort();
    if dumps.is_empty() {
        eprintln!("no dump directories found");
        std::process::exit(1);
    }

    let mut slots_total = 0usize;
    let mut unchanged = 0usize;
    let mut changed: Vec<(String, u8, String, String)> = Vec::new();
    let mut skipped = 0usize;

    for dir in &dumps {
        let stamp = dir.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let Ok(text) = std::fs::read_to_string(dir.join("readings.txt")) else {
            skipped += 1;
            continue;
        };
        let (rects, old_names) = parse_readings(&text);
        let Some(capture) = load_capture(&dir.join("capture.png")) else {
            skipped += 1;
            continue;
        };
        if rects.is_empty() {
            skipped += 1;
            continue;
        }

        let observations = observe_slots(&capture, &rects);
        let new_names: BTreeMap<u8, String> = observations
            .iter()
            .filter_map(|o| {
                Some((u8::try_from(o.row).ok()?, o.name_text.clone()?))
            })
            .collect();

        for &(slot, ..) in &rects {
            let old = old_names.get(&slot).map(String::as_str).unwrap_or("-");
            let new = new_names.get(&slot).map(String::as_str).unwrap_or("-");
            // A slot unreadable both times is not a comparison.
            if old == "-" && new == "-" {
                continue;
            }
            slots_total += 1;
            if old == new {
                unchanged += 1;
            } else {
                changed.push((stamp.clone(), slot, old.to_string(), new.to_string()));
            }
        }
    }

    println!("dumps replayed: {} (skipped {skipped})", dumps.len());
    println!("name readings compared: {slots_total}");
    println!("unchanged: {unchanged}");
    println!("changed:   {}", changed.len());
    println!();
    for (stamp, slot, old, new) in &changed {
        println!("{stamp} slot {slot}: {old:?} -> {new:?}");
    }
}

/// Slot rectangles and the name reading each slot produced live.
fn parse_readings(text: &str) -> (Vec<SlotRect>, BTreeMap<u8, String>) {
    let mut rects = Vec::new();
    let mut names = BTreeMap::new();
    let mut slot: Option<u8> = None;
    let mut in_name_band = false;

    for line in text.lines() {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("slot ")
            && t.contains("rect")
        {
            let id: u8 = rest.split_whitespace().next().and_then(|s| s.parse().ok()).unwrap_or(0);
            let field = |key: &str| -> i64 {
                rest.split_whitespace()
                    .find_map(|tok| tok.strip_prefix(key))
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0)
            };
            rects.push((id, field("x=") as i32, field("y=") as i32, field("w=") as u32, field("h=") as u32));
            slot = Some(id);
            in_name_band = false;
        } else if t.starts_with("name") {
            in_name_band = true;
        } else if t.starts_with("health") {
            in_name_band = false;
        } else if in_name_band
            && let Some(rest) = t.strip_prefix("text=\"")
            && let Some(value) = rest.strip_suffix('"')
            && let Some(slot) = slot
        {
            if !value.is_empty() {
                names.insert(slot, value.to_string());
            }
            in_name_band = false;
        }
    }
    (rects, names)
}

fn load_capture(path: &Path) -> Option<CapturedImage> {
    let decoder = png::Decoder::new(File::open(path).ok()?);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());

    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        _ => return None,
    };
    Some(CapturedImage {
        width: info.width,
        height: info.height,
        rgba,
    })
}

fn shellexpand(arg: &str) -> String {
    match (arg.strip_prefix("~/"), dirs::home_dir()) {
        (Some(rest), Some(home)) => home.join(rest).to_string_lossy().into_owned(),
        _ => arg.to_string(),
    }
}
