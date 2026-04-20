//! Ability Queue Overlay
//!
//! Displays a three-tier ability queue:
//! - Tier 1 (pinned top): GCD countdown bar — thicker, gold border
//! - Tier 2 (middle): Queued/ready entries — light blue border, "READY" label
//! - Tier 3 (bottom): Active countdown entries — standard progress bar

use super::{AbilityQueueData, Overlay, OverlayConfigUpdate, OverlayData};
use crate::frame::OverlayFrame;
use crate::platform::{OverlayConfig, PlatformError};
use crate::utils::{color_from_rgba, shared_scaled_icons};
use crate::widgets::{colors, ProgressBar};

// ─────────────────────────────────────────────────────────────────────────────
// Configuration
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AbilityQueueConfig {
    pub max_display: u8,
    pub font_scale: f32,
    pub font_color: [u8; 4],
    /// GCD bar fill color (RGBA)
    pub gcd_color: [u8; 4],
    pub dynamic_background: bool,
}

impl Default for AbilityQueueConfig {
    fn default() -> Self {
        Self {
            max_display: 12,
            font_scale: 1.0,
            font_color: [255, 255, 255, 255],
            gcd_color: [120, 200, 255, 255],
            dynamic_background: true,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Layout Constants
// ─────────────────────────────────────────────────────────────────────────────

const BASE_WIDTH: f32 = 220.0;
const BASE_HEIGHT: f32 = 200.0;
/// Standard bar height (READY + countdown entries, matches timers A/B)
const BASE_BAR_HEIGHT: f32 = 18.0;
/// GCD bar is noticeably taller than regular entries
const BASE_GCD_BAR_HEIGHT: f32 = 26.0;
/// Vertical gap after the GCD bar before the first regular entry
const BASE_GCD_GAP: f32 = 6.0;
/// Spacing between regular (non-GCD) entries — matches timers A/B
const BASE_ENTRY_SPACING: f32 = 4.0;
const BASE_PADDING: f32 = 6.0;
const BASE_FONT_SIZE: f32 = 11.0;

/// Gold border color for the GCD bar
const GCD_BORDER: [u8; 4] = [220, 170, 50, 255];
/// Light blue border color for READY entries (not the highlighted "next" row)
const READY_BORDER: [u8; 4] = [100, 180, 255, 200];
/// Gold glow color for the ability (or tied group) that would be cast next.
/// Split across outer halo, main border, and inner tint for a layered glow.
const NEXT_HALO: [u8; 4] = [255, 210, 90, 140];
const NEXT_BORDER: [u8; 4] = [255, 210, 90, 255];
const NEXT_TINT: [u8; 4] = [255, 210, 90, 45];

// ─────────────────────────────────────────────────────────────────────────────
// Rendering data (owned, extracted before mut frame calls)
// ─────────────────────────────────────────────────────────────────────────────

enum RowKind {
    Gcd,
    Ready,
    Countdown,
}

struct RenderRow {
    kind: RowKind,
    name: String,
    progress: f32,
    right_text: String,
    bar_color: [u8; 4],
    icon_ability_id: Option<u64>,
    /// True when this row is one of the abilities that could fire on the
    /// next GCD resolution (max priority among eligible entries). Ties all
    /// get highlighted.
    highlighted: bool,
    /// True when a configured blocking timer is active. Blocked rows are
    /// dimmed and excluded from the `highlighted` eligibility set.
    is_blocked: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Implementation
// ─────────────────────────────────────────────────────────────────────────────

pub struct AbilityQueueOverlay {
    frame: OverlayFrame,
    config: AbilityQueueConfig,
    data: AbilityQueueData,
}

impl AbilityQueueOverlay {
    pub fn new(
        window_config: OverlayConfig,
        config: AbilityQueueConfig,
        background_alpha: u8,
    ) -> Result<Self, PlatformError> {
        let mut frame = OverlayFrame::new(window_config, BASE_WIDTH, BASE_HEIGHT)?;
        frame.set_background_alpha(background_alpha);
        frame.set_label("Ability Queue");
        Ok(Self { frame, config, data: AbilityQueueData::default() })
    }

    pub fn set_config(&mut self, config: AbilityQueueConfig) {
        self.config = config;
    }

    pub fn set_background_alpha(&mut self, alpha: u8) {
        self.frame.set_background_alpha(alpha);
    }

    pub fn set_data(&mut self, data: AbilityQueueData) {
        let bar_height = self.frame.scaled(BASE_BAR_HEIGHT);
        let icon_size = (bar_height - 4.0 * self.frame.scale_factor()).round() as u32;
        for entry in &data.entries {
            if let (Some(ability_id), Some(icon_arc)) = (entry.icon_ability_id, &entry.icon) {
                let (src_w, src_h, ref src_data) = **icon_arc;
                let _ = shared_scaled_icons()
                    .get_or_scale(ability_id, icon_size, src_data, src_w, src_h);
            }
        }
        self.data = data;
    }

    fn render_preview(&mut self) {
        let width = self.frame.width() as f32;
        let padding = self.frame.scaled(BASE_PADDING);
        let gcd_h = self.frame.scaled(BASE_GCD_BAR_HEIGHT);
        let bar_h = self.frame.scaled(BASE_BAR_HEIGHT);
        let entry_spacing = self.frame.scaled(BASE_ENTRY_SPACING);
        let gcd_gap = self.frame.scaled(BASE_GCD_GAP);
        let font_scale = self.config.font_scale.clamp(1.0, 2.0);
        let font_size = self.frame.scaled(BASE_FONT_SIZE * font_scale);
        let font_color = color_from_rgba(self.config.font_color);
        let gcd_fill = color_from_rgba(self.config.gcd_color);
        let bar_radius = 3.0 * self.frame.scale_factor();
        let content_width = width - padding * 2.0;
        let accent = colors::effect_icon_bg();

        self.frame.begin_frame();
        let mut y = padding;

        ProgressBar::new("", 0.45)
            .with_fill_color(gcd_fill)
            .with_bg_color(colors::dps_bar_bg())
            .with_text_color(font_color)
            .render(&mut self.frame, padding, y, content_width, gcd_h, font_size, bar_radius);
        {
            let text = "GCD  0.9";
            let (tw, _) = self.frame.measure_text_styled(text, font_size, true, false);
            let tx = padding + (content_width - tw).max(0.0) / 2.0;
            let ty = y + gcd_h / 2.0 + font_size / 3.0;
            self.frame.draw_text_with_glow(text, tx, ty, font_size, font_color, true, false);
        }
        draw_bar_border(&mut self.frame, padding, y, content_width, gcd_h, bar_radius, GCD_BORDER);
        y += gcd_h + gcd_gap;

        // Static "Next:" row — centered horizontally within the content area.
        {
            let label = "Next:";
            let name = "Top Priority";
            let gap = 6.0 * self.frame.scale_factor();
            let (label_w, _) = self.frame.measure_text_styled(label, font_size, true, false);
            let (name_w, _) = self.frame.measure_text_styled(name, font_size, true, false);
            let total_w = label_w + gap + name_w;
            let start_x = padding + (content_width - total_w).max(0.0) / 2.0;
            let text_y = y + bar_h / 2.0 + font_size / 3.0;
            self.frame.draw_text_with_glow(label, start_x, text_y, font_size, font_color, true, false);
            self.frame.draw_text_with_glow(name, start_x + label_w + gap, text_y, font_size, font_color, true, false);
        }
        y += bar_h + entry_spacing;

        // Static priority-ordered list. Top entry glows gold — it's the
        // ability that would fire on the next GCD resolution. Rows below
        // stay put regardless of cooldown state; their position reflects
        // queue_priority desc, alphabetical tiebreak.
        ProgressBar::new("Top Priority", 1.0)
            .with_fill_color(accent)
            .with_bg_color(colors::dps_bar_bg())
            .with_text_color(font_color)
            .with_right_text("READY")
            .with_bold_text()
            .with_text_glow()
            .render(&mut self.frame, padding, y, content_width, bar_h, font_size, bar_radius);
        draw_highlight_glow(&mut self.frame, padding, y, content_width, bar_h, bar_radius);
        y += bar_h + entry_spacing;

        ProgressBar::new("Mid Priority", 1.0)
            .with_fill_color(accent)
            .with_bg_color(colors::dps_bar_bg())
            .with_text_color(font_color)
            .with_right_text("READY")
            .with_bold_text()
            .with_text_glow()
            .render(&mut self.frame, padding, y, content_width, bar_h, font_size, bar_radius);
        draw_bar_border(&mut self.frame, padding, y, content_width, bar_h, bar_radius, READY_BORDER);
        y += bar_h + entry_spacing;

        ProgressBar::new("Low Priority", 0.55)
            .with_fill_color(accent)
            .with_bg_color(colors::dps_bar_bg())
            .with_text_color(font_color)
            .with_right_text("4.2")
            .with_bold_text()
            .with_text_glow()
            .render(&mut self.frame, padding, y, content_width, bar_h, font_size, bar_radius);

        let _ = y;
        self.frame.end_frame();
    }

    pub fn render(&mut self) {
        if self.frame.is_in_move_mode() {
            self.render_preview();
            return;
        }

        let width = self.frame.width() as f32;
        let padding = self.frame.scaled(BASE_PADDING);
        let gcd_h = self.frame.scaled(BASE_GCD_BAR_HEIGHT);
        let bar_h = self.frame.scaled(BASE_BAR_HEIGHT);
        let entry_spacing = self.frame.scaled(BASE_ENTRY_SPACING);
        let gcd_gap = self.frame.scaled(BASE_GCD_GAP);
        let font_scale = self.config.font_scale.clamp(1.0, 2.0);
        let font_size = self.frame.scaled(BASE_FONT_SIZE * font_scale);
        let font_color = color_from_rgba(self.config.font_color);
        let gcd_color = self.config.gcd_color;
        let bar_radius = 3.0 * self.frame.scale_factor();
        let content_width = width - padding * 2.0;
        let max = self.config.max_display as usize;
        let icon_size = bar_h - 4.0 * self.frame.scale_factor();
        let icon_padding = 2.0 * self.frame.scale_factor();
        let icon_size_u32 = icon_size.round() as u32;

        // ── Collect owned render rows ──────────────────────────────────────────
        //
        // Static-position layout: non-pinned entries sort by queue_priority
        // descending, with name ascending as a stable tiebreak. Time remaining
        // is intentionally NOT a sort factor — positions should never shuffle
        // as cooldowns tick. Time only feeds the "what fires next" highlight.
        let rows: Vec<RenderRow> = {
            let pinned: Vec<_> = self.data.entries.iter().filter(|e| e.is_pinned).collect();
            let gcd_remaining = pinned.first().map(|e| e.remaining_secs).unwrap_or(0.0);

            let mut regular: Vec<_> =
                self.data.entries.iter().filter(|e| !e.is_pinned).collect();
            regular.sort_by(|x, y| {
                y.queue_priority.cmp(&x.queue_priority)
                    .then_with(|| x.name.cmp(&y.name))
            });

            // Determine the "next cast" set — entries eligible to fire on the
            // next GCD resolution (already ready, or coming off CD inside the
            // current GCD window) that tie for the highest priority among the
            // eligible. All tied winners glow gold. Blocked entries are
            // excluded entirely.
            let max_eligible_priority = regular
                .iter()
                .filter(|e| !e.is_blocked && (e.is_queued || e.remaining_secs < gcd_remaining))
                .map(|e| e.queue_priority)
                .max();

            let mut rows = Vec::new();
            let mut rendered = 0;

            for e in &pinned {
                if rendered >= max { break; }
                rows.push(RenderRow {
                    kind: RowKind::Gcd,
                    name: "GCD".to_string(),
                    progress: 1.0 - e.progress(),
                    right_text: format!("{:.1}", e.remaining_secs.max(0.0)),
                    bar_color: gcd_color,
                    icon_ability_id: None,
                    highlighted: false,
                    is_blocked: false,
                });
                rendered += 1;
            }
            for e in &regular {
                if rendered >= max { break; }
                let (right_text, progress) = if e.is_queued {
                    // Ready rows with a blocker active read "Blocked" instead
                    // of "READY" so the user knows why it isn't glowing gold.
                    let text = if e.is_blocked { "Blocked" } else { "READY" };
                    (text.to_string(), 1.0)
                } else {
                    (
                        baras_types::formatting::format_countdown(e.remaining_secs, "", "0:00", false),
                        e.progress(),
                    )
                };
                let eligible = !e.is_blocked && (e.is_queued || e.remaining_secs < gcd_remaining);
                let highlighted =
                    eligible && max_eligible_priority == Some(e.queue_priority);
                rows.push(RenderRow {
                    kind: if e.is_queued { RowKind::Ready } else { RowKind::Countdown },
                    name: e.name.clone(),
                    progress,
                    right_text,
                    bar_color: e.color,
                    icon_ability_id: e.icon_ability_id,
                    highlighted,
                    is_blocked: e.is_blocked,
                });
                rendered += 1;
            }

            let _ = (pinned.len(), regular.len());
            rows
        };

        // ── Content height ─────────────────────────────────────────────────────
        // The GCD slot is always reserved when anything is visible — even when
        // no GCD is active a phantom outline is drawn so the ready/countdown
        // rows below don't shift position when a GCD appears or disappears.
        // The "Next:" label row is also always reserved (static slot between
        // the GCD and the ability list).
        let gcd_count = rows.iter().filter(|r| matches!(r.kind, RowKind::Gcd)).count();
        let has_gcd_row = gcd_count > 0;
        let other_count = rows.len() - gcd_count;
        let content_height = if rows.is_empty() {
            0.0
        } else {
            let gcd_height = gcd_h + gcd_gap;
            let next_height = bar_h;
            let spacer_after_next = if other_count > 0 { entry_spacing } else { 0.0 };
            let other_height = if other_count > 0 {
                other_count as f32 * bar_h + (other_count - 1) as f32 * entry_spacing
            } else {
                0.0
            };
            padding * 2.0 + gcd_height + next_height + spacer_after_next + other_height
        };

        if self.config.dynamic_background {
            self.frame.begin_frame_with_content_height(content_height);
        } else {
            self.frame.begin_frame();
        }

        if rows.is_empty() {
            self.frame.end_frame();
            return;
        }

        let mut y = padding;

        // Phantom GCD outline — keeps the GCD slot reserved so downstream rows
        // don't shift vertically when the GCD comes and goes between frames.
        if !has_gcd_row {
            draw_bar_border(&mut self.frame, padding, y, content_width, gcd_h, bar_radius, GCD_BORDER);
            y += gcd_h + gcd_gap;
        }

        let mut next_section_drawn = false;

        for (i, row) in rows.iter().enumerate() {
            let is_gcd = matches!(row.kind, RowKind::Gcd);
            let is_ready = matches!(row.kind, RowKind::Ready);
            let row_h = if is_gcd { gcd_h } else { bar_h };
            let has_icon = !is_gcd && row.icon_ability_id.is_some();

            // Static "Next:" slot — drawn once, directly after the GCD area and
            // before the first regular row. Shows icon + name of whichever
            // ability (or tied abilities) would fire on the next GCD resolution.
            if !is_gcd && !next_section_drawn {
                self.draw_next_section(
                    &rows, padding, y, bar_h, content_width, icon_size, icon_size_u32,
                    icon_padding, font_size, font_color,
                );
                y += bar_h + entry_spacing;
                next_section_drawn = true;
            }

            // Blocked rows render with reduced opacity — bar fill, icon, and
            // text all get the same dim multiplier so the row reads as
            // "unavailable" at a glance.
            let dim = row.is_blocked;
            let fill_color = if dim {
                color_from_rgba(apply_dim_alpha(row.bar_color, 0.35))
            } else {
                color_from_rgba(row.bar_color)
            };
            let row_font_color = if dim { dim_color(font_color, 0.5) } else { font_color };

            if is_gcd {
                // Render the bar with no built-in label/right text — the
                // ProgressBar widget doesn't truly center text in 2-column
                // mode, so we draw "GCD  1.2" manually centered below.
                ProgressBar::new("", row.progress)
                    .with_fill_color(fill_color)
                    .with_bg_color(colors::dps_bar_bg())
                    .with_text_color(row_font_color)
                    .render(&mut self.frame, padding, y, content_width, row_h, font_size, bar_radius);

                let text = format!("{}  {}", row.name, row.right_text);
                let (tw, _) = self.frame.measure_text_styled(&text, font_size, true, false);
                let tx = padding + (content_width - tw).max(0.0) / 2.0;
                let text_y = y + row_h / 2.0 + font_size / 3.0;
                self.frame.draw_text_with_glow(&text, tx, text_y, font_size, font_color, true, false);
            } else {
                let mut bar = ProgressBar::new(&row.name, row.progress)
                    .with_fill_color(fill_color)
                    .with_bg_color(colors::dps_bar_bg())
                    .with_text_color(row_font_color)
                    .with_right_text(&row.right_text)
                    .with_bold_text()
                    .with_text_glow();

                if has_icon {
                    bar = bar.with_label_offset(icon_size + icon_padding);
                }

                bar.render(&mut self.frame, padding, y, content_width, row_h, font_size, bar_radius);
            }

            // Highlight layer — gold halo + border + subtle tint for "next cast"
            // candidates. Drawn before the standard READY border so the gold
            // stroke sits on top unchallenged.
            if row.highlighted {
                draw_highlight_glow(&mut self.frame, padding, y, content_width, row_h, bar_radius);
            } else if is_gcd {
                draw_bar_border(&mut self.frame, padding, y, content_width, row_h, bar_radius, GCD_BORDER);
            } else if is_ready {
                draw_bar_border(&mut self.frame, padding, y, content_width, row_h, bar_radius, READY_BORDER);
            }

            // Draw icon
            if let Some(ability_id) = row.icon_ability_id {
                let icon_x = padding + icon_padding;
                let icon_y = y + icon_padding;

                let icon_drawn = if let Some(scaled) =
                    shared_scaled_icons().get(ability_id, icon_size_u32)
                {
                    self.frame.draw_image(&scaled, icon_size_u32, icon_size_u32, icon_x, icon_y, icon_size, icon_size);
                    true
                } else if let Some(entry) = self.data.entries.iter().find(|e| e.icon_ability_id == Some(ability_id)) {
                    if let Some(ref icon_arc) = entry.icon {
                        let (img_w, img_h, ref rgba) = **icon_arc;
                        self.frame.draw_image(rgba, img_w, img_h, icon_x, icon_y, icon_size, icon_size);
                        true
                    } else { false }
                } else { false };

                if icon_drawn {
                    let icon_radius = 2.0 * self.frame.scale_factor();
                    let glow_expand = 1.0 * self.frame.scale_factor();
                    let outer_glow = tiny_skia::Color::from_rgba(1.0, 1.0, 1.0, 0.25).unwrap();
                    self.frame.stroke_rounded_rect(
                        icon_x - glow_expand, icon_y - glow_expand,
                        icon_size + glow_expand * 2.0, icon_size + glow_expand * 2.0,
                        icon_radius + glow_expand, 1.5 * self.frame.scale_factor(), outer_glow,
                    );
                    let inner_border = tiny_skia::Color::from_rgba(1.0, 1.0, 1.0, 0.6).unwrap();
                    self.frame.stroke_rounded_rect(
                        icon_x, icon_y, icon_size, icon_size,
                        icon_radius, 1.0 * self.frame.scale_factor(), inner_border,
                    );
                }
            }

            // Advance y: GCD uses gcd_gap, others use entry_spacing (skip trailing gap on last row)
            if i + 1 < rows.len() {
                y += row_h + if is_gcd { gcd_gap } else { entry_spacing };
            }
        }

        // GCD-only case — no regular row triggered the inline Next section.
        // Draw it in its reserved slot below the GCD.
        if !next_section_drawn {
            let next_y = padding + gcd_h + gcd_gap;
            self.draw_next_section(
                &rows, padding, next_y, bar_h, content_width, icon_size, icon_size_u32,
                icon_padding, font_size, font_color,
            );
        }

        self.frame.end_frame();
    }

    /// Draw the static "Next:" row showing which ability (or tied set) would
    /// fire on the next GCD resolution. Icons appear inline where available.
    /// The whole "Next: [icon] Name" block is centered horizontally within
    /// the content area.
    fn draw_next_section(
        &mut self,
        rows: &[RenderRow],
        padding: f32,
        y: f32,
        row_h: f32,
        content_width: f32,
        icon_size: f32,
        icon_size_u32: u32,
        icon_padding: f32,
        font_size: f32,
        font_color: tiny_skia::Color,
    ) {
        let label = "Next:";
        let gap = 6.0 * self.frame.scale_factor();
        let (label_w, _) = self.frame.measure_text_styled(label, font_size, true, false);
        // Match the baseline formula used by ProgressBar so Next: aligns
        // visually with the ability-name text in the bars below.
        let text_y = y + row_h / 2.0 + font_size / 3.0;
        let ic_y = y + (row_h - icon_size) / 2.0;

        let highlighted: Vec<&RenderRow> = rows.iter().filter(|r| r.highlighted).collect();

        // ── Measure pass: compute the total width so we can center the block.
        let mut total_w = label_w + gap;
        let sep = " / ";
        let (sep_w, _) = self.frame.measure_text_styled(sep, font_size, false, false);

        if highlighted.is_empty() {
            let (dash_w, _) = self.frame.measure_text_styled("—", font_size, false, false);
            total_w += dash_w;
        } else {
            for (idx, row) in highlighted.iter().enumerate() {
                if idx > 0 {
                    total_w += sep_w;
                }
                let has_icon = row
                    .icon_ability_id
                    .is_some_and(|id| shared_scaled_icons().get(id, icon_size_u32).is_some());
                if has_icon {
                    total_w += icon_size + icon_padding;
                }
                let (name_w, _) = self.frame.measure_text_styled(&row.name, font_size, true, false);
                total_w += name_w;
            }
        }

        // ── Draw pass: start at the centered offset.
        let start_x = padding + (content_width - total_w).max(0.0) / 2.0;
        self.frame.draw_text_with_glow(label, start_x, text_y, font_size, font_color, true, false);
        let mut cursor_x = start_x + label_w + gap;

        if highlighted.is_empty() {
            self.frame.draw_text_with_glow("—", cursor_x, text_y, font_size, font_color, false, false);
            return;
        }

        for (idx, row) in highlighted.iter().enumerate() {
            if idx > 0 {
                self.frame.draw_text_with_glow(sep, cursor_x, text_y, font_size, font_color, false, false);
                cursor_x += sep_w;
            }

            if let Some(ability_id) = row.icon_ability_id {
                if let Some(scaled) = shared_scaled_icons().get(ability_id, icon_size_u32) {
                    self.frame.draw_image(
                        &scaled, icon_size_u32, icon_size_u32,
                        cursor_x, ic_y, icon_size, icon_size,
                    );
                    cursor_x += icon_size + icon_padding;
                }
            }

            self.frame.draw_text_with_glow(&row.name, cursor_x, text_y, font_size, font_color, true, false);
            let (name_w, _) = self.frame.measure_text_styled(&row.name, font_size, true, false);
            cursor_x += name_w;
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Scale an RGBA byte color's alpha channel. Used for dimming blocked rows.
fn apply_dim_alpha(color: [u8; 4], alpha_mul: f32) -> [u8; 4] {
    let a = (color[3] as f32 * alpha_mul).clamp(0.0, 255.0) as u8;
    [color[0], color[1], color[2], a]
}

/// Scale the alpha channel of a `tiny_skia::Color` for dimmed text rendering.
fn dim_color(color: tiny_skia::Color, alpha_mul: f32) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba(
        color.red(),
        color.green(),
        color.blue(),
        (color.alpha() * alpha_mul).clamp(0.0, 1.0),
    )
    .unwrap_or(color)
}

fn draw_bar_border(
    frame: &mut OverlayFrame,
    x: f32, y: f32, w: f32, h: f32,
    radius: f32,
    color: [u8; 4],
) {
    let c = tiny_skia::Color::from_rgba(
        color[0] as f32 / 255.0,
        color[1] as f32 / 255.0,
        color[2] as f32 / 255.0,
        color[3] as f32 / 255.0,
    ).unwrap_or(tiny_skia::Color::WHITE);
    frame.stroke_rounded_rect(x, y, w, h, radius, 1.5 * frame.scale_factor(), c);
}

/// Layered gold glow marking the "next cast" row(s): outer halo, thick gold
/// border, and a subtle inner tint. Drawn on top of the progress bar.
fn draw_highlight_glow(
    frame: &mut OverlayFrame,
    x: f32, y: f32, w: f32, h: f32,
    radius: f32,
) {
    let scale = frame.scale_factor();

    // Inner tint for glow depth
    let tint = color_from_rgba(NEXT_TINT);
    frame.fill_rounded_rect(x, y, w, h, radius, tint);

    // Outer halo: expanded rect, thicker low-alpha stroke
    let expand = 2.0 * scale;
    let halo = color_from_rgba(NEXT_HALO);
    frame.stroke_rounded_rect(
        x - expand, y - expand,
        w + expand * 2.0, h + expand * 2.0,
        radius + expand, 2.5 * scale, halo,
    );

    // Main gold border — thicker than the standard 1.5px stroke
    let border = color_from_rgba(NEXT_BORDER);
    frame.stroke_rounded_rect(x, y, w, h, radius, 2.0 * scale, border);
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Trait Implementation
// ─────────────────────────────────────────────────────────────────────────────

impl Overlay for AbilityQueueOverlay {
    fn update_data(&mut self, data: OverlayData) -> bool {
        if let OverlayData::AbilityQueue(aq_data) = data {
            let was_empty = self.data.entries.is_empty();
            let is_empty = aq_data.entries.is_empty();
            self.set_data(aq_data);
            !(was_empty && is_empty)
        } else {
            false
        }
    }

    fn update_config(&mut self, config: OverlayConfigUpdate) {
        if let OverlayConfigUpdate::AbilityQueue(aq_config, alpha) = config {
            self.set_config(aq_config);
            self.set_background_alpha(alpha);
        }
    }

    fn render(&mut self) {
        AbilityQueueOverlay::render(self);
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
