//! Prepare a raid-frame band for recognition.
//!
//! Crops are contrast-stretched and scaled to the
//! recognition model's 64 px input height.

use fast_image_resize::images::Image;
use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};

use baras_overlay::capture::CapturedImage;

use super::bands::{Band, BandKind, name_text_span};

/// Target height for a preprocessed crop: the recognition model's input height.
/// Land on it exactly and ocrs resamples nothing; miss it and ocrs rescales on
/// top of our own, through whatever height we stopped at.
const TARGET_HEIGHT: u32 = 64;
/// Vertical padding for small band-position errors.
const PAD_Y: i32 = 2;

/// A crop ready for recognition: 8-bit grayscale, top-down.
#[derive(Debug, Clone)]
pub struct PreparedCrop {
    pub width: u32,
    pub height: u32,
    pub gray: Vec<u8>,
}

impl PreparedCrop {
    /// Narrow to a column span, pixels within it unchanged.
    ///
    /// `None` when the span is empty or changes nothing: recognition rescales
    /// to the model's width, so even a one-column trim shifts every glyph.
    pub fn narrowed(&self, left: u32, right: u32) -> Option<PreparedCrop> {
        let right = right.min(self.width);
        if right <= left || (left == 0 && right == self.width) {
            return None;
        }

        let width = right - left;
        let mut gray = Vec::with_capacity((width * self.height) as usize);
        for y in 0..self.height {
            let row = (y * self.width) as usize;
            gray.extend_from_slice(&self.gray[row + left as usize..row + right as usize]);
        }
        Some(PreparedCrop {
            width,
            height: self.height,
            gray,
        })
    }

    /// Expand to RGB, which is what `ocrs` accepts as input.
    pub fn to_rgb(&self) -> Vec<u8> {
        let mut rgb = Vec::with_capacity(self.gray.len() * 3);
        for &v in &self.gray {
            rgb.push(v);
            rgb.push(v);
            rgb.push(v);
        }
        rgb
    }
}

/// Extract and condition one band from a slot image.
///
/// Returns `None` when the band lies outside the slot or has no area.
pub fn prepare(slot: &CapturedImage, band: &Band) -> Option<PreparedCrop> {
    let (left, width) = match band.kind {
        // Just the text: the frame border and the buff icons read as letters,
        // and a tight crop magnifies the glyphs more at the model's height.
        BandKind::Name => {
            let (left, right) = name_text_span(slot, band);
            (left, right.saturating_sub(left).max(1))
        }
        BandKind::Health => (0, slot.width),
    };
    let crop = slot.crop(
        left as i32,
        band.top as i32 - PAD_Y,
        width,
        band.height.saturating_add((PAD_Y * 2) as u32),
    )?;

    let gray = match band.kind {
        // Health digits are light on saturated red: suppressing the red channel
        // separates glyph from bar far more cleanly than luminance alone, which
        // reads a bright red bar and white text as similar values.
        BandKind::Health => to_gray_suppressing_red(&crop),
        BandKind::Name => to_gray(&crop),
    };

    let stretched = stretch_contrast(&gray);
    let scale = (TARGET_HEIGHT as f32 / crop.height.max(1) as f32).max(1.0);
    let (out_w, out_h) = (
        ((crop.width as f32 * scale).round() as u32).max(1),
        ((crop.height as f32 * scale).round() as u32).max(1),
    );

    let scaled = upscale_lanczos3(stretched, crop.width, crop.height, out_w, out_h);
    // The resizer reports a rejected geometry as an empty buffer; a crop whose
    // pixels do not match its size would corrupt the canvas it is packed onto.
    if scaled.len() != (out_w * out_h) as usize {
        return None;
    }

    Some(PreparedCrop {
        width: out_w,
        height: out_h,
        gray: scaled,
    })
}

fn to_gray(img: &CapturedImage) -> Vec<u8> {
    img.rgba
        .chunks_exact(4)
        .map(|p| (((p[0] as u32 * 77) + (p[1] as u32 * 150) + (p[2] as u32 * 29)) >> 8) as u8)
        .collect()
}

/// Grayscale that treats red as background.
///
/// The health bar is strongly red while its digits are near-white, so weighting
/// green and blue heavily and subtracting red pushes the bar toward black and
/// leaves the glyphs bright.
fn to_gray_suppressing_red(img: &CapturedImage) -> Vec<u8> {
    img.rgba
        .chunks_exact(4)
        .map(|p| {
            let g = p[1] as i32;
            let b = p[2] as i32;
            let r = p[0] as i32;
            (((g + b) / 2) - (r - (g + b) / 2).max(0)).clamp(0, 255) as u8
        })
        .collect()
}

/// Rescale the intensity range so the darkest pixel becomes 0 and the brightest
/// 255. Raid frames are translucent, so raw crops often occupy a narrow band of
/// mid greys that thresholding would flatten entirely.
fn stretch_contrast(gray: &[u8]) -> Vec<u8> {
    let (min, max) = gray.iter().fold((255u8, 0u8), |(lo, hi), &v| (lo.min(v), hi.max(v)));

    if max <= min {
        return gray.to_vec();
    }

    let span = (max - min) as f32;
    gray.iter()
        .map(|&v| (((v - min) as f32 / span) * 255.0).round() as u8)
        .collect()
}

/// Lanczos-3 upscale: a sharper reconstruction than bilinear, at the cost of
/// ringing around hard edges.
///
/// `fast_image_resize` picks an SSE4.1/AVX2 kernel at runtime and falls back to
/// scalar, so this needs no target features of its own. Takes `src` by value:
/// the resizer wants an owned buffer, and the caller has no use for it after.
///
/// Three behaviours are the library's rather than ours, recorded here so they
/// are not re-derived from the pixels:
///
/// - The buffer between the horizontal and vertical passes is 8-bit, not
///   floating point, so the first pass' ringing is rounded before the second
///   sees it. Measured against a full-precision implementation this moved 9 of
///   266 readings, in neither direction; the 8-bit path is the one that gets
///   the SIMD kernels.
/// - At the image border the kernel is truncated and the surviving weights
///   renormalized, rather than the edge pixel being repeated. Our name crops
///   are cut flush against the glyphs, so repeating would invent ink that is
///   not there. [`name_text_span`] pads by the kernel radius to keep real
///   pixels under the window at the first and last letter.
/// - Coefficients are normalized to sum to 1 and results saturate into 0..=255,
///   so neither overall brightness nor ringing overshoot needs handling here.
///
/// Returns an empty buffer if the resizer rejects the geometry, which keeps the
/// failure shaped like the "band outside the slot" case `prepare` already has.
fn upscale_lanczos3(src: Vec<u8>, src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> Vec<u8> {
    if src_w == 0 || src_h == 0 {
        return Vec::new();
    }
    if src_w == dst_w && src_h == dst_h {
        return src;
    }

    let Ok(source) = Image::from_vec_u8(src_w, src_h, src, PixelType::U8) else {
        return Vec::new();
    };
    let mut dst = Image::new(dst_w, dst_h, PixelType::U8);
    let options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3));

    match Resizer::new().resize(&source, &mut dst, &options) {
        Ok(()) => dst.into_vec(),
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: u32, height: u32, rgb: (u8, u8, u8)) -> CapturedImage {
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for _ in 0..(width * height) {
            rgba.extend_from_slice(&[rgb.0, rgb.1, rgb.2, 255]);
        }
        CapturedImage { width, height, rgba }
    }

    #[test]
    fn red_suppression_darkens_bar_and_keeps_glyphs() {
        let bar = to_gray_suppressing_red(&solid(2, 2, (170, 30, 30)));
        let glyph = to_gray_suppressing_red(&solid(2, 2, (240, 240, 240)));

        assert!(
            glyph[0] > bar[0] + 100,
            "glyph {} should stand well clear of bar {}",
            glyph[0],
            bar[0]
        );
    }

    #[test]
    fn contrast_stretch_expands_narrow_range() {
        let stretched = stretch_contrast(&[100, 110, 120]);
        assert_eq!(stretched[0], 0);
        assert_eq!(stretched[2], 255);
    }

    #[test]
    fn contrast_stretch_handles_flat_input() {
        assert_eq!(stretch_contrast(&[42, 42, 42]), vec![42, 42, 42]);
    }

    #[test]
    fn upscale_preserves_corners() {
        let src = vec![0u8, 255, 255, 0];
        let out = upscale_lanczos3(src, 2, 2, 4, 4);
        assert_eq!(out.len(), 16);
        assert_eq!(out[0], 0, "top-left should stay dark");
        assert_eq!(out[3], 255, "top-right should stay bright");
    }

    #[test]
    fn upscale_is_identity_at_same_size() {
        let src = vec![1u8, 2, 3, 4];
        assert_eq!(upscale_lanczos3(src.clone(), 2, 2, 2, 2), src);
    }

    /// Guards the reason for choosing Lanczos over bilinear: the glyph edge
    /// arrives at recognition as an edge, not a ramp.
    #[test]
    fn upscale_keeps_an_edge_sharp() {
        // A vertical edge: dark half, bright half.
        let src: Vec<u8> = (0..8 * 8).map(|i| if i % 8 < 4 { 0 } else { 255 }).collect();
        let out = upscale_lanczos3(src, 8, 8, 32, 32);
        assert_eq!(out.len(), 32 * 32);

        let row = &out[16 * 32..17 * 32];
        assert_eq!(row[0], 0, "flat dark side should stay dark");
        assert_eq!(row[31], 255, "flat bright side should stay bright");

        // A 4x upscale of a step spreads over the kernel's support, no further.
        let ramp = row.iter().filter(|&&v| v > 8 && v < 247).count();
        assert!(ramp <= 8, "edge spread over {ramp} px, expected a narrow step");
    }

    /// The resizer must not be handed a degenerate source: `prepare` clamps
    /// crops to at least 1x1, and this is the shape that arrives.
    #[test]
    fn upscale_survives_a_single_pixel_source() {
        let out = upscale_lanczos3(vec![128u8], 1, 1, 8, 8);
        assert_eq!(out.len(), 64);
        assert!(out.iter().all(|&v| v == 128), "flat input must stay flat");
    }

    #[test]
    fn upscale_reports_empty_rather_than_panicking_on_zero_source() {
        assert!(upscale_lanczos3(Vec::new(), 0, 0, 8, 8).is_empty());
    }

    #[test]
    fn prepare_reaches_target_height() {
        let slot = solid(60, 30, (40, 50, 70));
        let band = Band {
            top: 10,
            height: 6,
            kind: BandKind::Name,
        };

        let crop = prepare(&slot, &band).expect("band lies inside the slot");
        assert!(
            crop.height >= TARGET_HEIGHT,
            "expected at least {TARGET_HEIGHT}px, got {}",
            crop.height
        );
        assert_eq!(crop.gray.len(), (crop.width * crop.height) as usize);
    }

    #[test]
    fn name_crop_leaves_out_the_icon_column() {
        let slot = solid(100, 30, (40, 50, 70));
        let band = Band {
            top: 10,
            height: 6,
            kind: BandKind::Name,
        };

        let crop = prepare(&slot, &band).expect("band lies inside the slot");
        // The 10 px source crop becomes 64 px high, so width scales by 6.4.
        assert_eq!(crop.width, 480);
    }

    #[test]
    fn prepare_rejects_band_outside_slot() {
        let slot = solid(60, 30, (40, 50, 70));
        let band = Band {
            top: 200,
            height: 6,
            kind: BandKind::Name,
        };
        assert!(prepare(&slot, &band).is_none());
    }

    #[test]
    fn rgb_expansion_triples_length() {
        let crop = PreparedCrop {
            width: 2,
            height: 1,
            gray: vec![10, 20],
        };
        assert_eq!(crop.to_rgb(), vec![10, 10, 10, 20, 20, 20]);
    }
}
