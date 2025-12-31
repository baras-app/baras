//! Challenge tracking overlay
//!
//! Displays challenge metrics during boss encounters. Each challenge is rendered
//! as its own "card" showing the challenge title, duration, and per-player bars
//! with contribution percentages.

use std::collections::HashMap;

use baras_core::context::OverlayAppearanceConfig;

use super::{Overlay, OverlayConfigUpdate, OverlayData};
use crate::frame::OverlayFrame;
use crate::platform::{OverlayConfig, PlatformError};
use crate::utils::{color_from_rgba, format_duration_short, format_number, truncate_name};
use crate::widgets::{colors, Header, ProgressBar};

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
}

// ═══════════════════════════════════════════════════════════════════════════════
// Layout Constants
// ═══════════════════════════════════════════════════════════════════════════════

const BASE_WIDTH: f32 = 320.0;
const BASE_HEIGHT: f32 = 400.0;

const BASE_PADDING: f32 = 6.0;
const BASE_CARD_SPACING: f32 = 8.0;
const BASE_BAR_HEIGHT: f32 = 18.0;
const BASE_BAR_SPACING: f32 = 3.0;
const BASE_FONT_SIZE: f32 = 13.0;
const BASE_HEADER_FONT_SIZE: f32 = 12.0;

const MAX_NAME_CHARS: usize = 14;
const MAX_CHALLENGES: usize = 4;
const MAX_PLAYERS: usize = 8;

// ═══════════════════════════════════════════════════════════════════════════════
// Challenge Overlay
// ═══════════════════════════════════════════════════════════════════════════════

/// Overlay displaying multiple challenge metrics as stacked cards
pub struct ChallengeOverlay {
    frame: OverlayFrame,
    data: ChallengeData,
    background_alpha: u8,
    appearance: OverlayAppearanceConfig,
}

impl ChallengeOverlay {
    pub fn new(
        config: OverlayConfig,
        appearance: OverlayAppearanceConfig,
        background_alpha: u8,
    ) -> Result<Self, PlatformError> {
        let mut frame = OverlayFrame::new(config, BASE_WIDTH, BASE_HEIGHT)?;
        frame.set_background_alpha(background_alpha);
        frame.set_label("Challenges");

        Ok(Self {
            frame,
            data: ChallengeData::default(),
            background_alpha,
            appearance,
        })
    }

    pub fn set_data(&mut self, data: ChallengeData) {
        self.data = data;
    }

    pub fn set_appearance(&mut self, appearance: OverlayAppearanceConfig) {
        self.appearance = appearance;
    }

    pub fn set_background_alpha(&mut self, alpha: u8) {
        self.background_alpha = alpha;
        self.frame.set_background_alpha(alpha);
    }

    pub fn render_overlay(&mut self) {
        let width = self.frame.width() as f32;
        let _height = self.frame.height() as f32;

        let padding = self.frame.scaled(BASE_PADDING);
        let card_spacing = self.frame.scaled(BASE_CARD_SPACING);
        let bar_height = self.frame.scaled(BASE_BAR_HEIGHT);
        let bar_spacing = self.frame.scaled(BASE_BAR_SPACING);
        let font_size = self.frame.scaled(BASE_FONT_SIZE);
        let header_font_size = self.frame.scaled(BASE_HEADER_FONT_SIZE);
        let bar_radius = 3.0 * self.frame.scale_factor();

        let font_color = color_from_rgba(self.appearance.font_color);
        let bar_color = color_from_rgba(self.appearance.bar_color);

        // Get display options
        let show_total = self.appearance.show_total;
        let show_per_second = self.appearance.show_per_second;
        let show_percent = self.appearance.show_percent;
        let show_duration = self.appearance.show_duration;

        self.frame.begin_frame();

        let content_width = width - padding * 2.0;
        let mut y = padding;

        // Render each challenge as a card
        let challenges: Vec<_> = self.data.entries.iter().take(MAX_CHALLENGES).collect();

        for (idx, challenge) in challenges.iter().enumerate() {
            if idx > 0 {
                y += card_spacing;
            }

            // Card header: challenge name + optional duration
            let header_text = if show_duration {
                let duration_str = format_duration_short(challenge.duration_secs);
                format!("{} ({})", challenge.name, duration_str)
            } else {
                challenge.name.clone()
            };

            y = Header::new(&header_text)
                .with_color(font_color)
                .render(&mut self.frame, padding, y, content_width, header_font_size, bar_spacing);

            // Player bars
            let players: Vec<_> = challenge.by_player.iter().take(MAX_PLAYERS).collect();

            // Find max for scaling (use value for bar fill, not percent)
            let max_value = players.iter().map(|p| p.value).fold(1_i64, |a, b| a.max(b));

            for player in &players {
                let display_name = truncate_name(&player.name, MAX_NAME_CHARS);
                let progress = if max_value > 0 {
                    player.value as f32 / max_value as f32
                } else {
                    0.0
                };

                let mut bar = ProgressBar::new(display_name, progress)
                    .with_fill_color(bar_color)
                    .with_bg_color(colors::dps_bar_bg())
                    .with_text_color(font_color);

                // Build column text based on config - similar to metric overlay
                // Layout: [Name] [Total?] [Per Second?] [Percent?]
                // Use center_text for middle column, right_text for rightmost
                let total_str = format_number(player.value);
                let per_sec_str = player.per_second
                    .map(|ps| format!("{}/s", format_number(ps as i64)))
                    .unwrap_or_default();
                let percent_str = format!("{:.1}%", player.percent);

                // Count enabled columns to determine layout
                let enabled_count = [show_total, show_per_second, show_percent]
                    .iter()
                    .filter(|&&x| x)
                    .count();

                match enabled_count {
                    3 => {
                        // All three: total center, per_sec + percent right
                        bar = bar
                            .with_center_text(total_str)
                            .with_right_text(format!("{}  {}", per_sec_str, percent_str));
                    }
                    2 => {
                        // Two columns: first goes center, second goes right
                        if show_total && show_per_second {
                            bar = bar.with_center_text(total_str).with_right_text(per_sec_str);
                        } else if show_total && show_percent {
                            bar = bar.with_center_text(total_str).with_right_text(percent_str);
                        } else if show_per_second && show_percent {
                            bar = bar.with_center_text(per_sec_str).with_right_text(percent_str);
                        }
                    }
                    1 => {
                        // Single column: goes right
                        if show_total {
                            bar = bar.with_right_text(total_str);
                        } else if show_per_second {
                            bar = bar.with_right_text(per_sec_str);
                        } else if show_percent {
                            bar = bar.with_right_text(percent_str);
                        }
                    }
                    _ => {} // No columns enabled - just show name
                }

                bar.render(&mut self.frame, padding, y, content_width, bar_height, font_size - 2.0, bar_radius);

                y += bar_height + bar_spacing;
            }
        }

        self.frame.end_frame();
    }
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
        if let OverlayConfigUpdate::Metric(appearance, alpha) = config {
            self.set_appearance(appearance);
            self.set_background_alpha(alpha);
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
