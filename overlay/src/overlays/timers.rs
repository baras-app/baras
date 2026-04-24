//! Timer Bar Overlay
//!
//! Displays countdown timers for boss mechanics, ability cooldowns, etc.

use std::sync::Arc;

use baras_core::context::TimerOverlayConfig;

use super::{Overlay, OverlayConfigUpdate, OverlayData};
use crate::frame::OverlayFrame;
use crate::platform::{OverlayConfig, PlatformError};
use crate::utils::{color_from_rgba, shared_scaled_icons};
use crate::widgets::{colors, ProgressBar};

/// A single timer entry for display
#[derive(Debug, Clone)]
pub struct TimerEntry {
    /// Timer display name
    pub name: String,
    /// Remaining time in seconds
    pub remaining_secs: f32,
    /// Total duration in seconds (for progress calculation)
    pub total_secs: f32,
    /// Bar color (RGBA)
    pub color: [u8; 4],
    /// Optional ability ID for icon display
    pub icon_ability_id: Option<u64>,
    /// Pre-loaded icon RGBA data (width, height, rgba_bytes) - Arc for cheap cloning
    pub icon: Option<Arc<(u32, u32, Vec<u8>)>>,
}

impl TimerEntry {
    /// Progress as 0.0 (expired) to 1.0 (full)
    pub fn progress(&self) -> f32 {
        if self.total_secs <= 0.0 {
            return 0.0;
        }
        (self.remaining_secs / self.total_secs).clamp(0.0, 1.0)
    }

    /// Format remaining time as MM:SS or S.s
    pub fn format_time(&self, european: bool) -> String {
        baras_types::formatting::format_countdown(self.remaining_secs, "", "0:00", european)
    }
}

/// Data sent from service to timer overlay
#[derive(Debug, Clone, Default)]
pub struct TimerData {
    /// Current active timers
    pub entries: Vec<TimerEntry>,
}

/// A single entry in the ability queue overlay.
///
/// - GCD entries: `is_pinned = true` → tier-1 accent bar (pinned top)
/// - Queued/ready entries: `is_queued = true` → tier-2 "READY" label
/// - Active countdown entries: both flags false → tier-3 progress bar
#[derive(Debug, Clone)]
pub struct AbilityQueueEntry {
    pub name: String,
    pub remaining_secs: f32,
    pub total_secs: f32,
    pub color: [u8; 4],
    /// Sort priority for tier-2 queued entries (higher = higher on screen).
    pub queue_priority: u8,
    /// True for the synthetic GCD entry — pinned at tier 1.
    pub is_pinned: bool,
    /// True when the timer has expired and is held in ready/queued state.
    pub is_queued: bool,
    /// True when one or more of this entry's configured blocking timers is
    /// currently active. Blocked entries are dimmed in the overlay and
    /// excluded from the "next cast" highlight set.
    pub is_blocked: bool,
    /// When true, this row's bar renders as a trickling-down bar (full → empty
    /// as the cooldown elapses) instead of the default filling-up progress bar.
    pub countdown_bar: bool,
    /// When true, this row is excluded from the "next cast" highlight set —
    /// used for display-only rows that aren't castable abilities.
    pub hide_from_next: bool,
    pub icon_ability_id: Option<u64>,
    pub icon: Option<Arc<(u32, u32, Vec<u8>)>>,
}

impl AbilityQueueEntry {
    pub fn progress(&self) -> f32 {
        if self.total_secs <= 0.0 {
            return 1.0;
        }
        let elapsed = self.total_secs - self.remaining_secs;
        (elapsed / self.total_secs).clamp(0.0, 1.0)
    }
}

/// Snapshot delivered to the Ability Queue overlay on every timer tick.
#[derive(Debug, Clone, Default)]
pub struct AbilityQueueData {
    pub entries: Vec<AbilityQueueEntry>,
}

/// Base dimensions for scaling calculations
const BASE_WIDTH: f32 = 220.0;
const BASE_HEIGHT: f32 = 150.0;

/// Base layout values (at BASE_WIDTH x BASE_HEIGHT)
const BASE_BAR_HEIGHT: f32 = 18.0;
const BASE_ENTRY_SPACING: f32 = 4.0;
const BASE_PADDING: f32 = 6.0;
const BASE_FONT_SIZE: f32 = 11.0;

/// Timer bar overlay
pub struct TimerOverlay {
    frame: OverlayFrame,
    config: TimerOverlayConfig,
    data: TimerData,
    european_number_format: bool,
}

impl TimerOverlay {
    /// Create a new timer overlay
    pub fn new(
        window_config: OverlayConfig,
        config: TimerOverlayConfig,
        background_alpha: u8,
        label: &str,
    ) -> Result<Self, PlatformError> {
        let mut frame = OverlayFrame::new(window_config, BASE_WIDTH, BASE_HEIGHT)?;
        frame.set_background_alpha(background_alpha);
        frame.set_label(label);

        Ok(Self {
            frame,
            config,
            data: TimerData::default(),
            european_number_format: false,
        })
    }

    /// Update the config
    pub fn set_config(&mut self, config: TimerOverlayConfig) {
        self.config = config;
    }

    /// Update background alpha
    pub fn set_background_alpha(&mut self, alpha: u8) {
        self.frame.set_background_alpha(alpha);
    }

    /// Update the data and pre-cache icons at current display size
    pub fn set_data(&mut self, data: TimerData) {
        let bar_height = self.frame.scaled(BASE_BAR_HEIGHT);
        let icon_size = (bar_height - 4.0 * self.frame.scale_factor()).round() as u32;

        let cache = shared_scaled_icons();
        for entry in &data.entries {
            if let (Some(ability_id), Some(icon_arc)) = (entry.icon_ability_id, &entry.icon) {
                let (src_w, src_h, ref src_data) = **icon_arc;
                let _ = cache.get_or_scale(ability_id, icon_size, src_data, src_w, src_h);
            }
        }

        self.data = data;
    }

    /// Render a skeleton preview when in move mode
    fn render_preview(&mut self) {
        let width = self.frame.width() as f32;

        let padding = self.frame.scaled(BASE_PADDING);
        let bar_height = self.frame.scaled(BASE_BAR_HEIGHT);
        let entry_spacing = self.frame.scaled(BASE_ENTRY_SPACING);
        let font_scale = self.config.font_scale.clamp(1.0, 2.0);
        let font_size = self.frame.scaled(BASE_FONT_SIZE * font_scale);
        let font_color = color_from_rgba(self.config.font_color);

        self.frame.begin_frame();

        let content_width = width - padding * 2.0;
        let bar_radius = 3.0 * self.frame.scale_factor();

        let previews = [
            ("Mechanic A", "12.3", 0.75_f32),
            ("Mechanic B", "45.0", 0.40_f32),
            ("Mechanic C", "1:30", 0.10_f32),
        ];

        let mut y = padding;

        for (name, time_text, progress) in &previews {
            ProgressBar::new(*name, *progress)
                .with_fill_color(colors::effect_icon_bg())
                .with_bg_color(colors::dps_bar_bg())
                .with_text_color(font_color)
                .with_right_text(*time_text)
                .with_bold_text()
                .with_text_glow()
                .render(
                    &mut self.frame,
                    padding,
                    y,
                    content_width,
                    bar_height,
                    font_size,
                    bar_radius,
                );

            y += bar_height + entry_spacing;
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

        let padding = self.frame.scaled(BASE_PADDING);
        let bar_height = self.frame.scaled(BASE_BAR_HEIGHT);
        let entry_spacing = self.frame.scaled(BASE_ENTRY_SPACING);
        let font_scale = self.config.font_scale.clamp(1.0, 2.0);
        let font_size = self.frame.scaled(BASE_FONT_SIZE * font_scale);

        let font_color = color_from_rgba(self.config.font_color);

        // Sort entries in place if needed
        if self.config.sort_by_remaining {
            self.data
                .entries
                .sort_by(|a, b| a.remaining_secs.partial_cmp(&b.remaining_secs).unwrap());
        }

        // Compute content height for dynamic background
        let max_display = self.config.max_display as usize;
        let num_entries = self.data.entries.iter().take(max_display).count();
        let content_height = if num_entries > 0 {
            padding * 2.0
                + num_entries as f32 * bar_height
                + (num_entries - 1).max(0) as f32 * entry_spacing
        } else {
            0.0
        };

        // Begin frame (clear, background, border)
        if self.config.dynamic_background {
            self.frame.begin_frame_with_content_height(content_height);
        } else {
            self.frame.begin_frame();
        }

        // Nothing to render if no timers
        if self.data.entries.is_empty() {
            self.frame.end_frame();
            return;
        }

        let content_width = width - padding * 2.0;
        let bar_radius = 3.0 * self.frame.scale_factor();

        // Icon rendering setup (scale with bar, not text)
        let icon_size = bar_height - 4.0 * self.frame.scale_factor(); // Slightly smaller than bar
        let icon_padding = 2.0 * self.frame.scale_factor();
        let icon_size_u32 = icon_size.round() as u32;

        let mut y = padding;

        for entry in self.data.entries.iter().take(max_display) {
            let bar_color = color_from_rgba(entry.color);
            let time_text = entry.format_time(self.european_number_format);

            // Check if we have an icon to show
            let has_icon = entry.icon_ability_id.is_some() && entry.icon.is_some();

            // Draw timer bar with name on left, time on right
            let mut bar = ProgressBar::new(&entry.name, entry.progress())
                .with_fill_color(bar_color)
                .with_bg_color(colors::dps_bar_bg())
                .with_text_color(font_color)
                .with_right_text(time_text)
                .with_bold_text()
                .with_text_glow();

            // Add label offset to make room for icon
            if has_icon {
                bar = bar.with_label_offset(icon_size + icon_padding);
            }

            bar.render(
                &mut self.frame,
                padding,
                y,
                content_width,
                bar_height,
                font_size,
                bar_radius,
            );

            // Draw icon on top of bar if available
            if has_icon {
                if let Some(ability_id) = entry.icon_ability_id {
                    let icon_x = padding + icon_padding;
                    let icon_y = y + icon_padding;
                    let icon_drawn = if let Some(scaled_icon) =
                        shared_scaled_icons().get(ability_id, icon_size_u32)
                    {
                        self.frame.draw_image(
                            &scaled_icon,
                            icon_size_u32,
                            icon_size_u32,
                            icon_x,
                            icon_y,
                            icon_size,
                            icon_size,
                        );
                        true
                    } else if let Some(ref icon_arc) = entry.icon {
                        let (img_w, img_h, ref rgba) = **icon_arc;
                        self.frame
                            .draw_image(rgba, img_w, img_h, icon_x, icon_y, icon_size, icon_size);
                        true
                    } else {
                        false
                    };

                    // Draw glowing white border around the icon
                    if icon_drawn {
                        let icon_radius = 2.0 * self.frame.scale_factor();
                        let glow_expand = 1.0 * self.frame.scale_factor();

                        // Outer glow: wider, softer white
                        let outer_glow = tiny_skia::Color::from_rgba(1.0, 1.0, 1.0, 0.25).unwrap();
                        self.frame.stroke_rounded_rect(
                            icon_x - glow_expand,
                            icon_y - glow_expand,
                            icon_size + glow_expand * 2.0,
                            icon_size + glow_expand * 2.0,
                            icon_radius + glow_expand,
                            1.5 * self.frame.scale_factor(),
                            outer_glow,
                        );

                        // Inner border: tight, brighter white
                        let inner_border = tiny_skia::Color::from_rgba(1.0, 1.0, 1.0, 0.6).unwrap();
                        self.frame.stroke_rounded_rect(
                            icon_x,
                            icon_y,
                            icon_size,
                            icon_size,
                            icon_radius,
                            1.0 * self.frame.scale_factor(),
                            inner_border,
                        );
                    }
                }
            }

            y += bar_height + entry_spacing;
        }

        // End frame (resize indicator, commit)
        self.frame.end_frame();
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Trait Implementation
// ─────────────────────────────────────────────────────────────────────────────

impl Overlay for TimerOverlay {
    fn update_data(&mut self, data: OverlayData) -> bool {
        // Handle both TimersA and TimersB data (same data structure)
        let timer_data = match data {
            OverlayData::TimersA(d) | OverlayData::TimersB(d) => d,
            _ => return false,
        };
        // Skip render only when transitioning empty → empty
        // Active timers need every frame for smooth bar animation
        let was_empty = self.data.entries.is_empty();
        let is_empty = timer_data.entries.is_empty();
        self.set_data(timer_data);
        !(was_empty && is_empty)
    }

    fn update_config(&mut self, config: OverlayConfigUpdate) {
        // Handle both TimersA and TimersB config (same config structure)
        let (timer_config, alpha, european) = match config {
            OverlayConfigUpdate::TimersA(c, a, eu) | OverlayConfigUpdate::TimersB(c, a, eu) => {
                (c, a, eu)
            }
            _ => return,
        };
        self.set_config(timer_config);
        self.set_background_alpha(alpha);
        self.european_number_format = european;
    }

    fn render(&mut self) {
        TimerOverlay::render(self);
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
