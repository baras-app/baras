//! Personal Stats Overlay
//!
//! Displays the primary player's combat statistics as text items.
//! Supports both single-value rows (info stats, APM) and compound
//! multi-value rows (damage group, healing group, etc.).

use baras_core::context::{PersonalOverlayConfig, PersonalStat, PersonalStatCategory};

use super::{Overlay, OverlayConfigUpdate, OverlayData};
use crate::frame::OverlayFrame;
use crate::platform::{OverlayConfig, PlatformError};
use crate::utils::color_from_rgba;
use crate::widgets::colors;
use crate::widgets::{CompoundRow, CompoundValue, LabeledValue};
use baras_types::formatting;
use tiny_skia::Color;

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
    pub total_healing_effective: i64,
    pub dtps: i32,
    pub edtps: i32,
    pub total_damage_taken: i64,
    pub total_damage_taken_effective: i64,
    pub tps: i32,
    pub total_threat: i64,
    pub damage_crit_pct: f32,
    pub heal_crit_pct: f32,
    pub effective_heal_pct: f32,
    pub defense_pct: f32,
    pub shield_pct: f32,
    pub total_shield_absorbed: i64,
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
    european_number_format: bool,
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
            european_number_format: false,
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

    /// Get the display label and value for a single-value stat
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
            PersonalStat::EncounterTime => (
                "Combat Time",
                formatting::format_duration_u64(self.stats.encounter_time_secs),
            ),
            PersonalStat::EncounterCount => (
                "Session Encounters",
                format!("{}", self.stats.encounter_count),
            ),
            PersonalStat::ClassDiscipline => {
                let value = self
                    .stats
                    .class_discipline
                    .clone()
                    .unwrap_or_else(|| "Unknown".to_string());
                ("Spec", value)
            }
            PersonalStat::Apm => (
                "APM",
                formatting::format_f32_1(self.stats.apm, self.european_number_format),
            ),
            // Compound stats should not be called here, but handle gracefully
            _ => (stat.label(), String::new()),
        }
    }

    /// Get the compound values for a group stat
    fn compound_values(&self, stat: PersonalStat) -> (&'static str, Vec<CompoundValue>) {
        let eu = self.european_number_format;
        match stat {
            PersonalStat::DamageGroup => (
                "Damage",
                vec![
                    CompoundValue::new(formatting::format_compact(self.stats.dps as i64, eu)),
                    CompoundValue::new(formatting::format_compact(self.stats.total_damage, eu)),
                    CompoundValue::new(formatting::format_pct_f32(self.stats.damage_crit_pct, eu))
                        .with_prefix("Crit:"),
                ],
            ),
            PersonalStat::BossDamageGroup => (
                "Boss Dmg",
                vec![
                    CompoundValue::new(formatting::format_compact(self.stats.bossdps as i64, eu)),
                    CompoundValue::new(formatting::format_compact(
                        self.stats.total_damage_boss,
                        eu,
                    )),
                ],
            ),
            PersonalStat::HealingGroup => (
                "HPS",
                vec![
                    CompoundValue::new(formatting::format_compact(self.stats.hps as i64, eu)),
                    CompoundValue::new(formatting::format_compact(self.stats.ehps as i64, eu)),
                    CompoundValue::new(formatting::format_pct_f32(
                        self.stats.effective_heal_pct,
                        eu,
                    ))
                    .with_prefix("Eff:"),
                ],
            ),
            PersonalStat::HealingAdvanced => (
                "Total Heal",
                vec![
                    CompoundValue::new(formatting::format_compact(self.stats.total_healing, eu)),
                    CompoundValue::new(formatting::format_compact(
                        self.stats.total_healing_effective,
                        eu,
                    )),
                    CompoundValue::new(formatting::format_pct_f32(self.stats.heal_crit_pct, eu))
                        .with_prefix("Crit:"),
                ],
            ),
            PersonalStat::ThreatGroup => (
                "Threat",
                vec![
                    CompoundValue::new(formatting::format_compact(self.stats.tps as i64, eu)),
                    CompoundValue::new(formatting::format_compact(self.stats.total_threat, eu)),
                ],
            ),
            PersonalStat::MitigationGroup => (
                "DTPS",
                vec![
                    CompoundValue::new(formatting::format_compact(self.stats.edtps as i64, eu)),
                    CompoundValue::new(formatting::format_compact(
                        self.stats.total_damage_taken_effective,
                        eu,
                    )),
                ],
            ),
            PersonalStat::DefensiveGroup => (
                "Defense",
                vec![
                    CompoundValue::new(formatting::format_pct_f32(self.stats.defense_pct, eu))
                        .with_prefix("Def:"),
                    CompoundValue::new(formatting::format_pct_f32(self.stats.shield_pct, eu))
                        .with_prefix("Shld:"),
                ],
            ),
            PersonalStat::PhaseGroup => {
                let phase = self
                    .stats
                    .current_phase
                    .as_deref()
                    .unwrap_or("")
                    .to_string();
                let time = if self.stats.current_phase.is_some() {
                    formatting::format_duration_u64(self.stats.phase_time_secs as u64)
                } else {
                    String::new()
                };
                (
                    "Phase",
                    vec![CompoundValue::new(phase), CompoundValue::new(time)],
                )
            }
            // Single-value stats should not be called here
            _ => (stat.label(), Vec::new()),
        }
    }

    /// Check if a stat currently has an empty/zero value
    fn is_stat_empty(&self, stat: PersonalStat) -> bool {
        match stat {
            PersonalStat::EncounterName => self
                .stats
                .encounter_name
                .as_deref()
                .unwrap_or("")
                .is_empty(),
            PersonalStat::Difficulty => self.stats.difficulty.is_none(),
            PersonalStat::EncounterTime => self.stats.encounter_time_secs == 0,
            PersonalStat::EncounterCount => false, // always meaningful
            PersonalStat::ClassDiscipline => self.stats.class_discipline.is_none(),
            PersonalStat::Apm => self.stats.apm == 0.0,

            // Compound groups: empty when all primary values are zero
            PersonalStat::DamageGroup => self.stats.dps == 0 && self.stats.total_damage == 0,
            PersonalStat::BossDamageGroup => {
                self.stats.bossdps == 0 && self.stats.total_damage_boss == 0
            }
            PersonalStat::HealingGroup => self.stats.hps == 0 && self.stats.ehps == 0,
            PersonalStat::HealingAdvanced => {
                self.stats.total_healing == 0 && self.stats.total_healing_effective == 0
            }
            PersonalStat::ThreatGroup => self.stats.tps == 0 && self.stats.total_threat == 0,
            PersonalStat::MitigationGroup => {
                self.stats.edtps == 0 && self.stats.total_damage_taken_effective == 0
            }
            PersonalStat::DefensiveGroup => {
                self.stats.defense_pct == 0.0 && self.stats.shield_pct == 0.0
            }
            PersonalStat::PhaseGroup => {
                self.stats.current_phase.as_deref().unwrap_or("").is_empty()
            }

            // Separators are never empty
            PersonalStat::Separator => false,

            // Legacy no-ops — treat as empty so they get hidden
            _ => true,
        }
    }

    /// Get the auto-color for a stat based on its category
    fn category_color(stat: PersonalStat, fallback: Color) -> Color {
        match stat.category() {
            PersonalStatCategory::Damage => colors::stat_damage(),
            PersonalStatCategory::Healing => colors::stat_healing(),
            PersonalStatCategory::Mitigation => colors::stat_mitigation(),
            PersonalStatCategory::Threat => colors::stat_threat(),
            PersonalStatCategory::Defensive => colors::stat_threat(), // blue like threat
            PersonalStatCategory::Utility => fallback,
            PersonalStatCategory::Info => fallback,
        }
    }

    /// Render the overlay
    pub fn render(&mut self) {
        let width = self.frame.width() as f32;

        let padding = self.frame.scaled(BASE_PADDING);
        let font_scale = self.config.font_scale.clamp(1.0, 2.0);
        let font_size = self.frame.scaled(BASE_FONT_SIZE * font_scale);
        let line_spacing = self.config.line_spacing.clamp(0.7, 1.5);
        let line_height = self.frame.scaled(BASE_LINE_HEIGHT) * line_spacing;
        let separator_height = line_height * 0.5;

        let label_color = color_from_rgba(self.config.label_color);
        let font_color = color_from_rgba(self.config.font_color);

        // Compute content height for dynamic background, accounting for hidden stats
        let mut content_height = padding + font_size;
        for stat in &self.config.visible_stats {
            // Skip legacy no-op variants
            if stat.is_legacy() {
                continue;
            }
            if *stat == PersonalStat::Separator {
                content_height += separator_height;
            } else if !self.config.hide_empty_values || !self.is_stat_empty(*stat) {
                content_height += line_height;
            }
        }
        content_height += padding;

        // Begin frame (clear, background, border)
        if self.config.dynamic_background {
            self.frame.begin_frame_with_content_height(content_height);
        } else {
            self.frame.begin_frame();
        }

        // Draw stats
        let mut y = padding + font_size;
        let content_width = width - padding * 2.0;
        let row_fs = font_size * 0.85;
        // Track the smallest font size used by centered info text (encounter name, difficulty, etc.)
        // so subsequent info rows don't render larger than an earlier one that had to shrink.
        let mut info_font_ceiling = font_size;

        for stat in &self.config.visible_stats {
            // Skip legacy no-op variants
            if stat.is_legacy() {
                continue;
            }

            // Handle separator
            if *stat == PersonalStat::Separator {
                // Draw line 85% down from the top of the separator section
                let line_y = y - line_height + separator_height * 0.85;
                self.frame.fill_rect(
                    padding,
                    line_y,
                    content_width,
                    2.0,
                    colors::separator_line(),
                );
                y += separator_height;
                continue;
            }

            // Skip empty values if configured
            if self.config.hide_empty_values && self.is_stat_empty(*stat) {
                continue;
            }

            // Determine colors
            let is_info = stat.is_info();
            let value_color = if self.config.auto_color_values && !is_info {
                Self::category_color(*stat, font_color)
            } else {
                font_color
            };

            if stat.is_compound() {
                // Compound multi-value row
                let (label, values) = self.compound_values(*stat);
                CompoundRow::new(label, values)
                    .with_label_color(label_color)
                    .with_value_color(value_color)
                    .with_text_glow()
                    .render(&mut self.frame, padding, y, content_width, row_fs);
            } else {
                // Single-value row
                let (label, value) = self.stat_display(*stat);

                if is_info && label.is_empty() {
                    // Centered info text (encounter name, difficulty) — auto-scale to fit.
                    // Also cap to info_font_ceiling so later rows never exceed an
                    // earlier row that had to shrink (e.g., difficulty <= encounter name).
                    let (text_w, _) = self
                        .frame
                        .measure_text_styled(&value, font_size, true, false);
                    let fit_fs = if text_w > content_width && text_w > 0.0 {
                        (font_size * content_width / text_w).max(font_size * 0.55)
                    } else {
                        font_size
                    };
                    let actual_fs = fit_fs.min(info_font_ceiling);
                    info_font_ceiling = actual_fs;
                    // Center horizontally
                    let (fitted_w, _) = self
                        .frame
                        .measure_text_styled(&value, actual_fs, true, false);
                    let cx = padding + (content_width - fitted_w) * 0.5;
                    self.frame.draw_text_with_glow(
                        &value,
                        cx,
                        y,
                        actual_fs,
                        value_color,
                        true,
                        false,
                    );
                } else {
                    // Metric single-value rows (APM, Combat Time, etc.)
                    LabeledValue::new(label, value)
                        .with_label_color(label_color)
                        .with_value_color(value_color)
                        .with_label_bold(false)
                        .with_value_bold(true)
                        .with_text_glow()
                        .render(&mut self.frame, padding, y, content_width, row_fs);
                }
            }

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
        if let OverlayConfigUpdate::Personal(personal_config, alpha, european) = config {
            self.set_config(personal_config);
            self.set_background_alpha(alpha);
            self.european_number_format = european;
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
