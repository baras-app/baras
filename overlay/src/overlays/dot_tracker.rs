//! DOT Tracker Overlay
//!
//! Displays DOTs on enemy targets as rows of icons per target.
//! Each row shows the target name followed by DOT icons with countdowns.
//! Supports tracking multiple targets (6-8) with automatic pruning.

use std::sync::Arc;
use std::time::Instant;

use super::{Overlay, OverlayConfigUpdate, OverlayData};
use crate::frame::OverlayFrame;
use crate::platform::{OverlayConfig, PlatformError};
use crate::utils::{color_from_rgba, shared_scaled_icons};
use crate::widgets::colors;
use crate::widgets::Header;

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
    pub fn format_time(&self, european: bool) -> String {
        baras_types::formatting::format_countdown_compact(self.remaining_secs, "0", european)
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
    /// Show header title above overlay
    pub show_header: bool,
    /// Show countdown timers on icons
    pub show_countdown: bool,
    /// Font scale multiplier (1.0 - 2.0, default 1.0)
    pub font_scale: f32,
    /// When true, background shrinks to fit content instead of filling the window
    pub dynamic_background: bool,
}

impl Default for DotTrackerConfig {
    fn default() -> Self {
        Self {
            max_targets: 6,
            icon_size: 20,
            prune_delay_secs: 2.0,
            show_effect_names: false,
            show_source_name: false,
            show_header: false,
            show_countdown: true,
            font_scale: 1.0,
            dynamic_background: true,
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
const BASE_NAME_WIDTH: f32 = 100.0;
/// Max characters per line before wrapping
const NAME_WRAP_CHARS: usize = 16;

/// DOT tracker overlay - rows of targets with DOT icons
pub struct DotTrackerOverlay {
    frame: OverlayFrame,
    config: DotTrackerConfig,
    background_alpha: u8,
    data: DotTrackerData,
    /// Last rendered state for dirty checking: Vec of (target_id, Vec of (effect_id, time_string, stacks))
    last_rendered: Vec<(i64, Vec<(u64, String, u8)>)>,
    european_number_format: bool,
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
            last_rendered: Vec::new(),
            european_number_format: false,
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

        // Pre-cache icons at display size in the shared cache
        let cache = shared_scaled_icons();
        for target in &data.targets {
            for dot in &target.dots {
                if let Some(ref icon_arc) = dot.icon {
                    let (src_w, src_h, ref src_data) = **icon_arc;
                    let _ =
                        cache.get_or_scale(dot.icon_ability_id, icon_size, src_data, src_w, src_h);
                }
            }
        }

        self.data = data;
    }

    /// Render the overlay
    pub fn render(&mut self) {
        // In move mode, always render preview (bypass dirty check)
        if self.frame.is_in_move_mode() {
            self.render_preview();
            return;
        }

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
                    .map(|d| {
                        (
                            d.effect_id,
                            d.format_time(self.european_number_format),
                            d.stacks,
                        )
                    })
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
        let font_scale = self.config.font_scale.clamp(1.0, 2.0);
        let font_size = self.frame.scaled(BASE_FONT_SIZE * font_scale);
        let icon_size = self.frame.scaled(self.config.icon_size as f32);
        let name_width = self.frame.scaled(BASE_NAME_WIDTH * font_scale);
        let row_height = icon_size + row_spacing;
        let scale = self.frame.scale_factor();
        let header_font_size = font_size * 1.4;

        // Calculate header space if enabled
        let header_space = if self.config.show_header {
            header_font_size + row_spacing + 2.0 + row_spacing + 4.0 * scale
        } else {
            0.0
        };

        // Compute content height for dynamic background
        let num_visible = self
            .data
            .targets
            .iter()
            .take(max_targets)
            .filter(|t| !t.dots.is_empty())
            .count();
        let content_height = if num_visible > 0 {
            padding * 2.0 + header_space + num_visible as f32 * row_height
        } else if self.config.show_header {
            padding * 2.0 + header_space
        } else {
            0.0
        };

        // Begin frame (clear, background, border)
        if self.config.dynamic_background {
            self.frame.begin_frame_with_content_height(content_height);
        } else {
            self.frame.begin_frame();
        }

        // Render header if enabled
        if self.config.show_header {
            let content_width = self.frame.width() as f32 - 2.0 * padding;
            Header::new("DOT Tracker")
                .with_color(colors::white())
                .render(
                    &mut self.frame,
                    padding,
                    padding,
                    content_width,
                    header_font_size,
                    row_spacing,
                );
        }

        if self.data.targets.is_empty() {
            self.frame.end_frame();
            return;
        }

        let mut y = padding + header_space;
        let icon_size_u32 = icon_size as u32;

        for target in self.data.targets.iter().take(max_targets) {
            // Skip targets with no DOTs
            if target.dots.is_empty() {
                continue;
            }

            let x = padding;

            // Wrap target name into lines
            let name_lines = wrap_name(&target.name, NAME_WRAP_CHARS);
            let line_height = font_size + 2.0;
            let total_lines = name_lines.len();
            let total_text_height = std::cmp::min(2, total_lines) as f32 * line_height;

            // Center the text block vertically relative to icon
            let text_start_y = y + (icon_size - total_text_height) / 2.0 + font_size;

            for (i, line) in name_lines.iter().enumerate() {
                if i == 1 && total_lines > 2 {
                    self.frame.draw_text_glowed(
                        &format!("{}...", line),
                        x,
                        text_start_y + i as f32 * line_height,
                        font_size,
                        colors::white(),
                    );
                    break;
                }

                self.frame.draw_text_glowed(
                    line,
                    x,
                    text_start_y + i as f32 * line_height,
                    font_size,
                    colors::white(),
                );
            }

            // DOT icons after name
            let mut icon_x = x + name_width;

            for dot in &target.dots {
                // Draw icon from cache or colored square fallback
                // Only show icon if show_icon is true
                let has_icon = if dot.show_icon {
                    if let Some(scaled_icon) =
                        shared_scaled_icons().get(dot.icon_ability_id, icon_size_u32)
                    {
                        self.frame.draw_image(
                            &scaled_icon,
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

                // Font size for countdown/stack text
                let time_font_size = font_size * 0.95;

                // Countdown text centered (if enabled)
                if self.config.show_countdown {
                    let time_text = dot.format_time(self.european_number_format);
                    let text_width = self.frame.measure_text(&time_text, time_font_size).0;
                    let text_x = icon_x + (icon_size - text_width) / 2.0;
                    let text_y = y + icon_size / 2.0 + time_font_size * 0.4;

                    let time_color = if dot.remaining_secs <= 3.0 {
                        colors::effect_debuff()
                    } else {
                        colors::white()
                    };
                    self.frame.draw_text_glowed(
                        &time_text,
                        text_x,
                        text_y,
                        time_font_size,
                        time_color,
                    );
                }

                icon_x += icon_size + icon_spacing;
            }

            y += row_height;
        }

        self.frame.end_frame();
    }

    /// Render preview placeholders in move mode
    fn render_preview(&mut self) {
        let padding = self.frame.scaled(BASE_PADDING);
        let row_spacing = self.frame.scaled(BASE_ROW_SPACING);
        let icon_spacing = self.frame.scaled(BASE_ICON_SPACING);
        let font_scale = self.config.font_scale.clamp(1.0, 2.0);
        let font_size = self.frame.scaled(BASE_FONT_SIZE * font_scale);
        let icon_size = self.frame.scaled(self.config.icon_size as f32);
        let name_width = self.frame.scaled(BASE_NAME_WIDTH * font_scale);
        let row_height = icon_size + row_spacing;
        let scale = self.frame.scale_factor();
        let header_font_size = font_size * 1.4;

        // Calculate header space if enabled
        let header_space = if self.config.show_header {
            header_font_size + row_spacing + 2.0 + row_spacing + 4.0 * scale
        } else {
            0.0
        };

        self.frame.begin_frame();

        // Render header if enabled
        if self.config.show_header {
            let content_width = self.frame.width() as f32 - 2.0 * padding;
            Header::new("DOT Tracker")
                .with_color(colors::white())
                .render(
                    &mut self.frame,
                    padding,
                    padding,
                    content_width,
                    header_font_size,
                    row_spacing,
                );
        }

        let mut y = padding + header_space;

        // Sample preview data: 2 targets with 3 DOTs each
        let targets = [
            ("Target 1", [("12.3", 2u8), ("8.5", 1u8), ("45", 0u8)]),
            ("Target 2", [("5.2", 3u8), ("18", 1u8), ("3.1", 0u8)]),
        ];

        for (target_name, dots) in &targets {
            let x = padding;

            // Wrap target name into lines
            let name_lines = wrap_name(target_name, NAME_WRAP_CHARS);
            let line_height = font_size + 2.0;
            let total_text_height = name_lines.len() as f32 * line_height;

            // Center the text block vertically relative to icon
            let text_start_y = y + (icon_size - total_text_height) / 2.0 + font_size;

            for (i, line) in name_lines.iter().enumerate() {
                self.frame.draw_text_glowed(
                    line,
                    x,
                    text_start_y + i as f32 * line_height,
                    font_size,
                    colors::white(),
                );
            }

            // DOT icons after name
            let mut icon_x = x + name_width;

            for (time_text, _stacks) in dots {
                // Placeholder icon background
                self.frame.fill_rounded_rect(
                    icon_x,
                    y,
                    icon_size,
                    icon_size,
                    2.0,
                    colors::effect_icon_bg(),
                );

                // Dashed border to indicate preview
                self.frame.stroke_rounded_rect_dashed(
                    icon_x,
                    y,
                    icon_size,
                    icon_size,
                    2.0,
                    1.0,
                    colors::preview_border(),
                    3.0,
                    2.0,
                );

                // Countdown text centered
                let time_font_size = font_size * 0.95;
                let text_width = self.frame.measure_text(time_text, time_font_size).0;
                let text_x = icon_x + (icon_size - text_width) / 2.0;
                let text_y = y + icon_size / 2.0 + time_font_size * 0.4;

                self.frame.draw_text_glowed(
                    time_text,
                    text_x,
                    text_y,
                    time_font_size,
                    colors::white(),
                );

                icon_x += icon_size + icon_spacing;
            }

            y += row_height;
        }

        self.frame.end_frame();
    }
}

/// Wrap a name into multiple lines at word boundaries
fn wrap_name(name: &str, max_chars: usize) -> Vec<String> {
    let total_chars = name.chars().count();
    if total_chars <= max_chars {
        return vec![name.to_string()];
    }

    let mut lines = Vec::new();
    let mut current_line = String::new();

    for word in name.split_whitespace() {
        if current_line.is_empty() {
            // First word on line - add it even if too long
            current_line = word.to_string();
        } else if current_line.chars().count() + 1 + word.chars().count() <= max_chars {
            // Word fits on current line
            current_line.push(' ');
            current_line.push_str(word);
        } else {
            // Word doesn't fit - start new line
            lines.push(current_line);
            current_line = word.to_string();
        }
    }

    // Don't forget the last line
    if !current_line.is_empty() {
        lines.push(current_line);
    }

    // If no lines were created (shouldn't happen), return original
    if lines.is_empty() {
        vec![name.to_string()]
    } else {
        lines
    }
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
        if let OverlayConfigUpdate::DotTracker(cfg, alpha, european) = config {
            self.set_config(cfg);
            self.set_background_alpha(alpha);
            self.european_number_format = european;
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
