//! Locate the health bar in a raid-frame slot and derive the text bands from it.
//!
//! Every cell of an ops frame is stamped from the same template: name at the
//! top left, health bar under it. Earlier revisions searched for both bands by
//! row-profiling ink, which read health digits as names whenever the bar's own
//! text split its red profile. The bar is the one element with a fixed colour
//! and the layout around it is fixed, so detection reduces to finding red and
//! deriving the rest geometrically.
//!
//! The bar drains right-to-left, so whatever red remains is anchored at its
//! left edge: scanning a narrow strip at the left of the cell finds the bar at
//! any fill level. Slots with no red left (dead players, empty frames) borrow
//! the position the other slots agree on, since every cell shares one layout.

use baras_overlay::capture::CapturedImage;

/// A horizontal strip within a slot believed to contain text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Band {
    /// Offset from the top of the slot, in pixels.
    pub top: u32,
    /// Height of the strip, in pixels.
    pub height: u32,
    pub kind: BandKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BandKind {
    /// Light text on the dark frame background.
    Name,
    /// Light text on the red health bar.
    Health,
}

/// The health bar's position within a slot: `(top, height)` in pixels.
pub type BarPosition = (u32, u32);

/// Portion of the cell width searched for the bar's red remnant. The bar sits
/// behind a border and a shaded left bevel of unpredictable width, so the
/// search area is generous; what identifies a bar row is a solid red run, not
/// the area's average colour.
const LEFT_SEARCH_FRACTION: f32 = 0.25;
/// Consecutive red (or, in a text gap, glyph-bright) pixels that mark a row.
const MIN_RUN_PX: u32 = 3;
/// A pixel counts as bar red past this much dominance over green and blue.
const RED_DOMINANCE: i32 = 30;
/// Anything thinner is a stray red pixel row, not a bar.
const MIN_BAR_HEIGHT: u32 = 3;
/// Bars thinner than this cannot render health text; recognizing them only
/// invites hallucinated digits from a blank crop.
const MIN_TEXT_BAR_HEIGHT: u32 = 8;
/// Name bands thinner than this cannot hold a legible glyph.
const MIN_NAME_HEIGHT: u32 = 4;
/// Rows skipped at the very top of the cell: the frame border.
const TOP_INSET: u32 = 1;
/// Ignore the right edge when cropping names. Raid markers and buff icons live there.
pub(super) const NAME_SCAN_FRACTION: f32 = 0.75;
/// Fewer detected bars than this and an outlier cannot be told from the truth.
const MIN_CONSENSUS_SLOTS: usize = 3;

/// Bar red, as distinct from everything else red-ish in a cell.
///
/// Dominance alone also accepts the orange resource strip under the bar; its
/// green sits near half of red where the bar's is far below, so capping green
/// separates them.
fn is_bar_red(r: u8, g: u8, b: u8) -> bool {
    (r as i32 - g as i32) > RED_DOMINANCE
        && (r as i32 - b as i32) > RED_DOMINANCE
        && (g as u32) * 2 < r as u32
}

/// Glyph ink is bright regardless of what it sits on.
const BRIGHT_LUMA: u8 = 150;

fn luma(r: u8, g: u8, b: u8) -> u8 {
    (((r as u32 * 77) + (g as u32 * 150) + (b as u32 * 29)) >> 8) as u8
}

/// Whether the left search area of a row holds a run of pixels satisfying
/// `keep`, `MIN_RUN_PX` or longer.
fn has_run(
    slot: &CapturedImage,
    y: u32,
    search: u32,
    keep: impl Fn(u8, u8, u8) -> bool,
) -> bool {
    let mut run = 0u32;
    for x in 0..search {
        let Some((r, g, b, _)) = slot.pixel(x, y) else {
            continue;
        };
        if keep(r, g, b) {
            run += 1;
            if run >= MIN_RUN_PX {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}

/// Find the health bar by its red remnant near the cell's left edge.
///
/// The bar is the longest vertical extent of rows holding a solid red run.
/// Health digits print over the bar's left edge and can carve bright gaps into
/// its red profile, so red runs are bridged across short gaps whose rows still
/// hold glyph-bright runs — the dark space above the bar never qualifies.
/// Returns `None` for a slot with no bar to see: dead, empty, or drained out
/// of the search area.
pub fn detect_health_bar(slot: &CapturedImage) -> Option<BarPosition> {
    if slot.width == 0 || slot.height < MIN_BAR_HEIGHT {
        return None;
    }
    let search = ((slot.width as f32 * LEFT_SEARCH_FRACTION).round() as u32)
        .max(MIN_RUN_PX)
        .min(slot.width);

    // Maximal runs of red rows.
    let mut runs: Vec<BarPosition> = Vec::new();
    let mut start: Option<u32> = None;
    for y in 0..=slot.height {
        let in_bar = y < slot.height && has_run(slot, y, search, is_bar_red);
        if in_bar {
            start.get_or_insert(y);
        } else if let Some(s) = start.take() {
            runs.push((s, y - s));
        }
    }

    // Bridge across text rows. The cap keeps a stray red mark higher in the
    // cell from chaining through bright name glyphs down to the bar.
    let max_gap = slot.height / 4;
    let mut merged: Vec<BarPosition> = Vec::new();
    for (top, height) in runs {
        match merged.last_mut() {
            Some((prev_top, prev_height))
                if top - (*prev_top + *prev_height) <= max_gap
                    && (*prev_top + *prev_height..top).all(|y| {
                        has_run(slot, y, search, |r, g, b| {
                            is_bar_red(r, g, b) || luma(r, g, b) >= BRIGHT_LUMA
                        })
                    }) =>
            {
                *prev_height = top + height - *prev_top;
            }
            _ => merged.push((top, height)),
        }
    }

    merged
        .into_iter()
        .max_by_key(|&(_, height)| height)
        .filter(|&(_, height)| height >= MIN_BAR_HEIGHT)
}

/// Reconcile bar positions across the grid.
///
/// Cells are uniform, so one detected bar places every other one. Missing bars
/// take the median of those found; with enough of a consensus, a bar far from
/// it is an imposter — a red marker or icon that strayed into the strip — and
/// is snapped back. Returns which slots hold an inferred rather than seen bar:
/// those are positioned well enough to read the name above them, but their bar
/// carries no red, so recognizing its health band would read a blank crop.
pub fn reconcile_bars(bars: &mut [Option<BarPosition>]) -> Vec<bool> {
    let mut inferred = vec![false; bars.len()];
    let found: Vec<BarPosition> = bars.iter().flatten().copied().collect();
    if found.is_empty() {
        return inferred;
    }

    let top = median(found.iter().map(|&(top, _)| top));
    let height = median(found.iter().map(|&(_, height)| height));

    if found.len() >= MIN_CONSENSUS_SLOTS {
        let tolerance = (height / 2).max(2);
        for (bar, flag) in bars.iter_mut().zip(&mut inferred) {
            if let Some((t, h)) = *bar
                && (t.abs_diff(top) > tolerance || h.abs_diff(height) > tolerance)
            {
                *bar = Some((top, height));
                *flag = true;
            }
        }
    }

    for (bar, flag) in bars.iter_mut().zip(&mut inferred) {
        if bar.is_none() {
            *bar = Some((top, height));
            *flag = true;
        }
    }

    inferred
}

/// The bands a slot's bar position implies.
///
/// The name is everything between the cell's top border and the bar; once the
/// bar is known its position needs no detecting. The health band is only worth
/// recognizing when the bar was actually seen and is tall enough to render
/// text.
pub fn slot_bands(bar: Option<BarPosition>, inferred: bool) -> Vec<Band> {
    let Some((top, height)) = bar else {
        return Vec::new();
    };
    let mut bands = Vec::new();

    // A real name row scales with the frame, so a strip much thinner than the
    // bar is a misaligned grid's sliver, not a name; reading it yields garbage
    // that would surface as a provisional label.
    let min_name = MIN_NAME_HEIGHT.max(height / 3);
    let name_top = TOP_INSET.min(top);
    if top - name_top >= min_name {
        bands.push(Band {
            top: name_top,
            height: top - name_top,
            kind: BandKind::Name,
        });
    }

    if !inferred && height >= MIN_TEXT_BAR_HEIGHT {
        bands.push(Band {
            top,
            height,
            kind: BandKind::Health,
        });
    }

    bands
}

/// Lower middle on an even count, so one tall outlier cannot drag the
/// consensus toward itself.
fn median(values: impl Iterator<Item = u32>) -> u32 {
    let mut sorted: Vec<u32> = values.collect();
    sorted.sort_unstable();
    sorted[(sorted.len() - 1) / 2]
}

#[cfg(test)]
mod tests {
    use super::*;

    const BAR_RED: (u8, u8, u8) = (200, 35, 45);
    const RESOURCE_ORANGE: (u8, u8, u8) = (220, 160, 50);

    /// A dark cell, roughly how SWTOR draws an ops frame background.
    fn cell(width: u32, height: u32) -> CapturedImage {
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for _ in 0..(width * height) {
            rgba.extend_from_slice(&[30, 40, 60, 255]);
        }
        CapturedImage {
            width,
            height,
            rgba,
        }
    }

    fn paint(
        image: &mut CapturedImage,
        x: std::ops::Range<u32>,
        y: std::ops::Range<u32>,
        rgb: (u8, u8, u8),
    ) {
        for y in y {
            for x in x.clone() {
                let i = ((y * image.width + x) * 4) as usize;
                image.rgba[i..i + 4].copy_from_slice(&[rgb.0, rgb.1, rgb.2, 255]);
            }
        }
    }

    #[test]
    fn finds_a_full_bar() {
        let mut slot = cell(100, 40);
        paint(&mut slot, 0..100, 20..30, BAR_RED);

        assert_eq!(detect_health_bar(&slot), Some((20, 10)));
    }

    #[test]
    fn finds_a_nearly_drained_bar_by_its_left_remnant() {
        let mut slot = cell(100, 40);
        // ~8% health: red only survives at the bar's left edge.
        paint(&mut slot, 0..8, 20..30, BAR_RED);

        assert_eq!(detect_health_bar(&slot), Some((20, 10)));
    }

    /// Observed live: left-aligned health text carves the strip's red profile
    /// into two slivers, and taking either sliver as the bar puts the digits
    /// into the name band.
    #[test]
    fn digits_over_the_left_edge_do_not_split_the_bar() {
        let mut slot = cell(100, 40);
        paint(&mut slot, 0..100, 20..32, BAR_RED);
        // "402,614 (84%)" starting at the bar's left edge.
        paint(&mut slot, 2..60, 23..29, (235, 235, 235));

        assert_eq!(detect_health_bar(&slot), Some((20, 12)));
    }

    #[test]
    fn the_dark_gap_above_the_bar_is_never_bridged() {
        let mut slot = cell(100, 40);
        // A red marker high in the strip, dark background, then the bar.
        paint(&mut slot, 0..12, 4..8, BAR_RED);
        paint(&mut slot, 0..100, 20..30, BAR_RED);

        assert_eq!(detect_health_bar(&slot), Some((20, 10)));
    }

    #[test]
    fn the_orange_resource_strip_is_not_a_bar() {
        let mut slot = cell(100, 40);
        paint(&mut slot, 0..100, 32..36, RESOURCE_ORANGE);

        assert_eq!(detect_health_bar(&slot), None, "orange is not the bar");
    }

    #[test]
    fn the_resource_strip_does_not_stretch_the_bar_downward() {
        let mut slot = cell(100, 40);
        paint(&mut slot, 0..100, 20..30, BAR_RED);
        paint(&mut slot, 0..100, 30..34, RESOURCE_ORANGE);

        assert_eq!(detect_health_bar(&slot), Some((20, 10)));
    }

    #[test]
    fn a_marker_away_from_the_left_edge_is_never_seen() {
        let mut slot = cell(100, 40);
        paint(&mut slot, 0..100, 20..30, BAR_RED);
        // A red raid marker over the name area, right of the strip.
        paint(&mut slot, 40..60, 2..12, (220, 20, 20));

        assert_eq!(detect_health_bar(&slot), Some((20, 10)));
    }

    #[test]
    fn an_empty_cell_yields_nothing() {
        assert_eq!(detect_health_bar(&cell(100, 40)), None);
    }

    #[test]
    fn a_stray_red_row_is_too_thin_to_be_a_bar() {
        let mut slot = cell(100, 40);
        paint(&mut slot, 0..100, 20..22, BAR_RED);

        assert_eq!(detect_health_bar(&slot), None);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // reconcile_bars
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn a_missing_bar_borrows_the_grid_consensus() {
        let mut bars = vec![Some((20, 10)), Some((21, 10)), Some((20, 10)), None];
        let inferred = reconcile_bars(&mut bars);

        assert_eq!(bars[3], Some((20, 10)));
        assert_eq!(inferred, vec![false, false, false, true]);
    }

    #[test]
    fn an_imposter_is_snapped_back_and_marked_inferred() {
        // Slot 2 latched onto a red icon high in the cell.
        let mut bars = vec![Some((20, 10)), Some((20, 10)), Some((4, 9)), Some((21, 10))];
        let inferred = reconcile_bars(&mut bars);

        assert_eq!(bars[2], Some((20, 10)));
        assert!(inferred[2], "a corrected bar holds no red worth reading");
        assert!(!inferred[0] && !inferred[3], "agreeing slots are left alone");
    }

    #[test]
    fn two_bars_still_seed_the_missing_but_correct_no_outliers() {
        let mut bars = vec![Some((20, 10)), Some((5, 10)), None];
        let inferred = reconcile_bars(&mut bars);

        // Too few to call either an imposter; the lower median seeds the gap.
        assert_eq!(bars[0], Some((20, 10)));
        assert_eq!(bars[1], Some((5, 10)));
        assert_eq!(bars[2], Some((5, 10)));
        assert_eq!(inferred, vec![false, false, true]);
    }

    #[test]
    fn a_grid_with_no_bars_stays_empty() {
        let mut bars = vec![None, None, None];
        let inferred = reconcile_bars(&mut bars);

        assert!(bars.iter().all(Option::is_none));
        assert!(inferred.iter().all(|i| !i));
    }

    // ─────────────────────────────────────────────────────────────────────────
    // slot_bands
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn a_seen_bar_yields_name_above_and_health_on_it() {
        let bands = slot_bands(Some((20, 10)), false);

        assert_eq!(
            bands,
            vec![
                Band {
                    top: TOP_INSET,
                    height: 20 - TOP_INSET,
                    kind: BandKind::Name
                },
                Band {
                    top: 20,
                    height: 10,
                    kind: BandKind::Health
                },
            ]
        );
    }

    #[test]
    fn a_thin_bar_cannot_hold_text_so_only_the_name_is_read() {
        let bands = slot_bands(Some((20, 5)), false);

        assert_eq!(bands.len(), 1);
        assert_eq!(bands[0].kind, BandKind::Name);
    }

    #[test]
    fn an_inferred_bar_places_the_name_but_is_not_recognized_itself() {
        let bands = slot_bands(Some((20, 10)), true);

        assert_eq!(bands.len(), 1);
        assert_eq!(bands[0].kind, BandKind::Name);
    }

    /// Observed live: a grid one cell off leaves a few pixels above the bar,
    /// and OCR turns that sliver into a garbage provisional name.
    #[test]
    fn a_sliver_above_a_tall_bar_is_not_a_name() {
        let bands = slot_bands(Some((5, 24)), false);

        assert!(bands.iter().all(|b| b.kind == BandKind::Health));
    }

    #[test]
    fn a_bar_at_the_cell_top_leaves_no_room_for_a_name() {
        let bands = slot_bands(Some((2, 10)), false);

        assert!(bands.iter().all(|b| b.kind == BandKind::Health));
    }

    #[test]
    fn no_bar_means_no_bands() {
        assert!(slot_bands(None, false).is_empty());
    }
}
