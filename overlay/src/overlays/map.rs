//! Map Overlay
//!
//! Displays an SVG map keyed to the current encounter + phase. The service layer
//! resolves the right file and pushes the raw SVG source here via [`MapData`];
//! this overlay parses, rasterizes, and draws it (stretched to the window).
//!
//! # Threading
//!
//! Parsing + rasterizing an SVG can be expensive, so it runs on a short-lived
//! worker thread rather than on the overlay's paint/event loop. While a raster
//! is in flight the overlay keeps drawing the previous image with a "Loading…"
//! label over it, so the paint loop stays responsive even for large SVGs.

use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, OnceLock};
use std::thread;

use resvg::tiny_skia;

use super::{Overlay, OverlayConfigUpdate, OverlayData};
use crate::frame::OverlayFrame;
use crate::platform::{OverlayConfig, PlatformError};
use crate::utils::color_from_rgba;

// ─────────────────────────────────────────────────────────────────────────────
// Configuration & Data
// ─────────────────────────────────────────────────────────────────────────────

/// Runtime configuration for the map overlay.
#[derive(Debug, Clone)]
pub struct MapConfig {
    /// Preserve the SVG's aspect ratio (letterbox + center) instead of
    /// stretching it to fill the whole window.
    pub preserve_aspect: bool,
    /// When true, the overlay is held at `width` x `height` and cannot be
    /// drag-resized (it snaps back). Moving is still allowed.
    pub lock_size: bool,
    /// Locked width (used when `lock_size` is true).
    pub width: u32,
    /// Locked height (used when `lock_size` is true).
    pub height: u32,
}

impl Default for MapConfig {
    fn default() -> Self {
        Self {
            // Stretch the SVG to fill the whole window by default, so the map
            // covers exactly the area you size the overlay to. Aspect-locked
            // resize is a separate future option.
            preserve_aspect: false,
            lock_size: false,
            width: 400,
            height: 400,
        }
    }
}

/// Data sent from the service layer to the map overlay.
///
/// `svg` carries the raw SVG source for the current encounter+phase, or `None`
/// when no map applies. `placeholder` is an optional root-level default that is
/// shown **only in move (edit) mode** when no specific map applies, so the user
/// can still see and position the overlay. Both are wrapped in `Arc` so repeated
/// identical sends are cheap to detect.
#[derive(Debug, Clone, Default)]
pub struct MapData {
    /// Raw SVG document source, or `None` to clear the map.
    pub svg: Option<Arc<String>>,
    /// Timed overlay SVG composited on top of the base while active, or `None`.
    pub overlay: Option<Arc<String>>,
    /// Root-level placeholder SVG shown only in edit mode when `svg` is `None`.
    pub placeholder: Option<Arc<String>>,
    /// Resolved markers to draw on top of the map, in overlay(map) space.
    pub markers: Vec<ResolvedMarker>,
}

/// A marker resolved for painting: an overlay(map)-space point and its ordered
/// visual elements (asset refs already resolved to bytes by the app layer).
#[derive(Debug, Clone, Default)]
pub struct ResolvedMarker {
    pub x: f32,
    pub y: f32,
    pub elements: Vec<ResolvedElement>,
}

/// One resolved, ready-to-paint element of a marker.
#[derive(Debug, Clone)]
pub enum ResolvedElement {
    /// Text drawn with a glow.
    Text {
        text: String,
        color: [u8; 4],
        /// Element size in overlay(map) units (scaled to the live window on draw).
        size: f32,
        offset: MarkerOffset,
        anchor: MarkerAnchor,
    },
    /// A pre-rasterized RGBA image (icon / svg / png), drawn at `size`.
    Image {
        image: Arc<(u32, u32, Vec<u8>)>,
        size: f32,
        offset: MarkerOffset,
        anchor: MarkerAnchor,
    },
}

/// Re-exported so callers build markers with the same offset/anchor types.
pub use baras_core::dsl::{Anchor as MarkerAnchor, Offset as MarkerOffset};

// ─────────────────────────────────────────────────────────────────────────────
// Layout Constants
// ─────────────────────────────────────────────────────────────────────────────

/// Base dimensions for scaling calculations / default window size.
const BASE_WIDTH: f32 = 400.0;
const BASE_HEIGHT: f32 = 400.0;

// ─────────────────────────────────────────────────────────────────────────────
// SVG helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Shared font database, loaded once. Used so SVG `<text>` elements in maps
/// render. Loads the bundled Inter faces (the same fonts the rest of the overlay
/// renders with) so the default `Inter` family resolves without relying on a
/// system install, plus system fonts so SVGs referencing other families work.
fn font_database() -> Arc<usvg::fontdb::Database> {
    static DB: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = usvg::fontdb::Database::new();
        db.load_font_data(include_bytes!("../../assets/fonts/Inter-Regular.ttf").to_vec());
        db.load_font_data(include_bytes!("../../assets/fonts/Inter-Bold.ttf").to_vec());
        db.load_font_data(include_bytes!("../../assets/fonts/Inter-Italic.ttf").to_vec());
        db.load_system_fonts();
        Arc::new(db)
    })
    .clone()
}

/// Parse an SVG document into a usvg tree. Returns `None` if the source is not
/// valid SVG.
fn parse_tree(svg: &str) -> Option<usvg::Tree> {
    let mut options = usvg::Options {
        fontdb: font_database(),
        ..usvg::Options::default()
    };
    // Ensure a sane default font family for unstyled text.
    options.font_family = "Inter".to_string();
    usvg::Tree::from_str(svg, &options).ok()
}

/// Rasterize `tree` into a straight-alpha RGBA buffer sized exactly to
/// `width` x `height`. The SVG is scaled (and centered when letterboxed) to fit.
///
/// Returns `(width, height, rgba)` where `rgba` is **non-premultiplied** RGBA,
/// matching what [`OverlayFrame::draw_image`] expects.
fn rasterize(
    tree: &usvg::Tree,
    config: &MapConfig,
    width: u32,
    height: u32,
) -> Option<(u32, u32, Vec<u8>)> {
    if width == 0 || height == 0 {
        return None;
    }

    let mut pixmap = tiny_skia::Pixmap::new(width, height)?;

    let size = tree.size();
    let (sw, sh) = (size.width(), size.height());
    if sw <= 0.0 || sh <= 0.0 {
        return None;
    }

    let (scale_x, scale_y) = if config.preserve_aspect {
        let s = (width as f32 / sw).min(height as f32 / sh);
        (s, s)
    } else {
        (width as f32 / sw, height as f32 / sh)
    };

    // Center the (possibly letterboxed) image within the window.
    let tx = (width as f32 - sw * scale_x) / 2.0;
    let ty = (height as f32 - sh * scale_y) / 2.0;

    let transform = tiny_skia::Transform::from_row(scale_x, 0.0, 0.0, scale_y, tx, ty);
    resvg::render(tree, transform, &mut pixmap.as_mut());

    // resvg produces premultiplied alpha; the overlay compositor expects
    // straight alpha, so demultiply into a plain RGBA byte buffer.
    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    for px in pixmap.pixels() {
        let c = px.demultiply();
        rgba.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }

    Some((width, height, rgba))
}

/// Rasterize an SVG document to a `px` × `px` RGBA image (aspect-preserved,
/// centered) for use as a marker element. Returns `(width, height, rgba)` in
/// non-premultiplied RGBA, or `None` on parse failure. Called app-side to resolve
/// `svg` marker elements to bytes the overlay can draw.
pub fn rasterize_svg(svg: &str, px: u32) -> Option<(u32, u32, Vec<u8>)> {
    let tree = parse_tree(svg)?;
    let cfg = MapConfig {
        preserve_aspect: true,
        ..MapConfig::default()
    };
    rasterize(&tree, &cfg, px, px)
}

/// Offset of an element box's center from the (offset) marker point, given the
/// anchor and box size `(w, h)`. Screen y is downward.
fn anchor_center(anchor: MarkerAnchor, w: f32, h: f32) -> (f32, f32) {
    let (hx, hy) = (w / 2.0, h / 2.0);
    match anchor {
        MarkerAnchor::Center => (0.0, 0.0),
        MarkerAnchor::Top => (0.0, hy),
        MarkerAnchor::Bottom => (0.0, -hy),
        MarkerAnchor::Left => (hx, 0.0),
        MarkerAnchor::Right => (-hx, 0.0),
        MarkerAnchor::TopLeft => (hx, hy),
        MarkerAnchor::TopRight => (-hx, hy),
        MarkerAnchor::BottomLeft => (hx, -hy),
        MarkerAnchor::BottomRight => (-hx, -hy),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Implementation
// ─────────────────────────────────────────────────────────────────────────────

/// A rasterized image: `(width, height, straight-alpha RGBA)`.
type Raster = (u32, u32, Vec<u8>);

/// Which SVG the current raster was produced from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Active {
    Map,
    Placeholder,
}

/// Uniquely identifies what a raster should contain: a source generation (bumped
/// on any source/config change), which source is active, and the pixel size.
type RasterId = (u64, Active, u32, u32);

/// Identifies an overlay-layer raster: its source generation and pixel size.
/// The overlay has a single source (no Map/Placeholder distinction).
type OverlayRasterId = (u64, u32, u32);

/// Rasterize `source` to a window-sized image on a worker thread. Returns a
/// receiver for `(raster, intrinsic_size)`, or `None` if it can't be rendered.
/// Shared by the base and overlay layers so both use the identical transform.
fn spawn_rasterize(
    source: Arc<String>,
    cfg: MapConfig,
    width: u32,
    height: u32,
) -> Receiver<Option<(Raster, (f32, f32))>> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let out = parse_tree(&source).and_then(|t| {
            let sz = t.size();
            rasterize(&t, &cfg, width, height).map(|r| (r, (sz.width(), sz.height())))
        });
        let _ = tx.send(out);
    });
    rx
}

/// Map overlay driven by the current encounter + phase.
pub struct MapOverlay {
    frame: OverlayFrame,
    config: MapConfig,
    /// Raw SVG source of the current map.
    svg_source: Option<Arc<String>>,
    /// Raw SVG source of the edit-mode placeholder.
    placeholder_source: Option<Arc<String>>,
    /// Bumped whenever a source or the config changes, to invalidate rasters.
    generation: u64,
    /// The currently displayed raster and what it corresponds to.
    raster: Option<Raster>,
    raster_id: Option<RasterId>,
    /// Intrinsic size (viewBox) of the SVG the current raster came from, used to
    /// map marker overlay(map)-space coords onto the window.
    svg_size: Option<(f32, f32)>,
    /// In-flight rasterization job (worker thread) and the id it targets.
    pending: Option<(RasterId, Receiver<Option<(Raster, (f32, f32))>>)>,
    /// Raw SVG source of the active timed overlay (drawn on top of the base).
    overlay_source: Option<Arc<String>>,
    /// Bumped whenever the overlay source changes, to invalidate its raster.
    overlay_generation: u64,
    /// The currently displayed overlay raster and the id it corresponds to.
    overlay_raster: Option<Raster>,
    overlay_raster_id: Option<OverlayRasterId>,
    /// In-flight overlay rasterization job.
    overlay_pending: Option<(OverlayRasterId, Receiver<Option<(Raster, (f32, f32))>>)>,
    /// Markers to paint over the map (in overlay(map) space), from `MapData`.
    markers: Vec<ResolvedMarker>,
}

impl MapOverlay {
    /// Create a new map overlay.
    pub fn new(
        window_config: OverlayConfig,
        config: MapConfig,
        background_alpha: u8,
    ) -> Result<Self, PlatformError> {
        // Scale the frame chrome relative to the actual (saved/spawn) window
        // size, falling back to defaults when unset (0).
        let base_w = if window_config.width > 0 { window_config.width as f32 } else { BASE_WIDTH };
        let base_h = if window_config.height > 0 { window_config.height as f32 } else { BASE_HEIGHT };
        let mut frame = OverlayFrame::new(window_config, base_w, base_h)?;
        frame.set_background_alpha(background_alpha);
        frame.set_label("Map");

        Ok(Self {
            frame,
            config,
            svg_source: None,
            placeholder_source: None,
            generation: 0,
            raster: None,
            raster_id: None,
            svg_size: None,
            pending: None,
            overlay_source: None,
            overlay_generation: 0,
            overlay_raster: None,
            overlay_raster_id: None,
            overlay_pending: None,
            markers: Vec::new(),
        })
    }

    /// Replace the current map SVG source. Returns `true` if it changed.
    fn set_map(&mut self, svg: Option<Arc<String>>) -> bool {
        if self.svg_source.as_deref() == svg.as_deref() {
            return false;
        }
        tracing::debug!(
            had = self.svg_source.is_some(),
            now = svg.is_some(),
            bytes = svg.as_deref().map(|s| s.len()),
            generation = self.generation + 1,
            "map.overlay: set_map changed"
        );
        self.svg_source = svg;
        self.generation = self.generation.wrapping_add(1);
        true
    }

    /// Replace the timed overlay SVG source. Returns `true` if it changed.
    fn set_overlay(&mut self, svg: Option<Arc<String>>) -> bool {
        if self.overlay_source.as_deref() == svg.as_deref() {
            return false;
        }
        self.overlay_source = svg;
        self.overlay_generation = self.overlay_generation.wrapping_add(1);
        true
    }

    /// Replace the edit-mode placeholder SVG source. Returns `true` if it changed.
    fn set_placeholder(&mut self, svg: Option<Arc<String>>) -> bool {
        if self.placeholder_source.as_deref() == svg.as_deref() {
            return false;
        }
        self.placeholder_source = svg;
        self.generation = self.generation.wrapping_add(1);
        true
    }

    /// Render the overlay.
    pub fn render(&mut self) {
        // Enforce the locked size: if the window has drifted (e.g. a drag-resize
        // attempt), snap it back. This is what makes "lock size" work without
        // any platform-level resize changes.
        if self.config.lock_size {
            let (w, h) = (self.config.width.max(1), self.config.height.max(1));
            if self.frame.width() != w || self.frame.height() != h {
                self.frame.window_mut().set_size(w, h);
            }
        }

        let width = self.frame.width();
        let height = self.frame.height();
        let edit_mode = self.frame.is_in_move_mode();

        // Pick what to show: the real map whenever available; otherwise, only in
        // edit mode, the placeholder so the overlay stays findable/positionable.
        let (active, source) = if let Some(s) = &self.svg_source {
            (Active::Map, Some(s.clone()))
        } else if edit_mode {
            match &self.placeholder_source {
                Some(s) => (Active::Placeholder, Some(s.clone())),
                None => (Active::Map, None),
            }
        } else {
            (Active::Map, None)
        };

        let Some(source) = source else {
            // Nothing to show: cancel any in-flight job, clear, draw no background.
            if self.raster.is_some() || self.raster_id.is_some() || self.pending.is_some() {
                tracing::debug!(edit_mode, "map.overlay: clearing (no source)");
            }
            self.raster = None;
            self.raster_id = None;
            self.pending = None;
            // A timed overlay never shows without a base map.
            self.overlay_raster = None;
            self.overlay_raster_id = None;
            self.overlay_pending = None;
            self.frame.begin_frame_with_content_height(0.0);
            self.frame.end_frame();
            return;
        };

        let want: RasterId = (self.generation, active, width, height);

        // Pick up a finished worker result (accept it only if it's still current).
        if let Some((id, rx)) = &self.pending {
            match rx.try_recv() {
                Ok(result) => {
                    if *id == want {
                        // An accepted-but-empty raster means rasterize() bailed
                        // (e.g. a 0-sized window) — it will be cached as "done"
                        // and not retried until the source/size/generation changes.
                        tracing::debug!(
                            ?want,
                            empty = result.is_none(),
                            "map.overlay: raster accepted"
                        );
                        match result {
                            Some((r, sz)) => {
                                self.raster = Some(r);
                                self.svg_size = Some(sz);
                            }
                            None => self.raster = None,
                        }
                        self.raster_id = Some(want);
                    } else {
                        tracing::debug!(got = ?*id, ?want, "map.overlay: raster rejected (stale)");
                    }
                    self.pending = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => self.pending = None,
            }
        }

        // Kick off a new rasterization on a worker thread when what we have (or
        // have in flight) no longer matches what we want.
        let pending_matches = self.pending.as_ref().is_some_and(|(id, _)| *id == want);
        if self.raster_id != Some(want) && !pending_matches {
            if let Some(mon) = self.frame.window().current_monitor()
                && (width > mon.width || height > mon.height)
            {
                tracing::warn!(
                    width,
                    height,
                    monitor_w = mon.width,
                    monitor_h = mon.height,
                    "map: overlay is larger than the display — rasterizing an oversized image"
                );
            }
            tracing::debug!(?want, "map.overlay: rasterizing (worker spawn)");
            let rx = spawn_rasterize(source, self.config.clone(), width, height);
            self.pending = Some((want, rx));
        }

        // Timed overlay layer: only over a real map (never the placeholder), and
        // only when a source is set. Same worker/accept/spawn machinery as the
        // base, its own generation so a base swap doesn't re-rasterize it.
        let overlay_source = (active == Active::Map)
            .then(|| self.overlay_source.clone())
            .flatten();
        match overlay_source {
            Some(src) => {
                let owant: OverlayRasterId = (self.overlay_generation, width, height);
                if let Some((id, rx)) = &self.overlay_pending {
                    match rx.try_recv() {
                        Ok(result) => {
                            if *id == owant {
                                self.overlay_raster = result.map(|(r, _sz)| r);
                                self.overlay_raster_id = Some(owant);
                            }
                            self.overlay_pending = None;
                        }
                        Err(mpsc::TryRecvError::Empty) => {}
                        Err(mpsc::TryRecvError::Disconnected) => self.overlay_pending = None,
                    }
                }
                let opending_matches =
                    self.overlay_pending.as_ref().is_some_and(|(id, _)| *id == owant);
                if self.overlay_raster_id != Some(owant) && !opending_matches {
                    let rx = spawn_rasterize(src, self.config.clone(), width, height);
                    self.overlay_pending = Some((owant, rx));
                }
            }
            None => {
                // No active overlay: drop any cached raster / in-flight job.
                self.overlay_raster = None;
                self.overlay_raster_id = None;
                self.overlay_pending = None;
            }
        }

        self.frame.begin_frame();
        if let Some((iw, ih, data)) = &self.raster {
            // The raster is sized exactly to the window, so draw it 1:1.
            self.frame
                .draw_image(data, *iw, *ih, 0.0, 0.0, *iw as f32, *ih as f32);
        }
        // Timed overlay on top of the base: also window-sized and aspect-fit the
        // same way, so it lines up 1:1 over the base art.
        if let Some((iw, ih, data)) = &self.overlay_raster {
            self.frame
                .draw_image(data, *iw, *ih, 0.0, 0.0, *iw as f32, *ih as f32);
        }
        // Markers are a separate layer drawn on top of the cached raster every
        // frame, so a map (SVG) swap never disturbs them.
        self.draw_markers(width, height);
        if self.pending.is_some() || self.overlay_pending.is_some() {
            self.draw_loading(width, height);
        }
        self.frame.end_frame();
    }

    /// Paint the current markers over the map. Markers are in overlay(map) space
    /// (the SVG's coordinate system); we map them to the window with the same
    /// transform `rasterize` uses for the map image, so they stay glued to the art.
    fn draw_markers(&mut self, width: u32, height: u32) {
        if self.markers.is_empty() {
            return;
        }
        let Some((sw, sh)) = self.svg_size else {
            return;
        };
        if sw <= 0.0 || sh <= 0.0 {
            return;
        }
        let (scale_x, scale_y) = if self.config.preserve_aspect {
            let s = (width as f32 / sw).min(height as f32 / sh);
            (s, s)
        } else {
            (width as f32 / sw, height as f32 / sh)
        };
        let off_x = (width as f32 - sw * scale_x) / 2.0;
        let off_y = (height as f32 - sh * scale_y) / 2.0;
        // Uniform scale for element sizes so icons/text aren't distorted by a
        // non-square window stretch (positions still use the per-axis scale).
        let s = (scale_x * scale_y).sqrt();

        // Move the list out so the `draw_*` calls can borrow `&mut self.frame`.
        let markers = std::mem::take(&mut self.markers);
        for m in &markers {
            for el in &m.elements {
                match el {
                    ResolvedElement::Text {
                        text,
                        color,
                        size,
                        offset,
                        anchor,
                    } => {
                        let px = off_x + (m.x + offset.x) * scale_x;
                        let py = off_y + (m.y + offset.y) * scale_y;
                        let font = (size * s).max(1.0);
                        let (tw, th) = self.frame.measure_text_styled(text, font, true, false);
                        let (cx, cy) = anchor_center(*anchor, tw, th);
                        let x = px + cx - tw / 2.0;
                        let y = py + cy + th / 2.0; // draw_text y is the baseline
                        self.frame.draw_text_with_glow(
                            text,
                            x,
                            y,
                            font,
                            color_from_rgba(*color),
                            true,
                            false,
                        );
                    }
                    ResolvedElement::Image {
                        image,
                        size,
                        offset,
                        anchor,
                    } => {
                        let (iw, ih, rgba) = (image.0, image.1, &image.2);
                        let d = (size * s).max(1.0);
                        let px = off_x + (m.x + offset.x) * scale_x;
                        let py = off_y + (m.y + offset.y) * scale_y;
                        let (cx, cy) = anchor_center(*anchor, d, d);
                        let dx = px + cx - d / 2.0;
                        let dy = py + cy - d / 2.0;
                        self.frame
                            .draw_image_with_shadow(rgba, iw, ih, dx, dy, d, d);
                    }
                }
            }
        }
        self.markers = markers;
    }

    /// Replace the marker layer. Returns `true` if it changed enough to warrant a
    /// repaint (position or element-count change — content bytes rarely change
    /// without one, and comparing them each update would be wasteful).
    fn set_markers(&mut self, markers: Vec<ResolvedMarker>) -> bool {
        let changed = markers.len() != self.markers.len()
            || markers.iter().zip(&self.markers).any(|(a, b)| {
                a.x != b.x || a.y != b.y || a.elements.len() != b.elements.len()
            });
        self.markers = markers;
        changed
    }

    /// Draw a centered "Loading…" label over the current content.
    fn draw_loading(&mut self, width: u32, height: u32) {
        let msg = "Loading…";
        let font_size = 16.0;
        let (text_w, _) = self.frame.measure_text_styled(msg, font_size, true, false);
        let x = (width as f32 - text_w) / 2.0;
        let y = height as f32 / 2.0 + font_size / 2.0;
        let color = color_from_rgba([255, 255, 255, 255]);
        self.frame
            .draw_text_with_glow(msg, x, y, font_size, color, true, false);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Trait Implementation
// ─────────────────────────────────────────────────────────────────────────────

impl Overlay for MapOverlay {
    fn update_data(&mut self, data: OverlayData) -> bool {
        if let OverlayData::Map(map_data) = data {
            // Evaluate all so one change isn't short-circuited away.
            let map_changed = self.set_map(map_data.svg);
            let overlay_changed = self.set_overlay(map_data.overlay);
            let placeholder_changed = self.set_placeholder(map_data.placeholder);
            let markers_changed = self.set_markers(map_data.markers);
            map_changed || overlay_changed || placeholder_changed || markers_changed
        } else {
            false
        }
    }

    fn update_config(&mut self, config: OverlayConfigUpdate) {
        if let OverlayConfigUpdate::Map(map_config, alpha) = config {
            self.config = map_config;
            self.frame.set_background_alpha(alpha);
            // Apply the configured size immediately when locked (so editing the
            // Width/Height settings resizes the overlay live).
            if self.config.lock_size {
                self.frame
                    .window_mut()
                    .set_size(self.config.width.max(1), self.config.height.max(1));
            }
            // Aspect/fit/size settings may have changed — invalidate the raster.
            self.generation = self.generation.wrapping_add(1);
        }
    }

    fn render(&mut self) {
        MapOverlay::render(self);
    }

    fn poll_events(&mut self) -> bool {
        self.frame.poll_events()
    }

    fn frame(&self) -> &OverlayFrame {
        &self.frame
    }

    fn frame_mut(&mut self) -> &mut OverlayFrame {
        &mut self.frame
    }

    /// Keep the paint loop rendering while a rasterization is in flight, so the
    /// finished image (and the "Loading…" indicator) get picked up promptly.
    fn needs_render(&self) -> bool {
        self.pending.is_some()
    }
}
