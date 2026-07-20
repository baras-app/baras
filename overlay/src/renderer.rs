//! Software renderer using tiny-skia and cosmic-text
//!
//! This provides cross-platform 2D rendering for overlay content.
//! All rendering is done on the CPU and produces an RGBA pixel buffer.
#![allow(clippy::too_many_arguments)]
use std::collections::HashMap;
use std::sync::OnceLock;

use cosmic_text::{
    Attrs, Buffer, Color as CosmicColor, Family, FontSystem, LayoutGlyph, Metrics, Shaping, Style,
    SwashCache, Weight,
};
use tiny_skia::{
    Color, FillRule, GradientStop, LineCap, LineJoin, LinearGradient, Paint, PathBuilder, PixmapMut,
    Point, Rect, SpreadMode, Stroke, StrokeDash, Transform,
};

// ─────────────────────────────────────────────────────────────────────────────
// Shared Font Database
// ─────────────────────────────────────────────────────────────────────────────

/// Global font database, initialized once and shared across all renderers.
/// Each overlay thread clones this database (cheap - uses Arc internally)
/// to create its own FontSystem, avoiding repeated system font scanning.
static SHARED_FONT_DB: OnceLock<fontdb::Database> = OnceLock::new();

/// Bundled Inter font (Regular + Bold + Italic) so overlays render with a
/// consistent typeface across Windows/macOS/Linux without depending on
/// whichever fonts the host happens to have installed.
const INTER_REGULAR: &[u8] = include_bytes!("../assets/fonts/Inter-Regular.ttf");
const INTER_BOLD: &[u8] = include_bytes!("../assets/fonts/Inter-Bold.ttf");
const INTER_ITALIC: &[u8] = include_bytes!("../assets/fonts/Inter-Italic.ttf");

/// Default overlay font family. Bundled, so always present as a fallback.
pub const DEFAULT_FONT_FAMILY: &str = "Inter";

/// Get a clone of the shared font database.
/// First call initializes by scanning system fonts and registering the
/// bundled Inter faces; subsequent calls are cheap clones.
fn get_shared_font_db() -> fontdb::Database {
    SHARED_FONT_DB
        .get_or_init(|| {
            let mut db = fontdb::Database::new();
            db.load_system_fonts();
            db.load_font_data(INTER_REGULAR.to_vec());
            db.load_font_data(INTER_BOLD.to_vec());
            db.load_font_data(INTER_ITALIC.to_vec());
            db
        })
        .clone()
}

/// Cached list of Latin-capable font families (computed once — scanning every
/// face's glyph coverage touches font files, so we don't want to repeat it).
static LATIN_FONT_FAMILIES: OnceLock<Vec<String>> = OnceLock::new();

/// Representative Latin letters a usable Roman-text font must cover. Fonts that
/// can't map these (Hebrew/Arabic/CJK-only, symbol/icon fonts) are filtered out.
const LATIN_PROBE: [char; 6] = ['A', 'a', 'g', 'R', 'z', 'e'];

/// Whether a face has cmap glyphs for the Latin probe characters.
fn face_supports_latin(db: &fontdb::Database, id: fontdb::ID) -> bool {
    db.with_face_data(id, |data, index| {
        match ttf_parser::Face::parse(data, index) {
            Ok(face) => LATIN_PROBE.iter().all(|&c| face.glyph_index(c).is_some()),
            Err(_) => false,
        }
    })
    .unwrap_or(false)
}

/// Enumerate the distinct font family names available on this system that can
/// render Roman/Latin text (plus the bundled Inter), sorted alphabetically.
/// Used to populate the font picker. Computed once and cached.
pub fn available_font_families() -> Vec<String> {
    LATIN_FONT_FAMILIES
        .get_or_init(|| {
            let db = get_shared_font_db();
            let mut families: Vec<String> = db
                .faces()
                .filter(|face| face_supports_latin(&db, face.id))
                .filter_map(|face| face.families.first().map(|(name, _)| name.clone()))
                .collect();
            families.sort_unstable();
            families.dedup();
            families
        })
        .clone()
}

/// Maximum entries in the text shaping cache (LRU eviction when exceeded)
const TEXT_CACHE_MAX_ENTRIES: usize = 512;

/// Cached result of text shaping
struct CachedText {
    /// Pre-shaped glyphs ready for rendering
    glyphs: Vec<LayoutGlyph>,
    width: f32,
    height: f32,
    /// LRU tracking: incremented on each access
    last_used: u64,
}

/// Key for text cache: (text content, font size rounded to tenths, is_bold, is_italic)
type TextCacheKey = (String, u32, bool, bool);

/// A software renderer for overlay content
pub struct Renderer {
    font_system: FontSystem,
    swash_cache: SwashCache,
    /// Cache of shaped text to avoid re-shaping every frame
    text_cache: HashMap<TextCacheKey, CachedText>,
    /// Counter for LRU tracking
    cache_access_counter: u64,
    /// Active font family name (resolved from the shared DB at shaping time).
    /// Global setting; changing it clears the text cache.
    font_family: String,
    /// Whether the active family actually has a bold / italic face. When it
    /// doesn't, we avoid requesting that weight/style so the text stays in the
    /// selected family instead of falling back to a different font (e.g. the
    /// bundled Inter Bold).
    font_has_bold: bool,
    font_has_italic: bool,
}

/// Inspect the shared DB for whether `family` provides a bold face and an
/// italic face. Returns `(has_bold, has_italic)`.
fn family_face_support(db: &fontdb::Database, family: &str) -> (bool, bool) {
    let mut has_bold = false;
    let mut has_italic = false;
    for face in db.faces() {
        if face.families.iter().any(|(n, _)| n == family) {
            // Weight 600+ (semibold/bold) counts as a usable bold face.
            if face.weight.0 >= 600 {
                has_bold = true;
            }
            if face.style != fontdb::Style::Normal {
                has_italic = true;
            }
        }
    }
    (has_bold, has_italic)
}

impl Renderer {
    /// Create a new renderer
    ///
    /// Uses a shared font database to avoid repeatedly scanning system fonts.
    /// The first renderer created will initialize the database; subsequent
    /// renderers clone it cheaply (fontdb uses Arc internally).
    pub fn new() -> Self {
        // Get system locale for proper text shaping
        let locale = sys_locale::get_locale().unwrap_or_else(|| "en-US".to_string());

        Self {
            font_system: FontSystem::new_with_locale_and_db(locale, get_shared_font_db()),
            swash_cache: SwashCache::new(),
            text_cache: HashMap::with_capacity(256),
            cache_access_counter: 0,
            font_family: DEFAULT_FONT_FAMILY.to_string(),
            // Bundled Inter ships Regular + Bold + Italic.
            font_has_bold: true,
            font_has_italic: true,
        }
    }

    /// Set the font family used for all text. A blank name resets to the
    /// bundled default. Clears the shaped-text cache so the change takes effect.
    pub fn set_font_family(&mut self, family: &str) {
        let family = if family.is_empty() {
            DEFAULT_FONT_FAMILY
        } else {
            family
        };
        if self.font_family != family {
            self.font_family = family.to_string();
            let (has_bold, has_italic) =
                family_face_support(self.font_system.db(), &self.font_family);
            self.font_has_bold = has_bold;
            self.font_has_italic = has_italic;
            self.text_cache.clear();
        }
    }

    /// Evict least recently used entries if cache is too large
    fn evict_lru_if_needed(&mut self) {
        if self.text_cache.len() <= TEXT_CACHE_MAX_ENTRIES {
            return;
        }

        // Find the oldest entries to remove (remove ~25% of cache)
        let target_size = TEXT_CACHE_MAX_ENTRIES * 3 / 4;
        let mut entries: Vec<_> = self
            .text_cache
            .iter()
            .map(|(k, v)| (k.clone(), v.last_used))
            .collect();
        entries.sort_by_key(|(_, last_used)| *last_used);

        // Remove oldest entries
        for (key, _) in entries
            .into_iter()
            .take(self.text_cache.len() - target_size)
        {
            self.text_cache.remove(&key);
        }
    }

    /// Find cached entry with style options
    fn find_cached_styled(
        &mut self,
        text: &str,
        font_size_key: u32,
        bold: bool,
        italic: bool,
    ) -> Option<&mut CachedText> {
        // Linear search through cache - faster than allocation for small cache hits
        // Most overlays have <20 unique text strings, so this is efficient
        self.text_cache
            .iter_mut()
            .find(|(k, _)| k.0 == text && k.1 == font_size_key && k.2 == bold && k.3 == italic)
            .map(|(_, v)| v)
    }

    /// Ensure text is cached, shaping if needed. Returns (width, height).
    fn ensure_cached(&mut self, text: &str, font_size: f32) -> (f32, f32) {
        self.ensure_cached_styled(text, font_size, false, false)
    }

    /// Ensure text is cached with styling options. Returns (width, height).
    fn ensure_cached_styled(
        &mut self,
        text: &str,
        font_size: f32,
        bold: bool,
        italic: bool,
    ) -> (f32, f32) {
        let font_size_key = (font_size * 10.0).round() as u32;

        self.cache_access_counter += 1;
        let current_access = self.cache_access_counter;

        // Fast path: check cache without allocation
        if let Some(cached) = self.find_cached_styled(text, font_size_key, bold, italic) {
            cached.last_used = current_access;
            return (cached.width, cached.height);
        }

        // Cache miss - shape the text
        let metrics = Metrics::new(font_size, font_size * 1.2);
        let mut text_buffer = Buffer::new(&mut self.font_system, metrics);

        // Clone the family name so the immutable borrow of `self.font_family`
        // doesn't conflict with the mutable `self.font_system` borrow below.
        let family = self.font_family.clone();
        // Only request bold/italic when the active family actually provides that
        // face. Otherwise the shaper would fall back to a *different* family that
        // does (e.g. bundled Inter Bold), breaking the chosen-font consistency.
        let mut attrs = Attrs::new().family(Family::Name(&family));
        if bold && self.font_has_bold {
            attrs = attrs.weight(Weight::BOLD);
        }
        if italic && self.font_has_italic {
            attrs = attrs.style(Style::Italic);
        }
        text_buffer.set_text(text, &attrs, Shaping::Advanced, None);
        text_buffer.shape_until_scroll(&mut self.font_system, false);

        // Extract glyph data for caching
        let mut glyphs = Vec::new();
        let mut width = 0.0f32;
        let mut height = 0.0f32;

        for run in text_buffer.layout_runs() {
            width = width.max(run.line_w);
            height += run.line_height;

            for glyph in run.glyphs.iter() {
                glyphs.push(glyph.clone());
            }
        }

        let cached = CachedText {
            glyphs,
            width,
            height,
            last_used: current_access,
        };

        // Store in cache (only allocate String here on miss)
        let cache_key = (text.to_string(), font_size_key, bold, italic);
        self.text_cache.insert(cache_key, cached);
        self.evict_lru_if_needed();

        (width, height)
    }

    /// Get cached glyphs for drawing. Must call ensure_cached first.
    fn get_cached_glyphs_styled(
        &mut self,
        text: &str,
        font_size: f32,
        bold: bool,
        italic: bool,
    ) -> Vec<LayoutGlyph> {
        let font_size_key = (font_size * 10.0).round() as u32;
        self.find_cached_styled(text, font_size_key, bold, italic)
            .map(|c| c.glyphs.clone())
            .unwrap_or_default()
    }

    /// Create a new pixel buffer (RGBA format)
    pub fn create_buffer(width: u32, height: u32) -> Vec<u8> {
        vec![0u8; (width * height * 4) as usize]
    }

    /// Clear a pixel buffer with a color
    pub fn clear(&self, buffer: &mut [u8], width: u32, height: u32, color: Color) {
        if let Some(mut pixmap) = PixmapMut::from_bytes(buffer, width, height) {
            pixmap.fill(color);
        }
    }

    /// Draw a filled rectangle
    pub fn fill_rect(
        &self,
        buffer: &mut [u8],
        width: u32,
        height: u32,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: Color,
    ) {
        // Validate dimensions to avoid tiny-skia panics
        if w <= 0.0 || h <= 0.0 || x < 0.0 || y < 0.0 {
            return;
        }

        let Some(mut pixmap) = PixmapMut::from_bytes(buffer, width, height) else {
            return;
        };

        let rect = match Rect::from_xywh(x, y, w, h) {
            Some(r) => r,
            None => return,
        };

        let mut paint = Paint::default();
        paint.set_color(color);
        paint.anti_alias = true;

        pixmap.fill_rect(rect, &paint, Transform::identity(), None);
    }

    /// Draw a rounded rectangle (filled)
    pub fn fill_rounded_rect(
        &self,
        buffer: &mut [u8],
        width: u32,
        height: u32,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        color: Color,
    ) {
        // Guard against degenerate dimensions that produce empty paths
        if w <= 0.0 || h <= 0.0 {
            return;
        }

        let Some(mut pixmap) = PixmapMut::from_bytes(buffer, width, height) else {
            return;
        };

        let path = create_rounded_rect_path(x, y, w, h, radius);
        let Some(path) = path else { return };

        let mut paint = Paint::default();
        paint.set_color(color);
        paint.anti_alias = true;

        pixmap.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    /// Draw a rounded rectangle filled with a horizontal linear gradient.
    /// The gradient runs left-to-right fading `start_color` (at `grad_x0`) to
    /// `end_color` (at `grad_x1`). The gradient span is given independently of
    /// the rect so a segment can show its own fade even when it is overdrawn
    /// by another segment (e.g. split healing/shield bars). Regions outside
    /// `[grad_x0, grad_x1]` are padded with the nearest stop color. Falls back
    /// to a solid `start_color` fill if the gradient shader cannot be built.
    pub fn fill_rounded_rect_gradient(
        &self,
        buffer: &mut [u8],
        width: u32,
        height: u32,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        grad_x0: f32,
        grad_x1: f32,
        start_color: Color,
        end_color: Color,
    ) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }

        let Some(mut pixmap) = PixmapMut::from_bytes(buffer, width, height) else {
            return;
        };

        let Some(path) = create_rounded_rect_path(x, y, w, h, radius) else {
            return;
        };

        let mut paint = Paint::default();
        paint.anti_alias = true;
        match LinearGradient::new(
            Point::from_xy(grad_x0, y),
            Point::from_xy(grad_x1, y),
            vec![
                GradientStop::new(0.0, start_color),
                GradientStop::new(1.0, end_color),
            ],
            SpreadMode::Pad,
            Transform::identity(),
        ) {
            Some(shader) => paint.shader = shader,
            None => paint.set_color(start_color),
        }

        pixmap.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    /// Draw a rounded rectangle outline
    pub fn stroke_rounded_rect(
        &self,
        buffer: &mut [u8],
        width: u32,
        height: u32,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        stroke_width: f32,
        color: Color,
    ) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }

        let Some(mut pixmap) = PixmapMut::from_bytes(buffer, width, height) else {
            return;
        };

        let path = create_rounded_rect_path(x, y, w, h, radius);
        let Some(path) = path else { return };

        let mut paint = Paint::default();
        paint.set_color(color);
        paint.anti_alias = true;

        let stroke = Stroke {
            width: stroke_width,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Default::default()
        };

        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }

    /// Stroke an open "folder tab" outline: up the left side, around the rounded
    /// top corners, across the top, and down the right side — leaving the bottom
    /// edge open so the tab visually flows into whatever sits below it.
    pub fn stroke_tab_outline(
        &self,
        buffer: &mut [u8],
        width: u32,
        height: u32,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        stroke_width: f32,
        color: Color,
    ) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }

        let Some(mut pixmap) = PixmapMut::from_bytes(buffer, width, height) else {
            return;
        };

        let r = radius.min(w / 2.0).min(h);
        let mut pb = PathBuilder::new();
        // Open path (not closed): bottom-left → up → top-left corner → top edge
        // → top-right corner → down → bottom-right. No bottom edge.
        pb.move_to(x, y + h);
        pb.line_to(x, y + r);
        pb.quad_to(x, y, x + r, y);
        pb.line_to(x + w - r, y);
        pb.quad_to(x + w, y, x + w, y + r);
        pb.line_to(x + w, y + h);
        let Some(path) = pb.finish() else { return };

        let mut paint = Paint::default();
        paint.set_color(color);
        paint.anti_alias = true;

        let stroke = Stroke {
            width: stroke_width,
            line_cap: LineCap::Butt,
            line_join: LineJoin::Round,
            ..Default::default()
        };

        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }

    /// Draw a dashed rounded rectangle outline (useful for alignment guides)
    pub fn stroke_rounded_rect_dashed(
        &self,
        buffer: &mut [u8],
        width: u32,
        height: u32,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        stroke_width: f32,
        color: Color,
        dash_length: f32,
        gap_length: f32,
    ) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }

        let Some(mut pixmap) = PixmapMut::from_bytes(buffer, width, height) else {
            return;
        };

        let path = create_rounded_rect_path(x, y, w, h, radius);
        let Some(path) = path else { return };

        let mut paint = Paint::default();
        paint.set_color(color);
        paint.anti_alias = true;

        let stroke = Stroke {
            width: stroke_width,
            line_cap: LineCap::Butt,
            line_join: LineJoin::Round,
            dash: StrokeDash::new(vec![dash_length, gap_length], 0.0),
            ..Default::default()
        };

        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }

    /// Draw text at the specified position (uses shaping cache)
    pub fn draw_text(
        &mut self,
        buffer: &mut [u8],
        buf_width: u32,
        buf_height: u32,
        text: &str,
        x: f32,
        y: f32,
        font_size: f32,
        color: Color,
    ) {
        self.draw_text_styled(
            buffer, buf_width, buf_height, text, x, y, font_size, color, false, false,
        );
    }

    /// Draw text at the specified position with bold/italic styling
    pub fn draw_text_styled(
        &mut self,
        buffer: &mut [u8],
        buf_width: u32,
        buf_height: u32,
        text: &str,
        x: f32,
        y: f32,
        font_size: f32,
        color: Color,
        bold: bool,
        italic: bool,
    ) {
        let Some(mut pixmap) = PixmapMut::from_bytes(buffer, buf_width, buf_height) else {
            return;
        };

        // Ensure text is cached (shapes if needed)
        let _ = self.ensure_cached_styled(text, font_size, bold, italic);

        // Get glyphs (still need clone due to borrow checker - swash_cache needs &mut self)
        let glyphs = self.get_cached_glyphs_styled(text, font_size, bold, italic);

        let text_color = CosmicColor::rgba(
            (color.red() * 255.0) as u8,
            (color.green() * 255.0) as u8,
            (color.blue() * 255.0) as u8,
            (color.alpha() * 255.0) as u8,
        );

        // Render each cached glyph
        for glyph in &glyphs {
            let physical_glyph = glyph.physical((x, y), 1.0);

            if let Some(image) = self
                .swash_cache
                .get_image(&mut self.font_system, physical_glyph.cache_key)
            {
                let glyph_x = physical_glyph.x + image.placement.left;
                let glyph_y = physical_glyph.y - image.placement.top;

                draw_glyph_to_pixmap(
                    &mut pixmap,
                    &image.data,
                    image.placement.width,
                    image.placement.height,
                    glyph_x,
                    glyph_y,
                    text_color,
                );
            }
        }
    }

    /// Measure text dimensions (uses shaping cache, no glyph clone)
    pub fn measure_text(&mut self, text: &str, font_size: f32) -> (f32, f32) {
        self.ensure_cached(text, font_size)
    }

    /// Measure text dimensions with style options (bold text is wider)
    pub fn measure_text_styled(
        &mut self,
        text: &str,
        font_size: f32,
        bold: bool,
        italic: bool,
    ) -> (f32, f32) {
        self.ensure_cached_styled(text, font_size, bold, italic)
    }

    /// Draw an RGBA image at the specified position with scaling
    ///
    /// The image is alpha-blended onto the buffer.
    pub fn draw_image(
        &self,
        buffer: &mut [u8],
        buf_width: u32,
        buf_height: u32,
        image_data: &[u8],
        image_width: u32,
        image_height: u32,
        dest_x: f32,
        dest_y: f32,
        dest_width: f32,
        dest_height: f32,
    ) {
        if image_data.len() != image_width as usize * image_height as usize * 4 {
            return;
        }

        let dest_x = dest_x as i32;
        let dest_y = dest_y as i32;
        let dest_w = dest_width as i32;
        let dest_h = dest_height as i32;

        let scale_x = image_width as f32 / dest_width;
        let scale_y = image_height as f32 / dest_height;

        for dy in 0..dest_h {
            let py = dest_y + dy;
            if py < 0 || py >= buf_height as i32 {
                continue;
            }

            for dx in 0..dest_w {
                let px = dest_x + dx;
                if px < 0 || px >= buf_width as i32 {
                    continue;
                }

                // Sample from source image (nearest neighbor)
                let src_x = ((dx as f32 * scale_x) as u32).min(image_width - 1);
                let src_y = ((dy as f32 * scale_y) as u32).min(image_height - 1);
                let src_idx = ((src_y * image_width + src_x) * 4) as usize;

                let src_r = image_data[src_idx];
                let src_g = image_data[src_idx + 1];
                let src_b = image_data[src_idx + 2];
                let src_a = image_data[src_idx + 3];

                if src_a == 0 {
                    continue;
                }

                let dest_idx = ((py as u32 * buf_width + px as u32) * 4) as usize;
                if dest_idx + 3 >= buffer.len() {
                    continue;
                }

                // Alpha blend
                let alpha = src_a as f32 / 255.0;
                let inv_alpha = 1.0 - alpha;

                buffer[dest_idx] =
                    (src_r as f32 * alpha + buffer[dest_idx] as f32 * inv_alpha) as u8;
                buffer[dest_idx + 1] =
                    (src_g as f32 * alpha + buffer[dest_idx + 1] as f32 * inv_alpha) as u8;
                buffer[dest_idx + 2] =
                    (src_b as f32 * alpha + buffer[dest_idx + 2] as f32 * inv_alpha) as u8;
                buffer[dest_idx + 3] = (src_a.max(buffer[dest_idx + 3])) as u8;
            }
        }
    }
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a rounded rectangle path
fn create_rounded_rect_path(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    let r = r.min(w / 2.0).min(h / 2.0);

    let mut pb = PathBuilder::new();

    // Start at top-left, after the corner
    pb.move_to(x + r, y);

    // Top edge and top-right corner
    pb.line_to(x + w - r, y);
    pb.quad_to(x + w, y, x + w, y + r);

    // Right edge and bottom-right corner
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);

    // Bottom edge and bottom-left corner
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);

    // Left edge and top-left corner
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);

    pb.close();
    pb.finish()
}

/// Draw a glyph image onto a pixmap with alpha blending
fn draw_glyph_to_pixmap(
    pixmap: &mut PixmapMut,
    glyph_data: &[u8],
    glyph_width: u32,
    glyph_height: u32,
    dest_x: i32,
    dest_y: i32,
    color: CosmicColor,
) {
    let pixmap_width = pixmap.width() as i32;
    let pixmap_height = pixmap.height() as i32;
    let data = pixmap.data_mut();

    for gy in 0..glyph_height as i32 {
        let py = dest_y + gy;
        if py < 0 || py >= pixmap_height {
            continue;
        }

        for gx in 0..glyph_width as i32 {
            let px = dest_x + gx;
            if px < 0 || px >= pixmap_width {
                continue;
            }

            let glyph_idx = (gy as u32 * glyph_width + gx as u32) as usize;
            if glyph_idx >= glyph_data.len() {
                continue;
            }

            let alpha = glyph_data[glyph_idx];
            if alpha == 0 {
                continue;
            }

            let pixel_idx = ((py as u32 * pixmap_width as u32 + px as u32) * 4) as usize;
            if pixel_idx + 3 >= data.len() {
                continue;
            }

            // Alpha blend the glyph onto the pixmap
            let src_a = (alpha as u32 * color.a() as u32) / 255;
            let inv_a = 255 - src_a;

            data[pixel_idx] =
                ((color.r() as u32 * src_a + data[pixel_idx] as u32 * inv_a) / 255) as u8;
            data[pixel_idx + 1] =
                ((color.g() as u32 * src_a + data[pixel_idx + 1] as u32 * inv_a) / 255) as u8;
            data[pixel_idx + 2] =
                ((color.b() as u32 * src_a + data[pixel_idx + 2] as u32 * inv_a) / 255) as u8;
            data[pixel_idx + 3] = (src_a + (data[pixel_idx + 3] as u32 * inv_a) / 255) as u8;
        }
    }
}
