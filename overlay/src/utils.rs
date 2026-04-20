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

/// Truncate a string to max_chars, adding "..." if truncated
pub fn truncate_name(name: &str, max_chars: usize) -> String {
    if name.chars().count() <= max_chars {
        name.to_string()
    } else {
        let truncated: String = name.chars().take(max_chars.saturating_sub(3)).collect();
        format!("{}...", truncated)
    }
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
        let key = (ability_id, target_size);
        if let Ok(map) = self.inner.lock() {
            if let Some(arc) = map.get(&key) {
                return arc.clone();
            }
        }
        let scaled = Arc::new(scale_icon(src, src_w, src_h, target_size));
        if let Ok(mut map) = self.inner.lock() {
            // Another thread may have raced us — entry() keeps its version.
            return map.entry(key).or_insert_with(|| scaled.clone()).clone();
        }
        scaled
    }

    /// Fetch without scaling (render hot path). Returns None if not yet cached.
    pub fn get(&self, ability_id: u64, target_size: u32) -> Option<Arc<Vec<u8>>> {
        self.inner.lock().ok()?.get(&(ability_id, target_size)).cloned()
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

/// Scale icon to target size using nearest-neighbor sampling (shared across overlays)
pub fn scale_icon(src: &[u8], src_w: u32, src_h: u32, target_size: u32) -> Vec<u8> {
    let mut dest = vec![0u8; (target_size * target_size * 4) as usize];
    let scale_x = src_w as f32 / target_size as f32;
    let scale_y = src_h as f32 / target_size as f32;
    for dy in 0..target_size {
        for dx in 0..target_size {
            let sx = ((dx as f32 * scale_x) as u32).min(src_w - 1);
            let sy = ((dy as f32 * scale_y) as u32).min(src_h - 1);
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
    fn test_truncate_name() {
        assert_eq!(truncate_name("short", 10), "short");
        assert_eq!(truncate_name("this is a very long name", 10), "this is...");
        assert_eq!(truncate_name("exactly10!", 10), "exactly10!");
    }

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
        assert_eq!(format_number(9999), "9.99K");
        assert_eq!(format_number(10000), "10.00K");
        assert_eq!(format_number(1500000), "1.50M");
    }
}
