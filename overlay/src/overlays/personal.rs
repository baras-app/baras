//! Personal Stats Overlay
//!
//! Displays the primary player's combat statistics as text items.

use baras_core::context::{PersonalOverlayConfig, PersonalStat};

use super::{Overlay, OverlayConfigUpdate, OverlayData};
use crate::frame::OverlayFrame;
use crate::platform::{OverlayConfig, PlatformError};
use crate::utils::{color_from_rgba, format_number, format_time};
use crate::widgets::LabeledValue;

/// Data for the personal overlay
#[derive(Debug, Clone, Default)]
pub struct PersonalStats {
    pub encounter_name: Option<String>,
    pub difficulty: Option<String>,
    pub encounter_time_secs: u64,
    pub encounter_count: usize,
    pub class_discipline: Option<String>,
    pub apm: f32,
    pub dps: i32,
    pub bossdps: i32,
    pub edps: i32,
    pub total_damage: i64,
    pub total_damage_boss: i64,
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
    pub current_phase: Option<String>,
    pub phase_time_secs: f32,
}

/// Base dimensions for scaling calculations
const BASE_WIDTH: f32 = 200.0;
const BASE_HEIGHT: f32 = 180.0;
const BASE_FONT_SIZE: f32 = 13.0;
const BASE_LINE_HEIGHT: f32 = 18.0;
const BASE_PADDING: f32 = 8.0;

/// Personal stats overlay showing player metrics as text
pub struct PersonalOverlay {
    frame: OverlayFrame,
    config: PersonalOverlayConfig,
    stats: PersonalStats,
}

impl PersonalOverlay {
    /// Create a new personal overlay
    pub fn new(
        window_config: OverlayConfig,
        config: PersonalOverlayConfig,
        background_alpha: u8,
    ) -> Result<Self, PlatformError> {
        let mut frame = OverlayFrame::new(window_config, BASE_WIDTH, BASE_HEIGHT)?;
        frame.set_background_alpha(background_alpha);
        frame.set_label("Personal Stats");

        Ok(Self {
            frame,
            config,
            stats: PersonalStats::default(),
        })
    }

    /// Update the config
    pub fn set_config(&mut self, config: PersonalOverlayConfig) {
        self.config = config;
    }

    /// Update background alpha
    pub fn set_background_alpha(&mut self, alpha: u8) {
        self.frame.set_background_alpha(alpha);
    }

    /// Update the stats
    pub fn set_stats(&mut self, stats: PersonalStats) {
        self.stats = stats;
    }

    /// Get the display value for a stat
    fn stat_display(&self, stat: PersonalStat) -> (&'static str, String) {
        match stat {
            PersonalStat::EncounterName => {
                let name = self.stats.encounter_name.as_deref().unwrap_or("");
                ("", name.to_string())
            }
            PersonalStat::Difficulty => {
                let diff = self.stats.difficulty.as_deref().unwrap_or("Open World");
                ("", diff.to_string())
            }
            PersonalStat::EncounterTime => {
                ("Combat Time", format_time(self.stats.encounter_time_secs))
            }
            PersonalStat::EncounterCount => (
                "Session Encounters",
                format!("{}", self.stats.encounter_count),
            ),
            PersonalStat::Apm => ("APM", format!("{:.1}", self.stats.apm)),
            PersonalStat::Dps => ("DPS", format_number(self.stats.dps as i64)),
            PersonalStat::EDps => ("eDPS", format_number(self.stats.edps as i64)),
            PersonalStat::BossDps => ("Boss DPS", format_number(self.stats.bossdps as i64)),
            PersonalStat::TotalDamage => ("Damage", format_number(self.stats.total_damage)),
            PersonalStat::BossDamage => ("Boss Dmg", format_number(self.stats.total_damage_boss)),
            PersonalStat::Hps => ("HPS", format_number(self.stats.hps as i64)),
            PersonalStat::EHps => ("eHPS", format_number(self.stats.ehps as i64)),
            PersonalStat::TotalHealing => ("Healing", format_number(self.stats.total_healing)),
            PersonalStat::Dtps => ("eDTPS", format_number(self.stats.edtps as i64)),
            PersonalStat::Tps => ("TPS", format_number(self.stats.tps as i64)),
            PersonalStat::TotalThreat => ("Threat", format_number(self.stats.total_threat)),
            PersonalStat::DamageCritPct => {
                ("Dmg Crit", format!("{:.1}%", self.stats.damage_crit_pct))
            }
            PersonalStat::HealCritPct => ("Heal Crit", format!("{:.1}%", self.stats.heal_crit_pct)),
            PersonalStat::EffectiveHealPct => {
                ("Eff Heal", format!("{:.1}%", self.stats.effective_heal_pct))
            }
            PersonalStat::ClassDiscipline => {
                let value = self
                    .stats
                    .class_discipline
                    .clone()
                    .unwrap_or_else(|| "Unknown".to_string());
                ("Spec", value)
            }
            PersonalStat::Phase => {
                let phase = self.stats.current_phase.as_deref().unwrap_or("");
                ("Phase", phase.to_string())
            }
            PersonalStat::PhaseTime => {
                // Only show phase time if there's an active phase
                let time_str = if self.stats.current_phase.is_some() {
                    format_time(self.stats.phase_time_secs as u64)
                } else {
                    String::new()
                };
                ("Phase Time", time_str)
            }
        }
    }

    /// Render the overlay
    pub fn render(&mut self) {
        let width = self.frame.width() as f32;

        let padding = self.frame.scaled(BASE_PADDING);
        let font_size = self.frame.scaled(BASE_FONT_SIZE);
        let line_height = self.frame.scaled(BASE_LINE_HEIGHT);

        let label_color = color_from_rgba(self.config.label_color);
        let font_color = color_from_rgba(self.config.font_color);

        // Begin frame (clear, background, border)
        self.frame.begin_frame();

        // Draw stats using LabeledValue widgets
        let mut y = padding + font_size;
        let content_width = width - padding * 2.0;

        for stat in &self.config.visible_stats {
            let (label, value) = self.stat_display(*stat);

            LabeledValue::new(label, value)
                .with_label_color(label_color)
                .with_value_color(font_color)
                .render(&mut self.frame, padding, y, content_width, font_size);

            y += line_height;
        }

        // End frame (resize indicator, commit)
        self.frame.end_frame();
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Trait Implementation
// ─────────────────────────────────────────────────────────────────────────────

impl Overlay for PersonalOverlay {
    fn update_data(&mut self, data: OverlayData) -> bool {
        if let OverlayData::Personal(stats) = data {
            self.set_stats(stats);
            true // Personal stats always render when updated
        } else {
            false
        }
    }

    fn update_config(&mut self, config: OverlayConfigUpdate) {
        if let OverlayConfigUpdate::Personal(personal_config, alpha) = config {
            self.set_config(personal_config);
            self.set_background_alpha(alpha);
        }
    }

    fn render(&mut self) {
        PersonalOverlay::render(self);
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
