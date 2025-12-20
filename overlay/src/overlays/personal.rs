//! Personal Stats Overlay
//!
//! Displays the primary player's combat statistics as text items.

use baras_core::context::{PersonalOverlayConfig, PersonalStat};
use tiny_skia::Color;

use crate::manager::OverlayWindow;
use crate::platform::{OverlayConfig, PlatformError};
use crate::renderer::colors;

/// Data for the personal overlay
#[derive(Debug, Clone, Default)]
pub struct PersonalStats {
    pub encounter_time_secs: u64,
    pub encounter_count: usize,
    pub class_discipline: Option<String>,
    pub apm: f32,
    pub dps: i32,
    pub edps: i32,
    pub total_damage: i64,
    pub hps: i32,
    pub ehps: i32,
    pub total_healing: i64,
    pub dtps: i32,
    pub edtps: i32,
    pub tps: i32,
    pub total_threat: i64,
    pub damage_crit_pct: f32,
    pub heal_crit_pct: f32,
    pub effective_heal_pct: f32,
}

/// Base dimensions for scaling calculations
const BASE_WIDTH: f32 = 200.0;
const BASE_HEIGHT: f32 = 180.0;
const BASE_FONT_SIZE: f32 = 13.0;
const BASE_LINE_HEIGHT: f32 = 18.0;
const BASE_PADDING: f32 = 8.0;

/// Convert [u8; 4] RGBA to tiny_skia Color
fn color_from_rgba(rgba: [u8; 4]) -> Color {
    Color::from_rgba8(rgba[0], rgba[1], rgba[2], rgba[3])
}

/// Format a duration as MM:SS
fn format_time(secs: u64) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

/// Format a large number with K suffix
fn format_number(n: i64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 10_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        format!("{}", n)
    }
}

/// Personal stats overlay showing player metrics as text
pub struct PersonalOverlay {
    window: OverlayWindow,
    config: PersonalOverlayConfig,
    background_alpha: u8,
    stats: PersonalStats,
}

impl PersonalOverlay {
    /// Create a new personal overlay
    pub fn new(
        window_config: OverlayConfig,
        config: PersonalOverlayConfig,
        background_alpha: u8,
    ) -> Result<Self, PlatformError> {
        let window = OverlayWindow::new(window_config)?;

        Ok(Self {
            window,
            config,
            background_alpha,
            stats: PersonalStats::default(),
        })
    }

    /// Update the config
    pub fn set_config(&mut self, config: PersonalOverlayConfig) {
        self.config = config;
    }

    /// Update background alpha
    pub fn set_background_alpha(&mut self, alpha: u8) {
        self.background_alpha = alpha;
    }

    /// Update the stats
    pub fn set_stats(&mut self, stats: PersonalStats) {
        self.stats = stats;
    }

    /// Calculate scale factor based on current window size
    fn scale_factor(&self) -> f32 {
        let width = self.window.width() as f32;
        let height = self.window.height() as f32;
        let width_ratio = width / BASE_WIDTH;
        let height_ratio = height / BASE_HEIGHT;
        (width_ratio * height_ratio).sqrt()
    }

    fn font_size(&self) -> f32 {
        BASE_FONT_SIZE * self.scale_factor()
    }

    fn line_height(&self) -> f32 {
        BASE_LINE_HEIGHT * self.scale_factor()
    }

    fn padding(&self) -> f32 {
        BASE_PADDING * self.scale_factor()
    }

    /// Get the display value for a stat
    fn stat_display(&self, stat: PersonalStat) -> (& 'static str, String) {
        match stat {
            PersonalStat::EncounterTime => ("Time", format_time(self.stats.encounter_time_secs)),
            PersonalStat::EncounterCount => ("Fight #", format!("{}", self.stats.encounter_count)),
            PersonalStat::Apm => ("APM", format!("{:.1}", self.stats.apm)),
            PersonalStat::Dps => ("DPS", format!("{}", self.stats.dps)),
            PersonalStat::EDps => ("eDPS", format!("{}", self.stats.edps)),
            PersonalStat::TotalDamage => ("Damage", format_number(self.stats.total_damage)),
            PersonalStat::Hps => ("HPS", format!("{}", self.stats.hps)),
            PersonalStat::EHps => ("eHPS", format!("{}", self.stats.ehps)),
            PersonalStat::TotalHealing => ("Healing", format_number(self.stats.total_healing)),
            PersonalStat::Dtps => ("DTPS", format!("{}", self.stats.dtps)),
            PersonalStat::EDtps => ("eDTPS", format!("{}", self.stats.edtps)),
            PersonalStat::Tps => ("TPS", format!("{}", self.stats.tps)),
            PersonalStat::TotalThreat => ("Threat", format_number(self.stats.total_threat)),
            PersonalStat::DamageCritPct => ("Dmg Crit", format!("{:.1}%", self.stats.damage_crit_pct)),
            PersonalStat::HealCritPct => ("Heal Crit", format!("{:.1}%", self.stats.heal_crit_pct)),
            PersonalStat::EffectiveHealPct => ("Eff Heal", format!("{:.1}%", self.stats.effective_heal_pct)),
            PersonalStat::ClassDiscipline => {
                let value = self.stats.class_discipline.clone().unwrap_or_else(|| "Unknown".to_string());
                ("Spec", value)
            }
        }
    }

    /// Render the overlay
    pub fn render(&mut self) {
        let width = self.window.width() as f32;
        let height = self.window.height() as f32;

        let padding = self.padding();
        let font_size = self.font_size();
        let line_height = self.line_height();
        let corner_radius = 6.0 * self.scale_factor();

        let font_color = color_from_rgba(self.config.font_color);
        let bg_color = Color::from_rgba8(30, 30, 30, self.background_alpha);
        let label_color = Color::from_rgba8(180, 180, 180, 255);

        // Clear with transparent
        self.window.clear(colors::transparent());

        // Draw background
        self.window
            .fill_rounded_rect(0.0, 0.0, width, height, corner_radius, bg_color);

        // Draw border when interactive
        if self.window.is_interactive() {
            let border_color = Color::from_rgba8(128, 128, 128, 200);
            self.window.stroke_rounded_rect(
                1.0, 1.0,
                width - 2.0, height - 2.0,
                corner_radius - 1.0,
                2.0,
                border_color,
            );
        }

        // Draw stats
        let mut y = padding + font_size;
        let value_x = width - padding;

        for stat in &self.config.visible_stats {
            let (label, value) = self.stat_display(*stat);

            // Draw label on left
            self.window.draw_text(label, padding, y, font_size, label_color);

            // Draw value on right (right-aligned)
            let (text_width, _) = self.window.measure_text(&value, font_size);
            self.window.draw_text(&value, value_x - text_width, y, font_size, font_color);

            y += line_height;
        }

        // Draw resize indicator when interactive
        if self.window.in_resize_corner() || self.window.is_interactive() {
            let indicator_size = 12.0;
            let corner_x = width - indicator_size - 4.0;
            let corner_y = height - indicator_size - 4.0;

            let highlight = if self.window.is_resizing() {
                colors::white()
            } else {
                Color::from_rgba8(255, 255, 255, 150)
            };

            for i in 0..3 {
                let offset = i as f32 * 4.0;
                self.window.fill_rect(
                    corner_x + offset,
                    corner_y + indicator_size - 2.0,
                    indicator_size - offset,
                    2.0,
                    highlight,
                );
                self.window.fill_rect(
                    corner_x + indicator_size - 2.0,
                    corner_y + offset,
                    2.0,
                    indicator_size - offset,
                    highlight,
                );
            }
        }

        self.window.commit();
    }

    /// Poll for events
    pub fn poll_events(&mut self) -> bool {
        self.window.poll_events()
    }

    /// Get mutable access to the underlying window
    pub fn window_mut(&mut self) -> &mut OverlayWindow {
        &mut self.window
    }
}
