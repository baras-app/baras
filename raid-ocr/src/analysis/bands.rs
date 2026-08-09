//! Locate the health bar in a raid-frame slot, and the name band above it.
//!
//! Every cell of an ops frame is stamped from the same template: name at the top
//! left, health bar under it. The bar is the one element with a fixed colour, so
//! it is found first and everything else is measured from it.
//!
//! The bar drains right to left, so whatever red remains is anchored at its left
//! edge: scanning a narrow strip at the left of the cell finds it at any fill
//! level. Slots with no red left — dead players, empty frames — borrow the
//! position the other slots agree on.
//!
//! The name is then the last run of glyph ink above the bar. Taking the whole
//! strip above the bar instead would be simpler, but it drags in the frame border
//! and the leader chevron, and it leaves so much empty space that upscaling to
//! the recognition model's input height barely magnifies the text at all. Both
//! cost accuracy, and the first character suffers worst.

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
/// behind a border and a shaded left bevel of unpredictable width, so the search
/// area is generous; what identifies a bar row is a solid red run, not the area's
/// average colour.
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
/// Glyph ink is bright regardless of what it sits on.
const BRIGHT_LUMA: u8 = 150;
/// Fewer detected bars than this and an outlier cannot be told from the truth.
const MIN_CONSENSUS_SLOTS: usize = 3;

/// Rows whose ink coverage is below this fraction hold no text worth reading.
const MIN_INK_FRACTION: f32 = 0.02;
/// Rows above this are a solid fill, not glyphs.
const MAX_INK_FRACTION: f32 = 0.75;
/// Bands thinner than this cannot hold a legible glyph even after upscaling.
const MIN_NAME_HEIGHT: u32 = 4;
/// Ignore the right edge when locating names. Raid markers and buff icons live there.
pub(super) const NAME_SCAN_FRACTION: f32 = 0.75;
/// Offset from the slot median. Fixed thresholds miss dimmed frames.
const INK_ABOVE_MEDIAN: u16 = 35;
/// Floor for the ink threshold, so a nearly black slot does not treat noise as text.
const MIN_INK_LUMA: u8 = 60;
/// A column must reach this fraction of the band's brightest ink to count as
/// text. Measured on live captures the frame border peaks at 30–45% of glyph
/// brightness, so this excludes it with a wide margin either way.
const TEXT_PEAK_FRACTION: f32 = 0.65;
/// Rows probed above the band for the border test.
const BORDER_PROBE_ROWS: u32 = 3;
/// Width of the left window that locates the name rows. Icons sit to the right
/// of the text, so only the leftmost characters are trusted vertically.
const NAME_ANCHOR_FRACTION: f32 = 0.25;
/// A column lit down this much of the strip is a border line, not glyphs.
const VERTICAL_LINE_FRACTION: f32 = 0.7;

fn luma(r: u8, g: u8, b: u8) -> u8 {
    // Integer approximation of Rec. 601 luma; exactness is irrelevant here, the
    // threshold is empirical either way.
    (((r as u32 * 77) + (g as u32 * 150) + (b as u32 * 29)) >> 8) as u8
}

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

// ─────────────────────────────────────────────────────────────────────────────
// The bar
// ─────────────────────────────────────────────────────────────────────────────

/// Whether the left search area of a row holds a run of pixels satisfying `keep`,
/// `MIN_RUN_PX` or longer.
fn has_run(slot: &CapturedImage, y: u32, search: u32, keep: impl Fn(u8, u8, u8) -> bool) -> bool {
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
/// The bar is the longest vertical extent of rows holding a solid red run. Health
/// digits print over the bar's left edge and can carve bright gaps into its red
/// profile, so red runs are bridged across short gaps whose rows still hold
/// glyph-bright runs — the dark space above the bar never qualifies. Returns
/// `None` for a slot with no bar to see: dead, empty, or drained out of the
/// search area.
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

    // Bridge across text rows. The cap keeps a stray red mark higher in the cell
    // from chaining through bright name glyphs down to the bar.
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
/// take the median of those found; with enough of a consensus, a bar far from it
/// is an imposter — a red marker or icon that strayed into the strip — and is
/// snapped back. Returns which slots hold an inferred rather than a seen bar:
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

// ─────────────────────────────────────────────────────────────────────────────
// The name
// ─────────────────────────────────────────────────────────────────────────────

/// Ink threshold for this slot, derived from its own brightness distribution.
fn ink_threshold(slot: &CapturedImage) -> u8 {
    // A histogram avoids sorting every pixel.
    let mut histogram = [0u32; 256];
    for px in slot.rgba.chunks_exact(4) {
        histogram[luma(px[0], px[1], px[2]) as usize] += 1;
    }

    let total: u32 = histogram.iter().sum();
    if total == 0 {
        return MIN_INK_LUMA;
    }

    let mut seen = 0u32;
    let mut median = 0u8;
    for (value, count) in histogram.iter().enumerate() {
        seen += count;
        if seen * 2 >= total {
            median = value as u8;
            break;
        }
    }

    (((median as u16 + INK_ABOVE_MEDIAN).min(255)) as u8).max(MIN_INK_LUMA)
}

/// Fraction of each row, up to `bottom`, bright enough to be glyph ink.
///
/// Red pixels never count: a raid marker painted over the name area is not text.
fn ink_rows(slot: &CapturedImage, bottom: u32) -> Vec<f32> {
    let ink_luma = ink_threshold(slot);
    let name_width =
        ((slot.width as f32 * NAME_SCAN_FRACTION).round() as u32).clamp(1, slot.width.max(1));

    (0..bottom.min(slot.height))
        .map(|y| {
            let mut ink = 0u32;
            for x in 0..name_width {
                let Some((r, g, b, _)) = slot.pixel(x, y) else {
                    continue;
                };
                if !is_bar_red(r, g, b) && luma(r, g, b) >= ink_luma {
                    ink += 1;
                }
            }
            ink as f32 / name_width as f32
        })
        .collect()
}

/// Horizontal span `(left, right)` of the name text within a slot.
///
/// A fixed-width crop drags in two artifacts recognition happily reads as
/// letters: the frame border at the left edge (an 'I' or 'L' glued onto the
/// name) and the buff icons to the right ('O', 'OR'). Both are separable
/// without recognizing anything:
///
/// - Brightness: glyph ink is near-white while the border sits at 30–45% of
///   it, so a threshold referenced to the band's own brightest ink excludes
///   the border and dim icons outright.
/// - Extent: the border runs the full cell height where glyphs stop at the
///   band, so a bright leading column continuing above the band is border.
///   This covers LOS-dimmed names, whose ink can fall to border brightness.
/// - Spacing: word spaces are around a third of the glyph height while the
///   run-out before the icons is several times it; the first gap wider than
///   the band height ends the name.
///
/// A band with no qualifying text keeps the full scan area, so recognition
/// still gets its chance on anything these measures cannot see.
pub(super) fn name_text_span(slot: &CapturedImage, band: &Band) -> (u32, u32) {
    let scan_width =
        ((slot.width as f32 * NAME_SCAN_FRACTION).round() as u32).clamp(1, slot.width.max(1));
    let top = band.top.min(slot.height);
    let bottom = (band.top + band.height).min(slot.height);
    let ink_floor = ink_threshold(slot);

    // Brightest non-red pixel of each column within the band.
    let col_peak: Vec<u8> = (0..scan_width)
        .map(|x| {
            (top..bottom)
                .filter_map(|y| slot.pixel(x, y))
                .filter(|&(r, g, b, _)| !is_bar_red(r, g, b))
                .map(|(r, g, b, _)| luma(r, g, b))
                .max()
                .unwrap_or(0)
        })
        .collect();

    let glyph_peak = col_peak.iter().copied().max().unwrap_or(0);
    if glyph_peak < ink_floor {
        return (0, scan_width);
    }
    let text_luma = ((glyph_peak as f32 * TEXT_PEAK_FRACTION) as u8).max(ink_floor);

    // The border continues above the band; glyphs never do. Every probed row
    // must be lit: the border is an unbroken vertical line, while frame chrome
    // higher in the cell has dark rows between itself and the text — without
    // this a name's own first letter can sit under chrome and be skipped.
    // Probed only while hunting for the first text column.
    let probe = top.saturating_sub(BORDER_PROBE_ROWS)..top;
    let is_border = |x: u32| {
        !probe.is_empty()
            && probe.clone().all(|y| {
                slot.pixel(x, y)
                    .is_some_and(|(r, g, b, _)| !is_bar_red(r, g, b) && luma(r, g, b) >= ink_floor)
            })
    };

    let max_gap = band.height.max(MIN_NAME_HEIGHT);
    let scan = |start_luma: u8| {
        let mut left: Option<u32> = None;
        let mut right = 0u32;
        for x in 0..scan_width {
            // The start threshold decides where text BEGINS — the border is far
            // dimmer than glyph ink. Past that, plain ink extends the name:
            // LOS-dimmed letters sit well below 65% of an adjacent buff icon's
            // brightness, and cutting on them splits the name mid-word.
            if col_peak[x as usize] < ink_floor {
                continue;
            }
            if left.is_some() {
                if x - right > max_gap {
                    // Ink on the far side of an over-wide gap is the icon column.
                    break;
                }
                right = x;
            } else if col_peak[x as usize] >= start_luma && !is_border(x) {
                left = Some(x);
                right = x;
            }
        }
        (left, right)
    };

    // A start deep into the scan area means the relative threshold skipped a
    // fully dimmed name and latched onto its icons; retry from plain ink.
    let (mut left, mut right) = scan(text_luma);
    if left.is_none_or(|l| l > scan_width / 3) {
        let (dim_left, dim_right) = scan(ink_floor);
        if dim_left.is_some_and(|l| l <= scan_width / 3) {
            (left, right) = (dim_left, dim_right);
        }
    }

    match left {
        // Two columns of padding keep the glyphs' antialiased edges.
        Some(left) => (left.saturating_sub(2), (right + 3).min(scan_width)),
        None => (0, scan_width),
    }
}

/// Row ink profile from the leftmost text columns only.
///
/// Icons and markers live to the right of the name, and on small frame styles
/// they overlap the name's own rows — a full-width profile swallows them and
/// the band inflates around the icons. The leftmost characters are pure text
/// in every style, so the vertical extent is measured there. A column lit down
/// most of the strip is the frame border, not glyphs, and is left out of the
/// window; without that the border merges every run into one.
fn anchor_ink_rows(slot: &CapturedImage, bottom: u32) -> Vec<f32> {
    let ink_luma = ink_threshold(slot);
    let bottom = bottom.min(slot.height);
    let width =
        ((slot.width as f32 * NAME_ANCHOR_FRACTION).round() as u32).clamp(1, slot.width.max(1));

    let lit = |x: u32, y: u32| {
        slot.pixel(x, y)
            .is_some_and(|(r, g, b, _)| !is_bar_red(r, g, b) && luma(r, g, b) >= ink_luma)
    };

    let usable: Vec<u32> = (0..width)
        .filter(|&x| {
            let lit_rows = (0..bottom).filter(|&y| lit(x, y)).count();
            (lit_rows as f32) < bottom.max(1) as f32 * VERTICAL_LINE_FRACTION
        })
        .collect();
    if usable.is_empty() {
        return Vec::new();
    }

    (0..bottom)
        .map(|y| usable.iter().filter(|&&x| lit(x, y)).count() as f32 / usable.len() as f32)
        .collect()
}

/// The last run of usable ink above the bar within a row profile.
fn last_ink_run(rows: &[f32]) -> Option<(u32, u32)> {
    let has_ink = |v: f32| v > MIN_INK_FRACTION && v < MAX_INK_FRACTION;

    let mut found: Option<(u32, u32)> = None;
    let mut end: Option<u32> = None;
    for (i, &row) in rows.iter().enumerate().rev() {
        if has_ink(row) {
            end.get_or_insert(i as u32 + 1);
        } else if let Some(end) = end.take() {
            let top = i as u32 + 1;
            if end - top >= MIN_NAME_HEIGHT {
                found = Some((top, end - top));
                break;
            }
        }
    }
    let (mut top, mut height) =
        found.or_else(|| end.and_then(|end| (end >= MIN_NAME_HEIGHT).then_some((0, end))))?;

    // Glyph caps over a selection highlight can push a row past
    // MAX_INK_FRACTION and clip the band's top. Absorb a few such adjacent
    // rows: it keeps the caps in the crop, and it keeps the border probe above
    // the band on real background instead of inside the clipped letters.
    let mut absorbed = 0;
    while top > 0 && absorbed < 3 && rows[top as usize - 1] > MIN_INK_FRACTION {
        top -= 1;
        height += 1;
        absorbed += 1;
    }

    Some((top, height))
}

/// The name band: the last run of ink above the bar.
///
/// Scanning up from the bar rather than taking the longest run avoids the taller
/// runs made by role icons and buffs at the top of the frame. The left-window
/// profile is tried first so icons sharing the text's rows cannot stretch the
/// band; the full-width profile remains as fallback for a name whose leftmost
/// characters were unreadable.
fn name_band(slot: &CapturedImage, bar_top: u32) -> Option<Band> {
    let (top, height) = last_ink_run(&anchor_ink_rows(slot, bar_top))
        .or_else(|| last_ink_run(&ink_rows(slot, bar_top)))?;

    Some(Band {
        top,
        height,
        kind: BandKind::Name,
    })
}

/// The bands a slot's bar position implies.
///
/// The health band is only worth recognizing when the bar was actually seen and
/// is tall enough to render text.
pub fn slot_bands(slot: &CapturedImage, bar: Option<BarPosition>, inferred: bool) -> Vec<Band> {
    let Some((top, height)) = bar else {
        return Vec::new();
    };
    let mut bands = Vec::new();

    if let Some(name) = name_band(slot, top) {
        bands.push(name);
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

// ─────────────────────────────────────────────────────────────────────────────
// Grid consensus
// ─────────────────────────────────────────────────────────────────────────────

/// Pull name bands that disagree with the grid back to consensus.
///
/// Positions are compared relative to each slot's bar, never to the slot's own
/// pixel grid: the game frames can sit at a slightly different vertical pitch
/// than the overlay's slots, so absolute positions drift across the rows of a
/// layout while the gap between a name and its bar stays fixed. Measuring from
/// the bar cancels the drift — each slot's bar has already absorbed its own
/// offset and been vetted by [`reconcile_bars`].
///
/// Only slots whose bar was actually seen contribute to, or are moved by, the
/// consensus. A borrowed bar is a hint, not an anchor: the gap to it means
/// nothing, so an inferred slot's own ink always wins, and such a slot only
/// inherits a position when it found no name at all.
pub fn harmonize_names(
    per_slot: &mut [Vec<Band>],
    bars: &[Option<BarPosition>],
    inferred: &[bool],
) {
    const KIND: BandKind = BandKind::Name;

    // (height, gap from name bottom to bar top) per seen-bar slot. Bottoms, not
    // tops: icons above the text stretch a band upward, never down, so the
    // bottom is the edge the slots agree on.
    let found: Vec<(u32, u32)> = per_slot
        .iter()
        .zip(bars)
        .zip(inferred)
        .filter_map(|((bands, bar), inferred)| {
            let (bar_top, _) = (*bar).filter(|_| !inferred)?;
            bands
                .iter()
                .find(|b| b.kind == KIND)
                .map(|b| (b.height, bar_top.saturating_sub(b.top + b.height)))
        })
        .collect();

    if found.len() < MIN_CONSENSUS_SLOTS {
        return;
    }

    let consensus_height = median(found.iter().map(|&(height, _)| height));
    let consensus_gap = median(found.iter().map(|&(_, gap)| gap));
    // Scales with the frame size.
    let tolerance = (consensus_height * 2 / 5).max(2);

    for ((bands, bar), inferred) in per_slot.iter_mut().zip(bars).zip(inferred) {
        let Some((bar_top, _)) = *bar else { continue };
        if let Some(band) = bands.iter_mut().find(|b| b.kind == KIND) {
            let bottom = band.top + band.height;
            let too_tall = band.height.abs_diff(consensus_height) > tolerance;
            let displaced =
                !inferred && bar_top.saturating_sub(bottom).abs_diff(consensus_gap) > tolerance;

            // Small variation is row-profile noise and the padding absorbs it. A
            // band that only grew upward keeps its own bottom edge.
            if too_tall || displaced {
                let anchor = if displaced {
                    bar_top.saturating_sub(consensus_gap)
                } else {
                    bottom
                };
                band.top = anchor.saturating_sub(consensus_height);
                band.height = consensus_height;
            }
        }
    }

    // A slot with a bar has a player in it, so a missing name is a detection
    // failure rather than an empty frame. Place it off the slot's own bar.
    for (bands, bar) in per_slot.iter_mut().zip(bars) {
        let Some((bar_top, _)) = *bar else { continue };
        if bands.is_empty() || bands.iter().any(|b| b.kind == KIND) {
            continue;
        }
        bands.push(Band {
            top: bar_top.saturating_sub(consensus_gap + consensus_height),
            height: consensus_height,
            kind: KIND,
        });
    }
}

/// Lower middle on an even count, so tall bands cannot drag the consensus up
/// toward themselves.
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

    /// Light glyphs on the dark panel, roughly 30% coverage.
    fn paint_text(image: &mut CapturedImage, x: std::ops::Range<u32>, y: std::ops::Range<u32>) {
        for y in y {
            for x in x.clone() {
                if x % 3 == 0 {
                    let i = ((y * image.width + x) * 4) as usize;
                    image.rgba[i..i + 4].copy_from_slice(&[220, 225, 235, 255]);
                }
            }
        }
    }

    /// A full ops cell: name row, then the health bar with digits on it.
    fn synthetic_slot() -> CapturedImage {
        let mut slot = cell(100, 40);
        paint_text(&mut slot, 0..75, 10..16);
        paint(&mut slot, 0..100, 22..32, BAR_RED);
        for y in 24..30 {
            for x in 0..60 {
                if x % 4 == 0 {
                    let i = ((y * slot.width + x) * 4) as usize;
                    slot.rgba[i..i + 4].copy_from_slice(&[240, 240, 240, 255]);
                }
            }
        }
        slot
    }

    // ─────────────────────────────────────────────────────────────────────────
    // detect_health_bar
    // ─────────────────────────────────────────────────────────────────────────

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
    /// into two slivers, and taking either sliver as the bar puts the digits into
    /// the name band.
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
    fn finds_both_bands() {
        let slot = synthetic_slot();
        let bar = detect_health_bar(&slot).expect("health bar should be found");
        let bands = slot_bands(&slot, Some(bar), false);

        let health = bands
            .iter()
            .find(|b| b.kind == BandKind::Health)
            .expect("health band");
        assert_eq!((health.top, health.height), (22, 10));

        let name = bands
            .iter()
            .find(|b| b.kind == BandKind::Name)
            .expect("name band");
        assert_eq!((name.top, name.height), (10, 6));
    }

    /// The name is measured from the bar, so it never reaches down into the
    /// digits printed on it.
    #[test]
    fn the_name_stops_above_the_bar() {
        let slot = synthetic_slot();
        let bands = slot_bands(&slot, detect_health_bar(&slot), false);
        let name = bands.iter().find(|b| b.kind == BandKind::Name).unwrap();

        assert!(name.top + name.height <= 22, "name ran into the bar: {name:?}");
    }

    #[test]
    fn a_name_nearest_the_bar_wins_over_icons_higher_up() {
        let mut slot = synthetic_slot();
        // A bright role icon at the top of the cell, taller than the name.
        paint(&mut slot, 0..40, 1..8, (225, 230, 240));

        let bands = slot_bands(&slot, detect_health_bar(&slot), false);
        let name = bands.iter().find(|b| b.kind == BandKind::Name).unwrap();

        assert_eq!((name.top, name.height), (10, 6));
    }

    #[test]
    fn a_red_marker_swallows_neither_band() {
        let mut slot = synthetic_slot();
        paint(&mut slot, 60..95, 2..40, (220, 20, 20));

        let bands = slot_bands(&slot, detect_health_bar(&slot), false);
        let health = bands.iter().find(|b| b.kind == BandKind::Health).unwrap();
        let name = bands.iter().find(|b| b.kind == BandKind::Name).unwrap();

        assert_eq!((health.top, health.height), (22, 10));
        assert_eq!((name.top, name.height), (10, 6));
    }

    #[test]
    fn a_thin_bar_cannot_hold_text_so_only_the_name_is_read() {
        let slot = synthetic_slot();
        let bands = slot_bands(&slot, Some((22, 5)), false);

        assert!(bands.iter().all(|b| b.kind == BandKind::Name));
    }

    #[test]
    fn an_inferred_bar_places_the_name_but_is_not_recognized_itself() {
        let slot = synthetic_slot();
        let bands = slot_bands(&slot, Some((22, 10)), true);

        assert!(bands.iter().all(|b| b.kind == BandKind::Name));
    }

    #[test]
    fn no_bar_means_no_bands() {
        assert!(slot_bands(&synthetic_slot(), None, false).is_empty());
    }

    #[test]
    fn an_empty_cell_yields_no_name() {
        let flat = CapturedImage {
            width: 50,
            height: 30,
            rgba: vec![20u8; 50 * 30 * 4],
        };
        assert!(slot_bands(&flat, Some((20, 10)), false)
            .iter()
            .all(|b| b.kind == BandKind::Health));
    }

    // ─────────────────────────────────────────────────────────────────────────
    // harmonize_names
    // ─────────────────────────────────────────────────────────────────────────

    fn name(top: u32, height: u32) -> Vec<Band> {
        vec![Band {
            top,
            height,
            kind: BandKind::Name,
        }]
    }

    /// Harmonize against a uniform grid of seen bars at (24, 18).
    fn harmonize(slots: &mut [Vec<Band>]) {
        let bars = vec![Some((24, 18)); slots.len()];
        let inferred = vec![false; slots.len()];
        harmonize_names(slots, &bars, &inferred);
    }

    #[test]
    fn pulls_an_outlier_to_the_consensus() {
        let mut slots = vec![name(10, 6), name(10, 6), name(10, 6), name(30, 6)];
        harmonize(&mut slots);

        assert_eq!(slots[3][0].top, 10, "outlier should snap to the consensus");
        assert_eq!(slots[0][0].top, 10, "agreeing slots must be left alone");
    }

    #[test]
    fn leaves_minor_variation_alone() {
        let mut slots = vec![name(10, 6), name(11, 6), name(12, 6)];
        harmonize(&mut slots);

        assert_eq!(slots[1][0].top, 11);
        assert_eq!(slots[2][0].top, 12);
    }

    /// Bands measured off a live 8-man frame. Slots 4 and 7 swallowed the icons
    /// above the text: tops ten pixels high, bottoms unchanged.
    #[test]
    fn trims_a_band_that_only_grew_upward() {
        let observed = [
            (13, 9),
            (10, 13),
            (13, 10),
            (13, 10),
            (3, 19),
            (10, 13),
            (12, 11),
            (4, 19),
        ];
        let mut slots: Vec<Vec<Band>> = observed
            .iter()
            .map(|&(top, height)| name(top, height))
            .collect();

        harmonize(&mut slots);

        for (slot, band) in slots.iter().enumerate() {
            let band = band[0];
            assert!(
                band.height <= 13,
                "slot {slot} kept an over-tall band: {band:?}"
            );
            assert_eq!(
                band.top + band.height,
                observed[slot].0 + observed[slot].1,
                "slot {slot} must keep its own bottom edge"
            );
        }
        // The slots that agreed are untouched.
        assert_eq!((slots[0][0].top, slots[0][0].height), (13, 9));
        assert_eq!((slots[6][0].top, slots[6][0].height), (12, 11));
    }

    #[test]
    fn needs_enough_slots_to_have_an_opinion() {
        let mut slots = vec![name(10, 6), name(30, 6)];
        harmonize(&mut slots);

        assert_eq!(slots[1][0].top, 30, "too few slots to override anything");
    }

    /// A slot that found a bar but no name inherits the band the others agree on.
    #[test]
    fn fills_in_a_name_the_slot_could_not_find() {
        let health = || {
            vec![Band {
                top: 24,
                height: 18,
                kind: BandKind::Health,
            }]
        };
        let mut with_both = name(10, 13);
        with_both.push(health()[0]);

        let mut slots = vec![
            with_both.clone(),
            with_both.clone(),
            with_both,
            health(),
            Vec::new(),
        ];
        harmonize(&mut slots);

        let filled = slots[3]
            .iter()
            .find(|b| b.kind == BandKind::Name)
            .expect("slot with a bar should inherit the name band");
        assert_eq!((filled.top, filled.height), (10, 13));
        assert!(slots[4].is_empty(), "an empty frame invents nothing");
    }

    /// The guard that made this function names-only: a slot whose bar was only
    /// inferred carries a name band and no health band, and must keep it that way.
    #[test]
    fn never_invents_a_health_band() {
        let mut slots = vec![
            {
                let mut b = name(10, 13);
                b.push(Band {
                    top: 24,
                    height: 18,
                    kind: BandKind::Health,
                });
                b
            },
            {
                let mut b = name(10, 13);
                b.push(Band {
                    top: 24,
                    height: 18,
                    kind: BandKind::Health,
                });
                b
            },
            {
                let mut b = name(10, 13);
                b.push(Band {
                    top: 24,
                    height: 18,
                    kind: BandKind::Health,
                });
                b
            },
            // Bar inferred: name only, deliberately no health band.
            name(10, 13),
        ];
        harmonize(&mut slots);

        assert!(
            slots[3].iter().all(|b| b.kind == BandKind::Name),
            "an inferred slot must not be handed a health band: {:?}",
            slots[3]
        );
    }
}
