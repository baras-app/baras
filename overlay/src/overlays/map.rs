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
    /// Root-level placeholder SVG shown only in edit mode when `svg` is `None`.
    pub placeholder: Option<Arc<String>>,
}

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
    /// In-flight rasterization job (worker thread) and the id it targets.
    pending: Option<(RasterId, Receiver<Option<Raster>>)>,
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
            pending: None,
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
                        self.raster = result;
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
            let (tx, rx) = mpsc::channel();
            let cfg = self.config.clone();
            thread::spawn(move || {
                let out = parse_tree(&source).and_then(|t| rasterize(&t, &cfg, width, height));
                let _ = tx.send(out);
            });
            self.pending = Some((want, rx));
        }

        self.frame.begin_frame();
        if let Some((iw, ih, data)) = &self.raster {
            // The raster is sized exactly to the window, so draw it 1:1.
            self.frame
                .draw_image(data, *iw, *ih, 0.0, 0.0, *iw as f32, *ih as f32);
        }
        if self.pending.is_some() {
            self.draw_loading(width, height);
        }
        self.frame.end_frame();
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
            // Evaluate both so a placeholder change isn't short-circuited away.
            let map_changed = self.set_map(map_data.svg);
            let placeholder_changed = self.set_placeholder(map_data.placeholder);
            map_changed || placeholder_changed
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
