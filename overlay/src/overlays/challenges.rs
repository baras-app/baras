//! Challenge tracking overlay
//!
//! Displays challenge metrics during boss encounters. Each challenge is rendered
//! as its own "card" showing the challenge title, duration, and per-player bars
//! with contribution percentages.

use std::collections::HashMap;

use baras_core::context::{ChallengeColumns, ChallengeLayout, ChallengeOverlayConfig};
use tiny_skia::Color;

use super::{Overlay, OverlayConfigUpdate, OverlayData};
use crate::class_icons::{
    get_discipline_icon, get_role_colored_class_icon, get_white_class_icon, Role,
};
use crate::frame::OverlayFrame;
use crate::platform::{OverlayConfig, PlatformError};
use crate::utils::{color_from_rgba, format_duration_short};
use crate::widgets::{colors, Footer, Header, ProgressBar};
use baras_types::{formatting, ClassIconMode};

/// Data for the challenges overlay
#[derive(Debug, Clone, Default)]
pub struct ChallengeData {
    /// Challenge entries to display
    pub entries: Vec<ChallengeEntry>,
    /// Boss encounter name (for header)
    pub boss_name: Option<String>,
    /// Total encounter duration in seconds
    pub duration_secs: f32,
    /// Phase durations (phase_id → seconds)
    pub phase_durations: HashMap<String, f32>,
}

/// Single challenge entry for display
#[derive(Debug, Clone)]
pub struct ChallengeEntry {
    /// Challenge display name
    pub name: String,
    /// Current total value
    pub value: i64,
    /// Number of events contributing
    pub event_count: u32,
    /// Value per second (if time-based)
    pub per_second: Option<f32>,
    /// Per-player breakdown (sorted by value descending)
    pub by_player: Vec<PlayerContribution>,
    /// Challenge duration in seconds (may differ from encounter duration for phase-specific)
    pub duration_secs: f32,
    /// Whether this challenge is enabled for display
    pub enabled: bool,
    /// Bar color for this challenge (optional, uses default if None)
    pub color: Option<Color>,
    /// Which columns to display for this challenge
    pub columns: ChallengeColumns,
}

impl Default for ChallengeEntry {
    fn default() -> Self {
        Self {
            name: String::new(),
            value: 0,
            event_count: 0,
            per_second: None,
            by_player: Vec::new(),
            duration_secs: 0.0,
            enabled: true,
            color: None,
            columns: ChallengeColumns::default(),
        }
    }
}

/// A player's contribution to a challenge
#[derive(Debug, Clone)]
pub struct PlayerContribution {
    /// Player entity ID (for linking to encounter data)
    pub entity_id: i64,
    /// Player name (resolved from encounter)
    pub name: String,
    /// Player's value contribution
    pub value: i64,
    /// Percentage of total (0.0-100.0)
    pub percent: f32,
    /// Value per second (if applicable)
    pub per_second: Option<f32>,
    /// True for the local player — bolded in display, like metric overlays
    pub is_local: bool,
    /// Optional class icon name (e.g. "assassin"); rendered when icon_mode is Class
    pub class_icon: Option<String>,
    /// Optional discipline icon name (e.g. "lightning"); rendered when icon_mode is Discipline
    pub discipline_icon: Option<String>,
    /// Optional role for class-icon tinting (Tank/Healer/Damage)
    pub role: Option<Role>,
}

// ═══════════════════════════════════════════════════════════════════════════════
// Layout Constants
// ═══════════════════════════════════════════════════════════════════════════════

const BASE_WIDTH: f32 = 320.0;
const BASE_HEIGHT: f32 = 400.0;

const BASE_PADDING: f32 = 6.0;
const BASE_CARD_SPACING: f32 = 8.0;
const BASE_BAR_HEIGHT: f32 = 18.0;
const BASE_FONT_SIZE: f32 = 13.0;
const BASE_HEADER_FONT_SIZE: f32 = 12.0;
const BASE_DURATION_FONT_SIZE: f32 = 10.0; // Smaller than header

const MAX_PLAYERS: usize = 8;

// ═══════════════════════════════════════════════════════════════════════════════
// Challenge Overlay
// ═══════════════════════════════════════════════════════════════════════════════

/// Overlay displaying multiple challenge metrics as stacked cards
pub struct ChallengeOverlay {
    frame: OverlayFrame,
    data: ChallengeData,
    background_alpha: u8,
    config: ChallengeOverlayConfig,
    european_number_format: bool,
    /// Mirrors the global `class_icon_mode` used by metric overlays.
    icon_mode: ClassIconMode,
}

impl ChallengeOverlay {
    pub fn new(
        overlay_config: OverlayConfig,
        config: ChallengeOverlayConfig,
        background_alpha: u8,
    ) -> Result<Self, PlatformError> {
        let mut frame = OverlayFrame::new(overlay_config, BASE_WIDTH, BASE_HEIGHT)?;
        frame.set_background_alpha(background_alpha);
        frame.set_label("Challenges");

        Ok(Self {
            frame,
            data: ChallengeData::default(),
            background_alpha,
            config,
            european_number_format: false,
            icon_mode: ClassIconMode::default(),
        })
    }

    pub fn set_data(&mut self, data: ChallengeData) {
        self.data = data;
    }

    pub fn set_config(&mut self, config: ChallengeOverlayConfig) {
        self.config = config;
    }

    pub fn set_icon_mode(&mut self, mode: ClassIconMode) {
        self.icon_mode = mode;
    }

    pub fn set_background_alpha(&mut self, alpha: u8) {
        self.background_alpha = alpha;
        self.frame.set_background_alpha(alpha);
    }

    /// Number of card slots the window is divided into: the reserved count
    /// when configured (0 = auto), growing only when more challenges than
    /// that are visible (which compresses cards, like the boss HP overlay).
    fn slot_count(&self, num_visible: usize) -> usize {
        let reserved = (self.config.visible_challenges as usize).min(8);
        if reserved == 0 {
            num_visible.max(1)
        } else {
            reserved.max(num_visible)
        }
    }

    pub fn render_overlay(&mut self) {
        let width = self.frame.width() as f32;
        let height = self.frame.height() as f32;
        let scale = self.frame.scale_factor();

        let padding = self.frame.scaled(BASE_PADDING);
        let card_spacing = self.frame.scaled(BASE_CARD_SPACING);
        let mut bar_height = self.frame.scaled(BASE_BAR_HEIGHT);
        // Gap between player bars is proportional to bar height so it stays
        // consistent across scales/resolutions. 0.0 = connected. The 0.2 default
        // closely matches the legacy fixed 3px gap at the 18px base bar height.
        let spacing_ratio = self.config.bar_spacing_ratio.clamp(0.0, 0.6);
        let mut bar_spacing = bar_height * spacing_ratio;
        let font_scale = self.config.font_scale.clamp(0.3, 3.0);
        let mut font_size = self.frame.scaled(BASE_FONT_SIZE * font_scale);
        let mut header_font_size = self.frame.scaled(BASE_HEADER_FONT_SIZE * font_scale);
        let mut duration_font_size = self.frame.scaled(BASE_DURATION_FONT_SIZE * font_scale);
        let mut bar_radius = 3.0 * scale;

        let font_color = color_from_rgba(self.config.font_color);
        let default_bar_color = color_from_rgba(self.config.default_bar_color);

        let show_duration = self.config.show_duration;
        let show_footer = self.config.show_footer;
        let max_display = self.config.max_display as usize;
        let layout = self.config.layout;

        // Filter to enabled challenges only - clone to avoid borrow issues
        // (must happen before begin_frame so we can compute content height for
        // dynamic background). In move mode, render sample cards instead — one
        // per reserved slot — so the user sees the true card size and where
        // reserved slots sit while positioning the window.
        let enabled_challenges: Vec<ChallengeEntry> = if self.frame.is_in_move_mode() {
            let reserved = (self.config.visible_challenges as usize).min(8);
            let n = if reserved > 0 { reserved } else { 2.min(max_display.max(1)) };
            sample_challenges(n)
        } else {
            self.data
                .entries
                .iter()
                .filter(|c| c.enabled)
                .take(max_display)
                .cloned()
                .collect()
        };

        let num_visible = enabled_challenges.len();
        let slots = self.slot_count(num_visible);
        if num_visible > 0 {
            // Scale content up to fill available space when fewer challenges are shown.
            // Estimate per-card height from the tallest card at unscaled sizes
            let max_players_in_cards = enabled_challenges
                .iter()
                .map(|c| c.by_player.len().min(MAX_PLAYERS))
                .max()
                .unwrap_or(MAX_PLAYERS);
            let card_height_est = card_height(
                max_players_in_cards,
                header_font_size,
                bar_height,
                bar_spacing,
                show_footer.then(|| footer_font_size(font_size)),
                scale,
            );

            // Size against the slot count, not the visible count: with
            // reserved slots a lone card keeps its 1/N share instead of
            // scaling up to fill the window.
            let content_height_est = match layout {
                ChallengeLayout::Vertical => {
                    slots as f32 * card_height_est
                        + (slots.saturating_sub(1)) as f32 * card_spacing
                        + padding * 2.0
                }
                ChallengeLayout::Horizontal => card_height_est + padding * 2.0,
            };

            // With reserved slots, allow more up-scaling than auto mode so a
            // card genuinely fills its 1/N share of the window instead of
            // stopping short and leaving whitespace.
            let max_upscale = if self.config.visible_challenges > 0 {
                2.5
            } else {
                1.8
            };
            let content_scale = (height / content_height_est).clamp(0.8, max_upscale);

            // Fonts are additionally capped by card width: horizontal slots
            // are narrow, and height-driven scaling alone makes the bar text
            // (name / value / percent) collide and truncate. The 1.5 slack
            // keeps full-width vertical cards at their familiar sizing.
            let card_width = match layout {
                ChallengeLayout::Vertical => width - padding * 2.0,
                ChallengeLayout::Horizontal => {
                    (width - padding * 2.0 - card_spacing * slots.saturating_sub(1) as f32)
                        / slots as f32
                }
            };
            let natural_width = (self.frame.scaled(BASE_WIDTH) - padding * 2.0).max(1.0);
            let font_content_scale = content_scale
                .min((card_width / natural_width * 1.5).max(0.5));

            bar_height *= content_scale;
            bar_spacing *= content_scale;
            bar_radius *= content_scale;
            font_size *= font_content_scale;
            header_font_size *= font_content_scale;
            duration_font_size *= font_content_scale;
        }

        // Compute actual content height from final (scaled) dimensions for dynamic background
        let footer_font = show_footer.then(|| footer_font_size(font_size));
        let card_h = |c: &ChallengeEntry| {
            let players = c.by_player.len().min(MAX_PLAYERS);
            card_height(players, header_font_size, bar_height, bar_spacing, footer_font, scale)
        };
        let content_height = if num_visible == 0 {
            0.0
        } else {
            match layout {
                ChallengeLayout::Vertical => {
                    let cards: f32 = enabled_challenges.iter().map(card_h).sum();
                    padding * 2.0 + cards + (num_visible - 1) as f32 * card_spacing
                }
                // All cards are same height (tallest card), side by side
                ChallengeLayout::Horizontal => {
                    let tallest = enabled_challenges.iter().map(card_h).fold(0.0, f32::max);
                    padding * 2.0 + tallest
                }
            }
        };

        // Bottom-anchor the card group in vertical layout when stacking from
        // the end; clamp to the top when content overflows the window.
        let content_y = if layout == ChallengeLayout::Vertical
            && self.config.stack_from_end
            && num_visible > 0
        {
            (height - content_height).max(0.0)
        } else {
            0.0
        };

        if self.config.dynamic_background {
            self.frame
                .begin_frame_with_content_rect(content_y, content_height);
        } else {
            self.frame.begin_frame();
        }

        match layout {
            ChallengeLayout::Vertical => {
                self.render_vertical(
                    &enabled_challenges,
                    padding,
                    card_spacing,
                    bar_height,
                    bar_spacing,
                    font_size,
                    header_font_size,
                    duration_font_size,
                    bar_radius,
                    font_color,
                    default_bar_color,
                    show_duration,
                    show_footer,
                    width,
                    content_y + padding,
                );
            }
            ChallengeLayout::Horizontal => {
                self.render_horizontal(
                    &enabled_challenges,
                    padding,
                    card_spacing,
                    bar_height,
                    bar_spacing,
                    font_size,
                    header_font_size,
                    duration_font_size,
                    bar_radius,
                    font_color,
                    default_bar_color,
                    show_duration,
                    show_footer,
                    width,
                    height,
                );
            }
        }

        self.frame.end_frame();
    }

    #[allow(clippy::too_many_arguments)]
    fn render_vertical(
        &mut self,
        challenges: &[ChallengeEntry],
        padding: f32,
        card_spacing: f32,
        bar_height: f32,
        bar_spacing: f32,
        font_size: f32,
        header_font_size: f32,
        duration_font_size: f32,
        bar_radius: f32,
        font_color: Color,
        default_bar_color: Color,
        show_duration: bool,
        show_footer: bool,
        width: f32,
        start_y: f32,
    ) {
        let content_width = width - padding * 2.0;
        let mut y = start_y;

        for (idx, challenge) in challenges.iter().enumerate() {
            if idx > 0 {
                y += card_spacing;
            }

            let bar_color = challenge.color.unwrap_or(default_bar_color);

            // Render challenge card header
            y = self.render_challenge_header(
                challenge,
                padding,
                y,
                content_width,
                header_font_size,
                duration_font_size,
                bar_spacing,
                font_color,
                show_duration,
            );

            // Render player bars (uses per-challenge columns setting)
            y = self.render_player_bars(
                challenge,
                padding,
                y,
                content_width,
                bar_height,
                bar_spacing,
                font_size,
                bar_radius,
                font_color,
                bar_color,
            );

            // Render footer for this challenge
            if show_footer {
                y = self.render_challenge_footer(
                    challenge,
                    padding,
                    y,
                    content_width,
                    footer_font_size(font_size),
                    bar_spacing,
                    font_color,
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_horizontal(
        &mut self,
        challenges: &[ChallengeEntry],
        padding: f32,
        card_spacing: f32,
        bar_height: f32,
        bar_spacing: f32,
        font_size: f32,
        header_font_size: f32,
        duration_font_size: f32,
        bar_radius: f32,
        font_color: Color,
        default_bar_color: Color,
        show_duration: bool,
        show_footer: bool,
        width: f32,
        _height: f32,
    ) {
        let num_challenges = challenges.len();
        if num_challenges == 0 {
            return;
        }

        // Card width is the window divided into slots (reserved count when
        // configured), so a lone card keeps its 1/N share instead of
        // stretching across the whole window.
        let slots = self.slot_count(num_challenges);
        let total_spacing = card_spacing * (slots - 1) as f32;
        let available_width = width - padding * 2.0 - total_spacing;
        let card_width = available_width / slots as f32;

        // Anchor the card group to the right edge when stacking from the end.
        let x_start = if self.config.stack_from_end {
            let group_w = card_width * num_challenges as f32
                + card_spacing * (num_challenges - 1) as f32;
            width - padding - group_w
        } else {
            padding
        };

        for (idx, challenge) in challenges.iter().enumerate() {
            let card_x = x_start + (card_width + card_spacing) * idx as f32;
            let mut y = padding;

            let bar_color = challenge.color.unwrap_or(default_bar_color);

            // Render challenge card header
            y = self.render_challenge_header(
                challenge,
                card_x,
                y,
                card_width,
                header_font_size,
                duration_font_size,
                bar_spacing,
                font_color,
                show_duration,
            );

            // Render player bars (uses per-challenge columns setting)
            y = self.render_player_bars(
                challenge,
                card_x,
                y,
                card_width,
                bar_height,
                bar_spacing,
                font_size,
                bar_radius,
                font_color,
                bar_color,
            );

            // Render footer for this challenge
            if show_footer {
                self.render_challenge_footer(
                    challenge,
                    card_x,
                    y,
                    card_width,
                    footer_font_size(font_size),
                    bar_spacing,
                    font_color,
                );
            }
        }
    }

    /// Render the challenge card header with name and optional duration
    #[allow(clippy::too_many_arguments)]
    fn render_challenge_header(
        &mut self,
        challenge: &ChallengeEntry,
        x: f32,
        y: f32,
        width: f32,
        header_font_size: f32,
        duration_font_size: f32,
        spacing: f32,
        font_color: Color,
        show_duration: bool,
    ) -> f32 {
        let title_y = y + header_font_size;
        let scale = self.frame.scale_factor();
        let gap = 4.0 * scale;

        // Reserve space for the duration text on the right so the title can
        // either truncate (with ellipsis) or shrink to fit instead of bleeding
        // underneath the duration on a narrow card.
        let (duration_str, duration_width, duration_x, duration_y) = if show_duration {
            let s = format!("({})", format_duration_short(challenge.duration_secs));
            let (w, _) = self.frame.measure_text(&s, duration_font_size);
            let dx = x + width - w;
            let dy = title_y - (header_font_size - duration_font_size) * 0.3;
            (Some(s), w + gap, dx, dy)
        } else {
            (None, 0.0, 0.0, 0.0)
        };

        let title_max = (width - duration_width).max(0.0);

        // Truncate first; if the ellipsis variant is still too wide for the
        // available room (very narrow card), step the font size down until it
        // fits — this keeps long challenge names like "Extraneous Expulsion"
        // readable instead of clipping into the duration text.
        let (display_title, display_font_size) =
            self.fit_header_title(&challenge.name, title_max, header_font_size);

        let baseline_shift = (header_font_size - display_font_size) * 0.3;
        let display_y = title_y - baseline_shift;
        self.frame.draw_text_glowed(
            &display_title,
            x,
            display_y,
            display_font_size,
            font_color,
        );

        if let Some(s) = duration_str {
            self.frame
                .draw_text_glowed(&s, duration_x, duration_y, duration_font_size, font_color);
        }
        let _ = duration_width; // silence unused-binding warning when show_duration=false

        // Draw separator line
        let sep_y = title_y + spacing + 2.0;
        self.frame.fill_rect(x, sep_y, width, 0.2 * scale, font_color);

        y + Header::height(header_font_size, spacing, scale)
    }

    /// Fit a header title into `max_width` by truncation first, then by
    /// shrinking the font when truncation alone can't fit a meaningful prefix.
    fn fit_header_title(
        &mut self,
        title: &str,
        max_width: f32,
        font_size: f32,
    ) -> (String, f32) {
        if max_width <= 0.0 {
            return (String::new(), font_size);
        }

        let (full_w, _) = self.frame.measure_text(title, font_size);
        if full_w <= max_width {
            return (title.to_string(), font_size);
        }

        // Try truncation at the requested font size first.
        let truncated = self.frame.truncate_to_width(title, max_width, font_size, false);
        if truncated.chars().count() > 3 {
            return (truncated, font_size);
        }

        // Truncation can't preserve enough of the name (card is very narrow).
        // Shrink font in 5% steps down to ~70% of original, retrying truncation.
        let mut size = font_size;
        for _ in 0..6 {
            size *= 0.95;
            let candidate = self.frame.truncate_to_width(title, max_width, size, false);
            if candidate.chars().count() > 3 {
                return (candidate, size);
            }
        }
        (self.frame.truncate_to_width(title, max_width, size, false), size)
    }

    /// Render player contribution bars for a challenge
    #[allow(clippy::too_many_arguments)]
    fn render_player_bars(
        &mut self,
        challenge: &ChallengeEntry,
        x: f32,
        mut y: f32,
        width: f32,
        bar_height: f32,
        bar_spacing: f32,
        font_size: f32,
        bar_radius: f32,
        font_color: Color,
        bar_color: Color,
    ) -> f32 {
        let players: Vec<_> = challenge.by_player.iter().take(MAX_PLAYERS).collect();
        let max_value = players.iter().map(|p| p.value).fold(1_i64, |a, b| a.max(b));

        // Icon sizing matches metric.rs: discipline icons fill bar height flush
        // to the edge, class icons inset slightly so the silhouette has padding.
        let scale = self.frame.scale_factor();
        let is_discipline_icon = self.icon_mode == ClassIconMode::Discipline;
        let (icon_size, icon_padding) = if is_discipline_icon {
            (bar_height, 0.0)
        } else {
            (bar_height - 4.0 * scale, 2.0 * scale)
        };

        for player in &players {
            let progress = if max_value > 0 {
                player.value as f32 / max_value as f32
            } else {
                0.0
            };

            let bg_color = if self.config.show_background_bar {
                colors::dps_bar_bg()
            } else {
                Color::from_rgba8(0, 0, 0, 0)
            };

            // Match metric overlays: icon name picked from the configured mode
            // (discipline falls back to class if no discipline icon is set).
            let icon_name = match self.icon_mode {
                ClassIconMode::None => None,
                ClassIconMode::Class => player.class_icon.as_ref(),
                ClassIconMode::Discipline => {
                    player.discipline_icon.as_ref().or(player.class_icon.as_ref())
                }
            };
            let has_icon = icon_name.is_some();

            let mut bar = ProgressBar::new(player.name.as_str(), progress)
                .with_fill_color(bar_color)
                .with_bg_color(bg_color)
                .with_text_color(font_color)
                .with_gradient(self.config.bar_gradient)
                .with_text_glow();

            if player.is_local {
                bar = bar.with_bold_text();
            }

            if has_icon {
                bar = bar.with_label_offset(icon_size + icon_padding);
            }

            // Use per-challenge columns setting
            let eu = self.european_number_format;
            match challenge.columns {
                ChallengeColumns::TotalPercent => {
                    // 2-column: total | percent
                    bar = bar
                        .with_center_text(formatting::format_compact(player.value, eu))
                        .with_right_text(formatting::format_pct(player.percent as f64, eu));
                }
                ChallengeColumns::TotalPerSecond => {
                    // 2-column: total | per_second
                    let per_sec_val = player.per_second.map(|ps| ps as i64).unwrap_or(0);
                    bar = bar
                        .with_center_text(formatting::format_compact(player.value, eu))
                        .with_right_text(formatting::format_compact(per_sec_val, eu));
                }
                ChallengeColumns::PerSecondPercent => {
                    // 2-column: per_second | percent
                    let per_sec_val = player.per_second.map(|ps| ps as i64).unwrap_or(0);
                    bar = bar
                        .with_center_text(formatting::format_compact(per_sec_val, eu))
                        .with_right_text(formatting::format_pct(player.percent as f64, eu));
                }
                ChallengeColumns::TotalOnly => {
                    // Single column: just total
                    bar = bar.with_right_text(formatting::format_compact(player.value, eu));
                }
                ChallengeColumns::PerSecondOnly => {
                    // Single column: just per_second
                    let per_sec_val = player.per_second.map(|ps| ps as i64).unwrap_or(0);
                    bar = bar.with_right_text(formatting::format_compact(per_sec_val, eu));
                }
                ChallengeColumns::PercentOnly => {
                    // Single column: just percent
                    bar = bar.with_right_text(formatting::format_pct(player.percent as f64, eu));
                }
            }

            bar.render(
                &mut self.frame,
                x,
                y,
                width,
                bar_height,
                font_size - 2.0,
                bar_radius,
            );

            // Draw class/discipline icon on top of the bar — same lookup
            // semantics as metric.rs: discipline icons are raw, class icons
            // are role-tinted (or white when no role is known).
            if let Some(name) = icon_name {
                let icon = if is_discipline_icon {
                    get_discipline_icon(name)
                } else if let Some(role) = player.role {
                    get_role_colored_class_icon(name, role)
                } else {
                    get_white_class_icon(name)
                };
                if let Some(icon) = icon {
                    let icon_x = x + icon_padding;
                    let icon_y = y + icon_padding;
                    self.frame.draw_image_with_shadow(
                        &icon.rgba,
                        icon.width,
                        icon.height,
                        icon_x,
                        icon_y,
                        icon_size,
                        icon_size,
                    );
                }
            }

            y += bar_height + bar_spacing;
        }

        y
    }

    /// Render footer with totals aligned to match bar columns
    #[allow(clippy::too_many_arguments)]
    fn render_challenge_footer(
        &mut self,
        challenge: &ChallengeEntry,
        x: f32,
        y: f32,
        width: f32,
        font_size: f32,
        spacing: f32,
        font_color: Color,
    ) -> f32 {
        let total_sum: i64 = challenge.by_player.iter().map(|p| p.value).sum();
        let total_per_sec: f32 = challenge
            .by_player
            .iter()
            .filter_map(|p| p.per_second)
            .sum();

        // Use Footer widget for consistent alignment with metric overlays
        let eu = self.european_number_format;
        let footer = match challenge.columns {
            ChallengeColumns::TotalPercent => {
                // 2-column: total | 100%
                Footer::new("100%".to_string())
                    .with_secondary(formatting::format_compact(total_sum, eu))
                    .with_color(font_color)
            }
            ChallengeColumns::TotalPerSecond => {
                // 2-column: total | per_second
                Footer::new(formatting::format_compact(total_per_sec as i64, eu))
                    .with_secondary(formatting::format_compact(total_sum, eu))
                    .with_color(font_color)
            }
            ChallengeColumns::PerSecondPercent => {
                // 2-column: per_second | 100%
                Footer::new("100%".to_string())
                    .with_secondary(formatting::format_compact(total_per_sec as i64, eu))
                    .with_color(font_color)
            }
            ChallengeColumns::TotalOnly => {
                // Single column: just total
                Footer::new(formatting::format_compact(total_sum, eu)).with_color(font_color)
            }
            ChallengeColumns::PerSecondOnly => {
                // Single column: just per_second
                Footer::new(formatting::format_compact(total_per_sec as i64, eu))
                    .with_color(font_color)
            }
            ChallengeColumns::PercentOnly => {
                // Single column: 100%
                Footer::new("100%".to_string()).with_color(font_color)
            }
        };

        footer.render(&mut self.frame, x, y, width, font_size);

        y + font_size + spacing
    }
}

/// Footer values render slightly smaller than bar text
fn footer_font_size(font_size: f32) -> f32 {
    font_size - 2.0
}

/// Height of one challenge card. Mirrors the render path: header (title +
/// separator, same metrics as `Header`), one row per player, then the footer
/// (`footer_font + spacing`, see `render_challenge_footer`).
fn card_height(
    players: usize,
    header_font: f32,
    bar_height: f32,
    bar_spacing: f32,
    footer_font: Option<f32>,
    scale: f32,
) -> f32 {
    Header::height(header_font, bar_spacing, scale)
        + players as f32 * (bar_height + bar_spacing)
        + footer_font.map_or(0.0, |f| f + bar_spacing)
}

/// Build `n` sample challenge cards for the move-mode preview so the user
/// sees real card sizing — including reserved slots — while positioning the
/// window. Values are static; the normal render path draws them.
fn sample_challenges(n: usize) -> Vec<ChallengeEntry> {
    const CARDS: [(&str, f32); 8] = [
        ("Add Damage", 154.0),
        ("Orb Healing", 154.0),
        ("Burn DPS", 62.0),
        ("Tank Damage Taken", 154.0),
        ("Kephess Burn", 45.0),
        ("Tentacle Damage", 90.0),
        ("Bulwark Damage", 120.0),
        ("Firebrand Damage", 120.0),
    ];
    const PLAYERS: [(&str, f32, &str, &str, Role); 8] = [
        ("Zas'kel", 1.00, "marauder", "annihilation", Role::Damage),
        ("Vharowyn", 0.92, "sniper", "virulence", Role::Damage),
        ("Ondorru", 0.84, "mercenary", "arsenal", Role::Damage),
        ("Xal-vyr", 0.76, "assassin", "hatred", Role::Damage),
        ("Miralei", 0.67, "sorcerer", "lightning", Role::Damage),
        ("Teekay", 0.58, "operative", "medicine", Role::Healer),
        ("Serapha", 0.49, "sorcerer", "corruption", Role::Healer),
        ("Brontes'bane", 0.38, "juggernaut", "immortal", Role::Tank),
    ];

    CARDS
        .iter()
        .take(n.clamp(1, CARDS.len()))
        .enumerate()
        .map(|(card_idx, &(name, duration))| {
            let base = 1_850_000.0 * (1.0 - card_idx as f32 * 0.07);
            let total: f32 = PLAYERS.iter().map(|p| p.1 * base).sum();
            let by_player = PLAYERS
                .iter()
                .enumerate()
                .map(|(i, &(pname, frac, class, disc, role))| {
                    let value = (base * frac) as i64;
                    PlayerContribution {
                        entity_id: 0,
                        name: pname.to_string(),
                        value,
                        percent: value as f32 / total * 100.0,
                        per_second: Some(value as f32 / duration),
                        is_local: i == 1,
                        class_icon: Some(class.to_string()),
                        discipline_icon: Some(disc.to_string()),
                        role: Some(role),
                    }
                })
                .collect::<Vec<_>>();
            ChallengeEntry {
                name: name.to_string(),
                value: total as i64,
                event_count: 128,
                per_second: Some(total / duration),
                by_player,
                duration_secs: duration,
                enabled: true,
                color: None,
                columns: ChallengeColumns::default(),
            }
        })
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════════════
// Overlay Trait Implementation
// ═══════════════════════════════════════════════════════════════════════════════

impl Overlay for ChallengeOverlay {
    fn update_data(&mut self, data: OverlayData) -> bool {
        if let OverlayData::Challenges(challenge_data) = data {
            // Skip render if both old and new have no challenges
            let old_empty = self.data.entries.is_empty();
            let new_empty = challenge_data.entries.is_empty();
            self.set_data(challenge_data);
            !(old_empty && new_empty)
        } else {
            false
        }
    }

    fn update_config(&mut self, config: OverlayConfigUpdate) {
        if let OverlayConfigUpdate::Challenge(challenge_config, alpha, european, icon_mode) = config
        {
            self.set_config(challenge_config);
            self.set_background_alpha(alpha);
            self.european_number_format = european;
            self.set_icon_mode(icon_mode);
        }
    }

    fn render(&mut self) {
        self.render_overlay();
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
