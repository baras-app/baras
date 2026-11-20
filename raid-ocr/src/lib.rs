//! OCR orchestration for SWTOR raid frames.
//!
//! This is separate from Tauri so recorded frames can run through the same
//! pipeline as live captures. Unreadable rows stay unassigned.

pub mod analysis;
pub mod cpu;
pub mod debug_dump;
pub mod engine;
mod runtime;

pub use debug_dump::DebugDump;
pub use runtime::{configure_threads, release_memory};

use std::time::Instant;

use analysis::{
    Band, BandKind, BarPosition, PreparedCrop, detect_health_bar, harmonize_names, parse_health_text, prepare,
    reconcile_bars, slot_bands,
};
use baras_core::raid_detect::{
    HEALTH_RESCUE_FLOOR, MIN_OCR_NAME_CHARS, MatchConfig, PlayerCandidate, RowAssignment, RowDecision, RowObservation,
    assign_rows_explained,
};
use baras_overlay::capture::CapturedImage;

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
        Some((first, rest)) if first.chars().count() < 3 && !rest.trim().is_empty() => rest.trim().to_string(),
        _ => trimmed.to_string(),
    }
}

/// Read the names in a captured overlay region. Empty slots are omitted.
pub fn observe_slots(image: &CapturedImage, slots: &[SlotRect]) -> Reading {
    observe_slots_dumping(image, slots, None)
}

/// [`observe_slots`], optionally recording every crop and reading to disk.
///
/// The dump lives here because this is the only place a crop and the text it
/// produced exist at the same time.
pub fn observe_slots_dumping(image: &CapturedImage, slots: &[SlotRect], mut dump: Option<&mut DebugDump>) -> Reading {
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
    //
    // Slots holding no frame are not weeded out here: a dead player's cell is
    // dimmed until nothing geometric distinguishes it from bare scenery, so
    // every slot gets its crop and the text detector arbitrates — background
    // yields a crop with no text and drops out on its own.
    harmonize_names(&mut per_slot_bands, &bars, &inferred);

    // Prepare everything first: crops can only overlap once they all exist.
    // Bands outside their slot stay `None` so readings keep lining up.
    let mut crops: Vec<(usize, Band, Option<PreparedCrop>)> = Vec::new();
    for (slot_index, (slot_image, slot_bands)) in slot_images.iter().zip(&per_slot_bands).enumerate() {
        let Some(slot_image) = slot_image else {
            continue;
        };
        for band in slot_bands {
            crops.push((slot_index, *band, prepare(slot_image, band)));
        }
    }

    let detection_started = Instant::now();
    let (trimmed, textless) = narrow_names(&mut crops);
    let detection_ms = detection_started.elapsed().as_millis() as u64;

    // Health is a second look, so set its crops aside rather than read them.
    let mut held: Vec<HeldHealth> = Vec::new();
    for (slot_index, band, crop) in &crops {
        if band.kind == BandKind::Health
            && let Some(crop) = crop
        {
            held.push(HeldHealth {
                row: slots[*slot_index].0 as usize,
                band: *band,
                crop: crop.clone(),
            });
        }
    }
    crops.retain(|(_, band, _)| band.kind == BandKind::Name);

    let recognition_started = Instant::now();
    let readings = recognize_all(&crops);
    let recognition_ms = recognition_started.elapsed().as_millis() as u64;

    tracing::info!(
        detection_ms,
        recognition_ms,
        names = crops.len(),
        trimmed,
        rejected = textless.len(),
        health_held = held.len(),
        "Read raid frame names"
    );

    let mut observations = Vec::new();
    let mut states: Vec<(u8, SlotState)> = Vec::with_capacity(slots.len());
    let mut next = 0;

    for ((slot_index, slot_rect), slot_bands) in slots.iter().enumerate().zip(&per_slot_bands) {
        if slot_images[slot_index].is_none() {
            continue;
        }

        if let Some(dump) = dump.as_deref_mut() {
            dump.slot(slot_rect.0, (slot_rect.1, slot_rect.2, slot_rect.3, slot_rect.4));
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

        // Empty is the detector's verdict, not geometry's: no bar of its own
        // and no text anywhere in its crop.
        let state = if observation.name_text.is_some() {
            SlotState::Read
        } else if inferred[slot_index] && textless.contains(&slot_index) {
            SlotState::Empty
        } else {
            SlotState::Unreadable
        };
        states.push((slot_rect.0, state));

        if saw_anything || held.iter().any(|h| h.row == observation.row) {
            observations.push(observation);
        }
    }

    // Per slot: a count cannot show "slot 3 is always empty".
    let layout: String = states
        .iter()
        .map(|(_, s)| match s {
            SlotState::Read => 'R',
            SlotState::Unreadable => 'U',
            SlotState::Empty => '.',
        })
        .collect();
    tracing::info!(
        read = states.iter().filter(|(_, s)| *s == SlotState::Read).count(),
        unreadable = states.iter().filter(|(_, s)| *s == SlotState::Unreadable).count(),
        empty = states.iter().filter(|(_, s)| *s == SlotState::Empty).count(),
        slots = %layout,
        "Raid frame slots"
    );

    Reading {
        rows: observations,
        states,
        held,
    }
}

/// One pass over the frames, health prepared but not yet read.
pub struct Reading {
    pub rows: Vec<RowObservation>,
    /// What each slot turned out to hold, in slot order.
    pub states: Vec<(u8, SlotState)>,
    held: Vec<HeldHealth>,
}

/// What a slot turned out to hold.
///
/// A slot with no bar of its own and no text anywhere in its crop is empty,
/// not a failed reading: telling a user we could not read four names when the
/// group is four players short is wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotState {
    /// A name was read.
    Read,
    /// A frame is there, the name is not legible.
    Unreadable,
    /// No frame here.
    Empty,
}

/// A health crop prepared but not recognized.
struct HeldHealth {
    row: usize,
    band: Band,
    crop: PreparedCrop,
}

impl Reading {
    /// Read health for `rows`. Crops read here are dropped, so asking twice is
    /// free.
    pub fn read_health(&mut self, rows: &[usize], mut dump: Option<&mut DebugDump>) -> usize {
        let wanted: Vec<usize> = (0..self.held.len())
            .filter(|&i| rows.contains(&self.held[i].row))
            .collect();
        if wanted.is_empty() {
            return 0;
        }

        let crops: Vec<&PreparedCrop> = wanted.iter().map(|&i| &self.held[i].crop).collect();
        let readings = spread(engine::recognize_many(&crops), crops.len());

        let mut read = 0;
        for (&i, reading) in wanted.iter().zip(readings) {
            let held = &self.held[i];
            let Some(row) = self.rows.iter_mut().find(|r| r.row == held.row) else {
                continue;
            };
            let text = match reading {
                Ok(text) if !text.trim().is_empty() => text,
                other => {
                    if let Some(dump) = dump.as_deref_mut() {
                        let why = match other {
                            Err(e) => format!("unreadable: {e}"),
                            Ok(_) => "empty reading".to_string(),
                        };
                        dump.band(held.row as u8, &held.band, &held.crop, "", &why);
                    }
                    continue;
                }
            };
            let (value, percent) = parse_health_text(&text);
            if let Some(dump) = dump.as_deref_mut() {
                let outcome = format!(
                    "value={} percent={}",
                    value.map_or("-".into(), |v: u32| v.to_string()),
                    percent.map_or("-".into(), |p: u8| p.to_string())
                );
                dump.band(held.row as u8, &held.band, &held.crop, &text, &outcome);
            }
            row.hp_value = value;
            row.hp_percent = percent;
            read += 1;
        }

        // Drop what we read, keep the rest.
        let mut keep = wanted.iter().peekable();
        let mut index = 0;
        self.held.retain(|_| {
            let drop = keep.peek() == Some(&&index);
            if drop {
                keep.next();
            }
            index += 1;
            !drop
        });
        read
    }
}

/// One reading per crop from a batched result. The batch fails as a whole, so
/// a failure is reported against every crop in it.
fn spread(batch: Result<Vec<String>, engine::OcrError>, len: usize) -> Vec<Result<String, engine::OcrError>> {
    match batch {
        Ok(texts) => texts.into_iter().map(Ok).collect(),
        Err(e) => std::iter::repeat_n(Err(e), len).collect(),
    }
}

/// Narrow name crops to the columns detection finds text in, dropping those
/// with none. Health digits sit on the bar, already bounded, so they are left.
///
/// Returns crops trimmed, and slots with no text. Different outcomes, counted
/// apart.
fn narrow_names(crops: &mut [(usize, Band, Option<PreparedCrop>)]) -> (usize, Vec<usize>) {
    if engine::warm().is_err() {
        return (0, Vec::new());
    }

    // Which entries to ask about, and the crops themselves. Detection packs
    // them onto one canvas, so they go in together rather than one per thread.
    let asked: Vec<usize> = crops
        .iter()
        .enumerate()
        .filter(|(_, e)| e.1.kind == BandKind::Name && e.2.is_some())
        .map(|(i, _)| i)
        .collect();
    let batch: Vec<&PreparedCrop> = asked.iter().filter_map(|&i| crops[i].2.as_ref()).collect();

    let mut changed = 0;
    let mut textless = Vec::new();
    for (&i, span) in asked.iter().zip(engine::text_spans(&batch)) {
        match span {
            Some(Some((left, right))) => {
                if let Some(crop) = crops[i].2.as_ref().and_then(|c| c.narrowed(left, right)) {
                    crops[i].2 = Some(crop);
                    changed += 1;
                }
            }
            // No text anywhere in the crop: an empty frame, not a trim.
            Some(None) => {
                crops[i].2 = None;
                textless.push(crops[i].0);
            }
            // Undecided; keep the crop whole rather than lose the slot.
            None => {}
        }
    }
    (changed, textless)
}

/// Reads every crop in one batch. Lines up with `crops`, `None` where there
/// was nothing to read.
fn recognize_all(crops: &[(usize, Band, Option<PreparedCrop>)]) -> Vec<Option<Result<String, engine::OcrError>>> {
    let present: Vec<&PreparedCrop> = crops.iter().filter_map(|e| e.2.as_ref()).collect();
    let mut readings = spread(engine::recognize_many(&present), present.len()).into_iter();
    crops
        .iter()
        .map(|entry| entry.2.as_ref().and_then(|_| readings.next()))
        .collect()
}

/// Full detection pass: read the screen, then match against known players.
pub fn detect(image: &CapturedImage, slots: &[SlotRect], candidates: &[PlayerCandidate]) -> Vec<RowAssignment> {
    let mut reading = observe_slots(image, slots);
    tracing::debug!(
        "Raid detection read {} of {} slots against {} candidates",
        reading.rows.len(),
        slots.len(),
        candidates.len()
    );
    assign_with_health(&mut reading, candidates, None).0
}

/// Match on names, then re-match the unsettled rows with their health read.
///
/// Health is only worth its recognition pass where names could not decide: a
/// tie between two candidates, or a name too weak to stand alone.
pub fn assign_with_health(
    reading: &mut Reading,
    candidates: &[PlayerCandidate],
    dump: Option<&mut DebugDump>,
) -> (Vec<RowAssignment>, Vec<RowDecision>) {
    let config = MatchConfig::default();
    let (assignments, decisions) = assign_rows_explained(&reading.rows, candidates, &config);

    let retry = rows_needing_health(&decisions, &config);
    if retry.is_empty() {
        return (assignments, decisions);
    }

    let read = reading.read_health(&retry, dump);
    if read == 0 {
        tracing::info!(rows = retry.len(), read, "Re-read health for undecided rows");
        return (assignments, decisions);
    }

    let (retried, retried_decisions) =
        assign_rows_explained(&reading.rows, candidates, &config.clone().with_health_rescue());

    // Whether health earns its pass is still open; this answers it live.
    let before: Vec<(usize, i64)> = assignments.iter().map(|a| (a.row, a.entity_id)).collect();
    let after: Vec<(usize, i64)> = retried.iter().map(|a| (a.row, a.entity_id)).collect();
    let gained = after.iter().filter(|a| !before.contains(a)).count();
    let lost = before.iter().filter(|b| !after.contains(b)).count();
    tracing::info!(
        rows = retry.len(),
        read,
        gained,
        lost,
        "Re-read health for undecided rows"
    );

    (retried, retried_decisions)
}

/// Rows a second signal could still settle.
fn rows_needing_health(decisions: &[RowDecision], config: &MatchConfig) -> Vec<usize> {
    decisions
        .iter()
        .filter(|d| {
            let Some(best) = d.best.as_ref() else {
                return false;
            };
            // Two candidates within the margin, or one that almost cleared the bar.
            best.total - d.runner_up < config.min_margin
                || (best.name_score >= HEALTH_RESCUE_FLOOR && best.name_score < config.min_confidence)
        })
        .map(|d| d.row)
        .collect()
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
