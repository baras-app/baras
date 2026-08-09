//! Prepare a raid-frame band for recognition.
//!
//! Crops are contrast-stretched and scaled to the
//! recognition model's 32 px input height.

use baras_overlay::capture::CapturedImage;

use super::bands::{Band, BandKind, name_text_span};

/// Target height for a preprocessed crop. Matches what ocrs' recognition model
/// is trained on; wider or narrower is fine, shorter is not.
const TARGET_HEIGHT: u32 = 32;
/// Vertical padding for small band-position errors.
const PAD_Y: i32 = 2;

/// A crop ready for recognition: 8-bit grayscale, top-down.
#[derive(Debug)]
pub struct PreparedCrop {
    pub width: u32,
    pub height: u32,
    pub gray: Vec<u8>,
}

impl PreparedCrop {
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

    let scaled = upscale_bilinear(&stretched, crop.width, crop.height, out_w, out_h);

    Some(PreparedCrop {
        width: out_w,
        height: out_h,
        gray: scaled,
    })
}

fn to_gray(img: &CapturedImage) -> Vec<u8> {
    img.rgba
        .chunks_exact(4)
        .map(|p| {
            (((p[0] as u32 * 77) + (p[1] as u32 * 150) + (p[2] as u32 * 29)) >> 8) as u8
        })
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
    let (min, max) = gray
        .iter()
        .fold((255u8, 0u8), |(lo, hi), &v| (lo.min(v), hi.max(v)));

    if max <= min {
        return gray.to_vec();
    }

    let span = (max - min) as f32;
    gray.iter()
        .map(|&v| (((v - min) as f32 / span) * 255.0).round() as u8)
        .collect()
}

/// Bilinear upscale keeps thin antialiased strokes connected.
fn upscale_bilinear(src: &[u8], src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> Vec<u8> {
    if src_w == 0 || src_h == 0 {
        return Vec::new();
    }
    if src_w == dst_w && src_h == dst_h {
        return src.to_vec();
    }

    let mut out = vec![0u8; (dst_w * dst_h) as usize];
    let x_ratio = src_w as f32 / dst_w as f32;
    let y_ratio = src_h as f32 / dst_h as f32;

    for y in 0..dst_h {
        let sy = ((y as f32 + 0.5) * y_ratio - 0.5).max(0.0);
        let y0 = sy.floor() as u32;
        let y1 = (y0 + 1).min(src_h - 1);
        let wy = sy - y0 as f32;

        for x in 0..dst_w {
            let sx = ((x as f32 + 0.5) * x_ratio - 0.5).max(0.0);
            let x0 = sx.floor() as u32;
            let x1 = (x0 + 1).min(src_w - 1);
            let wx = sx - x0 as f32;

            let p = |px: u32, py: u32| src[(py * src_w + px) as usize] as f32;
            let top = p(x0, y0) * (1.0 - wx) + p(x1, y0) * wx;
            let bottom = p(x0, y1) * (1.0 - wx) + p(x1, y1) * wx;

            out[(y * dst_w + x) as usize] = (top * (1.0 - wy) + bottom * wy).round() as u8;
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: u32, height: u32, rgb: (u8, u8, u8)) -> CapturedImage {
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for _ in 0..(width * height) {
            rgba.extend_from_slice(&[rgb.0, rgb.1, rgb.2, 255]);
        }
        CapturedImage {
            width,
            height,
            rgba,
        }
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
        let out = upscale_bilinear(&src, 2, 2, 4, 4);
        assert_eq!(out.len(), 16);
        assert_eq!(out[0], 0, "top-left should stay dark");
        assert_eq!(out[3], 255, "top-right should stay bright");
    }

    #[test]
    fn upscale_is_identity_at_same_size() {
        let src = vec![1u8, 2, 3, 4];
        assert_eq!(upscale_bilinear(&src, 2, 2, 2, 2), src);
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
        // The 10 px source crop becomes 32 px high, so width scales by 3.2.
        assert_eq!(crop.width, 240);
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
