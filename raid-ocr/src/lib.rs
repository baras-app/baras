//! OCR orchestration for SWTOR raid frames.
//!
//! This is separate from Tauri so recorded frames can run through the same
//! pipeline as live captures. Unreadable rows stay unassigned.

pub mod analysis;
pub mod cpu;
pub mod debug_dump;
pub mod engine;

pub use debug_dump::DebugDump;

use std::sync::OnceLock;

use rayon::prelude::*;

use analysis::{
    Band, BandKind, BarPosition, PreparedCrop, detect_health_bar, harmonize_names,
    parse_health_text, prepare, reconcile_bars, slot_bands,
};
use baras_core::raid_detect::{
    MIN_OCR_NAME_CHARS, MatchConfig, PlayerCandidate, RowAssignment, RowObservation,
};
use baras_overlay::capture::CapturedImage;

/// Crops kept in flight during recognition.
///
/// This is the queue depth, not the number of threads, but the amount of crops
/// 'in-flight' at the same time.
const RECOGNITION_JOBS: usize = 8;

/// A slot rectangle within the captured overlay image.
pub type SlotRect = (u8, i32, i32, u32, u32);

/// Usable slot labels when there is no combat-log roster to match against.
pub fn ocr_only_names(observations: &[RowObservation]) -> Vec<(u8, String)> {
    observations
        .iter()
        .filter_map(|row| {
            let name = clean_for_display(row.name_text.as_deref()?);
            if baras_core::raid_detect::normalize(&name).len() < MIN_OCR_NAME_CHARS {
                return None;
            }
            Some((u8::try_from(row.row).ok()?, name))
        })
        .collect()
}

/// Tidy a raw reading for display.
///
/// Provisional names are shown to the user verbatim, so the stray glyph the
/// crop's left edge picks up — `[VENNDRICK`, `|AZURIS` — reaches the screen and
/// reads as a bug even though matching discards it. SWTOR names contain only
/// letters, one space, one hyphen and up to two apostrophes, so anything else at
/// either end cannot belong to a real name and is safe to drop.
///
/// Only the ends are touched. A reading is still a guess, and rewriting the
/// middle would make a wrong guess harder to recognise as wrong.
fn clean_for_display(text: &str) -> String {
    let trimmed = text.trim_matches(|c: char| !c.is_alphabetic());

    // The other shape this artifact takes is a lone leading letter before a
    // space (`I CHIE SIA`). SWTOR requires at least three characters either side
    // of a space, so a one- or two-letter first part cannot be genuine.
    match trimmed.split_once(' ') {
        Some((first, rest)) if first.chars().count() < 3 && !rest.trim().is_empty() => {
            rest.trim().to_string()
        }
        _ => trimmed.to_string(),
    }
}

/// Read every slot in a captured overlay region.
///
/// Empty slots are omitted.
pub fn observe_slots(image: &CapturedImage, slots: &[SlotRect]) -> Vec<RowObservation> {
    observe_slots_dumping(image, slots, None)
}

/// [`observe_slots`], optionally recording every crop and reading to disk.
///
/// The dump lives here because this is the only place a crop and the text it
/// produced exist at the same time.
pub fn observe_slots_dumping(
    image: &CapturedImage,
    slots: &[SlotRect],
    mut dump: Option<&mut DebugDump>,
) -> Vec<RowObservation> {
    if let Some(dump) = dump.as_deref_mut() {
        dump.capture(image);
    }

    // Find every bar first, so slots that lost theirs can borrow the grid's.
    let mut slot_images = Vec::with_capacity(slots.len());
    let mut bars: Vec<Option<BarPosition>> = Vec::with_capacity(slots.len());

    for &(_, x, y, w, h) in slots {
        match image.crop(x, y, w, h) {
            Some(slot_image) => {
                bars.push(detect_health_bar(&slot_image));
                slot_images.push(Some(slot_image));
            }
            None => {
                bars.push(None);
                slot_images.push(None);
            }
        }
    }

    let inferred = reconcile_bars(&mut bars);

    let mut per_slot_bands: Vec<Vec<Band>> = slot_images
        .iter()
        .zip(&bars)
        .zip(&inferred)
        .map(|((slot_image, bar), inferred)| {
            slot_image
                .as_ref()
                .map_or_else(Vec::new, |slot| slot_bands(slot, *bar, *inferred))
        })
        .collect();

    // Names only: the bars were already reconciled. Positions are compared
    // relative to each slot's bar, so a borrowed bar never acts as an anchor.
    harmonize_names(&mut per_slot_bands, &bars, &inferred);

    // Prepare everything first: crops can only overlap once they all exist.
    // Bands outside their slot stay `None` so readings keep lining up.
    let mut crops: Vec<(usize, Band, Option<PreparedCrop>)> = Vec::new();
    for (slot_index, (slot_image, slot_bands)) in
        slot_images.iter().zip(&per_slot_bands).enumerate()
    {
        let Some(slot_image) = slot_image else {
            continue;
        };
        for band in slot_bands {
            crops.push((slot_index, *band, prepare(slot_image, band)));
        }
    }

    let readings = recognize_all(&crops);

    let mut observations = Vec::new();
    let mut next = 0;

    for ((slot_index, slot_rect), slot_bands) in slots.iter().enumerate().zip(&per_slot_bands) {
        if slot_images[slot_index].is_none() {
            continue;
        }

        if let Some(dump) = dump.as_deref_mut() {
            dump.slot(
                slot_rect.0,
                (slot_rect.1, slot_rect.2, slot_rect.3, slot_rect.4),
            );
            dump.bar(slot_rect.0, bars[slot_index], inferred[slot_index]);
            if slot_bands.is_empty() {
                dump.no_bands(slot_rect.0);
            }
        }

        let mut observation = RowObservation {
            row: slot_rect.0 as usize,
            ..Default::default()
        };
        let mut saw_anything = false;

        while let Some((_, band, crop)) = crops.get(next).filter(|entry| entry.0 == slot_index) {
            let reading = readings[next].as_ref();
            next += 1;

            let (Some(crop), Some(reading)) = (crop.as_ref(), reading) else {
                if let Some(dump) = dump.as_deref_mut() {
                    dump.band_skipped(band, "band lies outside the slot");
                }
                continue;
            };
            let text = match reading {
                Ok(text) if !text.trim().is_empty() => text,
                Ok(_) => {
                    if let Some(dump) = dump.as_deref_mut() {
                        dump.band(slot_rect.0, band, crop, "", "empty reading");
                    }
                    continue;
                }
                Err(e) => {
                    tracing::debug!("Slot {} band {:?} unreadable: {e}", slot_rect.0, band.kind);
                    if let Some(dump) = dump.as_deref_mut() {
                        dump.band(slot_rect.0, band, crop, "", &format!("unreadable: {e}"));
                    }
                    continue;
                }
            };

            match band.kind {
                BandKind::Name => {
                    if let Some(dump) = dump.as_deref_mut() {
                        let outcome = format!("display={:?}", clean_for_display(text));
                        dump.band(slot_rect.0, band, crop, text, &outcome);
                    }
                    observation.name_text = Some(text.clone());
                    saw_anything = true;
                }
                BandKind::Health => {
                    let (value, percent) = parse_health_text(text);
                    if let Some(dump) = dump.as_deref_mut() {
                        let outcome = match (value, percent) {
                            (None, None) => "parsed nothing".to_string(),
                            (v, p) => format!(
                                "value={} percent={}",
                                v.map_or("-".into(), |v| v.to_string()),
                                p.map_or("-".into(), |p| p.to_string())
                            ),
                        };
                        dump.band(slot_rect.0, band, crop, text, &outcome);
                    }
                    observation.hp_value = value;
                    observation.hp_percent = percent;
                    saw_anything |= value.is_some() || percent.is_some();
                }
            }
        }

        if saw_anything {
            observations.push(observation);
        }
    }

    observations
}

/// `None` when the pool cannot be built.
/// Fallback:read one at a time rather than fail.
fn recognition_pool() -> Option<&'static rayon::ThreadPool> {
    static POOL: OnceLock<Option<rayon::ThreadPool>> = OnceLock::new();
    POOL.get_or_init(|| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(RECOGNITION_JOBS)
            .thread_name(|index| format!("baras-ocr-{index}"))
            .build()
            .inspect_err(|e| tracing::warn!("Reading crops one at a time: {e}"))
            .ok()
    })
    .as_ref()
}

/// Reads every crop. Lines up with `crops`, `None` where there was nothing to read.
fn recognize_all(
    crops: &[(usize, Band, Option<PreparedCrop>)],
) -> Vec<Option<Result<String, engine::OcrError>>> {
    fn read(entry: &(usize, Band, Option<PreparedCrop>)) -> Option<Result<String, engine::OcrError>> {
        entry.2.as_ref().map(engine::recognize)
    }

    // Load the model before spreading out, never inside a worker: see `warm`.
    match (engine::warm(), recognition_pool()) {
        (Ok(()), Some(pool)) => pool.install(|| crops.par_iter().map(read).collect()),
        _ => crops.iter().map(read).collect(),
    }
}

/// Full detection pass: read the screen, then match against known players.
pub fn detect(
    image: &CapturedImage,
    slots: &[SlotRect],
    candidates: &[PlayerCandidate],
) -> Vec<RowAssignment> {
    let observations = observe_slots(image, slots);
    tracing::debug!(
        "Raid detection read {} of {} slots against {} candidates",
        observations.len(),
        slots.len(),
        candidates.len()
    );
    baras_core::raid_detect::assign_rows(&observations, candidates, &MatchConfig::default())
}

#[cfg(test)]
mod display_tests {
    use super::clean_for_display;

    #[test]
    fn strips_the_left_edge_artifact() {
        // Observed live: the crop's left edge yields a stray glyph.
        assert_eq!(clean_for_display("[VENNDRICK"), "VENNDRICK");
        assert_eq!(clean_for_display("|AZURIS"), "AZURIS");
        assert_eq!(clean_for_display("|MORE ORDER"), "MORE ORDER");
        assert_eq!(clean_for_display("[VENNDRICK $"), "VENNDRICK");
    }

    #[test]
    fn strips_a_lone_leading_letter_before_a_space() {
        // SWTOR requires three characters either side of a space, so a one-letter
        // first part is an artifact rather than a name part.
        assert_eq!(clean_for_display("I CHIE SIA"), "CHIE SIA");
        assert_eq!(clean_for_display("ISOA"), "ISOA", "no space, leave it alone");
    }

    #[test]
    fn leaves_legitimate_names_untouched() {
        assert_eq!(clean_for_display("FRYKANYSUYA"), "FRYKANYSUYA");
        assert_eq!(clean_for_display("MORE ORDER"), "MORE ORDER");
        assert_eq!(clean_for_display("Test Long-name"), "Test Long-name");
        // Interior punctuation is part of the name and must survive.
        assert_eq!(clean_for_display("T'EST ALPHA"), "T'EST ALPHA");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ocr_only_mode_keeps_names_without_inventing_entities() {
        let observations = vec![
            RowObservation {
                row: 2,
                name_text: Some("  PLAYER 8K  ".into()),
                hp_value: Some(493_172),
                hp_percent: Some(100),
            },
            RowObservation {
                row: 3,
                name_text: Some("DS".into()),
                ..Default::default()
            },
            RowObservation {
                row: 4,
                name_text: Some("BOT".into()),
                ..Default::default()
            },
            RowObservation {
                row: 300,
                name_text: Some("OUT OF RANGE".into()),
                ..Default::default()
            },
        ];

        assert_eq!(
            ocr_only_names(&observations),
            vec![(2, "PLAYER 8K".into()), (4, "BOT".into())]
        );
    }
}
