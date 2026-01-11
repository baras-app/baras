//! DOT Tracker Overlay
//!
//! Displays DOTs on enemy targets as rows of icons per target.
//! Each row shows the target name followed by DOT icons with countdowns.
//! Supports tracking multiple targets (6-8) with automatic pruning.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use super::{Overlay, OverlayConfigUpdate, OverlayData};
use crate::frame::OverlayFrame;
use crate::platform::{OverlayConfig, PlatformError};
use crate::utils::color_from_rgba;
use crate::widgets::colors;

/// Cache for pre-scaled icons
type ScaledIconCache = HashMap<(u64, u32), Vec<u8>>;

/// A single DOT entry on a target
#[derive(Debug, Clone)]
pub struct DotEntry {
    /// Effect ID for identification
    pub effect_id: u64,
    /// Ability ID for icon lookup
    pub icon_ability_id: u64,
    /// Display name of the DOT
    pub name: String,
    /// Remaining time in seconds
    pub remaining_secs: f32,
    /// Total duration in seconds
    pub total_secs: f32,
    /// Color (RGBA) - used as fallback if no icon
    pub color: [u8; 4],
    /// Stack count (0 = don't show)
    pub stacks: u8,
    /// Source entity name (who applied)
    pub source_name: String,
    /// Target entity name
    pub target_name: String,
    /// Pre-loaded icon RGBA data (width, height, rgba_bytes) - Arc for cheap cloning
    pub icon: Option<Arc<(u32, u32, Vec<u8>)>>,
    /// Whether to show the icon (true) or use colored square (false)
    pub show_icon: bool,
}

impl DotEntry {
    /// Progress as 0.0 (expired) to 1.0 (full)
    pub fn progress(&self) -> f32 {
        if self.total_secs <= 0.0 {
            return 1.0;
        }
        (self.remaining_secs / self.total_secs).clamp(0.0, 1.0)
    }

    /// Format remaining time
    pub fn format_time(&self) -> String {
        if self.remaining_secs <= 0.0 {
            return "0".to_string();
        }
        let secs = self.remaining_secs;
        if secs >= 60.0 {
            let mins = (secs / 60.0).floor() as u32;
            format!("{}m", mins)
        } else if secs >= 10.0 {
            format!("{:.0}", secs)
        } else {
            format!("{:.1}", secs)
        }
    }
}

/// A target with its active DOTs
#[derive(Debug, Clone)]
pub struct DotTarget {
    /// Entity ID of the target
    pub entity_id: i64,
    /// Display name of the target
    pub name: String,
    /// Active DOTs on this target
    pub dots: Vec<DotEntry>,
    /// Last time this target was updated (for pruning)
    pub last_updated: Instant,
}

/// Data sent from service to DOT tracker overlay
#[derive(Debug, Clone, Default)]
pub struct DotTrackerData {
    pub targets: Vec<DotTarget>,
}

/// Configuration for DOT tracker overlay
#[derive(Debug, Clone)]
pub struct DotTrackerConfig {
    pub max_targets: u8,
    pub icon_size: u8,
    pub prune_delay_secs: f32,
    pub show_effect_names: bool,
    pub show_source_name: bool,
}

impl Default for DotTrackerConfig {
    fn default() -> Self {
        Self {
            max_targets: 6,
            icon_size: 20,
            prune_delay_secs: 2.0,
            show_effect_names: false,
            show_source_name: false,
        }
    }
}

/// Base dimensions
const BASE_WIDTH: f32 = 280.0;
const BASE_HEIGHT: f32 = 200.0;
const BASE_PADDING: f32 = 4.0;
const BASE_ROW_SPACING: f32 = 4.0;
const BASE_ICON_SPACING: f32 = 2.0;
const BASE_FONT_SIZE: f32 = 10.0;
const BASE_NAME_WIDTH: f32 = 80.0;

/// DOT tracker overlay - rows of targets with DOT icons
pub struct DotTrackerOverlay {
    frame: OverlayFrame,
    config: DotTrackerConfig,
    background_alpha: u8,
    data: DotTrackerData,
    icon_cache: ScaledIconCache,
    /// Last rendered state for dirty checking: Vec of (target_id, Vec of (effect_id, time_string, stacks))
    last_rendered: Vec<(i64, Vec<(u64, String, u8)>)>,
}

impl DotTrackerOverlay {
    /// Create a new DOT tracker overlay
    pub fn new(
        window_config: OverlayConfig,
        config: DotTrackerConfig,
        background_alpha: u8,
    ) -> Result<Self, PlatformError> {
        let mut frame = OverlayFrame::new(window_config, BASE_WIDTH, BASE_HEIGHT)?;
        frame.set_background_alpha(background_alpha);
        frame.set_label("DOT Tracker");

        Ok(Self {
            frame,
            config,
            background_alpha,
            data: DotTrackerData::default(),
            icon_cache: HashMap::new(),
            last_rendered: Vec::new(),
        })
    }

    /// Update the config
    pub fn set_config(&mut self, config: DotTrackerConfig) {
        self.config = config;
    }

    /// Update background alpha
    pub fn set_background_alpha(&mut self, alpha: u8) {
        self.background_alpha = alpha;
        self.frame.set_background_alpha(alpha);
    }

    /// Update the data and pre-cache icons
    pub fn set_data(&mut self, data: DotTrackerData) {
        let icon_size = self.frame.scaled(self.config.icon_size as f32) as u32;

        // Pre-cache icons at display size
        for target in &data.targets {
            for dot in &target.dots {
                if let Some(ref icon_arc) = dot.icon {
                    let cache_key = (dot.icon_ability_id, icon_size);
                    if !self.icon_cache.contains_key(&cache_key) {
                        let (src_w, src_h, ref src_data) = **icon_arc;
                        let scaled = scale_icon(src_data, src_w, src_h, icon_size);
                        self.icon_cache.insert(cache_key, scaled);
                    }
                }
            }
        }

        self.data = data;
    }

    /// Render the overlay
    pub fn render(&mut self) {
        let max_targets = self.config.max_targets as usize;

        // Build current visible state for dirty check
        let current_state: Vec<(i64, Vec<(u64, String, u8)>)> = self
            .data
            .targets
            .iter()
            .take(max_targets)
            .filter(|t| !t.dots.is_empty())
            .map(|t| {
                let dots: Vec<(u64, String, u8)> = t
                    .dots
                    .iter()
                    .map(|d| (d.effect_id, d.format_time(), d.stacks))
                    .collect();
                (t.entity_id, dots)
            })
            .collect();

        // Skip render if nothing changed (but always render at least once)
        if current_state == self.last_rendered && !self.last_rendered.is_empty() {
            return;
        }
        self.last_rendered = current_state;

        let padding = self.frame.scaled(BASE_PADDING);
        let row_spacing = self.frame.scaled(BASE_ROW_SPACING);
        let icon_spacing = self.frame.scaled(BASE_ICON_SPACING);
        let font_size = self.frame.scaled(BASE_FONT_SIZE);
        let icon_size = self.frame.scaled(self.config.icon_size as f32);
        let name_width = self.frame.scaled(BASE_NAME_WIDTH);
        let row_height = icon_size + row_spacing;

        self.frame.begin_frame();

        if self.data.targets.is_empty() {
            self.frame.end_frame();
            return;
        }

        let mut y = padding;
        let icon_size_u32 = icon_size as u32;

        for target in self.data.targets.iter().take(max_targets) {
            // Skip targets with no DOTs
            if target.dots.is_empty() {
                continue;
            }

            let x = padding;

            // Target name (truncated to fit)
            let display_name = truncate_name(&target.name, 12);
            self.frame.draw_text(
                &display_name,
                x,
                y + icon_size / 2.0 + font_size / 3.0,
                font_size,
                colors::white(),
            );

            // DOT icons after name
            let mut icon_x = x + name_width;

            for dot in &target.dots {
                // Draw icon from cache or colored square fallback
                // Only show icon if show_icon is true
                let cache_key = (dot.icon_ability_id, icon_size_u32);
                let has_icon = if dot.show_icon {
                    if let Some(scaled_icon) = self.icon_cache.get(&cache_key) {
                        self.frame.draw_image(
                            scaled_icon,
                            icon_size_u32,
                            icon_size_u32,
                            icon_x,
                            y,
                            icon_size,
                            icon_size,
                        );
                        true
                    } else if let Some(ref icon_arc) = dot.icon {
                        // Fallback if cache miss
                        let (img_w, img_h, ref rgba) = **icon_arc;
                        self.frame
                            .draw_image(rgba, img_w, img_h, icon_x, y, icon_size, icon_size);
                        true
                    } else {
                        false
                    }
                } else {
                    false
                };

                if !has_icon {
                    // Fallback: colored square
                    let bg_color = color_from_rgba(dot.color);
                    self.frame
                        .fill_rounded_rect(icon_x, y, icon_size, icon_size, 2.0, bg_color);
                }

                // Clock wipe - dark overlay grows from top as time runs out
                // progress = remaining/total: 1 at start (bright), 0 when expired (dark)
                let progress = dot.progress();
                let overlay_height = icon_size * (1.0 - progress);
                if overlay_height > 1.0 {
                    self.frame.fill_rect(
                        icon_x,
                        y,
                        icon_size,
                        overlay_height,
                        color_from_rgba([0, 0, 0, 140]),
                    );
                }

                // Border
                self.frame.stroke_rounded_rect(
                    icon_x,
                    y,
                    icon_size,
                    icon_size,
                    2.0,
                    1.0,
                    colors::white(),
                );

                // Countdown text centered
                let time_text = dot.format_time();
                let time_font_size = font_size * 0.85;
                let text_width = self.frame.measure_text(&time_text, time_font_size).0;
                let text_x = icon_x + (icon_size - text_width) / 2.0;
                let text_y = y + icon_size / 2.0 + time_font_size / 3.0;

                // Shadow
                self.frame.draw_text(
                    &time_text,
                    text_x + 1.0,
                    text_y + 1.0,
                    time_font_size,
                    colors::text_shadow(),
                );
                // Text
                let time_color = if dot.remaining_secs <= 3.0 {
                    colors::effect_debuff()
                } else {
                    colors::white()
                };
                self.frame
                    .draw_text(&time_text, text_x, text_y, time_font_size, time_color);

                // Stack count - prominent display when stacks exist
                if dot.stacks >= 1 {
                    let stack_text = format!("{}", dot.stacks);
                    let stack_font_size = time_font_size * 1.1;
                    // Position at bottom-right corner
                    let stack_x = icon_x + icon_size
                        - self.frame.measure_text(&stack_text, stack_font_size).0
                        - 1.0;
                    let stack_y = y + icon_size - 1.0;

                    self.frame.draw_text(
                        &stack_text,
                        stack_x + 1.0,
                        stack_y + 1.0,
                        stack_font_size,
                        colors::text_shadow(),
                    );
                    self.frame.draw_text(
                        &stack_text,
                        stack_x,
                        stack_y,
                        stack_font_size,
                        colors::effect_buff(),
                    );
                }

                icon_x += icon_size + icon_spacing;
            }

            y += row_height;
        }

        self.frame.end_frame();
    }
}

/// Truncate a name to fit within a character limit
fn truncate_name(name: &str, max_chars: usize) -> String {
    if name.chars().count() <= max_chars {
        name.to_string()
    } else {
        let truncated: String = name.chars().take(max_chars - 1).collect();
        format!("{}…", truncated)
    }
}

/// Scale icon to target size (nearest neighbor for speed)
fn scale_icon(src: &[u8], src_w: u32, src_h: u32, target_size: u32) -> Vec<u8> {
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

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Trait Implementation
// ─────────────────────────────────────────────────────────────────────────────

impl Overlay for DotTrackerOverlay {
    fn update_data(&mut self, data: OverlayData) -> bool {
        if let OverlayData::DotTracker(tracker_data) = data {
            let was_empty = self.data.targets.is_empty();
            let is_empty = tracker_data.targets.is_empty();
            self.set_data(tracker_data);
            !(was_empty && is_empty)
        } else {
            false
        }
    }

    fn update_config(&mut self, config: OverlayConfigUpdate) {
        if let OverlayConfigUpdate::DotTracker(cfg, alpha) = config {
            self.set_config(cfg);
            self.set_background_alpha(alpha);
        }
    }

    fn render(&mut self) {
        DotTrackerOverlay::render(self);
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
}
