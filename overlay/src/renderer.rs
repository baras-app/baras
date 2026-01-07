//! Software renderer using tiny-skia and cosmic-text
//!
//! This provides cross-platform 2D rendering for overlay content.
//! All rendering is done on the CPU and produces an RGBA pixel buffer.
#![allow(clippy::too_many_arguments)]
use std::collections::HashMap;

use cosmic_text::{
    Attrs, Buffer, Color as CosmicColor, Family, FontSystem, LayoutGlyph, Metrics, Shaping,
    SwashCache,
};
use tiny_skia::{
    Color, FillRule, LineCap, LineJoin, Paint, PathBuilder, PixmapMut, Rect, Stroke, StrokeDash,
    Transform,
};

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

/// Key for text cache: (text content, font size rounded to tenths)
type TextCacheKey = (String, u32);

/// A software renderer for overlay content
pub struct Renderer {
    font_system: FontSystem,
    swash_cache: SwashCache,
    /// Cache of shaped text to avoid re-shaping every frame
    text_cache: HashMap<TextCacheKey, CachedText>,
    /// Counter for LRU tracking
    cache_access_counter: u64,
}

impl Renderer {
    /// Create a new renderer
    pub fn new() -> Self {
        Self {
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
            text_cache: HashMap::with_capacity(256),
            cache_access_counter: 0,
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

    /// Find cached entry by borrowed key (avoids String allocation on hit)
    fn find_cached(&mut self, text: &str, font_size_key: u32) -> Option<&mut CachedText> {
        // Linear search through cache - faster than allocation for small cache hits
        // Most overlays have <20 unique text strings, so this is efficient
        self.text_cache
            .iter_mut()
            .find(|(k, _)| k.0 == text && k.1 == font_size_key)
            .map(|(_, v)| v)
    }

    /// Ensure text is cached, shaping if needed. Returns (width, height).
    fn ensure_cached(&mut self, text: &str, font_size: f32) -> (f32, f32) {
        let font_size_key = (font_size * 10.0).round() as u32;

        self.cache_access_counter += 1;
        let current_access = self.cache_access_counter;

        // Fast path: check cache without allocation
        if let Some(cached) = self.find_cached(text, font_size_key) {
            cached.last_used = current_access;
            return (cached.width, cached.height);
        }

        // Cache miss - shape the text
        let metrics = Metrics::new(font_size, font_size * 1.2);
        let mut text_buffer = Buffer::new(&mut self.font_system, metrics);

        let attrs = Attrs::new().family(Family::Name("Noto Sans"));
        text_buffer.set_text(&mut self.font_system, text, &attrs, Shaping::Advanced, None);
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
        let cache_key = (text.to_string(), font_size_key);
        self.text_cache.insert(cache_key, cached);
        self.evict_lru_if_needed();

        (width, height)
    }

    /// Get cached glyphs for drawing. Must call ensure_cached first.
    fn get_cached_glyphs(&mut self, text: &str, font_size: f32) -> Vec<LayoutGlyph> {
        let font_size_key = (font_size * 10.0).round() as u32;
        self.find_cached(text, font_size_key)
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
        let Some(mut pixmap) = PixmapMut::from_bytes(buffer, buf_width, buf_height) else {
            return;
        };

        // Ensure text is cached (shapes if needed)
        let _ = self.ensure_cached(text, font_size);

        // Get glyphs (still need clone due to borrow checker - swash_cache needs &mut self)
        let glyphs = self.get_cached_glyphs(text, font_size);

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
