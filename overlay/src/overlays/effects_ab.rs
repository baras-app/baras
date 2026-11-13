//! Effects Overlay (A/B)
//!
//! Consolidated overlay for displaying effect icons with countdowns.
//! Supports both horizontal (row) and vertical (column) layouts.
//! Used for Effects A and Effects B overlays.

use std::sync::Arc;

use super::{Overlay, OverlayConfigUpdate, OverlayData};
use crate::frame::OverlayFrame;
use crate::platform::{OverlayConfig, PlatformError};
use crate::utils::{color_from_rgba, shared_scaled_icons};
use crate::widgets::{colors, Header, IconBarRow, ProgressBar, RowIcon};

/// Layout direction for effects display
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EffectsLayout {
    /// Horizontal row of icons
    #[default]
    Horizontal,
    /// Vertical column of icons with text
    Vertical,
    /// Vertically stacked progress bars (timer-style)
    Bar,
}

/// A single effect entry for display
#[derive(Debug, Clone)]
pub struct EffectABEntry {
    /// Effect ID for identification
    pub effect_id: u64,
    /// Ability ID for icon lookup
    pub icon_ability_id: u64,
    /// Display name of the effect
    pub name: String,
    /// Display text override (from definition; falls back to name if empty)
    pub display_text: String,
    /// Remaining time in seconds
    pub remaining_secs: f32,
    /// Total duration in seconds (for progress calculation)
    pub total_secs: f32,
    /// Color (RGBA) - used as fallback if no icon
    pub color: [u8; 4],
    /// Stack count (0 = don't show)
    pub stacks: u8,
    /// Source entity name
    pub source_name: String,
    /// Target entity name
    pub target_name: String,
    /// Pre-loaded icon RGBA data (width, height, rgba_bytes) - Arc for cheap cloning
    pub icon: Option<Arc<(u32, u32, Vec<u8>)>>,
    /// Whether to show the icon (true) or use colored square (false)
    pub show_icon: bool,
    /// Whether to display the source entity name
    pub display_source: bool,
    /// Whether to display the target entity name
    pub show_target: bool,
    /// Max total duration budget in seconds (from modifier max_duration_secs).
    pub max_total_secs: Option<f32>,
    /// Remaining seconds until the absolute max cap expires.
    pub max_remaining_secs: Option<f32>,
    /// Uptime placeholder: effect is not currently active — render desaturated,
    /// no countdown or clock wipe.
    pub inactive: bool,
    /// Per-effect stack emphasis (ORed with the overlay's stack_priority config)
    pub stack_priority: bool,
    /// Per-effect countdown visibility (ANDed with the overlay's show_countdown config)
    pub show_countdown: bool,
}

impl EffectABEntry {
    /// Source/target annotation, if any flag is set and the name is known
    fn entity_label(&self) -> Option<String> {
        super::entity_label(self.display_source, &self.source_name, self.show_target, &self.target_name)
    }

    fn has_entity_label(&self) -> bool {
        (self.display_source && !self.source_name.is_empty())
            || (self.show_target && !self.target_name.is_empty())
    }

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

    /// Format max budget text like "12/30" (elapsed / max) if max_total_secs is set
    pub fn format_budget(&self) -> Option<String> {
        let max = self.max_total_secs?;
        let max_remaining = self.max_remaining_secs?;
        let elapsed = (max - max_remaining).max(0.0);
        Some(format!("{:.0}/{:.0}", elapsed, max))
    }
}

/// Data sent from service to effects overlay
#[derive(Debug, Clone, Default)]
pub struct EffectsABData {
    pub effects: Vec<EffectABEntry>,
}

/// Configuration for effects overlay
#[derive(Debug, Clone)]
pub struct EffectsABConfig {
    pub icon_size: u8,
    pub max_display: u8,
    pub layout: EffectsLayout,
    pub show_effect_names: bool,
    pub show_countdown: bool,
    /// When true, stacks are shown large and centered; timer is secondary
    pub stack_priority: bool,
    /// Show header title above overlay
    pub show_header: bool,
    /// Title to display in header
    pub header_title: String,
    /// Font scale multiplier (0.3 - 3.0, default 1.0)
    pub font_scale: f32,
    /// When true, background shrinks to fit content instead of filling the window
    pub dynamic_background: bool,
    /// When true, entries stack from the bottom of the overlay window
    pub stack_from_bottom: bool,
    /// When true (and in bar layout), draw an outline around each entry
    pub show_border: bool,
    /// Color of the per-entry border outline (bar layout only)
    pub border_color: [u8; 4],
    /// Fade each bar's fill from its color (left) to a darkened version (right).
    pub bar_gradient: bool,
}

impl Default for EffectsABConfig {
    fn default() -> Self {
        Self {
            icon_size: 32,
            max_display: 8,
            layout: EffectsLayout::Horizontal,
            show_effect_names: false,
            show_countdown: true,
            stack_priority: false,
            show_header: false,
            header_title: String::new(),
            font_scale: 1.0,
            dynamic_background: true,
            stack_from_bottom: false,
            show_border: true,
            border_color: [128, 128, 128, 255],
            bar_gradient: false,
        }
    }
}

/// Base dimensions
const BASE_WIDTH: f32 = 300.0;
const BASE_HEIGHT: f32 = 300.0;
const BASE_PADDING: f32 = 4.0;
const BASE_SPACING: f32 = 4.0;
const BASE_FONT_SIZE: f32 = 10.0;
/// Icon-layout fonts track the configured icon size so countdown/stack text
/// stays proportional; at the default 32px icon this equals BASE_FONT_SIZE.
const ICON_FONT_RATIO: f32 = BASE_FONT_SIZE / 32.0;
/// Bar mode dimensions (matches timer overlay style)
const BASE_BAR_FONT_SIZE: f32 = 17.0;

/// Effects overlay - displays effect icons in horizontal or vertical layout
pub struct EffectsABOverlay {
    frame: OverlayFrame,
    config: EffectsABConfig,
    background_alpha: u8,
    data: EffectsABData,
    /// Last rendered state for dirty checking: (effect_id, time_string, stacks, inactive, stack_priority, remaining_bits)
    last_rendered: Vec<(u64, String, u8, bool, bool, u32)>,
    /// Last rendered state for bar mode dirty checking: (effect_id, time_string, stacks, remaining_bits, inactive)
    last_rendered_bar: Vec<(u64, String, u8, u32, bool)>,
    /// Label for this overlay instance
    _label: String,
    european_number_format: bool,
}

impl EffectsABOverlay {
    /// Create a new effects overlay
    pub fn new(
        window_config: OverlayConfig,
        config: EffectsABConfig,
        background_alpha: u8,
        label: &str,
    ) -> Result<Self, PlatformError> {
        let mut frame = OverlayFrame::new(window_config, BASE_WIDTH, BASE_HEIGHT)?;
        frame.set_background_alpha(background_alpha);
        frame.set_label(label);

        Ok(Self {
            frame,
            config,
            background_alpha,
            data: EffectsABData::default(),
            last_rendered: Vec::new(),
            last_rendered_bar: Vec::new(),
            _label: label.to_string(),
            european_number_format: false,
        })
    }

    /// Update the config
    pub fn set_config(&mut self, config: EffectsABConfig) {
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

    /// Update the data and pre-cache any new icons
    pub fn set_data(&mut self, data: EffectsABData) {
        // Pre-cache icons at current display size
        let icon_size = self.bar_or_icon_size(matches!(self.config.layout, EffectsLayout::Bar));

        let cache = shared_scaled_icons();
        for effect in &data.effects {
            if let Some(ref icon_arc) = effect.icon {
                let (src_w, src_h, ref src_data) = **icon_arc;
                let _ = cache.get_or_scale_variant(
                    effect.icon_ability_id,
                    icon_size,
                    src_data,
                    src_w,
                    src_h,
                    effect.inactive,
                );
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
            self.render_preview();
            return;
        }

        match self.config.layout {
            EffectsLayout::Horizontal => self.render_horizontal(),
            EffectsLayout::Vertical => self.render_vertical(),
            EffectsLayout::Bar => self.render_bar_mode(),
        }
    }

    /// Render horizontal layout (row of icons)
    fn render_horizontal(&mut self) {
        let max_display = self.config.max_display as usize;

        // Build current visible state for dirty check
        // Exact remaining time keeps the clock wipe moving while the text is
        // stable (whole seconds when >= 10s)
        let current_state: Vec<(u64, String, u8, bool, bool, u32)> = self
            .data
            .effects
            .iter()
            .take(max_display)
            .map(|e| {
                (
                    e.effect_id,
                    e.format_time(self.european_number_format),
                    e.stacks,
                    e.inactive,
                    e.stack_priority,
                    e.remaining_secs.to_bits(),
                )
            })
            .collect();

        // Skip render if nothing changed
        if current_state == self.last_rendered && !self.last_rendered.is_empty() {
            return;
        }
        self.last_rendered = current_state;

        let padding = self.frame.scaled(BASE_PADDING);
        let spacing = self.frame.scaled(BASE_SPACING);
        let font_scale = self.config.font_scale.clamp(0.3, 3.0);
        let font_size = self
            .frame
            .scaled(self.config.icon_size as f32 * ICON_FONT_RATIO * font_scale);
        let icon_size = self.frame.scaled(self.config.icon_size as f32);
        let scale = self.frame.scale_factor();
        let header_font_size = font_size * 1.4;

        // Calculate header space if enabled
        let header_space = Header::space(self.config.show_header, header_font_size, spacing, scale);

        // Count visible entries for dynamic background
        let num_visible = self.data.effects.iter().take(max_display).count();

        // Text rows drawn under the icons: effect name and source/target label
        let has_labels = self
            .data
            .effects
            .iter()
            .take(max_display)
            .any(|e| e.has_entity_label());
        let extra_below = below_icon_height(self.config.show_effect_names, has_labels, font_size);

        // Inner content height (without padding) and total content height
        let inner_height = if num_visible == 0 {
            if self.config.show_header { header_space } else { 0.0 }
        } else {
            header_space + icon_size + extra_below
        };
        let content_height = if num_visible == 0 && !self.config.show_header {
            0.0
        } else {
            padding * 2.0 + inner_height
        };

        // Compute icon row y position based on stack direction
        let window_height = self.frame.height() as f32;
        let icon_row_y = if self.config.stack_from_bottom && num_visible > 0 {
            (window_height - padding - icon_size - extra_below).max(padding + header_space)
        } else {
            padding + header_space
        };
        let header_y = if self.config.stack_from_bottom && num_visible > 0 {
            (icon_row_y - header_space).max(padding)
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

        // Render header if enabled
        if self.config.show_header && !self.config.header_title.is_empty() {
            let content_width = self.frame.width() as f32 - 2.0 * padding;
            Header::new(&self.config.header_title)
                .with_color(colors::white())
                .render(
                    &mut self.frame,
                    padding,
                    header_y,
                    content_width,
                    header_font_size,
                    spacing,
                );
        }

        if self.data.effects.is_empty() {
            self.frame.end_frame();
            return;
        }

        let mut x = padding;
        let y = icon_row_y;
        let icon_size_u32 = icon_size.round() as u32;

        // Clone effects to avoid borrow issues
        let effects: Vec<_> = self
            .data
            .effects
            .iter()
            .take(max_display)
            .cloned()
            .collect();

        for effect in &effects {
            // Draw icon
            let has_icon = self.draw_icon(effect, x, y, icon_size, icon_size_u32);

            // Clock wipe overlay
            let progress = effect.progress();
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

            // Border (after wipe so it stays bright); colored squares use the
            // effect's own color so identity survives the wipe darkening
            let border = if effect.inactive {
                color_from_rgba([160, 160, 160, 160])
            } else if has_icon {
                colors::white()
            } else {
                color_from_rgba([effect.color[0], effect.color[1], effect.color[2], 255])
            };
            self.frame
                .stroke_rounded_rect(x, y, icon_size, icon_size, 3.0, 1.0, border);

            // Stack priority vs normal mode
            if (self.config.stack_priority || effect.stack_priority) && effect.stacks >= 1 {
                self.draw_stack_priority(effect, x, y, icon_size, font_size);
            } else {
                self.draw_normal_mode(effect, x, y, icon_size, font_size);
            }

            // Effect name below icon (labels may use the full icon pitch)
            let label_max = icon_size + spacing;
            let mut text_y_offset = 0.0;
            if self.config.show_effect_names {
                let name_font_size = font_size * 0.85;
                let name =
                    self.frame
                        .truncate_to_width(&effect.name, label_max, name_font_size, false);
                let name_width = self.frame.measure_text(&name, name_font_size).0;
                let name_x = x + (icon_size - name_width) / 2.0;
                let name_y = y + icon_size + name_font_size + 2.0;

                self.frame
                    .draw_text_glowed(&name, name_x, name_y, name_font_size, colors::white());
                text_y_offset = name_font_size + 2.0;
            }

            // Source/target label below effect name
            if let Some(label) = effect.entity_label() {
                let source_font_size = font_size * 0.75;
                let source =
                    self.frame
                        .truncate_to_width(&label, label_max, source_font_size, false);
                let source_width = self.frame.measure_text(&source, source_font_size).0;
                let source_x = x + (icon_size - source_width) / 2.0;
                let source_y = y + icon_size + source_font_size + 2.0 + text_y_offset;

                self.frame.draw_text_glowed(
                    &source,
                    source_x,
                    source_y,
                    source_font_size,
                    colors::white(),
                );
            }

            x += icon_size + spacing;
        }

        self.frame.end_frame();
    }

    /// Render vertical layout (column with text beside icons)
    fn render_vertical(&mut self) {
        let max_display = self.config.max_display as usize;

        // Build current visible state for dirty check
        // Exact remaining time keeps the clock wipe moving while the text is
        // stable (whole seconds when >= 10s)
        let current_state: Vec<(u64, String, u8, bool, bool, u32)> = self
            .data
            .effects
            .iter()
            .take(max_display)
            .map(|e| {
                (
                    e.effect_id,
                    e.format_time(self.european_number_format),
                    e.stacks,
                    e.inactive,
                    e.stack_priority,
                    e.remaining_secs.to_bits(),
                )
            })
            .collect();

        // Skip render if nothing changed
        if current_state == self.last_rendered && !self.last_rendered.is_empty() {
            return;
        }
        self.last_rendered = current_state;

        let padding = self.frame.scaled(BASE_PADDING);
        let row_spacing = self.frame.scaled(BASE_SPACING);
        let font_scale = self.config.font_scale.clamp(0.3, 3.0);
        let font_size = self
            .frame
            .scaled(self.config.icon_size as f32 * ICON_FONT_RATIO * font_scale);
        let icon_size = self.frame.scaled(self.config.icon_size as f32);
        let row_height = icon_size + row_spacing;
        let scale = self.frame.scale_factor();
        let header_font_size = font_size * 1.4;

        // Calculate header space if enabled
        let header_space =
            Header::space(self.config.show_header, header_font_size, row_spacing, scale);

        // Count visible entries for dynamic background
        let num_visible = self.data.effects.iter().take(max_display).count();

        // Compute content height for dynamic background
        let content_height = if num_visible == 0 {
            if self.config.show_header {
                padding * 2.0 + header_space
            } else {
                0.0
            }
        } else {
            padding * 2.0 + header_space + num_visible as f32 * row_height
        };

        // Position rows based on stack direction. Vertical layout uses row_height
        // (icon_size + row_spacing); the bottom of the last row should sit at
        // window_height - padding minus the trailing inter-row spacing.
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
        if self.config.show_header && !self.config.header_title.is_empty() {
            let content_width = self.frame.width() as f32 - 2.0 * padding;
            Header::new(&self.config.header_title)
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

        if self.data.effects.is_empty() {
            self.frame.end_frame();
            return;
        }

        let mut y = rows_start_y;
        let icon_size_u32 = icon_size.round() as u32;

        // Clone effects to avoid borrow issues
        let effects: Vec<_> = self
            .data
            .effects
            .iter()
            .take(max_display)
            .cloned()
            .collect();

        for effect in &effects {
            let x = padding;

            // Draw icon
            let has_icon = self.draw_icon(effect, x, y, icon_size, icon_size_u32);

            // Clock wipe overlay
            let progress = effect.progress();
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

            // Border; colored squares use the effect's own color so identity
            // survives the wipe darkening
            let border = if effect.inactive {
                color_from_rgba([160, 160, 160, 160])
            } else if has_icon {
                colors::white()
            } else {
                color_from_rgba([effect.color[0], effect.color[1], effect.color[2], 255])
            };
            self.frame
                .stroke_rounded_rect(x, y, icon_size, icon_size, 3.0, 1.0, border);

            // Stack priority vs normal mode
            if (self.config.stack_priority || effect.stack_priority) && effect.stacks >= 1 {
                self.draw_stack_priority(effect, x, y, icon_size, font_size);
            } else {
                self.draw_normal_mode(effect, x, y, icon_size, font_size);
            }

            // Text to the right of icon
            let text_x = x + icon_size + padding;
            let text_y = y + icon_size / 2.0;
            let text_max = (self.frame.width() as f32 - padding - text_x).max(0.0);

            if self.config.show_effect_names {
                // Effect name on top
                let name_y = text_y - font_size * 0.3;
                let name = self
                    .frame
                    .truncate_to_width(&effect.name, text_max, font_size, false);
                self.frame.draw_text_glowed(
                    &name,
                    text_x,
                    name_y,
                    font_size,
                    colors::white(),
                );

                // Source/target label below effect name
                if let Some(label) = effect.entity_label() {
                    let source_font_size = font_size * 0.8;
                    let label =
                        self.frame
                            .truncate_to_width(&label, text_max, source_font_size, false);
                    self.frame.draw_text_glowed(
                        &label,
                        text_x,
                        name_y + font_size + 2.0,
                        source_font_size,
                        colors::white(),
                    );
                }
            } else {
                // Source/target label centered to the right of icon
                if let Some(label) = effect.entity_label() {
                    let source_font_size = font_size * 0.8;
                    let label =
                        self.frame
                            .truncate_to_width(&label, text_max, source_font_size, false);
                    self.frame.draw_text_glowed(
                        &label,
                        text_x,
                        text_y + font_size / 3.0,
                        source_font_size,
                        colors::white(),
                    );
                }
            }

            y += row_height;
        }

        self.frame.end_frame();
    }

    /// Render bar mode (vertically stacked progress bars, timer-style)
    fn render_bar_mode(&mut self) {
        let max_display = self.config.max_display as usize;

        // Dirty check — include remaining_secs bits so bar fill updates each frame
        let current_state: Vec<(u64, String, u8, u32, bool)> = self
            .data.effects.iter().take(max_display)
            .map(|e| (e.effect_id, e.format_time(self.european_number_format), e.stacks, e.remaining_secs.to_bits(), e.inactive))
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
        let entry_spacing = self.frame.scaled(BASE_SPACING);
        let padding = self.frame.scaled(BASE_PADDING);
        let bar_radius = 3.0 * scale;
        let content_width = self.frame.width() as f32 - 2.0 * padding;
        let font_color = colors::white();
        let header_font_size = font_size * 1.4;

        let row = IconBarRow::new(padding, content_width, bar_height, bar_radius, scale);
        let border = self.config.show_border.then(|| color_from_rgba(self.config.border_color));

        let header_space =
            Header::space(self.config.show_header, header_font_size, entry_spacing, scale);

        let num_visible = self.data.effects.iter().take(max_display).count();
        let total_bars_height = if num_visible > 0 {
            num_visible as f32 * bar_height + (num_visible - 1) as f32 * entry_spacing
        } else {
            0.0
        };
        let content_height = if num_visible == 0 {
            if self.config.show_header { padding * 2.0 + header_space } else { 0.0 }
        } else {
            padding * 2.0 + header_space + total_bars_height
        };

        let window_height = self.frame.height() as f32;
        let bars_start_y = if self.config.stack_from_bottom && num_visible > 0 {
            (window_height - padding - total_bars_height).max(padding + header_space)
        } else {
            padding + header_space
        };
        let header_y = if self.config.stack_from_bottom && num_visible > 0 {
            (bars_start_y - header_space).max(padding)
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

        if self.config.show_header && !self.config.header_title.is_empty() {
            Header::new(&self.config.header_title)
                .with_color(colors::white())
                .render(&mut self.frame, padding, header_y, content_width, header_font_size, entry_spacing);
        }

        if self.data.effects.is_empty() {
            self.frame.end_frame();
            return;
        }

        let effects: Vec<_> = self.data.effects.iter().take(max_display).cloned().collect();
        let mut y = bars_start_y;

        for effect in &effects {
            let has_icon = effect.show_icon && effect.icon.is_some();

            // Build label: optional name + optional source.
            // When an icon is present the stack count is drawn inside the icon
            // (like the icon layouts); otherwise it's prefixed to the label text.
            // Name is optional, but always shown when there's no icon to
            // identify the effect.
            let mut label = String::new();
            if effect.stacks > 0 && !has_icon {
                label.push_str(&format!("{}x ", effect.stacks));
            }
            if self.config.show_effect_names || !has_icon {
                let text = if !effect.display_text.is_empty() { &effect.display_text } else { &effect.name };
                label.push_str(text);
            }
            if let Some(entity) = effect.entity_label() {
                label.push_str(&format!(" ({entity})"));
            }

            // Inactive uptime entries render as an empty gray bar
            let bar_color = if effect.inactive {
                color_from_rgba([90, 90, 90, 200])
            } else {
                color_from_rgba(effect.color)
            };
            let progress = if effect.inactive { 0.0 } else { effect.progress() };

            let mut bar = ProgressBar::new(&label, progress)
                .with_fill_color(bar_color)
                .with_bg_color(colors::dps_bar_bg())
                .with_text_color(font_color)
                .with_bold_text()
                .with_gradient(self.config.bar_gradient)
                .with_text_glow();

            if self.config.show_countdown && effect.show_countdown && effect.total_secs > 0.0 {
                let mut right = effect.format_time(self.european_number_format);
                if let Some(budget) = effect.format_budget() {
                    right.push_str(&format!(" ({budget})"));
                }
                bar = bar.with_right_text(right);
            }
            // Inactive entries use the cached grayscale variant
            let icon_px = row.icon_px();
            let scaled_icon = has_icon
                .then(|| {
                    shared_scaled_icons()
                        .get_variant(effect.icon_ability_id, icon_px, effect.inactive)
                        .or_else(|| {
                            effect.icon.as_ref().map(|icon_arc| {
                                let (src_w, src_h, ref src_data) = **icon_arc;
                                shared_scaled_icons().get_or_scale_variant(
                                    effect.icon_ability_id,
                                    icon_px,
                                    src_data,
                                    src_w,
                                    src_h,
                                    effect.inactive,
                                )
                            })
                        })
                })
                .flatten();
            let icon = scaled_icon.map_or(RowIcon::None, RowIcon::Scaled);
            let icon_drawn = row.render(&mut self.frame, y, icon, bar_color, &bar, font_size);

            if let Some(c) = border {
                row.outline(&mut self.frame, y, 0.8 * scale, c);
            }
            if icon_drawn && effect.stacks >= 1 {
                row.draw_icon_badge(&mut self.frame, y, &effect.stacks.to_string());
            }

            y += bar_height + entry_spacing;
        }

        self.frame.end_frame();
    }

    /// Draw icon or colored square fallback; returns whether an icon was drawn
    fn draw_icon(
        &mut self,
        effect: &EffectABEntry,
        x: f32,
        y: f32,
        icon_size: f32,
        icon_size_u32: u32,
    ) -> bool {
        let has_icon = if effect.show_icon {
            // On cache miss, scale (and desaturate if inactive) through the cache
            // rather than drawing the raw color RGBA — keeps inactive entries gray.
            let scaled_icon = shared_scaled_icons()
                .get_variant(effect.icon_ability_id, icon_size_u32, effect.inactive)
                .or_else(|| {
                    effect.icon.as_ref().map(|icon_arc| {
                        let (src_w, src_h, ref src_data) = **icon_arc;
                        shared_scaled_icons().get_or_scale_variant(
                            effect.icon_ability_id,
                            icon_size_u32,
                            src_data,
                            src_w,
                            src_h,
                            effect.inactive,
                        )
                    })
                });
            if let Some(scaled_icon) = scaled_icon {
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
            } else {
                false
            }
        } else {
            false
        };

        if !has_icon {
            // Fallback: colored square (desaturated gray when inactive)
            let bg_color = if effect.inactive {
                color_from_rgba([90, 90, 90, 200])
            } else {
                color_from_rgba(effect.color)
            };
            self.frame
                .fill_rounded_rect(x, y, icon_size, icon_size, 3.0, bg_color);
        }
        has_icon
    }

    /// Draw stack-priority mode (big stacks centered, timer in corner)
    fn draw_stack_priority(
        &mut self,
        effect: &EffectABEntry,
        x: f32,
        y: f32,
        icon_size: f32,
        font_size: f32,
    ) {
        let stack_text = format!("{}", effect.stacks);
        let stack_font_size = font_size * 1.9;
        let text_width = self.frame.measure_text(&stack_text, stack_font_size).0;
        let text_x = x + (icon_size - text_width) / 2.0;
        let text_y = y + icon_size / 2.0 + stack_font_size / 3.0;

        self.frame.draw_text_glowed(
            &stack_text,
            text_x,
            text_y,
            stack_font_size,
            colors::white(),
        );

        // Timer small in top-right corner
        if self.config.show_countdown && effect.show_countdown && effect.total_secs > 0.0 {
            let time_text = effect.format_time(self.european_number_format);
            let time_font_size = font_size * 0.9;
            let time_x =
                x + icon_size - self.frame.measure_text(&time_text, time_font_size).0 - 2.0;
            let time_y = y + time_font_size + 2.0;

            self.frame.draw_text_glowed(
                &time_text,
                time_x,
                time_y,
                time_font_size,
                colors::icon_countdown(),
            );
        }

        // Budget text top-left corner (top-right has timer in stack priority)
        if let Some(budget) = effect.format_budget() {
            let budget_size = font_size * 0.7;
            let bx = x + 2.0;
            let by = y + budget_size + 2.0;
            self.frame.draw_text_glowed(&budget, bx, by, budget_size, colors::white());
        }
    }

    /// Draw normal mode (timer centered, stacks in corner)
    fn draw_normal_mode(
        &mut self,
        effect: &EffectABEntry,
        x: f32,
        y: f32,
        icon_size: f32,
        font_size: f32,
    ) {
        if self.config.show_countdown && effect.show_countdown && effect.total_secs > 0.0 {
            let time_text = effect.format_time(self.european_number_format);
            let text_width = self.frame.measure_text(&time_text, font_size).0;
            let text_x = x + (icon_size - text_width) / 2.0;
            let text_y = y + icon_size / 2.0 + font_size * 0.4;

            self.frame.draw_text_glowed(
                &time_text,
                text_x,
                text_y,
                font_size,
                colors::icon_countdown(),
            );
        }

        // Budget text (elapsed/max) top-right corner
        if let Some(budget) = effect.format_budget() {
            let budget_size = font_size * 0.7;
            let bw = self.frame.measure_text(&budget, budget_size).0;
            let bx = x + icon_size - bw - 2.0;
            let by = y + budget_size + 2.0;
            self.frame.draw_text_glowed(&budget, bx, by, budget_size, colors::white());
        }

        // Stack count in bottom-right corner
        if effect.stacks >= 1 {
            let stack_text = format!("{}", effect.stacks);
            let stack_font_size = font_size * 0.9;
            let stack_x =
                x + icon_size - self.frame.measure_text(&stack_text, stack_font_size).0 - 2.0;
            let stack_y = y + icon_size - 3.0;

            self.frame.draw_text_glowed(
                &stack_text,
                stack_x,
                stack_y,
                stack_font_size,
                colors::white(),
            );
        }
    }

    /// Render preview placeholders in move mode
    fn render_preview(&mut self) {
        match self.config.layout {
            EffectsLayout::Horizontal => self.render_preview_horizontal(),
            EffectsLayout::Vertical => self.render_preview_vertical(),
            EffectsLayout::Bar => self.render_preview_bar(),
        }
    }

    /// Render horizontal preview (row of icons)
    fn render_preview_horizontal(&mut self) {
        let padding = self.frame.scaled(BASE_PADDING);
        let spacing = self.frame.scaled(BASE_SPACING);
        let font_size = self.frame.scaled(self.config.icon_size as f32 * ICON_FONT_RATIO);
        let icon_size = self.frame.scaled(self.config.icon_size as f32);
        let scale = self.frame.scale_factor();
        let header_font_size = font_size * 1.4;

        // Calculate header space if enabled
        let header_space = Header::space(self.config.show_header, header_font_size, spacing, scale);

        self.frame.begin_frame();

        let window_height = self.frame.height() as f32;
        let extra_below = below_icon_height(self.config.show_effect_names, false, font_size);
        let icon_row_y = if self.config.stack_from_bottom {
            (window_height - padding - icon_size - extra_below).max(padding + header_space)
        } else {
            padding + header_space
        };
        let header_y = if self.config.stack_from_bottom {
            (icon_row_y - header_space).max(padding)
        } else {
            padding
        };

        // Render header if enabled
        if self.config.show_header && !self.config.header_title.is_empty() {
            let content_width = self.frame.width() as f32 - 2.0 * padding;
            Header::new(&self.config.header_title)
                .with_color(colors::white())
                .render(
                    &mut self.frame,
                    padding,
                    header_y,
                    content_width,
                    header_font_size,
                    spacing,
                );
        }

        let mut x = padding;
        let y = icon_row_y;

        // Sample preview data: (time, stacks)
        let previews = [("12.3", 3u8), ("45", 1u8), ("8.5", 2u8)];

        for (time_text, stacks) in &previews {
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

            // Stack priority vs normal mode
            if self.config.stack_priority && *stacks >= 1 {
                self.draw_preview_stack_priority(x, y, icon_size, font_size, time_text, *stacks);
            } else {
                self.draw_preview_normal_mode(x, y, icon_size, font_size, time_text, *stacks);
            }

            // Effect name below icon
            if self.config.show_effect_names {
                let name_font_size = font_size * 0.85;
                let name = "Effect";
                let name_width = self.frame.measure_text(name, name_font_size).0;
                let name_x = x + (icon_size - name_width) / 2.0;
                let name_y = y + icon_size + name_font_size + 2.0;

                self.frame
                    .draw_text_glowed(name, name_x, name_y, name_font_size, colors::white());
            }

            x += icon_size + spacing;
        }

        self.frame.end_frame();
    }

    /// Render vertical preview (column with text beside icons)
    fn render_preview_vertical(&mut self) {
        let padding = self.frame.scaled(BASE_PADDING);
        let row_spacing = self.frame.scaled(BASE_SPACING);
        let font_size = self.frame.scaled(self.config.icon_size as f32 * ICON_FONT_RATIO);
        let icon_size = self.frame.scaled(self.config.icon_size as f32);
        let row_height = icon_size + row_spacing;
        let scale = self.frame.scale_factor();
        let header_font_size = font_size * 1.4;

        // Calculate header space if enabled
        let header_space =
            Header::space(self.config.show_header, header_font_size, row_spacing, scale);

        self.frame.begin_frame();

        // Sample preview data: (time, stacks)
        let previews = [("12.3", 3u8), ("45", 1u8), ("8.5", 2u8)];

        let window_height = self.frame.height() as f32;
        let n = previews.len() as f32;
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
        if self.config.show_header && !self.config.header_title.is_empty() {
            let content_width = self.frame.width() as f32 - 2.0 * padding;
            Header::new(&self.config.header_title)
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

        for (_time_text, stacks) in &previews {
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

            // Stack count in corner
            if *stacks >= 1 {
                let stack_text = format!("{}", stacks);
                let stack_font_size = font_size * 1.0;
                let stack_x =
                    x + icon_size - self.frame.measure_text(&stack_text, stack_font_size).0 - 2.0;
                let stack_y = y + stack_font_size + 2.0;

                self.frame.draw_text_glowed(
                    &stack_text,
                    stack_x,
                    stack_y,
                    stack_font_size,
                    colors::icon_countdown(),
                );
            }

            // Text to the right of icon
            let text_x = x + icon_size + padding;
            let text_y = y + icon_size / 2.0;

            if self.config.show_effect_names {
                // Effect name on top
                let name_y = text_y - font_size * 0.3;
                self.frame
                    .draw_text_glowed("Effect", text_x, name_y, font_size, colors::white());
            }

            y += row_height;
        }

        self.frame.end_frame();
    }

    /// Render bar mode preview (timer-style stacked bars)
    fn render_preview_bar(&mut self) {
        let font_scale = self.config.font_scale.clamp(0.3, 3.0);
        let scale = self.frame.scale_factor();
        let bar_height = self.frame.scaled(self.config.icon_size as f32).round() + 4.0 * scale;
        let font_size = self.frame.scaled(BASE_BAR_FONT_SIZE * font_scale);
        let entry_spacing = self.frame.scaled(BASE_SPACING);
        let padding = self.frame.scaled(BASE_PADDING);
        let bar_radius = 3.0 * scale;
        let content_width = self.frame.width() as f32 - 2.0 * padding;
        let font_color = colors::white();
        let header_font_size = font_size * 1.4;

        let header_space =
            Header::space(self.config.show_header, header_font_size, entry_spacing, scale);

        self.frame.begin_frame();

        let previews = if self.config.show_effect_names {
            [
                ("3x Effect Name", "12.3", 0.75_f32),
                ("Effect Name", "45.0", 0.40_f32),
                ("2x Effect Name (Source)", "8.5", 0.10_f32),
            ]
        } else {
            [("3x", "12.3", 0.75_f32), ("", "45.0", 0.40_f32), ("2x", "8.5", 0.10_f32)]
        };

        let window_height = self.frame.height() as f32;
        let n = previews.len() as f32;
        let total_bars_height = n * bar_height + (n - 1.0) * entry_spacing;
        let bars_start_y = if self.config.stack_from_bottom {
            (window_height - padding - total_bars_height).max(padding + header_space)
        } else {
            padding + header_space
        };
        let header_y = if self.config.stack_from_bottom {
            (bars_start_y - header_space).max(padding)
        } else {
            padding
        };

        if self.config.show_header && !self.config.header_title.is_empty() {
            Header::new(&self.config.header_title)
                .with_color(colors::white())
                .render(&mut self.frame, padding, header_y, content_width, header_font_size, entry_spacing);
        }

        let row = IconBarRow::new(padding, content_width, bar_height, bar_radius, scale);

        let mut y = bars_start_y;
        for (name, time_text, progress) in &previews {
            let accent = colors::effect_icon_bg();
            let mut bar = ProgressBar::new(*name, *progress)
                .with_fill_color(accent)
                .with_bg_color(colors::dps_bar_bg())
                .with_text_color(font_color)
                .with_bold_text()
                .with_text_glow();
            if self.config.show_countdown {
                bar = bar.with_right_text(*time_text);
            }
            row.render(&mut self.frame, y, RowIcon::None, accent, &bar, font_size);
            y += bar_height + entry_spacing;
        }

        self.frame.end_frame();
    }

    /// Draw preview stack-priority mode (big stacks centered, timer in corner)
    fn draw_preview_stack_priority(
        &mut self,
        x: f32,
        y: f32,
        icon_size: f32,
        font_size: f32,
        time_text: &str,
        stacks: u8,
    ) {
        let stack_text = format!("{}", stacks);
        let stack_font_size = font_size * 1.9;
        let text_width = self.frame.measure_text(&stack_text, stack_font_size).0;
        let text_x = x + (icon_size - text_width) / 2.0;
        let text_y = y + icon_size / 2.0 + stack_font_size / 3.0;

        self.frame.draw_text_glowed(
            &stack_text,
            text_x,
            text_y,
            stack_font_size,
            colors::white(),
        );

        // Timer small in top-right corner
        if self.config.show_countdown {
            let time_font_size = font_size * 0.9;
            let time_x = x + icon_size - self.frame.measure_text(time_text, time_font_size).0 - 2.0;
            let time_y = y + time_font_size + 2.0;

            self.frame.draw_text_glowed(
                time_text,
                time_x,
                time_y,
                time_font_size,
                colors::icon_countdown(),
            );
        }
    }

    /// Draw preview normal mode (timer centered, stacks in corner)
    fn draw_preview_normal_mode(
        &mut self,
        x: f32,
        y: f32,
        icon_size: f32,
        font_size: f32,
        time_text: &str,
        stacks: u8,
    ) {
        if self.config.show_countdown {
            let text_width = self.frame.measure_text(time_text, font_size).0;
            let text_x = x + (icon_size - text_width) / 2.0;
            let text_y = y + icon_size / 2.0 + font_size * 0.4;

            self.frame.draw_text_glowed(
                time_text,
                text_x,
                text_y,
                font_size,
                colors::icon_countdown(),
            );
        }

        // Stack count in bottom-right corner
        if stacks >= 1 {
            let stack_text = format!("{}", stacks);
            let stack_font_size = font_size * 1.4;
            let stack_x =
                x + icon_size - self.frame.measure_text(&stack_text, stack_font_size).0 - 2.0;
            let stack_y = y + icon_size - 3.0;

            self.frame.draw_text_glowed(
                &stack_text,
                stack_x,
                stack_y,
                stack_font_size,
                colors::icon_countdown(),
            );
        }
    }
}

/// Height of the text rows drawn under horizontal-mode icons. Must match the
/// offsets used when drawing the name (0.85×font) and label (0.75×font) rows.
fn below_icon_height(show_names: bool, has_labels: bool, font_size: f32) -> f32 {
    let mut h = 0.0;
    if show_names {
        h += font_size * 0.85 + 2.0;
    }
    if has_labels {
        h += font_size * 0.75 + 2.0;
    }
    h
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Trait Implementation
// ─────────────────────────────────────────────────────────────────────────────

impl Overlay for EffectsABOverlay {
    fn update_data(&mut self, data: OverlayData) -> bool {
        match data {
            OverlayData::EffectsA(effects_data)
            | OverlayData::EffectsB(effects_data)
            | OverlayData::EffectsC(effects_data) => {
                let was_empty = self.data.effects.is_empty();
                let is_empty = effects_data.effects.is_empty();
                self.set_data(effects_data);
                !(was_empty && is_empty)
            }
            _ => false,
        }
    }

    fn update_config(&mut self, config: OverlayConfigUpdate) {
        match config {
            OverlayConfigUpdate::EffectsA(cfg, alpha, european)
            | OverlayConfigUpdate::EffectsB(cfg, alpha, european)
            | OverlayConfigUpdate::EffectsC(cfg, alpha, european) => {
                self.set_config(cfg);
                self.set_background_alpha(alpha);
                self.european_number_format = european;
            }
            _ => {}
        }
    }

    fn render(&mut self) {
        EffectsABOverlay::render(self);
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
