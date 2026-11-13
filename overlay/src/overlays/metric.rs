//! DPS/HPS Meter Overlay
//!
//! Displays a ranked list of players with their damage/healing output.

use baras_core::context::OverlayAppearanceConfig;
use baras_types::{ClassColorConfig, ClassIconMode, formatting};
use tiny_skia::Color;

use super::{Overlay, OverlayConfigUpdate, OverlayData};
use crate::frame::OverlayFrame;
use crate::platform::{OverlayConfig, PlatformError};
use crate::utils::color_from_rgba;
use crate::widgets::colors;
use crate::widgets::{draw_icon_placeholder, Footer, Header, ProgressBar};

/// Entry in a DPS/HPS metric
#[derive(Debug, Clone)]
pub struct MetricEntry {
    pub name: String,
    /// Per-second rate (e.g., DPS, HPS)
    pub value: i64,
    /// Maximum value for progress bar scaling
    pub max_value: i64,
    /// Cumulative total (e.g., total damage dealt)
    pub total_value: i64,
    pub color: Color,
    /// Optional primary portion of value (for split bar rendering)
    pub split_value: Option<i64>,
    /// Optional primary portion of total (for split bar rendering)
    pub total_split_value: Option<i64>,
    /// Optional custom color for secondary portion of split bar
    pub split_color: Option<Color>,
    /// Optional class icon name (e.g., "assassin", "guardian")
    pub class_icon: Option<String>,
    /// Optional role for icon tinting
    pub role: Option<crate::class_icons::Role>,
    /// Optional discipline icon name (e.g., "lightning.png"). Shown instead of class icon when set.
    pub discipline_icon: Option<String>,
    /// Optional class name for class-color bar rendering (e.g., "Sorcerer", "Sage")
    pub class_name: Option<String>,
    /// Whether this entry belongs to the local player
    pub is_local: bool,
    /// Whether this entry is an enemy player (PvP) — sorted below friendlies
    pub is_enemy: bool,
    /// Pre-formatted rate string (overrides format_compact when set)
    pub display_value: Option<String>,
    /// Pre-formatted total string (overrides format_compact when set)
    pub display_total: Option<String>,
}

/// Payload for a metric overlay data update
#[derive(Debug, Clone, Default)]
pub struct MetricData {
    pub entries: Vec<MetricEntry>,
    /// Suppress the totals column regardless of appearance config
    /// (set during warzones, where cumulative totals are noise)
    pub hide_totals: bool,
}

impl MetricEntry {
    pub fn new(name: impl Into<String>, value: i64, max_value: i64) -> Self {
        Self {
            name: name.into(),
            value,
            max_value,
            total_value: 0,
            color: colors::dps_bar_fill(),
            split_value: None,
            total_split_value: None,
            split_color: None,
            class_icon: None,
            role: None,
            discipline_icon: None,
            class_name: None,
            is_local: false,
            is_enemy: false,
            display_value: None,
            display_total: None,
        }
    }

    /// Set the cumulative total value
    pub fn with_total(mut self, total: i64) -> Self {
        self.total_value = total;
        self
    }

    pub fn with_display(mut self, value: String, total: String) -> Self {
        self.display_value = Some(value);
        self.display_total = Some(total);
        self
    }

    /// Set primary portion values for split bar rendering
    pub fn with_split(mut self, split_rate: i64, split_total: i64) -> Self {
        self.split_value = Some(split_rate);
        self.total_split_value = Some(split_total);
        self
    }

    /// Set custom color for secondary portion of split bar
    pub fn with_split_color(mut self, color: Color) -> Self {
        self.split_color = Some(color);
        self
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Set class icon for display
    pub fn with_icon(mut self, icon: String) -> Self {
        self.class_icon = Some(icon);
        self
    }

    /// Set class icon and role for icon display
    pub fn with_class_icon(mut self, icon: String, role: crate::class_icons::Role) -> Self {
        self.class_icon = Some(icon);
        self.role = Some(role);
        self
    }

    /// Set the discipline-specific icon (e.g., "lightning.png"). Takes priority over class icon.
    pub fn with_discipline_icon(mut self, icon: String) -> Self {
        self.discipline_icon = Some(icon);
        self
    }

    /// Set the class name for class-color bar rendering.
    pub fn with_class_name(mut self, name: String) -> Self {
        self.class_name = Some(name);
        self
    }
}

/// Base dimensions for scaling calculations
const BASE_WIDTH: f32 = 280.0;
const BASE_HEIGHT: f32 = 200.0;

/// Base layout values (at BASE_WIDTH x BASE_HEIGHT)
const BASE_BAR_HEIGHT: f32 = 20.0;
const BASE_BAR_SPACING: f32 = 4.0;
const BASE_PADDING: f32 = 8.0;
const MIN_BAR_HEIGHT_ABSOLUTE: f32 = 16.0; // Absolute minimum bar height (not scaled)
const MIN_BAR_SPACING_ABSOLUTE: f32 = 2.0; // Absolute minimum spacing (not scaled)
const BASE_FONT_SIZE: f32 = 14.0;


/// A specialized DPS/HPS metric overlay
pub struct MetricOverlay {
    frame: OverlayFrame,
    entries: Vec<MetricEntry>,
    title: String,
    appearance: OverlayAppearanceConfig,
    show_empty_bars: bool,
    stack_from_bottom: bool,
    scaling_factor: f32,
    icon_mode: ClassIconMode,
    /// Global font scale for metric bar text (0.3 - 3.0)
    font_scale: f32,
    /// Global gradient darkening intensity for metric bars (0.0 - 1.0)
    gradient_intensity: f32,
    /// Global dynamic background setting for metrics
    dynamic_background: bool,
    /// Show grey background bar behind each player's fill bar
    show_background_bar: bool,
    /// Use European number formatting (swap `.` and `,`)
    european_number_format: bool,
    /// Color bars by class archetype color when true
    use_class_color: bool,
    /// Per-archetype color palette
    class_colors: ClassColorConfig,
    /// Suppress totals column (data-driven: set during warzones)
    hide_totals: bool,
    /// Compact signature of last rendered entries (detects no-op updates).
    /// (value, total_value) per entry is enough — names/order only change on
    /// join/leave/sort which also shifts the numeric signature.
    last_sig: Vec<(i64, i64)>,
}

impl MetricOverlay {
    /// Create a new metric overlay
    pub fn new(
        config: OverlayConfig,
        title: &str,
        appearance: OverlayAppearanceConfig,
        background_alpha: u8,
        show_empty_bars: bool,
        stack_from_bottom: bool,
        scaling_factor: f32,
        icon_mode: ClassIconMode,
        font_scale: f32,
        dynamic_background: bool,
        show_background_bar: bool,
        gradient_intensity: f32,
    ) -> Result<Self, PlatformError> {
        let mut frame = OverlayFrame::new(config, BASE_WIDTH, BASE_HEIGHT)?;
        frame.set_background_alpha(background_alpha);
        frame.set_label(title);

        Ok(Self {
            frame,
            entries: Vec::new(),
            title: title.to_string(),
            appearance,
            show_empty_bars,
            stack_from_bottom,
            scaling_factor: scaling_factor.clamp(0.3, 2.0),
            icon_mode,
            font_scale: font_scale.clamp(0.3, 3.0),
            gradient_intensity: gradient_intensity.clamp(0.0, 1.0),
            dynamic_background,
            show_background_bar,
            european_number_format: false,
            use_class_color: false,
            class_colors: ClassColorConfig::default(),
            hide_totals: false,
            last_sig: Vec::new(),
        })
    }

    /// Update appearance config
    pub fn set_appearance(&mut self, appearance: OverlayAppearanceConfig) {
        self.appearance = appearance;
    }

    /// Update background alpha
    pub fn set_background_alpha(&mut self, alpha: u8) {
        self.frame.set_background_alpha(alpha);
    }

    /// Update show empty bars setting
    pub fn set_show_empty_bars(&mut self, show: bool) {
        self.show_empty_bars = show;
    }

    /// Update stack from bottom setting
    pub fn set_stack_from_bottom(&mut self, stack: bool) {
        self.stack_from_bottom = stack;
    }

    /// Update scaling factor (clamped to 0.3-2.0)
    pub fn set_scaling_factor(&mut self, factor: f32) {
        self.scaling_factor = factor.clamp(0.3, 2.0);
    }

    /// Update icon display mode
    pub fn set_icon_mode(&mut self, mode: ClassIconMode) {
        self.icon_mode = mode;
    }

    /// Update font scale (clamped to 0.3-2.0)
    pub fn set_font_scale(&mut self, scale: f32) {
        self.font_scale = scale.clamp(0.3, 2.0);
    }

    /// Update gradient darkening intensity (clamped to 0.0-1.0)
    pub fn set_gradient_intensity(&mut self, intensity: f32) {
        self.gradient_intensity = intensity.clamp(0.0, 1.0);
    }

    /// Update dynamic background setting
    pub fn set_dynamic_background(&mut self, dynamic: bool) {
        self.dynamic_background = dynamic;
    }

    /// Update show background bar setting
    pub fn set_show_background_bar(&mut self, show: bool) {
        self.show_background_bar = show;
    }

    /// Update the metric entries
    pub fn set_entries(&mut self, entries: Vec<MetricEntry>) {
        self.entries = entries;
    }

    /// Set the title
    pub fn set_title(&mut self, title: &str) {
        self.title = title.to_string();
    }

    /// Get current entry count
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Render the metric
    pub fn render(&mut self) {
        let width = self.frame.width() as f32;
        let height = self.frame.height() as f32;

        // Get scaled layout values
        let padding = self.frame.scaled(BASE_PADDING);
        // Base font size for the header (NOT affected by font_scale or scaling_factor)
        let base_font_size = self.frame.scaled(BASE_FONT_SIZE);
        // Font scale from global metric settings — affects bar text and footer, not header
        let font_scale = self.font_scale.clamp(0.3, 3.0);
        let bar_font_size = self.frame.scaled(BASE_FONT_SIZE * font_scale);
        let scaled_bar_height = BASE_BAR_HEIGHT * self.scaling_factor;
        let ideal_bar_height = self.frame.scaled(scaled_bar_height);
        // Inter-bar gap is proportional to bar height so it stays consistent
        // across scales/resolutions. 0.0 = connected (no gap). Default 0.2
        // reproduces the legacy fixed 4px gap at the default 20px bar height.
        let spacing_ratio = self.appearance.bar_spacing_ratio.clamp(0.0, 0.6);
        let bar_spacing = ideal_bar_height * spacing_ratio;
        // Header/footer separators keep a stable gap regardless of bar density.
        let header_spacing = self.frame.scaled(BASE_BAR_SPACING);
        // Use absolute minimum bar height (not scaled) to handle extreme aspect ratios
        let min_bar_height = MIN_BAR_HEIGHT_ABSOLUTE;

        // Get colors from config
        let font_color = color_from_rgba(self.appearance.font_color);
        let bar_color = color_from_rgba(self.appearance.bar_color);

        // Get display options (totals suppressed during warzones)
        let show_total = self.appearance.show_total && !self.hide_totals;
        let show_per_second = self.appearance.show_per_second;

        // Limit to the max_entries highest values overall, then display in the
        // original friendly-first order: entries are sorted friendly then enemy,
        // so a flat take() would let friendlies exhaust the cap before any
        // enemy row appears
        let max_entries = self.appearance.max_entries as usize;
        let mut visible_idx: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| self.show_empty_bars || e.value != 0)
            .map(|(i, _)| i)
            .collect();
        if visible_idx.len() > max_entries {
            visible_idx.select_nth_unstable_by_key(max_entries - 1, |&i| {
                std::cmp::Reverse(self.entries[i].value)
            });
            visible_idx.truncate(max_entries);
            visible_idx.sort_unstable();
        }
        let visible_entries: Vec<_> = visible_idx.iter().map(|&i| &self.entries[i]).collect();
        let num_entries = visible_entries.len();

        // PvP faction divider: entries are sorted friendly-first, so a single
        // friendly→enemy transition marks where the separator line goes
        let divider_before = visible_entries
            .iter()
            .position(|e| e.is_enemy)
            .filter(|&i| i > 0);
        let divider_space = if divider_before.is_some() {
            header_spacing * 2.0 + 2.0
        } else {
            0.0
        };

        // Calculate space reserved for header and footer (uses base_font_size, unaffected by font_scale)
        // Footer: 2.0 (separator offset) + spacing + font_size + buffer
        let scale = self.frame.scale_factor();
        let header_space =
            Header::space(self.appearance.show_header, base_font_size, header_spacing, scale);
        let footer_space = if self.appearance.show_footer {
            2.0 + header_spacing + bar_font_size + 6.0 * scale // separator + spacing + text + buffer
        } else {
            0.0
        };

        // Calculate available space for bars (reserve footer space first)
        let available_for_bars = height - padding * 2.0 - header_space - footer_space;

        // Calculate effective bar height and spacing - compress proportionally if needed
        let (bar_height, effective_spacing) = if num_entries > 0 {
            let n = num_entries as f32;
            let ideal_total = n * ideal_bar_height + (n - 1.0) * bar_spacing + divider_space;

            if ideal_total > available_for_bars && ideal_total > 0.0 {
                // Compress both bars and spacing proportionally (divider stays fixed)
                let compression_ratio =
                    (available_for_bars - divider_space).max(0.0) / (ideal_total - divider_space);
                let compressed_bar = (ideal_bar_height * compression_ratio).max(min_bar_height);
                // Preserve a fully-connected look (ratio 0) under compression;
                // otherwise keep a readable minimum gap.
                let min_spacing = if spacing_ratio <= 0.0 {
                    0.0
                } else {
                    MIN_BAR_SPACING_ABSOLUTE
                };
                let compressed_spacing = (bar_spacing * compression_ratio).max(min_spacing);
                (compressed_bar, compressed_spacing)
            } else {
                (ideal_bar_height, bar_spacing)
            }
        } else {
            (ideal_bar_height, bar_spacing)
        };

        // Calculate total height of all bars (for content height and layout)
        let total_bars_height = if num_entries > 0 {
            num_entries as f32 * bar_height
                + (num_entries.saturating_sub(1)) as f32 * effective_spacing
                + divider_space
        } else {
            0.0
        };

        // Compute content height for dynamic background
        let content_height = if num_entries == 0
            && !self.appearance.show_header
            && !self.appearance.show_footer
            && !self.show_empty_bars
        {
            0.0
        } else if num_entries == 0 && !self.show_empty_bars {
            padding * 2.0 + header_space + footer_space
        } else {
            padding * 2.0 + header_space + total_bars_height + footer_space
        };

        // Calculate bar start position based on stack direction
        // (computed before begin_frame so we can position the dynamic background)
        let bars_start_y = if self.stack_from_bottom {
            // Stack from bottom: position bars at bottom of available space
            padding + header_space + available_for_bars - total_bars_height
        } else {
            // Stack from top: bars start after header
            padding + header_space
        };

        // Begin frame (clear, background, border)
        if self.dynamic_background {
            if self.stack_from_bottom {
                // When stacking from bottom, the content starts higher up from
                // bars_start_y by the header and padding, so align the background there
                let content_y = bars_start_y - header_space - padding;
                self.frame
                    .begin_frame_with_content_rect(content_y, content_height);
            } else {
                self.frame.begin_frame_with_content_height(content_height);
            }
        } else {
            self.frame.begin_frame();
        }

        let content_width = width - padding * 2.0;
        let bar_radius = 4.0 * self.frame.scale_factor();

        // Draw header just above the first bar (uses base_font_size, not bar_font_size)
        let mut y = if self.appearance.show_header {
            let header_y = bars_start_y - header_space;
            Header::new(&self.title).with_color(font_color).render(
                &mut self.frame,
                padding,
                header_y,
                content_width,
                base_font_size,
                header_spacing,
            );
            bars_start_y
        } else {
            bars_start_y
        };

        // Find max value for scaling (use actual rate values, not max_value field)
        let max_val = visible_entries
            .iter()
            .map(|e| e.value as f64)
            .fold(1.0, f64::max);

        // Draw entries using ProgressBar widget
        // Scale text font size proportionally if bars are compressed
        let base_text_size = bar_font_size - 2.0 * self.frame.scale_factor();
        let compression_ratio = bar_height / ideal_bar_height;
        let text_font_size = if compression_ratio < 1.0 {
            // When bars are compressed, scale text proportionally (keep minimum readable)
            let compressed = base_text_size * compression_ratio;
            compressed.max(10.0) // Absolute minimum 10px
        } else {
            base_text_size
        };

        // Calculate footer sums (friendly players only — enemy rows would
        // inflate the group total in PvP)
        let rate_sum: i64 = visible_entries
            .iter()
            .filter(|e| !e.is_enemy)
            .map(|e| e.value)
            .sum();
        let total_sum: i64 = visible_entries
            .iter()
            .filter(|e| !e.is_enemy)
            .map(|e| e.total_value)
            .sum();

        // Icon rendering setup
        // Discipline icons are opaque squares, so they get a dedicated
        // full-height column left of the bar — overlaying them would hide the
        // fill's origin and erase small values entirely. Class icons are
        // transparent silhouettes and keep overlaying the bar's left edge,
        // slightly inset for breathing room.
        let is_discipline_icon = self.icon_mode == ClassIconMode::Discipline;
        let (icon_size, icon_padding) = if is_discipline_icon {
            (bar_height, 0.0)
        } else {
            (
                bar_height - 4.0 * self.frame.scale_factor(),
                2.0 * self.frame.scale_factor(),
            )
        };
        let icon_overlap = 1.0 * self.frame.scale_factor();
        let (bar_x, bar_w) = if is_discipline_icon {
            (
                padding + icon_size - icon_overlap,
                content_width - icon_size + icon_overlap,
            )
        } else {
            (padding, content_width)
        };

        for (i, entry) in visible_entries.iter().enumerate() {
            // Faction divider between friendly and enemy groups
            if divider_before == Some(i) {
                self.frame.fill_rect(
                    padding,
                    y + header_spacing,
                    content_width,
                    2.0,
                    colors::separator_line(),
                );
                y += divider_space;
            }

            // Determine fill color: class color > custom entry color > configured bar_color
            let fill_color = if self.use_class_color {
                entry.class_name.as_deref()
                    .and_then(|n| self.class_colors.for_class_name(n))
                    .map(color_from_rgba)
                    .unwrap_or(bar_color)
            } else if entry.color != colors::dps_bar_fill() {
                entry.color
            } else {
                bar_color
            };

            // Select icon based on mode
            let icon_name = match self.icon_mode {
                ClassIconMode::None => None,
                ClassIconMode::Class => entry.class_icon.as_ref(),
                ClassIconMode::Discipline => entry.discipline_icon.as_ref().or(entry.class_icon.as_ref()),
            };
            let has_icon = icon_name.is_some();

            let progress = if max_val > 0.0 {
                (entry.value as f64 / max_val) as f32
            } else {
                0.0
            };

            let bg_color = if self.show_background_bar {
                colors::dps_bar_bg()
            } else {
                Color::from_rgba8(0, 0, 0, 0)
            };

            let mut bar = ProgressBar::new(entry.name.as_str(), progress)
                .with_fill_color(fill_color)
                .with_bg_color(bg_color)
                .with_text_color(font_color)
                .with_gradient(self.appearance.bar_gradient)
                .with_gradient_intensity(self.gradient_intensity)
                .with_text_glow();

            if entry.is_local {
                bar = bar.with_bold_text();
            }

            // Overlaid class icons push the label right; the discipline
            // column already starts the bar past the icon.
            if has_icon && !is_discipline_icon {
                bar = bar.with_label_offset(icon_size + icon_padding);
            }

            // Add split visualization if split data is available
            if let Some(split_val) = entry.split_value {
                if entry.value > 0 {
                    let split_fraction = (split_val as f32 / entry.value as f32).clamp(0.0, 1.0);
                    bar = bar.with_split(split_fraction);
                    if let Some(color) = entry.split_color {
                        bar = bar.with_split_color(color);
                    }
                }
            }

            // Add text based on show_total and show_per_second settings
            // Per-second is always rightmost when enabled, total goes center or right
            let eu = self.european_number_format;
            let rate_text = entry.display_value.clone().unwrap_or_else(|| {
                formatting::format_compact(entry.value, eu)
            });
            let total_text = entry.display_total.clone().unwrap_or_else(|| {
                formatting::format_compact(entry.total_value, eu)
            });

            if show_per_second && show_total {
                bar = bar.with_center_text(total_text).with_right_text(rate_text);
            } else if show_per_second {
                bar = bar.with_right_text(rate_text);
            } else if show_total {
                bar = bar.with_right_text(total_text);
            }
            // If neither, just show name (no values)

            bar.render(
                &mut self.frame,
                bar_x,
                y,
                bar_w,
                bar_height,
                text_font_size,
                bar_radius,
            );

            // Draw icon: discipline icons fill their reserved column, class
            // icons draw on top of the bar (role-tinted/white)
            let mut icon_drawn = false;
            if let Some(icon_name) = icon_name {
                let icon = if is_discipline_icon {
                    crate::class_icons::get_discipline_icon(icon_name)
                } else if let Some(role) = entry.role {
                    crate::class_icons::get_role_colored_class_icon(icon_name, role)
                } else {
                    crate::class_icons::get_white_class_icon(icon_name)
                };
                if let Some(icon) = icon {
                    self.frame.draw_image_with_shadow(
                        &icon.rgba,
                        icon.width,
                        icon.height,
                        padding + icon_padding,
                        y + icon_padding,
                        icon_size,
                        icon_size,
                    );
                    icon_drawn = true;
                }
            }
            // Keep the reserved column from reading as a hole on rows with
            // no resolvable discipline icon.
            if is_discipline_icon && !icon_drawn {
                draw_icon_placeholder(
                    &mut self.frame,
                    padding,
                    y,
                    icon_size,
                    icon_size,
                    bar_radius,
                    fill_color,
                );
            }

            y += bar_height + effective_spacing;
        }

        // Draw footer using Footer widget
        if self.appearance.show_footer {
            let eu = self.european_number_format;
            let footer = if show_per_second && show_total {
                // Both enabled: show total sum in center, rate sum on right
                Footer::new(formatting::format_compact(rate_sum, eu))
                    .with_secondary(formatting::format_compact(total_sum, eu))
                    .with_color(font_color)
            } else if show_per_second {
                // Rate only: show rate sum on right
                Footer::new(formatting::format_compact(rate_sum, eu)).with_color(font_color)
            } else if show_total {
                // Total only: show total sum on right
                Footer::new(formatting::format_compact(total_sum, eu)).with_color(font_color)
            } else {
                // Neither: empty footer (just separator)
                Footer::new("").with_color(font_color)
            };

            footer.render(
                &mut self.frame,
                padding,
                y,
                content_width,
                text_font_size,
            );
        }

        // End frame (resize indicator, commit)
        self.frame.end_frame();
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Trait Implementation
// ─────────────────────────────────────────────────────────────────────────────

impl Overlay for MetricOverlay {
    fn update_data(&mut self, data: OverlayData) -> bool {
        if let OverlayData::Metrics(data) = data {
            // Skip render when numeric state is unchanged (service may emit
            // DataUpdated more often than values actually tick).
            let changed = data.hide_totals != self.hide_totals
                || data.entries.len() != self.last_sig.len()
                || data
                    .entries
                    .iter()
                    .zip(self.last_sig.iter())
                    .any(|(e, sig)| e.value != sig.0 || e.total_value != sig.1);
            if changed {
                self.last_sig.clear();
                self.last_sig
                    .extend(data.entries.iter().map(|e| (e.value, e.total_value)));
            }
            self.hide_totals = data.hide_totals;
            self.set_entries(data.entries);
            changed
        } else {
            false
        }
    }

    fn update_config(&mut self, config: OverlayConfigUpdate) {
        if let OverlayConfigUpdate::Metric(
            appearance,
            alpha,
            show_empty,
            stack_bottom,
            scale,
            show_icons,
            font_scale,
            dynamic_bg,
            european,
            show_bg_bar,
            class_colors,
            gradient_intensity,
        ) = config
        {
            self.use_class_color = appearance.use_class_color;
            self.set_appearance(appearance);
            self.set_background_alpha(alpha);
            self.set_show_empty_bars(show_empty);
            self.set_stack_from_bottom(stack_bottom);
            self.set_scaling_factor(scale);
            self.set_icon_mode(show_icons);
            self.set_font_scale(font_scale);
            self.set_dynamic_background(dynamic_bg);
            self.european_number_format = european;
            self.set_show_background_bar(show_bg_bar);
            self.class_colors = class_colors;
            self.set_gradient_intensity(gradient_intensity);
        }
    }

    fn render(&mut self) {
        MetricOverlay::render(self);
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
