//! Cooldown Tracker Overlay
//!
//! Displays ability cooldowns as a vertical list of icons with countdown timers.
//! Shows cooldowns sorted by remaining time with optional ability names.

use std::sync::Arc;

use super::{Overlay, OverlayConfigUpdate, OverlayData};
use crate::frame::OverlayFrame;
use crate::platform::{OverlayConfig, PlatformError};
use crate::utils::{color_from_rgba, shared_scaled_icons};
use crate::widgets::{colors, ProgressBar};
use crate::widgets::Header;

/// A single cooldown entry for display
#[derive(Debug, Clone)]
pub struct CooldownEntry {
    /// Ability ID for identification
    pub ability_id: u64,
    /// Display name of the ability
    pub name: String,
    /// Remaining cooldown in seconds
    pub remaining_secs: f32,
    /// Total cooldown duration in seconds
    pub total_secs: f32,
    /// Ability ID for icon lookup
    pub icon_ability_id: u64,
    /// Charge count (0 = don't show, 1 = single charge, 2+ = show count)
    pub charges: u8,
    /// Max charges (for display purposes)
    pub max_charges: u8,
    /// Color (RGBA) - used as fallback if no icon
    pub color: [u8; 4],
    /// Source entity name (who cast)
    pub source_name: String,
    /// Target entity name (target of ability)
    pub target_name: String,
    /// Pre-loaded icon RGBA data (width, height, rgba_bytes) - Arc for cheap cloning
    pub icon: Option<Arc<(u32, u32, Vec<u8>)>>,
    /// Whether to show the icon (true) or use colored square (false)
    pub show_icon: bool,
    /// Whether to display the source entity name
    pub display_source: bool,
    /// Whether cooldown is in "ready" state (remaining <= cooldown_ready_secs)
    pub is_in_ready_state: bool,
}

impl CooldownEntry {
    /// Progress as 0.0 (just started) to 1.0 (ready)
    pub fn progress(&self) -> f32 {
        if self.total_secs <= 0.0 {
            return 1.0;
        }
        let elapsed = self.total_secs - self.remaining_secs;
        (elapsed / self.total_secs).clamp(0.0, 1.0)
    }

    /// Format remaining time
    pub fn format_time(&self, european: bool) -> String {
        baras_types::formatting::format_countdown(self.remaining_secs, "s", "Ready", european)
    }

    /// Is the cooldown ready (off cooldown)?
    pub fn is_ready(&self) -> bool {
        self.remaining_secs <= 0.0
    }
}

/// Data sent from service to cooldown overlay
#[derive(Debug, Clone, Default)]
pub struct CooldownData {
    pub entries: Vec<CooldownEntry>,
}

/// Configuration for cooldown overlay
#[derive(Debug, Clone)]
pub struct CooldownConfig {
    pub icon_size: u8,
    pub max_display: u8,
    pub show_ability_names: bool,
    pub sort_by_remaining: bool,
    pub show_source_name: bool,
    pub show_target_name: bool,
    /// Show header title above overlay
    pub show_header: bool,
    /// Font scale multiplier (1.0 - 2.0, default 1.0)
    pub font_scale: f32,
    /// When true, background shrinks to fit content instead of filling the window
    pub dynamic_background: bool,
    /// Render cooldowns as stacked progress bars instead of icons
    pub layout_bar: bool,
}

impl Default for CooldownConfig {
    fn default() -> Self {
        Self {
            icon_size: 32,
            max_display: 10,
            show_ability_names: true,
            sort_by_remaining: true,
            show_source_name: false,
            show_target_name: false,
            show_header: false,
            font_scale: 1.0,
            dynamic_background: true,
            layout_bar: false,
        }
    }
}

/// Base dimensions
const BASE_WIDTH: f32 = 180.0;
const BASE_HEIGHT: f32 = 300.0;
const BASE_PADDING: f32 = 4.0;
const BASE_ROW_SPACING: f32 = 2.0;
const BASE_FONT_SIZE: f32 = 11.0;

/// Bar mode dimensions (matches effects bar mode)
const BASE_BAR_HEIGHT: f32 = 24.0;
const BASE_BAR_FONT_SIZE: f32 = 11.0;

/// Light-blue border for ready-state bars
const READY_BORDER: [u8; 4] = [100, 180, 255, 200];

/// Cooldown overlay - vertical list of ability cooldowns
pub struct CooldownOverlay {
    frame: OverlayFrame,
    config: CooldownConfig,
    background_alpha: u8,
    data: CooldownData,
    /// Last rendered state for dirty checking (icon mode): (ability_id, time_string, charges)
    last_rendered: Vec<(u64, String, u8)>,
    /// Last rendered state for dirty checking (bar mode): (ability_id, time_string, charges, is_ready_state)
    last_rendered_bar: Vec<(u64, String, u8, bool)>,
    european_number_format: bool,
}

impl CooldownOverlay {
    /// Create a new cooldown overlay
    pub fn new(
        window_config: OverlayConfig,
        config: CooldownConfig,
        background_alpha: u8,
    ) -> Result<Self, PlatformError> {
        let mut frame = OverlayFrame::new(window_config, BASE_WIDTH, BASE_HEIGHT)?;
        frame.set_background_alpha(background_alpha);
        frame.set_label("Cooldowns");

        Ok(Self {
            frame,
            config,
            background_alpha,
            data: CooldownData::default(),
            last_rendered: Vec::new(),
            last_rendered_bar: Vec::new(),
            european_number_format: false,
        })
    }

    /// Update the config
    pub fn set_config(&mut self, config: CooldownConfig) {
        self.config = config;
    }

    /// Update background alpha
    pub fn set_background_alpha(&mut self, alpha: u8) {
        self.background_alpha = alpha;
        self.frame.set_background_alpha(alpha);
    }

    /// Update the data and pre-cache icons
    pub fn set_data(&mut self, mut data: CooldownData) {
        let icon_size = if self.config.layout_bar {
            let bar_h = self.frame.scaled(BASE_BAR_HEIGHT * self.config.font_scale.clamp(1.0, 2.0));
            (bar_h - 4.0 * self.frame.scale_factor()).round() as u32
        } else {
            self.frame.scaled(self.config.icon_size as f32) as u32
        };

        // Pre-cache icons at display size in the process-wide shared cache
        let cache = shared_scaled_icons();
        for entry in &data.entries {
            if let Some(ref icon_arc) = entry.icon {
                let (src_w, src_h, ref src_data) = **icon_arc;
                let _ =
                    cache.get_or_scale(entry.icon_ability_id, icon_size, src_data, src_w, src_h);
            }
        }

        // Sort by remaining time if configured
        if self.config.sort_by_remaining {
            data.entries.sort_by(|a, b| {
                a.remaining_secs
                    .partial_cmp(&b.remaining_secs)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        self.data = data;
    }

    /// Render the overlay
    pub fn render(&mut self) {
        // In move mode, always render preview (bypass dirty check)
        if self.frame.is_in_move_mode() {
            if self.config.layout_bar {
                self.render_preview_bar();
            } else {
                self.render_preview();
            }
            return;
        }

        if self.config.layout_bar {
            self.render_bar_mode();
            return;
        }

        let max_display = self.config.max_display as usize;

        // Build current visible state for dirty check
        let current_state: Vec<(u64, String, u8)> = self
            .data
            .entries
            .iter()
            .take(max_display)
            .map(|e| {
                (
                    e.ability_id,
                    e.format_time(self.european_number_format),
                    e.charges,
                )
            })
            .collect();

        // Skip render if nothing changed (but always render at least once)
        if current_state == self.last_rendered && !self.last_rendered.is_empty() {
            return;
        }
        self.last_rendered = current_state;

        let padding = self.frame.scaled(BASE_PADDING);
        let row_spacing = self.frame.scaled(BASE_ROW_SPACING);
        let font_scale = self.config.font_scale.clamp(1.0, 2.0);
        let font_size = self.frame.scaled(BASE_FONT_SIZE * font_scale);
        let icon_size = self.frame.scaled(self.config.icon_size as f32);
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
        let num_entries = self.data.entries.iter().take(max_display).count();
        let content_height = if num_entries > 0 && self.config.show_header {
            padding * 2.0 + header_space + num_entries as f32 * row_height
        } else if num_entries > 0 {
            padding * 2.0 + num_entries as f32 * row_height
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
            Header::new("Cooldowns").with_color(colors::white()).render(
                &mut self.frame,
                padding,
                padding,
                content_width,
                header_font_size,
                row_spacing,
            );
        }

        if self.data.entries.is_empty() {
            self.frame.end_frame();
            return;
        }

        let mut y = padding + header_space;

        let icon_size_u32 = icon_size as u32;

        for entry in self.data.entries.iter().take(max_display) {
            let x = padding;

            // Draw icon from cache or colored square fallback
            // Only show icon if show_icon is true
            let has_icon = if entry.show_icon {
                if let Some(scaled_icon) =
                    shared_scaled_icons().get(entry.icon_ability_id, icon_size_u32)
                {
                    self.frame.draw_image(
                        &scaled_icon,
                        icon_size_u32,
                        icon_size_u32,
                        x,
                        y,
                        icon_size,
                        icon_size,
                    );
                    true
                } else if let Some(ref icon_arc) = entry.icon {
                    // Fallback if cache miss
                    let (img_w, img_h, ref rgba) = **icon_arc;
                    self.frame
                        .draw_image(rgba, img_w, img_h, x, y, icon_size, icon_size);
                    true
                } else {
                    false
                }
            } else {
                false
            };

            if !has_icon {
                // Fallback: colored square
                let bg_color = color_from_rgba(entry.color);
                self.frame
                    .fill_rounded_rect(x, y, icon_size, icon_size, 3.0, bg_color);
            }

            // Decreasing clock wipe - overlay shrinks from top, revealing icon
            // progress: 0 = just used, 1 = ready
            // Overlay starts full (when progress=0), shrinks as progress→1
            let progress = entry.progress();
            let overlay_height = icon_size * (1.0 - progress);
            if overlay_height > 1.0 {
                self.frame.fill_rect(
                    x,
                    y,
                    icon_size,
                    overlay_height,
                    color_from_rgba([0, 0, 0, 140]),
                );
            }

            // Border - use light-blue when in "ready" state
            let border_color = if entry.is_in_ready_state {
                colors::cooldown_ready() // Light-blue when in ready state
            } else {
                colors::white()
            };
            self.frame
                .stroke_rounded_rect(x, y, icon_size, icon_size, 3.0, 1.0, border_color);

            // Charge count in corner (if > 1 or showing max charges)
            if entry.charges > 1 || (entry.max_charges > 1 && entry.charges > 0) {
                let charge_text = format!("{}", entry.charges);
                let charge_font_size = font_size * 1.0;
                let charge_x =
                    x + icon_size - self.frame.measure_text(&charge_text, charge_font_size).0 - 2.0;
                let charge_y = y + charge_font_size + 2.0;

                self.frame.draw_text_glowed(
                    &charge_text,
                    charge_x,
                    charge_y,
                    charge_font_size,
                    colors::icon_countdown(),
                );
            }

            // Ability name and countdown text
            let text_x = x + icon_size + padding;
            let text_y = y + icon_size / 2.0;

            // Determine text color based on state
            let ready_text_color = if entry.is_in_ready_state {
                colors::cooldown_ready() // Light-blue for ready state
            } else {
                colors::white()
            };

            if self.config.show_ability_names {
                // Ability name on top
                let name_y = text_y - font_size * 0.3;
                self.frame.draw_text_glowed(
                    &entry.name,
                    text_x,
                    name_y,
                    font_size,
                    colors::white(),
                );

                // Countdown below
                let time_text = entry.format_time(self.european_number_format);
                let time_font_size = font_size * 0.9;
                let time_y = name_y + font_size + 2.0;
                self.frame.draw_text_glowed(
                    &time_text,
                    text_x,
                    time_y,
                    time_font_size,
                    ready_text_color,
                );

                // Source name inline to the right of countdown
                if entry.display_source && !entry.source_name.is_empty() {
                    let source_font_size = font_size * 0.8;
                    let time_width = self.frame.measure_text(&time_text, time_font_size).0;
                    self.frame.draw_text_glowed(
                        &entry.source_name,
                        text_x + time_width + padding,
                        time_y,
                        source_font_size,
                        colors::white(),
                    );
                }
            } else {
                // Just countdown centered
                let time_text = entry.format_time(self.european_number_format);
                let time_color = if entry.is_ready() {
                    ready_text_color
                } else {
                    colors::white()
                };
                let time_y = text_y + font_size / 3.0;
                self.frame
                    .draw_text_glowed(&time_text, text_x, time_y, font_size, time_color);

                // Source name inline to the right of countdown
                if entry.display_source && !entry.source_name.is_empty() {
                    let source_font_size = font_size * 0.8;
                    let time_width = self.frame.measure_text(&time_text, font_size).0;
                    self.frame.draw_text_glowed(
                        &entry.source_name,
                        text_x + time_width + padding,
                        time_y,
                        source_font_size,
                        colors::white(),
                    );
                }
            }

            y += row_height;
        }

        self.frame.end_frame();
    }

    /// Render preview placeholders in move mode
    fn render_preview(&mut self) {
        let padding = self.frame.scaled(BASE_PADDING);
        let row_spacing = self.frame.scaled(BASE_ROW_SPACING);
        let font_size = self.frame.scaled(BASE_FONT_SIZE);
        let icon_size = self.frame.scaled(self.config.icon_size as f32);
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
            Header::new("Cooldowns").with_color(colors::white()).render(
                &mut self.frame,
                padding,
                padding,
                content_width,
                header_font_size,
                row_spacing,
            );
        }

        let mut y = padding + header_space;

        // Sample preview data
        let previews = [
            ("Ability", "12.3s", 2u8),
            ("Ability", "45.0s", 1u8),
            ("Ability", "1:30", 0u8),
        ];

        for (name, time_text, charges) in &previews {
            let x = padding;

            // Placeholder icon background
            self.frame
                .fill_rounded_rect(x, y, icon_size, icon_size, 3.0, colors::effect_icon_bg());

            // Dashed border to indicate preview
            self.frame.stroke_rounded_rect_dashed(
                x,
                y,
                icon_size,
                icon_size,
                3.0,
                1.0,
                colors::preview_border(),
                3.0,
                2.0,
            );

            // Charge count in corner (if > 1)
            if *charges > 1 {
                let charge_text = format!("{}", charges);
                let charge_font_size = font_size * 1.0;
                let charge_x =
                    x + icon_size - self.frame.measure_text(&charge_text, charge_font_size).0 - 2.0;
                let charge_y = y + charge_font_size + 2.0;

                self.frame.draw_text_glowed(
                    &charge_text,
                    charge_x,
                    charge_y,
                    charge_font_size,
                    colors::icon_countdown(),
                );
            }

            // Text to the right
            let text_x = x + icon_size + padding;
            let text_y = y + icon_size / 2.0;

            if self.config.show_ability_names {
                // Ability name on top
                let name_y = text_y - font_size * 0.3;
                self.frame
                    .draw_text_glowed(name, text_x, name_y, font_size, colors::white());

                // Countdown below
                let time_y = name_y + font_size + 2.0;
                self.frame.draw_text_glowed(
                    time_text,
                    text_x,
                    time_y,
                    font_size * 0.9,
                    colors::white(),
                );
            } else {
                // Just countdown centered
                self.frame.draw_text_glowed(
                    time_text,
                    text_x,
                    text_y + font_size / 3.0,
                    font_size,
                    colors::white(),
                );
            }

            y += row_height;
        }

        self.frame.end_frame();
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Bar Mode
    // ─────────────────────────────────────────────────────────────────────────

    fn render_bar_mode(&mut self) {
        let max_display = self.config.max_display as usize;

        // Dirty check: include ready state so border changes are caught
        let current_state: Vec<(u64, String, u8, bool)> = self
            .data
            .entries
            .iter()
            .take(max_display)
            .map(|e| {
                (
                    e.ability_id,
                    e.format_time(self.european_number_format),
                    e.charges,
                    e.is_in_ready_state,
                )
            })
            .collect();

        if current_state == self.last_rendered_bar && !self.last_rendered_bar.is_empty() {
            return;
        }
        self.last_rendered_bar = current_state;

        let font_scale = self.config.font_scale.clamp(1.0, 2.0);
        let bar_height = self.frame.scaled(BASE_BAR_HEIGHT * font_scale);
        let font_size = self.frame.scaled(BASE_BAR_FONT_SIZE * font_scale);
        let entry_spacing = self.frame.scaled(BASE_ROW_SPACING);
        let padding = self.frame.scaled(BASE_PADDING);
        let bar_radius = 3.0 * self.frame.scale_factor();
        let content_width = self.frame.width() as f32 - 2.0 * padding;
        let scale = self.frame.scale_factor();
        let header_font_size = font_size * 1.4;
        let icon_size = bar_height - 4.0 * scale;
        let icon_padding = 2.0 * scale;
        let icon_size_u32 = icon_size.round() as u32;

        let header_space = if self.config.show_header {
            header_font_size + entry_spacing + 2.0 + entry_spacing + 4.0 * scale
        } else {
            0.0
        };

        let n = self.data.entries.iter().take(max_display).count();
        let content_height = if n == 0 {
            0.0
        } else {
            padding * 2.0
                + header_space
                + n as f32 * bar_height
                + (n - 1) as f32 * entry_spacing
        };

        if self.config.dynamic_background {
            self.frame.begin_frame_with_content_height(content_height);
        } else {
            self.frame.begin_frame();
        }

        if self.config.show_header {
            let content_width_h = self.frame.width() as f32 - 2.0 * padding;
            Header::new("Cooldowns").with_color(colors::white()).render(
                &mut self.frame,
                padding,
                padding,
                content_width_h,
                header_font_size,
                entry_spacing,
            );
        }

        if self.data.entries.is_empty() {
            self.frame.end_frame();
            return;
        }

        let font_color = tiny_skia::Color::WHITE;
        let mut y = padding + header_space;

        for entry in self.data.entries.iter().take(max_display) {
            // Label: optional charges prefix + name + optional source
            let mut label = String::new();
            if entry.charges > 1 {
                label.push_str(&format!("{}x ", entry.charges));
            }
            if self.config.show_ability_names || label.is_empty() {
                label.push_str(&entry.name);
            }
            if entry.display_source && !entry.source_name.is_empty() {
                label.push_str(&format!(" ({})", entry.source_name));
            }

            let right_text = entry.format_time(self.european_number_format);
            let has_icon = entry.show_icon && entry.icon.is_some();
            let bar_color = color_from_rgba(entry.color);

            let mut bar = ProgressBar::new(&label, entry.progress())
                .with_fill_color(bar_color)
                .with_bg_color(colors::dps_bar_bg())
                .with_text_color(font_color)
                .with_right_text(&right_text)
                .with_bold_text()
                .with_text_glow();

            if has_icon {
                bar = bar.with_label_offset(icon_size + icon_padding);
            }

            bar.render(&mut self.frame, padding, y, content_width, bar_height, font_size, bar_radius);

            // Light-blue border for ready state
            if entry.is_in_ready_state {
                let c = tiny_skia::Color::from_rgba(
                    READY_BORDER[0] as f32 / 255.0,
                    READY_BORDER[1] as f32 / 255.0,
                    READY_BORDER[2] as f32 / 255.0,
                    READY_BORDER[3] as f32 / 255.0,
                )
                .unwrap_or(tiny_skia::Color::WHITE);
                self.frame.stroke_rounded_rect(padding, y, content_width, bar_height, bar_radius, 1.5 * scale, c);
            }

            // Draw icon with glow border
            if has_icon {
                let icon_x = padding + icon_padding;
                let icon_y = y + icon_padding;

                let icon_drawn = if let Some(scaled) =
                    shared_scaled_icons().get(entry.icon_ability_id, icon_size_u32)
                {
                    self.frame.draw_image(&scaled, icon_size_u32, icon_size_u32, icon_x, icon_y, icon_size, icon_size);
                    true
                } else if let Some(ref icon_arc) = entry.icon {
                    let (img_w, img_h, ref rgba) = **icon_arc;
                    self.frame.draw_image(rgba, img_w, img_h, icon_x, icon_y, icon_size, icon_size);
                    true
                } else {
                    false
                };

                if icon_drawn {
                    let icon_radius = 2.0 * scale;
                    let glow_expand = 1.0 * scale;
                    let outer_glow = tiny_skia::Color::from_rgba(1.0, 1.0, 1.0, 0.25).unwrap();
                    self.frame.stroke_rounded_rect(
                        icon_x - glow_expand, icon_y - glow_expand,
                        icon_size + glow_expand * 2.0, icon_size + glow_expand * 2.0,
                        icon_radius + glow_expand, 1.5 * scale, outer_glow,
                    );
                    let inner_border = tiny_skia::Color::from_rgba(1.0, 1.0, 1.0, 0.6).unwrap();
                    self.frame.stroke_rounded_rect(
                        icon_x, icon_y, icon_size, icon_size,
                        icon_radius, 1.0 * scale, inner_border,
                    );
                }
            }

            y += bar_height + entry_spacing;
        }

        self.frame.end_frame();
    }

    fn render_preview_bar(&mut self) {
        let padding = self.frame.scaled(BASE_PADDING);
        let entry_spacing = self.frame.scaled(BASE_ROW_SPACING);
        let font_scale = self.config.font_scale.clamp(1.0, 2.0);
        let bar_height = self.frame.scaled(BASE_BAR_HEIGHT * font_scale);
        let font_size = self.frame.scaled(BASE_BAR_FONT_SIZE * font_scale);
        let bar_radius = 3.0 * self.frame.scale_factor();
        let content_width = self.frame.width() as f32 - 2.0 * padding;
        let font_color = tiny_skia::Color::WHITE;
        let accent = colors::effect_icon_bg();

        self.frame.begin_frame();

        let mut y = padding;

        let previews = [
            ("Ability A", "Ready", true),
            ("Ability B", "12.3s", false),
            ("Ability C", "1:30", false),
        ];

        for (name, time_text, is_ready) in previews {
            ProgressBar::new(name, if is_ready { 1.0 } else { 0.4 })
                .with_fill_color(accent)
                .with_bg_color(colors::dps_bar_bg())
                .with_text_color(font_color)
                .with_right_text(time_text)
                .with_bold_text()
                .with_text_glow()
                .render(&mut self.frame, padding, y, content_width, bar_height, font_size, bar_radius);

            if is_ready {
                let c = tiny_skia::Color::from_rgba(
                    READY_BORDER[0] as f32 / 255.0,
                    READY_BORDER[1] as f32 / 255.0,
                    READY_BORDER[2] as f32 / 255.0,
                    READY_BORDER[3] as f32 / 255.0,
                )
                .unwrap_or(tiny_skia::Color::WHITE);
                self.frame.stroke_rounded_rect(padding, y, content_width, bar_height, bar_radius, 1.5 * self.frame.scale_factor(), c);
            }

            y += bar_height + entry_spacing;
        }

        let _ = y;
        self.frame.end_frame();
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Trait Implementation
// ─────────────────────────────────────────────────────────────────────────────

impl Overlay for CooldownOverlay {
    fn update_data(&mut self, data: OverlayData) -> bool {
        if let OverlayData::Cooldowns(cooldown_data) = data {
            let was_empty = self.data.entries.is_empty();
            let is_empty = cooldown_data.entries.is_empty();
            self.set_data(cooldown_data);
            !(was_empty && is_empty)
        } else {
            false
        }
    }

    fn update_config(&mut self, config: OverlayConfigUpdate) {
        if let OverlayConfigUpdate::Cooldowns(cfg, alpha, european) = config {
            self.set_config(cfg);
            self.set_background_alpha(alpha);
            self.european_number_format = european;
        }
    }

    fn render(&mut self) {
        CooldownOverlay::render(self);
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
