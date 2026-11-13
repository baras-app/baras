//! Common utility functions for overlay rendering
//!
//! These are shared across different overlay types.
//! Number formatting is delegated to `baras_types::formatting` for consistency.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use tiny_skia::Color;

// Re-export formatting functions from baras-types for convenience
pub use baras_types::formatting;

/// Convert [u8; 4] RGBA array to tiny_skia Color
#[inline]
pub fn color_from_rgba(rgba: [u8; 4]) -> Color {
    Color::from_rgba8(rgba[0], rgba[1], rgba[2], rgba[3])
}

/// Format a duration in seconds as MM:SS
///
/// Delegates to [`baras_types::formatting::format_duration_u64`].
pub fn format_time(secs: u64) -> String {
    formatting::format_duration_u64(secs)
}

/// Format a duration in seconds as compact M:SS string
///
/// Delegates to [`baras_types::formatting::format_duration_f32`].
pub fn format_duration_short(secs: f32) -> String {
    formatting::format_duration_f32(secs)
}

/// Process-wide cache of pre-scaled icon RGBA buffers, keyed by
/// (ability_id, target_size_px). Every overlay thread reads/writes the same
/// map, so identical icons at identical sizes are scaled once total instead
/// of once per overlay.
#[derive(Default)]
pub struct SharedScaledIconCache {
    inner: Mutex<HashMap<(u64, u32), Arc<Vec<u8>>>>,
}

/// High bit flag distinguishing grayscale icon variants in the cache key.
/// Game ability GUIDs never use the top bit, so the flagged key can't collide.
const GRAYSCALE_KEY_FLAG: u64 = 1 << 63;

impl SharedScaledIconCache {
    /// Return the cached scaled icon, scaling and inserting on miss.
    /// The expensive `scale_icon` call happens OUTSIDE the lock so parallel
    /// overlay threads don't serialize on scaling work.
    pub fn get_or_scale(
        &self,
        ability_id: u64,
        target_size: u32,
        src: &[u8],
        src_w: u32,
        src_h: u32,
    ) -> Arc<Vec<u8>> {
        self.get_or_scale_variant(ability_id, target_size, src, src_w, src_h, false)
    }

    /// Like [`get_or_scale`], with an optional desaturated (grayscale) variant
    /// cached under a separate key.
    pub fn get_or_scale_variant(
        &self,
        ability_id: u64,
        target_size: u32,
        src: &[u8],
        src_w: u32,
        src_h: u32,
        grayscale: bool,
    ) -> Arc<Vec<u8>> {
        let key = (Self::variant_key(ability_id, grayscale), target_size);
        if let Ok(map) = self.inner.lock() {
            if let Some(arc) = map.get(&key) {
                return arc.clone();
            }
        }
        let mut scaled = scale_icon(src, src_w, src_h, target_size);
        if grayscale {
            desaturate_rgba(&mut scaled);
        }
        let scaled = Arc::new(scaled);
        if let Ok(mut map) = self.inner.lock() {
            // Another thread may have raced us — entry() keeps its version.
            return map.entry(key).or_insert_with(|| scaled.clone()).clone();
        }
        scaled
    }

    /// Fetch without scaling (render hot path). Returns None if not yet cached.
    pub fn get(&self, ability_id: u64, target_size: u32) -> Option<Arc<Vec<u8>>> {
        self.get_variant(ability_id, target_size, false)
    }

    /// Fetch a color or grayscale variant without scaling.
    pub fn get_variant(
        &self,
        ability_id: u64,
        target_size: u32,
        grayscale: bool,
    ) -> Option<Arc<Vec<u8>>> {
        self.inner
            .lock()
            .ok()?
            .get(&(Self::variant_key(ability_id, grayscale), target_size))
            .cloned()
    }

    fn variant_key(ability_id: u64, grayscale: bool) -> u64 {
        if grayscale { ability_id | GRAYSCALE_KEY_FLAG } else { ability_id }
    }
}

/// Desaturate an RGBA buffer in place (luma-weighted, slightly dimmed).
pub fn desaturate_rgba(rgba: &mut [u8]) {
    for px in rgba.chunks_exact_mut(4) {
        let luma =
            (0.299 * px[0] as f32 + 0.587 * px[1] as f32 + 0.114 * px[2] as f32) * 0.7;
        let v = luma as u8;
        px[0] = v;
        px[1] = v;
        px[2] = v;
    }
}

static SHARED_SCALED_ICONS: OnceLock<SharedScaledIconCache> = OnceLock::new();

/// Global scaled-icon cache shared by every overlay thread.
///
/// Previously each overlay kept its own `HashMap<(u64, u32), Vec<u8>>` of
/// pre-scaled icons; with 8+ overlays displaying the same ability at the same
/// size, the RGBA buffer was scaled and stored N times. This global cache
/// scales once and hands out `Arc<Vec<u8>>` references to every overlay.
pub fn shared_scaled_icons() -> &'static SharedScaledIconCache {
    SHARED_SCALED_ICONS.get_or_init(SharedScaledIconCache::default)
}

/// Scale icon to target size using nearest-neighbor sampling (shared across overlays).
/// Transparent margins baked into the source art (game icons often carry a
/// 1-2px empty right/bottom edge) are trimmed so the icon fills the target.
pub fn scale_icon(src: &[u8], src_w: u32, src_h: u32, target_size: u32) -> Vec<u8> {
    let (off_x, off_y, crop_w, crop_h) = opaque_bounds(src, src_w, src_h);
    let mut dest = vec![0u8; (target_size * target_size * 4) as usize];
    let scale_x = crop_w as f32 / target_size as f32;
    let scale_y = crop_h as f32 / target_size as f32;
    for dy in 0..target_size {
        for dx in 0..target_size {
            let sx = off_x + ((dx as f32 * scale_x) as u32).min(crop_w - 1);
            let sy = off_y + ((dy as f32 * scale_y) as u32).min(crop_h - 1);
            let src_idx = ((sy * src_w + sx) * 4) as usize;
            let dest_idx = ((dy * target_size + dx) * 4) as usize;
            dest[dest_idx] = src[src_idx];
            dest[dest_idx + 1] = src[src_idx + 1];
            dest[dest_idx + 2] = src[src_idx + 2];
            dest[dest_idx + 3] = src[src_idx + 3];
        }
    }
    dest
}

/// Bounding box (x, y, w, h) of the non-transparent pixels in an RGBA buffer.
/// Returns the full image if it is entirely transparent.
fn opaque_bounds(src: &[u8], w: u32, h: u32) -> (u32, u32, u32, u32) {
    const ALPHA_MIN: u8 = 8;
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (w, h, 0u32, 0u32);
    let mut found = false;
    for y in 0..h {
        for x in 0..w {
            if src[((y * w + x) * 4 + 3) as usize] >= ALPHA_MIN {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
                found = true;
            }
        }
    }
    if !found {
        return (0, 0, w, h);
    }
    (min_x, min_y, max_x - min_x + 1, max_y - min_y + 1)
}

/// Format a large number with K/M suffix for compact display.
///
/// This is a convenience wrapper that passes `european = false`.
/// Overlays should use [`formatting::format_compact`] directly with their
/// `european_number_format` field for locale-aware formatting.
pub fn format_number(n: i64) -> String {
    formatting::format_compact(n, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_time() {
        assert_eq!(format_time(0), "0:00");
        assert_eq!(format_time(59), "0:59");
        assert_eq!(format_time(60), "1:00");
        assert_eq!(format_time(125), "2:05");
    }

    #[test]
    fn test_format_number() {
        // Now standardized to K at 1,000+ threshold
        assert_eq!(format_number(500), "500");
        assert_eq!(format_number(999), "999");
        assert_eq!(format_number(1000), "1.00K");
        // The shared formatter rounds rather than truncates.
        assert_eq!(format_number(9999), "10.00K");
        assert_eq!(format_number(10000), "10.00K");
        assert_eq!(format_number(1500000), "1.50M");
    }
}
