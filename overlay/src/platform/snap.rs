//! Move/resize snapping: to the edges of the other open overlays first, then
//! to the alignment grid.
//!
//! Every `OverlayWindow` publishes its screen rect here; the platform backends
//! ask `Snap` where a dragged or resized window should land. The registry is
//! process-wide because each overlay lives on its own thread (or, on macOS,
//! all of them share the main thread).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use super::{MIN_OVERLAY_SIZE, ResizeEdges, SNAP_GRID_SIZE};

/// How close (px) an edge must come to another overlay's edge to stick to it
const EDGE_THRESHOLD: i32 = 8;

/// Screen rect in global coordinates: (x, y, width, height)
type Rect = (i32, i32, i32, i32);

fn registry() -> &'static Mutex<HashMap<String, Rect>> {
    static RECTS: OnceLock<Mutex<HashMap<String, Rect>>> = OnceLock::new();
    RECTS.get_or_init(Default::default)
}

/// Record where an overlay currently sits on screen
pub fn publish(id: &str, x: i32, y: i32, width: u32, height: u32) {
    let rect = (x, y, width as i32, height as i32);
    let mut map = registry().lock().unwrap_or_else(|p| p.into_inner());
    match map.get_mut(id) {
        Some(r) => *r = rect,
        None => {
            map.insert(id.to_string(), rect);
        }
    }
}

/// Forget an overlay that closed
pub fn remove(id: &str) {
    registry().lock().unwrap_or_else(|p| p.into_inner()).remove(id);
}

/// Edge snapping for one overlay window
pub struct Snap {
    /// The overlay's namespace; excludes its own rect from the targets
    id: String,
    /// Off for overlays that line up with game UI instead of other overlays
    enabled: bool,
}

impl Snap {
    pub fn new(id: &str, enabled: bool) -> Self {
        Self {
            id: id.to_string(),
            enabled,
        }
    }

    /// Where a window dragged to (`x`, `y`) lands. Coordinates are relative to
    /// `origin` (the output's top-left on Wayland, (0, 0) elsewhere).
    pub fn position(&self, x: i32, y: i32, width: u32, height: u32, origin: (i32, i32)) -> (i32, i32) {
        if !self.enabled {
            return (x, y);
        }
        let (gx, gy, w, h) = (x + origin.0, y + origin.1, width as i32, height as i32);
        let others = self.others();

        let mut best_x: Option<i32> = None;
        let mut best_y: Option<i32> = None;
        for &(ox, oy, ow, oh) in &others {
            // Sitting flush only makes sense beside the other overlay
            let beside_y = overlaps(gy, h, oy, oh);
            let beside_x = overlaps(gx, w, ox, ow);
            // Left edge to its left/right edge, right edge to its left/right edge
            for (cand, flush) in [(ox, false), (ox + ow, true), (ox + ow - w, false), (ox - w, true)] {
                if !flush || beside_y {
                    closer(&mut best_x, gx, cand);
                }
            }
            for (cand, flush) in [(oy, false), (oy + oh, true), (oy + oh - h, false), (oy - h, true)] {
                if !flush || beside_x {
                    closer(&mut best_y, gy, cand);
                }
            }
        }

        (
            best_x.map_or_else(|| grid(x), |g| g - origin.0),
            best_y.map_or_else(|| grid(y), |g| g - origin.1),
        )
    }

    /// The size a window at (`x`, `y`) resized to `width` x `height` lands on.
    /// Only the grabbed `edges` stick to other overlays.
    pub fn size(&self, x: i32, y: i32, width: u32, height: u32, edges: ResizeEdges, origin: (i32, i32)) -> (u32, u32) {
        if !self.enabled {
            return (width, height);
        }
        let (gx, gy) = (x + origin.0, y + origin.1);
        let (right, bottom) = (gx + width as i32, gy + height as i32);
        let mut best_r: Option<i32> = None;
        let mut best_b: Option<i32> = None;
        for (ox, oy, ow, oh) in self.others() {
            if edges.right {
                for cand in [ox, ox + ow] {
                    closer(&mut best_r, right, cand);
                }
            }
            if edges.bottom {
                for cand in [oy, oy + oh] {
                    closer(&mut best_b, bottom, cand);
                }
            }
        }
        let fit = |edge: Option<i32>, start: i32, size: u32| match edge {
            Some(e) if e - start >= MIN_OVERLAY_SIZE as i32 => (e - start) as u32,
            _ => grid(size as i32) as u32,
        };
        (fit(best_r, gx, width), fit(best_b, gy, height))
    }

    fn others(&self) -> Vec<Rect> {
        let map = registry().lock().unwrap_or_else(|p| p.into_inner());
        map.iter().filter(|(id, _)| **id != self.id).map(|(_, r)| *r).collect()
    }
}

/// Nearest grid point
fn grid(value: i32) -> i32 {
    (value + SNAP_GRID_SIZE / 2).div_euclid(SNAP_GRID_SIZE) * SNAP_GRID_SIZE
}

/// Keep `cand` in `best` when it is the nearest edge to `value` within the threshold
fn closer(best: &mut Option<i32>, value: i32, cand: i32) {
    let d = (cand - value).abs();
    if d <= EDGE_THRESHOLD && best.is_none_or(|b| d < (b - value).abs()) {
        *best = Some(cand);
    }
}

/// Whether two spans overlap or come within the threshold of each other
fn overlaps(a: i32, a_len: i32, b: i32, b_len: i32) -> bool {
    a < b + b_len + EDGE_THRESHOLD && b < a + a_len + EDGE_THRESHOLD
}
