//! DOT Tracker Overlay
//!
//! Displays DOTs on enemy targets as rows of icons per target.
//! Each row shows the target name followed by DOT icons with countdowns.
//! Supports tracking multiple targets (6-8).

use std::sync::Arc;

use super::{Overlay, OverlayConfigUpdate, OverlayData};
use crate::frame::OverlayFrame;
use crate::platform::{OverlayConfig, PlatformError};
use crate::utils::{color_from_rgba, shared_scaled_icons};
use crate::widgets::{colors, draw_icon_placeholder, ProgressBar};
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
    pub show_effect_names: bool,
    pub show_source_name: bool,
    /// Show header title above overlay
    pub show_header: bool,
    /// Show countdown timers on icons
    pub show_countdown: bool,
    /// Font scale multiplier (0.3 - 3.0, default 1.0)
    pub font_scale: f32,
    /// When true, background shrinks to fit content instead of filling the window
    pub dynamic_background: bool,
    /// When true, target rows stack from the bottom of the overlay window
    pub stack_from_bottom: bool,
    /// Flush DOT icons against the right edge of the window (icon layout only)
    pub right_align_icons: bool,
    /// Render DOTs as stacked progress bars grouped under target names
    pub layout_bar: bool,
    /// When true (and in bar layout), draw an outline around each entry
    pub show_border: bool,
    /// Color of the per-entry border outline (bar layout only)
    pub border_color: [u8; 4],
    /// Fade each bar's fill from its color (left) to a darkened version (right).
    pub bar_gradient: bool,
}

impl Default for DotTrackerConfig {
    fn default() -> Self {
        Self {
            max_targets: 6,
            icon_size: 20,
            show_effect_names: false,
            show_source_name: false,
            show_header: false,
            show_countdown: true,
            font_scale: 1.0,
            dynamic_background: true,
            stack_from_bottom: false,
            right_align_icons: false,
            layout_bar: false,
            show_border: true,
            border_color: [128, 128, 128, 255],
            bar_gradient: false,
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
/// Bar mode font size (sized for the 20px default icon / 24px bar)
const BASE_BAR_FONT_SIZE: f32 = 17.0;

/// DOT tracker overlay - rows of targets with DOT icons
pub struct DotTrackerOverlay {
    frame: OverlayFrame,
    config: DotTrackerConfig,
    background_alpha: u8,
    data: DotTrackerData,
    /// Last rendered state for dirty checking: Vec of (target_id, Vec of (effect_id, time_string, remaining_bits))
    last_rendered: Vec<(i64, Vec<(u64, String, u32)>)>,
    /// Last rendered state for bar mode: includes remaining_secs bits so bar fill updates each frame
    last_rendered_bar: Vec<(i64, Vec<(u64, String, u32)>)>,
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
            last_rendered_bar: Vec::new(),
            european_number_format: false,
        })
    }

    /// Update the config
    pub fn set_config(&mut self, config: DotTrackerConfig) {
        self.config = config;
        // Force re-render — a layout/config change invalidates the dirty-check state
        self.last_rendered.clear();
        self.last_rendered_bar.clear();
    }

    /// Update background alpha
    pub fn set_background_alpha(&mut self, alpha: u8) {
        self.background_alpha = alpha;
        self.frame.set_background_alpha(alpha);
    }

    /// Icon pixel size to cache: the drawn size in bar layout, else the configured icon size
    fn bar_or_icon_size(&self, bar_layout: bool) -> u32 {
        let icon = self.frame.scaled(self.config.icon_size as f32).round();
        if bar_layout {
            // Bar layout: icon fills the full bar height
            (icon + 4.0 * self.frame.scale_factor()).round() as u32
        } else {
            icon as u32
        }
    }

    /// Update the data and pre-cache icons
    pub fn set_data(&mut self, data: DotTrackerData) {
        let icon_size = self.bar_or_icon_size(self.config.layout_bar);

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
        // In move mode, always render preview (bypass dirty check).
        // Clear dirty-check state so the first locked render repaints over the preview.
        if self.frame.is_in_move_mode() {
            self.last_rendered.clear();
            self.last_rendered_bar.clear();
            if self.config.layout_bar {
                self.render_preview_bar();
            } else {
                self.render_preview();
            }
            return;
        }

        if self.config.layout_bar {
            self.render_bar_mode();
        } else {
            self.render_icon_mode();
        }
    }

    /// Starting x for a row's icon run. Left-aligned runs begin at the end of
    /// the name column; right-aligned runs end at the window's right edge but
    /// never start before the name column, so wide runs degrade to left-aligned.
    fn icon_run_start_x(
        &self,
        count: usize,
        icon_size: f32,
        icon_spacing: f32,
        name_end_x: f32,
        padding: f32,
    ) -> f32 {
        if !self.config.right_align_icons || count == 0 {
            return name_end_x;
        }
        let run_width = count as f32 * icon_size + (count - 1) as f32 * icon_spacing;
        (self.frame.width() as f32 - padding - run_width).max(name_end_x)
    }

    /// Render icon mode (rows of target name + DOT icons)
    fn render_icon_mode(&mut self) {
        let max_targets = self.config.max_targets as usize;

        // Build current visible state for dirty check
        let current_state: Vec<(i64, Vec<(u64, String, u32)>)> = self
            .data
            .targets
            .iter()
            .take(max_targets)
            .filter(|t| !t.dots.is_empty())
            .map(|t| {
                // Exact remaining time keeps the clock wipe moving between text changes
                let dots: Vec<(u64, String, u32)> = t
                    .dots
                    .iter()
                    .map(|d| {
                        let progress = d.remaining_secs.to_bits();
                        (d.effect_id, d.format_time(self.european_number_format), progress)
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
        let font_scale = self.config.font_scale.clamp(0.3, 3.0);
        let font_size = self.frame.scaled(BASE_FONT_SIZE * font_scale);
        let icon_size = self.frame.scaled(self.config.icon_size as f32);
        let name_width = self.frame.scaled(BASE_NAME_WIDTH * font_scale);
        let row_height = icon_size + row_spacing;
        let scale = self.frame.scale_factor();
        let header_font_size = font_size * 1.4;

        // Calculate header space if enabled
        let header_space =
            Header::space(self.config.show_header, header_font_size, row_spacing, scale);

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

        // Compute starting y based on stack direction
        let window_height = self.frame.height() as f32;
        let total_rows_height = num_visible as f32 * row_height;
        let rows_start_y = if self.config.stack_from_bottom && num_visible > 0 {
            (window_height - padding - total_rows_height + row_spacing)
                .max(padding + header_space)
        } else {
            padding + header_space
        };
        let header_y = if self.config.stack_from_bottom && num_visible > 0 {
            (rows_start_y - header_space).max(padding)
        } else {
            padding
        };

        // Begin frame (clear, background, border)
        if self.config.dynamic_background {
            if self.config.stack_from_bottom && num_visible > 0 {
                let content_y = (header_y - padding).max(0.0);
                self.frame
                    .begin_frame_with_content_rect(content_y, content_height);
            } else {
                self.frame.begin_frame_with_content_height(content_height);
            }
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
                    header_y,
                    content_width,
                    header_font_size,
                    row_spacing,
                );
        }

        if self.data.targets.is_empty() {
            self.frame.end_frame();
            return;
        }

        let mut y = rows_start_y;
        // Must match the rounding in bar_or_icon_size() or the cache misses
        let icon_size_u32 = icon_size.round() as u32;

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

            // DOT icons after name (or flushed right)
            let mut icon_x = self.icon_run_start_x(
                target.dots.len(),
                icon_size,
                icon_spacing,
                x + name_width,
                padding,
            );

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

                // Border; colored squares use the DOT's own color so identity
                // survives the wipe darkening
                let border = if has_icon {
                    colors::white()
                } else {
                    color_from_rgba([dot.color[0], dot.color[1], dot.color[2], 255])
                };
                self.frame.stroke_rounded_rect(
                    icon_x,
                    y,
                    icon_size,
                    icon_size,
                    2.0,
                    1.0,
                    border,
                );

                // Font size for countdown text
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

    /// Render bar mode (progress bars grouped under target-name headers)
    fn render_bar_mode(&mut self) {
        let max_targets = self.config.max_targets as usize;

        // Dirty check — include remaining_secs bits so bar fill updates each frame
        let current_state: Vec<(i64, Vec<(u64, String, u32)>)> = self
            .data
            .targets
            .iter()
            .take(max_targets)
            .filter(|t| !t.dots.is_empty())
            .map(|t| {
                let dots = t
                    .dots
                    .iter()
                    .map(|d| {
                        (
                            d.effect_id,
                            d.format_time(self.european_number_format),
                            d.remaining_secs.to_bits(),
                        )
                    })
                    .collect();
                (t.entity_id, dots)
            })
            .collect();

        if current_state == self.last_rendered_bar && !self.last_rendered_bar.is_empty() {
            return;
        }
        self.last_rendered_bar = current_state;

        let font_scale = self.config.font_scale.clamp(0.3, 3.0);
        let scale = self.frame.scale_factor();

        // Bar height wraps the icon (icon_size = geometry); font_scale = text only
        let bar_height = self.frame.scaled(self.config.icon_size as f32).round() + 4.0 * scale;
        let font_size = self.frame.scaled(BASE_BAR_FONT_SIZE * font_scale);
        let name_font_size = font_size * 0.85;
        let entry_spacing = self.frame.scaled(BASE_ROW_SPACING);
        let padding = self.frame.scaled(BASE_PADDING);
        let bar_radius = 3.0 * scale;
        let content_width = self.frame.width() as f32 - 2.0 * padding;
        let header_font_size = font_size * 1.4;

        // Reserved icon column: full bar height, left of every bar. Always
        // reserved so bars stay aligned whether or not an entry has an icon.
        // The bar overlaps the icon's right edge by 1px so they read as one
        // continuous shape under a single outline.
        let icon_size = bar_height;
        let icon_overlap = 1.0 * scale;
        let icon_size_u32 = icon_size.round() as u32;
        let bar_x = padding + icon_size - icon_overlap;
        let bar_width = content_width - icon_size + icon_overlap;

        let header_space =

            Header::space(self.config.show_header, header_font_size, entry_spacing, scale);

        // Group layout: target name line, then one bar per DOT
        let name_line_height = name_font_size + entry_spacing;
        let group_spacing = entry_spacing * 2.0;

        let total_groups_height: f32 = {
            let mut total = 0.0;
            let mut num_groups = 0usize;
            for t in self.data.targets.iter().take(max_targets) {
                let n = t.dots.len();
                if n == 0 {
                    continue;
                }
                total += name_line_height
                    + n as f32 * bar_height
                    + (n - 1) as f32 * entry_spacing;
                num_groups += 1;
            }
            total + num_groups.saturating_sub(1) as f32 * group_spacing
        };
        let num_visible = self
            .data
            .targets
            .iter()
            .take(max_targets)
            .filter(|t| !t.dots.is_empty())
            .count();
        let content_height = if num_visible > 0 {
            padding * 2.0 + header_space + total_groups_height
        } else if self.config.show_header {
            padding * 2.0 + header_space
        } else {
            0.0
        };

        let window_height = self.frame.height() as f32;
        let groups_start_y = if self.config.stack_from_bottom && num_visible > 0 {
            (window_height - padding - total_groups_height).max(padding + header_space)
        } else {
            padding + header_space
        };
        let header_y = if self.config.stack_from_bottom && num_visible > 0 {
            (groups_start_y - header_space).max(padding)
        } else {
            padding
        };

        if self.config.dynamic_background {
            if self.config.stack_from_bottom && num_visible > 0 {
                let content_y = (header_y - padding).max(0.0);
                self.frame
                    .begin_frame_with_content_rect(content_y, content_height);
            } else {
                self.frame.begin_frame_with_content_height(content_height);
            }
        } else {
            self.frame.begin_frame();
        }

        if self.config.show_header {
            Header::new("DOT Tracker")
                .with_color(colors::white())
                .render(
                    &mut self.frame,
                    padding,
                    header_y,
                    content_width,
                    header_font_size,
                    entry_spacing,
                );
        }

        if num_visible == 0 {
            self.frame.end_frame();
            return;
        }

        let mut y = groups_start_y;

        for target in self.data.targets.iter().take(max_targets) {
            if target.dots.is_empty() {
                continue;
            }

            // Target name header line
            self.frame.draw_text_glowed(
                &target.name,
                padding,
                y + name_font_size,
                name_font_size,
                colors::white(),
            );
            y += name_line_height;

            for dot in &target.dots {
                let has_icon = dot.show_icon && dot.icon.is_some();

                // Name in the label is optional, but always shown when there's
                // no icon to identify the DOT
                let mut label = String::new();
                if self.config.show_effect_names || !has_icon {
                    label.push_str(&dot.name);
                }
                if self.config.show_source_name && !dot.source_name.is_empty() {
                    label.push_str(&format!(" ({})", dot.source_name));
                }

                let mut bar = ProgressBar::new(&label, dot.progress())
                    .with_fill_color(color_from_rgba(dot.color))
                    .with_bg_color(colors::dps_bar_bg())
                    .with_text_color(colors::white())
                    .with_bold_text()
                    .with_gradient(self.config.bar_gradient)
                    .with_text_glow();

                if self.config.show_countdown {
                    bar = bar.with_right_text(dot.format_time(self.european_number_format));
                }

                // Draw icon in the reserved column first so the bar's left
                // edge overlaps its right border; entries without an icon get
                // the shared tinted placeholder so bars stay aligned.
                let mut icon_drawn = false;
                if has_icon {
                    if let Some(scaled_icon) =
                        shared_scaled_icons().get(dot.icon_ability_id, icon_size_u32)
                    {
                        self.frame.draw_image(
                            &scaled_icon,
                            icon_size_u32,
                            icon_size_u32,
                            padding,
                            y,
                            icon_size,
                            icon_size,
                        );
                        icon_drawn = true;
                    } else if let Some(ref icon_arc) = dot.icon {
                        let (img_w, img_h, ref rgba) = **icon_arc;
                        self.frame
                            .draw_image(rgba, img_w, img_h, padding, y, icon_size, icon_size);
                        icon_drawn = true;
                    }
                }
                if !icon_drawn {
                    draw_icon_placeholder(
                        &mut self.frame,
                        padding,
                        y,
                        icon_size,
                        bar_height,
                        bar_radius,
                        color_from_rgba(dot.color),
                    );
                }

                bar.render(&mut self.frame, bar_x, y, bar_width, bar_height, font_size, bar_radius);

                // Per-entry border outline (user-configurable colour, toggleable):
                // one continuous outline around the icon slot + bar.
                if self.config.show_border {
                    self.frame.stroke_rounded_rect(
                        padding,
                        y,
                        content_width,
                        bar_height,
                        bar_radius,
                        0.8 * scale,
                        color_from_rgba(self.config.border_color),
                    );
                }

                y += bar_height + entry_spacing;
            }

            // Swap the trailing bar spacing for the larger group gap
            y += group_spacing - entry_spacing;
        }

        self.frame.end_frame();
    }

    /// Render bar mode preview (grouped bars with placeholder targets)
    fn render_preview_bar(&mut self) {
        let font_scale = self.config.font_scale.clamp(0.3, 3.0);
        let scale = self.frame.scale_factor();
        let icon_size = self.frame.scaled(self.config.icon_size as f32).round();
        let bar_height = icon_size + 4.0 * scale;
        let font_size = self.frame.scaled(BASE_BAR_FONT_SIZE * font_scale);
        let name_font_size = font_size * 0.85;
        let entry_spacing = self.frame.scaled(BASE_ROW_SPACING);
        let padding = self.frame.scaled(BASE_PADDING);
        let bar_radius = 3.0 * scale;
        let content_width = self.frame.width() as f32 - 2.0 * padding;
        let header_font_size = font_size * 1.4;

        let header_space =

            Header::space(self.config.show_header, header_font_size, entry_spacing, scale);

        let name_line_height = name_font_size + entry_spacing;
        let group_spacing = entry_spacing * 2.0;

        self.frame.begin_frame();

        // Sample preview data: 2 targets with 2 DOTs each
        let label = if self.config.show_effect_names { "DOT Name" } else { "" };
        let targets = [
            ("Target 1", [(label, "12.3", 0.75_f32), (label, "8.5", 0.40)]),
            ("Target 2", [(label, "5.2", 0.55), (label, "3.1", 0.10)]),
        ];

        let window_height = self.frame.height() as f32;
        let n_groups = targets.len();
        let bars_per_group = 2usize;
        let group_height = name_line_height
            + bars_per_group as f32 * bar_height
            + (bars_per_group - 1) as f32 * entry_spacing;
        let total_groups_height =
            n_groups as f32 * group_height + (n_groups - 1) as f32 * group_spacing;

        let groups_start_y = if self.config.stack_from_bottom {
            (window_height - padding - total_groups_height).max(padding + header_space)
        } else {
            padding + header_space
        };
        let header_y = if self.config.stack_from_bottom {
            (groups_start_y - header_space).max(padding)
        } else {
            padding
        };

        if self.config.show_header {
            Header::new("DOT Tracker")
                .with_color(colors::white())
                .render(
                    &mut self.frame,
                    padding,
                    header_y,
                    content_width,
                    header_font_size,
                    entry_spacing,
                );
        }

        let mut y = groups_start_y;

        for (target_name, dots) in &targets {
            self.frame.draw_text_glowed(
                target_name,
                padding,
                y + name_font_size,
                name_font_size,
                colors::white(),
            );
            y += name_line_height;

            for (name, time_text, progress) in dots {
                // Placeholder icon slot (matches render_bar_mode geometry)
                let icon_size = bar_height;
                let icon_overlap = 1.0 * scale;
                let bar_x = padding + icon_size - icon_overlap;
                let bar_width = content_width - icon_size + icon_overlap;

                draw_icon_placeholder(
                    &mut self.frame,
                    padding,
                    y,
                    icon_size,
                    bar_height,
                    bar_radius,
                    colors::effect_icon_bg(),
                );

                let mut bar = ProgressBar::new(*name, *progress)
                    .with_fill_color(colors::effect_icon_bg())
                    .with_bg_color(colors::dps_bar_bg())
                    .with_text_color(colors::white())
                    .with_bold_text()
                    .with_text_glow();
                if self.config.show_countdown {
                    bar = bar.with_right_text(*time_text);
                }
                bar.render(&mut self.frame, bar_x, y, bar_width, bar_height, font_size, bar_radius);

                if self.config.show_border {
                    self.frame.stroke_rounded_rect(
                        padding,
                        y,
                        content_width,
                        bar_height,
                        bar_radius,
                        0.8 * scale,
                        color_from_rgba(self.config.border_color),
                    );
                }

                y += bar_height + entry_spacing;
            }

            y += group_spacing - entry_spacing;
        }

        self.frame.end_frame();
    }

    /// Render preview placeholders in move mode
    fn render_preview(&mut self) {
        let padding = self.frame.scaled(BASE_PADDING);
        let row_spacing = self.frame.scaled(BASE_ROW_SPACING);
        let icon_spacing = self.frame.scaled(BASE_ICON_SPACING);
        let font_scale = self.config.font_scale.clamp(0.3, 3.0);
        let font_size = self.frame.scaled(BASE_FONT_SIZE * font_scale);
        let icon_size = self.frame.scaled(self.config.icon_size as f32);
        let name_width = self.frame.scaled(BASE_NAME_WIDTH * font_scale);
        let row_height = icon_size + row_spacing;
        let scale = self.frame.scale_factor();
        let header_font_size = font_size * 1.4;

        // Calculate header space if enabled
        let header_space =
            Header::space(self.config.show_header, header_font_size, row_spacing, scale);

        self.frame.begin_frame();

        // Sample preview data: 2 targets with 3 DOTs each
        let targets = [
            ("Target 1", ["12.3", "8.5", "45"]),
            ("Target 2", ["5.2", "18", "3.1"]),
        ];

        let window_height = self.frame.height() as f32;
        let n = targets.len() as f32;
        let total_rows_height = n * row_height;
        let rows_start_y = if self.config.stack_from_bottom {
            (window_height - padding - total_rows_height + row_spacing)
                .max(padding + header_space)
        } else {
            padding + header_space
        };
        let header_y = if self.config.stack_from_bottom {
            (rows_start_y - header_space).max(padding)
        } else {
            padding
        };

        // Render header if enabled
        if self.config.show_header {
            let content_width = self.frame.width() as f32 - 2.0 * padding;
            Header::new("DOT Tracker")
                .with_color(colors::white())
                .render(
                    &mut self.frame,
                    padding,
                    header_y,
                    content_width,
                    header_font_size,
                    row_spacing,
                );
        }

        let mut y = rows_start_y;

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

            // DOT icons after name (or flushed right)
            let mut icon_x =
                self.icon_run_start_x(dots.len(), icon_size, icon_spacing, x + name_width, padding);

            for time_text in dots {
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
