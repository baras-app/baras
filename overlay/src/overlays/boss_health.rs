//! Boss Health Bar Overlay
//!
//! Displays real-time health bars for boss NPCs in the current encounter.
//! Supports HP threshold markers (vertical lines at key HP%) and shield bars.

use std::collections::HashMap;
use std::sync::Arc;

use baras_core::context::{BossHealthConfig, IconPosition};
use baras_core::game_data::Role;
use baras_core::OverlayHealthEntry;
use tiny_skia::Color;

use super::{Overlay, OverlayConfigUpdate, OverlayData};
use crate::frame::OverlayFrame;
use crate::platform::{OverlayConfig, PlatformError};
use crate::utils::color_from_rgba;
use crate::widgets::colors;
use crate::widgets::{draw_icon_placeholder, ProgressBar};
use baras_types::formatting;

/// A single effect icon to render beneath a boss HP bar.
#[derive(Debug, Clone)]
pub struct BossEffectIcon {
    pub effect_id: u64,
    pub icon_ability_id: u64,
    pub name: String,
    pub remaining_secs: f32,
    pub total_secs: f32,
    pub color: [u8; 4],
    pub show_icon: bool,
    pub icon: Option<Arc<(u32, u32, Vec<u8>)>>,
}

impl BossEffectIcon {
    pub fn progress(&self) -> f32 {
        if self.total_secs > 0.0 {
            (self.remaining_secs / self.total_secs).clamp(0.0, 1.0)
        } else {
            1.0
        }
    }

    pub fn format_time(&self, european: bool) -> String {
        formatting::format_countdown_compact(self.remaining_secs, "0", european)
    }
}

/// Data sent from service to boss health overlay
#[derive(Debug, Clone, Default)]
pub struct BossHealthData {
    /// Current boss health entries (sorted by encounter order)
    pub entries: Vec<OverlayHealthEntry>,
    /// Effect icons keyed by NPC entity id (matches OverlayHealthEntry::entity_id).
    /// Keyed by id rather than name so two NPCs that share a display name show
    /// only the effects actually applied to each one.
    pub boss_icons: HashMap<i64, Vec<BossEffectIcon>>,
    /// Decoded game icons for active shields, keyed by `ActiveShield::icon_ability_id`.
    pub shield_icons: HashMap<u64, Arc<(u32, u32, Vec<u8>)>>,
    /// Force the bar to clear even when `clear_after_combat` is disabled. Sent at
    /// the start of a new encounter so a stale boss HP bar doesn't linger into the
    /// next fight (e.g. pulling trash after a boss).
    pub force_clear: bool,
}

/// Base dimensions for scaling calculations
const BASE_WIDTH: f32 = 250.0;
const BASE_HEIGHT: f32 = 100.0;

/// Base layout values (at BASE_WIDTH x BASE_HEIGHT)
const BASE_BAR_HEIGHT: f32 = 20.0;
const BASE_ENTRY_SPACING: f32 = 8.0;
const BASE_PADDING: f32 = 8.0;
const BASE_FONT_SIZE: f32 = 13.0;

fn shield_bar_color() -> Color {
    Color::from_rgba8(100, 180, 255, 200)
}

fn marker_line_color() -> Color {
    Color::from_rgba8(255, 255, 255, 180)
}

/// Neutral dark background for the gutter row beneath each bar.
fn gutter_bg() -> Color {
    Color::from_rgba(0.10, 0.10, 0.10, 0.88).unwrap_or(Color::BLACK)
}

/// Quantize the vertical factor into discrete font steps so text size stays
/// put while entries are added or removed; bar geometry still scales smoothly.
fn font_step(factor: f32) -> f32 {
    if factor >= 1.5 {
        1.5
    } else if factor >= 1.25 {
        1.25
    } else if factor >= 1.0 {
        1.0
    } else if factor >= 0.85 {
        0.85
    } else if factor >= 0.70 {
        0.70
    } else {
        0.55
    }
}

/// Maximum number of bosses we optimize scaling for
const MAX_SUPPORTED_BOSSES: usize = 7;
/// Icon slots reserved when the effect-icon strip sits beside the bar
/// (left/right): horizontal space is scarce there, so only this many icons
/// are reserved and drawn; extras are dropped.
const SIDE_ICON_SLOTS: usize = 4;
/// Gap between adjacent effect icons.
const ICON_SPACING: f32 = 2.0;
/// Minimum compression factor to keep entries readable
const MIN_COMPRESSION: f32 = 0.4;
/// Upper bound on the height-scale-driven vertical factor
const MAX_HEIGHT_FACTOR: f32 = 2.0;

/// Boss health bar overlay
pub struct BossHealthOverlay {
    frame: OverlayFrame,
    config: BossHealthConfig,
    data: BossHealthData,
    european_number_format: bool,
    /// (current, max, shield_remaining_per_shield) per entry — used to skip re-renders
    /// when HP and shields are unchanged and no boss effects are ticking. Tracking
    /// per-shield `remaining` (not just count) lets the shield bar animate as it absorbs.
    last_hp_sig: Vec<(i32, i32, Vec<i64>)>,
    /// Total boss-effect icon count from the last frame. Forces one final re-render
    /// on the trailing edge when icons disappear, so a stale "0.0" countdown text
    /// doesn't remain on screen.
    last_icon_count: usize,
}

impl BossHealthOverlay {
    /// Create a new boss health overlay
    pub fn new(
        window_config: OverlayConfig,
        config: BossHealthConfig,
        background_alpha: u8,
    ) -> Result<Self, PlatformError> {
        let mut frame = OverlayFrame::new(window_config, BASE_WIDTH, BASE_HEIGHT)?;
        frame.set_background_alpha(background_alpha);
        frame.set_label("Boss Health");

        Ok(Self {
            frame,
            config,
            data: BossHealthData::default(),
            european_number_format: false,
            last_hp_sig: Vec::new(),
            last_icon_count: 0,
        })
    }

    /// Update the config
    pub fn set_config(&mut self, config: BossHealthConfig) {
        self.config = config;
    }

    /// Update background alpha
    pub fn set_background_alpha(&mut self, alpha: u8) {
        self.frame.set_background_alpha(alpha);
    }

    /// Update the data
    pub fn set_data(&mut self, data: BossHealthData) {
        self.data = data;
    }

    /// Width-derived scale for all bar geometry and text. For a bar-list
    /// overlay, window width is "zoom" and height is capacity — sizing the
    /// window taller fits more bosses without inflating each bar (the frame's
    /// own scale_factor mixes height in, which blows a lone bar up in a
    /// window sized for several). Mildly clamped so extreme shapes stay
    /// reasonable.
    fn scale(&self) -> f32 {
        (self.frame.width() as f32 / BASE_WIDTH).clamp(0.5, 2.5)
    }

    /// Scale a base value by the width-derived scale factor.
    fn scaled(&self, base: f32) -> f32 {
        base * self.scale()
    }

    /// All text in this overlay renders bold; these wrappers keep the style
    /// and its width measurements in sync (bold glyphs are wider).
    fn draw_text_bold(&mut self, text: &str, x: f32, y: f32, font_size: f32, color: Color) {
        self.frame
            .draw_text_with_glow(text, x, y, font_size, color, true, false);
    }

    fn measure_text_bold(&mut self, text: &str, font_size: f32) -> (f32, f32) {
        self.frame.measure_text_styled(text, font_size, true, false)
    }

    /// Draw the gutter row contents: the bottom strip of the contiguous
    /// bar+gutter unit, whose space is always reserved so entries never resize
    /// as elements toggle. Hosts the shield (blue fill + game icon and
    /// remaining amount, centered), the next phase marker (left) and the current target (right);
    /// absent elements simply leave their slot empty. The target renders its
    /// role icon (tank/heal/dps) before the name when the role is known, and
    /// falls back to a "⌖" prefix otherwise. The unit's shared background
    /// and outline are drawn by the caller.
    #[allow(clippy::too_many_arguments)]
    fn draw_gutter_row(
        &mut self,
        marker: Option<&str>,
        shield: Option<(f32, &str, Option<&Arc<(u32, u32, Vec<u8>)>>)>,
        target: Option<(&str, Option<Role>)>,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        font: f32,
        radius: f32,
        font_color: Color,
    ) {
        let pad_x = 4.0 * self.scale();
        if let Some((frac, _, _)) = shield {
            let sh_w = w * frac.clamp(0.0, 1.0);
            if sh_w > 0.0 {
                self.frame
                    .fill_rounded_rect(x, y, sh_w, h, radius, shield_bar_color());
            }
        }
        let text_y = y + h / 2.0 + font / 3.0;
        if let Some(marker_text) = marker {
            // Only cede the center to the shield amount when one is shown;
            // otherwise the marker may run up to the target block (or nearly
            // the full width when there's no target either).
            let marker_max = if shield.is_some() {
                w * 0.4
            } else if target.is_some() {
                w * 0.5
            } else {
                w - pad_x * 2.0
            };
            let display = self.frame.truncate_to_width(marker_text, marker_max, font, true);
            self.draw_text_bold(&display, x + pad_x, text_y, font, font_color);
        }
        if let Some((target_name, role)) = target {
            let icon = role.and_then(|r| crate::class_icons::get_role_icon(r.icon_name()));
            if let Some(icon) = icon {
                let icon_size = h * 0.8;
                let icon_gap = 2.0 * self.scale();
                let display = self.frame.truncate_to_width(target_name, w * 0.4, font, true);
                let (tw, _) = self.measure_text_bold(&display, font);
                let text_x = x + w - tw - pad_x;
                self.frame.draw_image(
                    &icon.rgba,
                    icon.width,
                    icon.height,
                    text_x - icon_gap - icon_size,
                    y + (h - icon_size) / 2.0,
                    icon_size,
                    icon_size,
                );
                self.draw_text_bold(&display, text_x, text_y, font, font_color);
            } else {
                let text = format!("⌖ {}", target_name);
                let display = self.frame.truncate_to_width(&text, w * 0.4, font, true);
                let (tw, _) = self.measure_text_bold(&display, font);
                self.draw_text_bold(&display, x + w - tw - pad_x, text_y, font, font_color);
            }
        }
        if let Some((_, amount, icon)) = shield {
            // Game icon + amount, centered together as one group.
            let (tw, _) = self.measure_text_bold(amount, font);
            let icon_size = h * 0.8;
            let icon_gap = 2.0 * self.scale();
            let icon_w = icon.map_or(0.0, |_| icon_size + icon_gap);
            let group_x = x + (w - icon_w - tw) / 2.0;
            if let Some(img) = icon {
                let (iw, ih, ref rgba) = **img;
                self.frame.draw_image(
                    rgba,
                    iw,
                    ih,
                    group_x,
                    y + (h - icon_size) / 2.0,
                    icon_size,
                    icon_size,
                );
            }
            self.draw_text_bold(amount, group_x + icon_w, text_y, font, font_color);
        }
    }

    /// Draw the bar text: boss name on its own top line (left-aligned), then
    /// a readout line with the HP value left-aligned. The HP percent gets an
    /// enlarged, vertically centered column spanning the bar's full height on
    /// the right; that column's width is reserved (sized to "100.0%" so it
    /// stays stable as the number shrinks) and the name and HP value
    /// ellipsis-truncate before running into it. Parts are emptied upstream
    /// by the show_hp_value / show_percent config toggles; with the HP value
    /// off, the name drops to the percent's vertical plane and enlarges.
    #[allow(clippy::too_many_arguments)]
    fn draw_bar_text(
        &mut self,
        name: &str,
        health_text: &str,
        percent_text: &str,
        bar_x: f32,
        bar_top: f32,
        bar_w: f32,
        bar_h: f32,
        name_font_size: f32,
        hp_font_size: f32,
        font_color: Color,
    ) {
        let pad_x = 4.0 * self.scale();

        // Percent column: full bar height, right-aligned, centered vertically.
        let mut reserved_w = 0.0;
        if !percent_text.is_empty() {
            let percent_font = (hp_font_size * 1.45).min(bar_h * 0.55);
            let (tw, _) = self.measure_text_bold(percent_text, percent_font);
            let (template_w, _) = self.measure_text_bold("100.0%", percent_font);
            reserved_w = tw.max(template_w) + pad_x;
            let pct_y = bar_top + bar_h / 2.0 + percent_font / 3.0;
            self.draw_text_bold(
                percent_text,
                bar_x + bar_w - tw - pad_x * 2.0,
                pct_y,
                percent_font,
                font_color,
            );
        }
        let text_max_w = bar_w - pad_x * 2.0 - reserved_w;

        // Name line: centered in the top half of the bar. With no HP readout
        // below it the name instead takes the whole bar — enlarged to fill
        // the freed space and centered on the same plane as the percent.
        if health_text.is_empty() {
            let name_font = (name_font_size * 1.45).min(bar_h * 0.55);
            let name_y = bar_top + bar_h / 2.0 + name_font / 3.0;
            let display = self.frame.truncate_to_width(name, text_max_w, name_font, true);
            self.draw_text_bold(&display, bar_x + pad_x, name_y, name_font, font_color);
            return;
        }
        let name_y = bar_top + bar_h * 0.25 + name_font_size / 3.0;
        let display = self.frame.truncate_to_width(name, text_max_w, name_font_size, true);
        self.draw_text_bold(&display, bar_x + pad_x, name_y, name_font_size, font_color);

        // Readout line: centered in the bottom half, nudged up so the text
        // doesn't hug the bar's bottom edge.
        let hp_text_y =
            bar_top + bar_h * 0.75 + hp_font_size / 3.0 - 2.0 * self.scale();
        let display = self.frame.truncate_to_width(health_text, text_max_w, hp_font_size, true);
        self.draw_text_bold(&display, bar_x + pad_x, hp_text_y, hp_font_size, font_color);
    }

    /// True when any gutter element (target / HP markers / shield) is enabled;
    /// with all three toggled off the gutter row is not drawn at all.
    fn gutter_enabled(&self) -> bool {
        self.config.show_target || self.config.show_hp_markers || self.config.show_shield
    }

    /// Heights of the name row (top strip of the contiguous bar holding the
    /// boss name line) and the always-reserved gutter row below the bar
    /// (shield / marker / target). Both are constant per frame, so entry
    /// heights never depend on which elements are present. The gutter height
    /// is zero when every gutter element is toggled off in the config, and
    /// tracks the user's lower-bar scale at half rate so the strip grows
    /// gently as its text scales up.
    fn row_metrics(&self, bar_font_size: f32, compression: f32) -> (f32, f32) {
        let pad_y = 1.5 * self.scale() * compression;
        let name_row = bar_font_size * 0.79 + pad_y * 2.0;
        let gutter = if self.gutter_enabled() {
            let height_factor = 1.0 + (self.lower_bar_scale() - 1.0) * 0.5;
            (bar_font_size * 0.60 + pad_y * 2.0) * height_factor
        } else {
            0.0
        };
        (name_row, gutter)
    }

    /// The user's lower-bar (gutter) scale, clamped to its valid range.
    fn lower_bar_scale(&self) -> f32 {
        self.config.lower_bar_scale.clamp(0.5, 2.0)
    }

    /// Per-entry height: one contiguous unit (name row + readout row + gutter
    /// row), plus the always-reserved icon row when icons sit above or below
    /// the bar (beside the bar they cost width, not height). Every part is
    /// constant per frame, so neither config toggles nor effects landing
    /// mid-fight ever resize the layout.
    fn entry_height(&self, bar_height: f32, name_row_h: f32, gutter_h: f32) -> f32 {
        let icon_row = if self.config.show_icons && !self.icons_beside_bar() {
            self.icon_row_height(bar_height)
        } else {
            0.0
        };
        name_row_h + bar_height + gutter_h + icon_row
    }

    /// True when the effect-icon strip sits beside the bar (left/right)
    /// rather than above/below it.
    fn icons_beside_bar(&self) -> bool {
        matches!(
            self.config.icon_position,
            IconPosition::Left | IconPosition::Right
        )
    }

    /// Width reserved for the side icon strip (fixed SIDE_ICON_SLOTS slots
    /// plus a gap to the bar); zero when icons are off or above/below.
    fn side_strip_width(&self, icon_size: f32) -> f32 {
        if self.config.show_icons && self.icons_beside_bar() {
            SIDE_ICON_SLOTS as f32 * (icon_size + ICON_SPACING) + 2.0 * self.scale()
        } else {
            0.0
        }
    }

    /// Where one entry's icon strip starts and how it's sized:
    /// (first icon x, y, drawn icon size, per-icon x advance, max icons).
    /// Above/below the bar the strip is a full-width row with no cap; beside
    /// it the strip is vertically centered on the unit, icons shrink to fit
    /// the unit height, and the count is capped at the reserved slots. Side
    /// icons hug the bar and grow outward (leftward on the left — the
    /// advance is negative there — so empty slots never open a gap between
    /// the icons and the bar). `entry_top` is the entry's very top (icon row
    /// included when on top); `bar_top`/`unit_h` locate the bar+gutter unit.
    fn icon_strip_geometry(
        &self,
        bar_x: f32,
        bar_w: f32,
        entry_top: f32,
        bar_top: f32,
        unit_h: f32,
        icon_size: f32,
    ) -> (f32, f32, f32, f32, usize) {
        let advance = icon_size + ICON_SPACING;
        match self.config.icon_position {
            IconPosition::Top => (bar_x, entry_top + 3.0, icon_size, advance, usize::MAX),
            IconPosition::Bottom => {
                (bar_x, bar_top + unit_h + 3.0, icon_size, advance, usize::MAX)
            }
            IconPosition::Left | IconPosition::Right => {
                let size = icon_size.min(unit_h);
                let gap = 2.0 * self.scale();
                let (x, advance) = if self.config.icon_position == IconPosition::Left {
                    (bar_x - gap - size, -(size + ICON_SPACING))
                } else {
                    (bar_x + bar_w + gap, size + ICON_SPACING)
                };
                (x, bar_top + (unit_h - size) / 2.0, size, advance, SIDE_ICON_SLOTS)
            }
        }
    }

    /// Boss-effect icon size for a given bar height, including the user's
    /// icon scale.
    fn icon_size(&self, bar_height: f32) -> f32 {
        bar_height * 0.72 * self.config.icon_scale.clamp(0.5, 2.0)
    }

    /// Icon row height for a given bar height (3px gap above icons + icon size + 3px gap below).
    fn icon_row_height(&self, bar_height: f32) -> f32 {
        self.icon_size(bar_height) + 6.0
    }

    /// The user's visible-bosses setting, clamped to its valid range.
    fn visible_bosses(&self) -> usize {
        (self.config.visible_bosses as usize).clamp(1, MAX_SUPPORTED_BOSSES)
    }

    /// Vertical factor at which exactly `entry_count` uniform entries (plus
    /// spacing and outer padding) fill the window height, measured against
    /// the natural entry height at the current width scale.
    fn fit_factor(&self, entry_count: usize) -> f32 {
        let height = self.frame.height() as f32;
        let padding = self.scaled(BASE_PADDING);
        let bar_height = self.scaled(BASE_BAR_HEIGHT);
        let entry_spacing = self.scaled(BASE_ENTRY_SPACING);
        let layout_bar_font = self.scaled(BASE_FONT_SIZE) * 0.70;
        let (name_row_h, gutter_h) = self.row_metrics(layout_bar_font, 1.0);
        let base_entry = self.entry_height(bar_height, name_row_h, gutter_h);
        let n = entry_count.max(1) as f32;
        let usable = (height - padding * 2.0).max(1.0);
        usable / (n * (base_entry + entry_spacing) - entry_spacing)
    }

    /// Vertical factor from the visible-bosses setting: entries are sized so
    /// exactly that many fill the window, giving every fight up to that boss
    /// count the same fixed entry size.
    fn height_factor(&self) -> f32 {
        self.fit_factor(self.visible_bosses())
            .clamp(MIN_COMPRESSION, MAX_HEIGHT_FACTOR)
    }

    /// Compression only kicks in past the visible-bosses count: the factor is
    /// the fixed height_factor until `entry_count` exceeds it, then shrinks
    /// entries just enough to fit them all.
    fn compression_factor(&self, entry_count: usize) -> f32 {
        self.height_factor()
            .min(self.fit_factor(entry_count))
            .clamp(MIN_COMPRESSION, MAX_HEIGHT_FACTOR)
    }

    /// Pre-compute the total content height for all visible entries.
    fn compute_content_height(&self, entry_count: usize, compression: f32) -> f32 {
        if entry_count == 0 {
            return 0.0;
        }
        let padding = self.scaled(BASE_PADDING);
        let bar_height = self.scaled(BASE_BAR_HEIGHT) * compression;
        let entry_spacing = self.scaled(BASE_ENTRY_SPACING) * compression;
        let layout_bar_font =
            self.scaled(BASE_FONT_SIZE) * font_step(compression) * 0.70;
        let (name_row_h, gutter_h) = self.row_metrics(layout_bar_font, compression);
        let n = entry_count as f32;
        padding * 2.0
            + n * (self.entry_height(bar_height, name_row_h, gutter_h) + entry_spacing)
            - entry_spacing
    }

    /// Find the next relevant HP marker: the highest hp_percent that is <= current HP%.
    /// This is the next threshold the boss will cross as HP decreases.
    fn next_marker(entry: &OverlayHealthEntry) -> Option<(f32, &str)> {
        let current_pct = entry.percent();
        entry
            .hp_markers
            .iter()
            .filter(|m| m.hp_percent <= current_pct)
            .max_by(|a, b| {
                a.hp_percent
                    .partial_cmp(&b.hp_percent)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|m| (m.hp_percent, m.label.as_str()))
    }

    /// Render a skeleton preview when in move mode: one sample boss per
    /// visible-bosses slot, each showing every element — name, HP readout,
    /// phase marker line, a gutter with marker text, shield, and target, and
    /// a reserved icon row of placeholder slots — all following the live
    /// config toggles. Showing the full visible-bosses count at the fixed
    /// per-entry size makes the sizing model concrete: this is exactly what
    /// fills the window, and fights with fewer bosses use the same entry
    /// size rather than scaling up.
    fn render_preview(&mut self) {
        let width = self.frame.width() as f32;

        let font_scale = self.config.font_scale.clamp(0.3, 3.0);
        // Same factor the live render uses at the visible-bosses count, so
        // the sample bars show their true in-fight size.
        let factor = self.height_factor();
        let padding = self.scaled(BASE_PADDING);
        let bar_height = self.scaled(BASE_BAR_HEIGHT) * factor;
        let entry_spacing = self.scaled(BASE_ENTRY_SPACING) * factor;
        let bar_radius = 4.0 * self.scale() * factor;

        let bar_color = color_from_rgba(self.config.bar_color);
        let font_color = color_from_rgba(self.config.font_color);

        let content_width = width - padding * 2.0;
        // Rows sized from the layout font; user font_scale only grows text
        // into the fixed rows (see render()).
        let layout_bar_font = self.scaled(BASE_FONT_SIZE) * font_step(factor) * 0.70;
        let (name_row_h, gutter_h) = self.row_metrics(layout_bar_font, factor);
        let bar_font_size =
            (layout_bar_font * font_scale).min((name_row_h + bar_height) * 0.42);
        let gutter_font =
            (layout_bar_font * 0.60 * font_scale * self.lower_bar_scale()).min(gutter_h * 0.9);
        let total_bar_h = name_row_h + bar_height;
        let unit_h = total_bar_h + gutter_h;
        let icon_size = self.icon_size(bar_height);
        let strip_w = self.side_strip_width(icon_size);
        let bar_x = padding
            + if self.config.icon_position == IconPosition::Left {
                strip_w
            } else {
                0.0
            };
        let bar_w = content_width - strip_w;
        let icons_above =
            self.config.show_icons && self.config.icon_position == IconPosition::Top;
        let marker_pct = 0.50;

        let samples = [
            ("Dread Master Calphayus", 0.72_f32, "7.1M", "72.0%"),
            ("Dread Master Raptus", 0.45_f32, "4.4M", "45.0%"),
            ("Dread Master Bestia", 0.88_f32, "8.7M", "88.0%"),
            ("Dread Master Tyrans", 0.31_f32, "3.1M", "31.0%"),
            ("Dread Master Styrak", 0.64_f32, "6.3M", "64.0%"),
            ("Dread Guard", 0.52_f32, "5.1M", "52.0%"),
            ("Kephess the Undying", 0.19_f32, "1.9M", "19.0%"),
        ];

        self.frame.begin_frame();
        let mut y = padding;

        for &(name, progress, hp, pct) in samples.iter().take(self.visible_bosses()) {
            let entry_top = y;
            let bar_top = entry_top
                + if icons_above {
                    self.icon_row_height(bar_height)
                } else {
                    0.0
                };
            let health_text = if self.config.show_hp_value { hp } else { "" };
            let percent_text = if self.config.show_percent { pct } else { "" };

            // Shared background for the contiguous bar + gutter unit.
            self.frame.fill_rounded_rect(
                bar_x,
                bar_top,
                bar_w,
                unit_h,
                bar_radius,
                gutter_bg(),
            );

            // Bar background + fill span both bar rows; text is drawn per-row below.
            ProgressBar::new("", progress)
                .with_fill_color(bar_color)
                .with_bg_color(colors::dps_bar_bg())
                .with_gradient(self.config.bar_gradient)
                .render(
                    &mut self.frame,
                    bar_x,
                    bar_top,
                    bar_w,
                    total_bar_h,
                    bar_font_size,
                    bar_radius,
                );

            // Sample phase HP marker line through the bar (thinner than the border).
            if self.config.show_hp_markers {
                let marker_x = bar_x + marker_pct * bar_w;
                let line_width = 0.6 * self.scale();
                self.frame.fill_rect(
                    marker_x - line_width / 2.0,
                    bar_top,
                    line_width,
                    total_bar_h,
                    marker_line_color(),
                );
            }

            self.draw_bar_text(
                name,
                health_text,
                percent_text,
                bar_x,
                bar_top,
                bar_w,
                total_bar_h,
                bar_font_size * 0.79,
                bar_font_size,
                font_color,
            );

            if gutter_h > 0.0 {
                let marker = self.config.show_hp_markers.then_some("50% Burn");
                let shield = self.config.show_shield.then_some((0.6, "150.00K", None));
                let target = self
                    .config
                    .show_target
                    .then_some(("Tank", Some(Role::Tank)));
                self.draw_gutter_row(
                    marker,
                    shield,
                    target,
                    bar_x,
                    bar_top + total_bar_h,
                    bar_w,
                    gutter_h,
                    gutter_font,
                    bar_radius,
                    font_color,
                );
            }

            if self.config.show_border {
                let border_width = 0.8 * self.scale();
                let border_color = color_from_rgba(self.config.border_color);
                if gutter_h > 0.0 {
                    self.frame.fill_rect(
                        bar_x,
                        bar_top + total_bar_h - border_width / 2.0,
                        bar_w,
                        border_width,
                        border_color,
                    );
                }
                self.frame.stroke_rounded_rect(
                    bar_x,
                    bar_top,
                    bar_w,
                    unit_h,
                    bar_radius,
                    border_width,
                    border_color,
                );
            }

            y = bar_top + unit_h;

            // Reserved icon strip: placeholder slots where boss-effect icons
            // appear in-fight.
            if self.config.show_icons {
                let (mut icon_x, icon_y, size, advance, max_icons) = self
                    .icon_strip_geometry(bar_x, bar_w, entry_top, bar_top, unit_h, icon_size);
                // Beside the bar every reserved slot is shown so the strip's
                // true footprint is visible; above/below (uncapped) show 3.
                let slots = if max_icons == usize::MAX { 3 } else { max_icons };
                for _ in 0..slots {
                    draw_icon_placeholder(
                        &mut self.frame,
                        icon_x,
                        icon_y,
                        size,
                        size,
                        2.0,
                        bar_color,
                    );
                    icon_x += advance;
                }
                if self.config.icon_position == IconPosition::Bottom {
                    y += self.icon_row_height(bar_height);
                }
            }

            y += entry_spacing;
        }

        self.frame.end_frame();
    }

    /// Render the overlay
    pub fn render(&mut self) {
        if self.frame.is_in_move_mode() {
            self.render_preview();
            return;
        }

        let width = self.frame.width() as f32;

        // Filter out dead bosses (0% health) and pushed bosses (HP at/below pushes_at threshold)
        let entries: Vec<_> = self
            .data
            .entries
            .iter()
            .filter(|e| e.percent() > 0.0 && !e.is_pushed())
            .take(MAX_SUPPORTED_BOSSES)
            .cloned()
            .collect();

        // Nothing to render if no living bosses
        if entries.is_empty() {
            if self.config.dynamic_background {
                self.frame.begin_frame_with_content_height(0.0);
            } else {
                self.frame.begin_frame();
            }
            self.frame.end_frame();
            return;
        }

        // Calculate compression factor based on entry count
        let compression = self.compression_factor(entries.len());

        // Clamp font_scale to sensible range
        let font_scale = self.config.font_scale.clamp(0.3, 3.0);

        // Apply compression to entry-specific dimensions. Fonts use quantized
        // steps (font_step) so text stays stable while geometry compresses.
        let padding = self.scaled(BASE_PADDING);
        let bar_height = self.scaled(BASE_BAR_HEIGHT) * compression;
        let entry_spacing = self.scaled(BASE_ENTRY_SPACING) * compression;

        let bar_color = color_from_rgba(self.config.bar_color);
        let font_color = color_from_rgba(self.config.font_color);

        let content_width = width - padding * 2.0;
        let bar_radius = 4.0 * self.scale() * compression;
        let icon_size = self.icon_size(bar_height);
        let strip_w = self.side_strip_width(icon_size);
        let bar_x = padding
            + if self.config.icon_position == IconPosition::Left {
                strip_w
            } else {
                0.0
            };
        let bar_w = content_width - strip_w;
        let icons_above =
            self.config.show_icons && self.config.icon_position == IconPosition::Top;

        // Fixed per-entry skeleton: one contiguous unit of name row + readout
        // row + gutter row. Row heights
        // are sized from the layout font (user font_scale excluded), so
        // scaling text up fills the bar's whitespace instead of growing the
        // unit; fonts are clamped to their rows.
        let layout_bar_font = self.scaled(BASE_FONT_SIZE) * font_step(compression) * 0.70;
        let (name_row_h, gutter_h) = self.row_metrics(layout_bar_font, compression);
        let bar_font_size =
            (layout_bar_font * font_scale).min((name_row_h + bar_height) * 0.42);
        let gutter_font =
            (layout_bar_font * 0.60 * font_scale * self.lower_bar_scale()).min(gutter_h * 0.9);

        // Pre-compute content height, then begin frame with content-aware background
        let content_height = self.compute_content_height(entries.len(), compression);
        if self.config.dynamic_background {
            self.frame.begin_frame_with_content_height(content_height);
        } else {
            self.frame.begin_frame();
        }

        let mut y = padding;

        for entry in &entries {
            let progress = entry.percent() / 100.0;

            // Find the next relevant HP marker (used for line + gutter label)
            let marker = if self.config.show_hp_markers {
                Self::next_marker(entry)
            } else {
                None
            };

            // Target name + role (rendered in the gutter, if present and enabled)
            let target_info = if self.config.show_target {
                entry
                    .target_name
                    .as_deref()
                    .map(|t| (t, entry.target_role))
            } else {
                None
            };

            // ── Contiguous bar: name row (top) + readout row (bottom) ───
            let health_text = if self.config.show_hp_value {
                formatting::format_compact(entry.current as i64, self.european_number_format)
            } else {
                String::new()
            };
            let percent_text = if self.config.show_percent {
                formatting::format_pct(entry.percent() as f64, self.european_number_format)
            } else {
                String::new()
            };

            let entry_top = y;
            let bar_top = entry_top
                + if icons_above {
                    self.icon_row_height(bar_height)
                } else {
                    0.0
                };
            let total_bar_h = name_row_h + bar_height;
            let unit_h = total_bar_h + gutter_h;

            // Shared background for the contiguous bar + gutter unit, so the
            // two read as one shape with no seam.
            self.frame.fill_rounded_rect(
                bar_x,
                bar_top,
                bar_w,
                unit_h,
                bar_radius,
                gutter_bg(),
            );

            // Bar background + fill span both bar rows; text is drawn per-row below.
            ProgressBar::new("", progress)
                .with_fill_color(bar_color)
                .with_bg_color(colors::dps_bar_bg())
                .with_gradient(self.config.bar_gradient)
                .render(
                    &mut self.frame,
                    bar_x,
                    bar_top,
                    bar_w,
                    total_bar_h,
                    bar_font_size,
                    bar_radius,
                );

            // ── HP Marker Line (vertical line through the bar) ──────────
            // Slightly thinner than the 0.8px border stroke.
            if let Some((hp_pct, _)) = marker {
                let marker_x = bar_x + (hp_pct / 100.0) * bar_w;
                let line_width = 0.6 * self.scale();
                self.frame.fill_rect(
                    marker_x - line_width / 2.0,
                    bar_top,
                    line_width,
                    total_bar_h,
                    marker_line_color(),
                );
            }

            // ── Bar text: boss name (left) + HP readout (right) ─────────
            self.draw_bar_text(
                &entry.name,
                &health_text,
                &percent_text,
                bar_x,
                bar_top,
                bar_w,
                total_bar_h,
                bar_font_size * 0.79,
                bar_font_size,
                font_color,
            );

            y = bar_top + total_bar_h;

            // ── Gutter row (reserved while enabled): shield/marker/target ──
            if gutter_h > 0.0 {
                let marker_label =
                    marker.map(|(hp_pct, label)| format!("{}% {}", hp_pct as u32, label));
                let shield_info = self
                    .config
                    .show_shield
                    .then(|| entry.active_shields.first())
                    .flatten()
                    .map(|shield| {
                        let frac = if shield.total > 0 {
                            (shield.remaining as f32 / shield.total as f32).clamp(0.0, 1.0)
                        } else {
                            0.0
                        };
                        let icon = shield
                            .icon_ability_id
                            .and_then(|id| self.data.shield_icons.get(&id).cloned());
                        (
                            frac,
                            formatting::format_compact(
                                shield.remaining,
                                self.european_number_format,
                            ),
                            icon,
                        )
                    });
                self.draw_gutter_row(
                    marker_label.as_deref(),
                    shield_info
                        .as_ref()
                        .map(|(frac, amt, icon)| (*frac, amt.as_str(), icon.as_ref())),
                    target_info,
                    bar_x,
                    y,
                    bar_w,
                    gutter_h,
                    gutter_font,
                    bar_radius,
                    font_color,
                );
                y += gutter_h;
            }

            // Single outline around the whole unit, over the gutter contents,
            // plus an inner border on the bar/gutter seam.
            if self.config.show_border {
                let border_width = 0.8 * self.scale();
                let border_color = color_from_rgba(self.config.border_color);
                if gutter_h > 0.0 {
                    self.frame.fill_rect(
                        bar_x,
                        bar_top + total_bar_h - border_width / 2.0,
                        bar_w,
                        border_width,
                        border_color,
                    );
                }
                self.frame.stroke_rounded_rect(
                    bar_x,
                    bar_top,
                    bar_w,
                    unit_h,
                    bar_radius,
                    border_width,
                    border_color,
                );
            }

            // ── Icon strip (space always reserved while icons are enabled,
            // so a landing effect never resizes the entry) ──────────────
            if self.config.show_icons {
                let icons = self
                    .data
                    .boss_icons
                    .get(&entry.entity_id)
                    .map(Vec::as_slice)
                    .unwrap_or(&[]);
                let (mut icon_x, icon_y, icon_size, advance, max_icons) = self
                    .icon_strip_geometry(bar_x, bar_w, entry_top, bar_top, unit_h, icon_size);
                let time_font_size = icon_size * 0.38;

                for icon_entry in icons.iter().take(max_icons) {
                    let drawn = if icon_entry.show_icon {
                        if let Some(ref img) = icon_entry.icon {
                            let (iw, ih, ref rgba) = **img;
                            self.frame.draw_image(rgba, iw, ih, icon_x, icon_y, icon_size, icon_size);
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    if !drawn {
                        self.frame.fill_rounded_rect(
                            icon_x, icon_y, icon_size, icon_size, 2.0,
                            color_from_rgba(icon_entry.color),
                        );
                    }

                    // Clock wipe — dark overlay from top, shrinks as time remains
                    let overlay_h = icon_size * (1.0 - icon_entry.progress());
                    if overlay_h > 1.0 {
                        self.frame.fill_rect(
                            icon_x, icon_y, icon_size, overlay_h,
                            color_from_rgba([0, 0, 0, 140]),
                        );
                    }

                    self.frame.stroke_rounded_rect(
                        icon_x, icon_y, icon_size, icon_size, 2.0, 1.0, colors::white(),
                    );

                    let time_text = icon_entry.format_time(self.european_number_format);
                    // Direct frame calls: `self.data` is borrowed by the icon
                    // loop, so the `self` bold wrappers can't be used here.
                    let (tw, _) = self.frame.measure_text_styled(&time_text, time_font_size, true, false);
                    let time_color = if icon_entry.remaining_secs <= 3.0 {
                        colors::effect_debuff()
                    } else {
                        colors::white()
                    };
                    self.frame.draw_text_with_glow(
                        &time_text,
                        icon_x + (icon_size - tw) / 2.0,
                        icon_y + icon_size / 2.0 + time_font_size * 0.4,
                        time_font_size,
                        time_color,
                        true,
                        false,
                    );

                    icon_x += advance;
                }

                if self.config.icon_position == IconPosition::Bottom {
                    y += self.icon_row_height(bar_height);
                }
            }

            y += entry_spacing;
        }

        // End frame (resize indicator, commit)
        self.frame.end_frame();
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Trait Implementation
// ─────────────────────────────────────────────────────────────────────────────

impl Overlay for BossHealthOverlay {
    fn update_data(&mut self, data: OverlayData) -> bool {
        if let OverlayData::BossHealth(boss_data) = data {
            // When clear_after_combat is disabled, ignore empty clears so the last
            // boss health remains visible — unless force_clear is set, which marks
            // the start of a new encounter and must always wipe the stale bar.
            if boss_data.entries.is_empty()
                && !self.config.clear_after_combat
                && !boss_data.force_clear
            {
                return false;
            }

            // Active effect icons tick every frame — render every frame while present.
            // Track total count so the trailing edge (last icon expires) forces one
            // final render to erase the stale "0.0" countdown text.
            let new_icon_count: usize =
                boss_data.boss_icons.values().map(|v| v.len()).sum();
            let icons_changed = new_icon_count != self.last_icon_count;
            let has_active_effects = new_icon_count > 0;
            self.last_icon_count = new_icon_count;

            // Re-render when HP or shield state changed. Per-shield `remaining` is
            // included so absorbing damage smoothly redraws the shield bar without
            // requiring a count change.
            let new_sig: Vec<(i32, i32, Vec<i64>)> = boss_data
                .entries
                .iter()
                .map(|e| {
                    (
                        e.current,
                        e.max,
                        e.active_shields.iter().map(|s| s.remaining).collect(),
                    )
                })
                .collect();
            let hp_changed = new_sig != self.last_hp_sig;
            self.last_hp_sig = new_sig;

            self.set_data(boss_data);
            has_active_effects || icons_changed || hp_changed
        } else {
            false
        }
    }

    fn update_config(&mut self, config: OverlayConfigUpdate) {
        if let OverlayConfigUpdate::BossHealth(boss_config, alpha, european) = config {
            self.set_config(boss_config);
            self.set_background_alpha(alpha);
            self.european_number_format = european;
        }
    }

    fn render(&mut self) {
        BossHealthOverlay::render(self);
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
