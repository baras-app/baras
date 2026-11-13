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
use crate::widgets::{CompoundLayout, CompoundRow, CompoundValue, LabeledValue};
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
    /// Effective healing received per second / total
    pub ehtps: i32,
    pub total_healing_received_effective: i64,
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

/// Base dimensions for scaling calculations (text scales with width only;
/// BASE_HEIGHT only feeds the frame's default scale_factor)
const BASE_WIDTH: f32 = 200.0;
const BASE_HEIGHT: f32 = 180.0;
const BASE_FONT_SIZE: f32 = 13.0;
const BASE_LINE_HEIGHT: f32 = 18.0;
const BASE_PADDING: f32 = 8.0;
const BASE_SEPARATOR_THICKNESS: f32 = 2.0;
/// Metric row font relative to the base font size
const ROW_FONT_RATIO: f32 = 0.85;
/// Heading text (encounter, difficulty, spec) relative to metric rows
const HEADING_RATIO: f32 = 1.1;
/// Minimum line height relative to row font size before rows would overlap
const MIN_LINE_RATIO: f32 = 1.1;
/// Smallest size heading text shrinks to before truncating
const MIN_INFO_RATIO: f32 = 0.55;
/// Size of single-letter value prefixes ("e" effective, "d" defense, "s" shield)
const LETTER_PREFIX_SCALE: f32 = 0.7;

/// One drawable row after filtering
enum Row {
    Separator,
    /// Label-less info text (encounter name, difficulty, spec), left-aligned and bold
    Heading(String, Color),
    Labeled(LabeledValue),
    /// `true` when the row shares the column grid with other compound rows
    Compound(CompoundRow, bool),
}

/// Personal stats overlay showing player metrics as text
pub struct PersonalOverlay {
    frame: OverlayFrame,
    config: PersonalOverlayConfig,
    stats: PersonalStats,
    /// Fed by operation timer ticks, independent of combat metric updates
    operation_time_secs: u64,
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
            operation_time_secs: 0,
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
                ("", self.stats.difficulty.clone().unwrap_or_default())
            }
            PersonalStat::EncounterTime => (
                "Combat Time",
                formatting::format_duration_u64(self.stats.encounter_time_secs),
            ),
            PersonalStat::OperationTime => (
                "Op Time",
                formatting::format_duration_u64(self.operation_time_secs),
            ),
            PersonalStat::EncounterCount => {
                ("Encounters", format!("{}", self.stats.encounter_count))
            }
            PersonalStat::ClassDiscipline => {
                let value = self
                    .stats
                    .class_discipline
                    .clone()
                    .unwrap_or_else(|| "Unknown".to_string());
                ("", value)
            }
            PersonalStat::Apm => (
                "APM",
                formatting::format_f32_1(self.stats.apm, self.european_number_format),
            ),
            PersonalStat::DamageCritPct => (
                "Dmg Crit",
                formatting::format_pct_f32(self.stats.damage_crit_pct, self.european_number_format),
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
                    CompoundValue::new(formatting::format_compact(self.stats.total_damage, eu)),
                    CompoundValue::new(formatting::format_compact(self.stats.dps as i64, eu)),
                ],
            ),
            PersonalStat::BossDamageGroup => (
                "Boss Dmg",
                vec![
                    CompoundValue::new(formatting::format_compact(
                        self.stats.total_damage_boss,
                        eu,
                    )),
                    CompoundValue::new(formatting::format_compact(self.stats.bossdps as i64, eu)),
                ],
            ),
            PersonalStat::HealingGroup => (
                "HPS",
                vec![
                    CompoundValue::new(formatting::format_compact(self.stats.hps as i64, eu)),
                    CompoundValue::new(formatting::format_compact(self.stats.ehps as i64, eu))
                        .with_prefix("e")
                        .with_prefix_scale(LETTER_PREFIX_SCALE),
                ],
            ),
            PersonalStat::HealingAdvanced => (
                "Total Heal",
                vec![
                    CompoundValue::new(formatting::format_compact(self.stats.total_healing, eu)),
                    CompoundValue::new(formatting::format_compact(
                        self.stats.total_healing_effective,
                        eu,
                    ))
                    .with_prefix("e")
                    .with_prefix_scale(LETTER_PREFIX_SCALE),
                ],
            ),
            PersonalStat::HealPctGroup => (
                "Heal %",
                vec![
                    CompoundValue::new(formatting::format_pct_f32(
                        self.stats.effective_heal_pct,
                        eu,
                    ))
                    .with_prefix("e")
                    .with_prefix_scale(LETTER_PREFIX_SCALE),
                    CompoundValue::new(formatting::format_pct_f32(self.stats.heal_crit_pct, eu))
                        .with_prefix("*"),
                ],
            ),
            PersonalStat::HealingTakenGroup => (
                "Heal Taken",
                vec![
                    CompoundValue::new(formatting::format_compact(
                        self.stats.total_healing_received_effective,
                        eu,
                    )),
                    CompoundValue::new(formatting::format_compact(self.stats.ehtps as i64, eu)),
                ],
            ),
            PersonalStat::ThreatGroup => (
                "Threat",
                vec![
                    CompoundValue::new(formatting::format_compact(self.stats.total_threat, eu)),
                    CompoundValue::new(formatting::format_compact(self.stats.tps as i64, eu)),
                ],
            ),
            PersonalStat::MitigationGroup => (
                "DTPS",
                vec![
                    CompoundValue::new(formatting::format_compact(
                        self.stats.total_damage_taken_effective,
                        eu,
                    )),
                    CompoundValue::new(formatting::format_compact(self.stats.edtps as i64, eu)),
                ],
            ),
            PersonalStat::DefensiveGroup => (
                "Defense",
                vec![
                    CompoundValue::new(formatting::format_pct_f32(self.stats.defense_pct, eu))
                        .with_prefix("d")
                        .with_prefix_scale(LETTER_PREFIX_SCALE),
                    CompoundValue::new(formatting::format_pct_f32(self.stats.shield_pct, eu))
                        .with_prefix("s")
                        .with_prefix_scale(LETTER_PREFIX_SCALE),
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
            PersonalStat::OperationTime => self.operation_time_secs == 0,
            PersonalStat::DamageCritPct => self.stats.damage_crit_pct == 0.0,

            // Compound groups: empty when all primary values are zero
            PersonalStat::DamageGroup => self.stats.dps == 0 && self.stats.total_damage == 0,
            PersonalStat::BossDamageGroup => {
                self.stats.bossdps == 0 && self.stats.total_damage_boss == 0
            }
            PersonalStat::HealingGroup => self.stats.hps == 0 && self.stats.ehps == 0,
            PersonalStat::HealingAdvanced => {
                self.stats.total_healing == 0 && self.stats.total_healing_effective == 0
            }
            PersonalStat::HealPctGroup => {
                self.stats.effective_heal_pct == 0.0 && self.stats.heal_crit_pct == 0.0
            }
            PersonalStat::HealingTakenGroup => {
                self.stats.ehtps == 0 && self.stats.total_healing_received_effective == 0
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

    /// Resolve visible stats into drawable rows. Drops legacy and hidden stats,
    /// plus separators that would lead the list or double up (e.g. when every
    /// row between two separators is hidden).
    fn build_rows(&self) -> Vec<Row> {
        let label_color = color_from_rgba(self.config.label_color);
        let font_color = color_from_rgba(self.config.font_color);
        let mut rows = Vec::with_capacity(self.config.visible_stats.len());

        for &stat in &self.config.visible_stats {
            if stat.is_legacy() {
                continue;
            }
            if stat == PersonalStat::Separator {
                if matches!(rows.last(), Some(r) if !matches!(r, Row::Separator)) {
                    rows.push(Row::Separator);
                }
                continue;
            }
            // No difficulty (open world) is always hidden: the encounter name already says it
            if (self.config.hide_empty_values || stat == PersonalStat::Difficulty)
                && self.is_stat_empty(stat)
            {
                continue;
            }

            let value_color = if self.config.auto_color_values && !stat.is_info() {
                Self::category_color(stat, font_color)
            } else {
                font_color
            };

            let row = if stat.is_compound() {
                let (label, values) = self.compound_values(stat);
                let row = CompoundRow::new(label, values)
                    .with_label_color(label_color)
                    .with_value_color(value_color);
                // Phase names vary wildly in width; keep them out of the shared grid
                Row::Compound(row, stat != PersonalStat::PhaseGroup)
            } else {
                let (label, value) = self.stat_display(stat);
                if label.is_empty() {
                    Row::Heading(value, value_color)
                } else {
                    Row::Labeled(
                        LabeledValue::new(label, value)
                            .with_label_color(label_color)
                            .with_value_color(value_color)
                            .with_label_bold(false),
                    )
                }
            };
            rows.push(row);
        }
        rows
    }

    /// Render the overlay
    pub fn render(&mut self) {
        // Width-only scaling: rows are laid out horizontally, so a taller
        // window adds rows of room, not bigger text.
        let width = self.frame.width() as f32;
        let scale = width / BASE_WIDTH;
        let padding = BASE_PADDING * scale;
        let content_width = width - padding * 2.0;
        let font_scale = self.config.font_scale.clamp(0.3, 3.0);
        let font_size = BASE_FONT_SIZE * font_scale * scale;
        let row_fs = font_size * ROW_FONT_RATIO;
        let line_spacing = self.config.line_spacing.clamp(0.7, 1.5);
        // Only grows past the configured spacing when text would overlap the next row
        let line_height = (BASE_LINE_HEIGHT * scale * line_spacing).max(row_fs * MIN_LINE_RATIO);
        let separator_height = line_height * 0.5;
        let separator_thickness = (BASE_SEPARATOR_THICKNESS * scale).max(1.0);

        let rows = self.build_rows();

        let grid = CompoundLayout::new(
            &mut self.frame,
            rows.iter().filter_map(|r| match r {
                Row::Compound(c, true) => Some(c),
                _ => None,
            }),
            content_width,
            row_fs,
        );

        // Every row follows the grid's scale so metric rows render at one size
        // and heading text keeps a fixed ratio to them
        let metric_fs = row_fs * grid.scale();
        let heading_fs = metric_fs * HEADING_RATIO;

        // Heading rows share one font size: the smallest any of them needs to fit
        let mut info_fs = heading_fs;
        for row in &rows {
            if let Row::Heading(text, _) = row {
                let (w, _) = self.frame.measure_text_styled(text, heading_fs, true, false);
                if w > content_width {
                    info_fs = info_fs.min(
                        (heading_fs * content_width / w).max(heading_fs * MIN_INFO_RATIO),
                    );
                }
            }
        }

        // Layout uses each row's nominal font so positions don't jitter when
        // heading text shrinks to fit.
        let nominal_fs = |row: &Row| match row {
            Row::Heading(..) => heading_fs,
            _ => metric_fs,
        };

        // Y per row: text baseline, or top edge for separators
        let mut ys = Vec::with_capacity(rows.len());
        let mut y = padding + rows.first().map_or(heading_fs, nominal_fs);
        let mut bottom = padding;
        for row in &rows {
            if matches!(row, Row::Separator) {
                // Center the line between the previous row's descenders and
                // the next row's cap height (y is the next row's baseline
                // before the separator advance).
                let line_y = y - line_height + (line_height + separator_height) * 0.5
                    - metric_fs * 0.25
                    - separator_thickness * 0.5;
                ys.push(line_y);
                bottom = line_y + separator_thickness;
                y += separator_height;
            } else {
                ys.push(y);
                bottom = y + nominal_fs(row) * 0.3;
                y += line_height;
            }
        }

        if self.config.dynamic_background {
            self.frame.begin_frame_with_content_height(bottom + padding);
        } else {
            self.frame.begin_frame();
        }

        for (row, y) in rows.iter().zip(ys) {
            match row {
                Row::Separator => self.frame.fill_rect(
                    padding,
                    y,
                    content_width,
                    separator_thickness,
                    colors::separator_line(),
                ),
                Row::Heading(text, color) => {
                    let text = self
                        .frame
                        .truncate_to_width(text, content_width, info_fs, true);
                    self.frame
                        .draw_text_with_glow(&text, padding, y, info_fs, *color, true, false);
                }
                Row::Labeled(lv) => {
                    lv.render(&mut self.frame, padding, y, content_width, metric_fs)
                }
                Row::Compound(c, true) => {
                    c.render(&mut self.frame, padding, y, content_width, row_fs, &grid)
                }
                Row::Compound(c, false) => {
                    let own = CompoundLayout::new(&mut self.frame, [c], content_width, metric_fs);
                    c.render(&mut self.frame, padding, y, content_width, metric_fs, &own);
                }
            }
        }

        self.frame.end_frame();
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Trait Implementation
// ─────────────────────────────────────────────────────────────────────────────

impl Overlay for PersonalOverlay {
    fn update_data(&mut self, data: OverlayData) -> bool {
        match data {
            OverlayData::Personal(stats) => {
                self.set_stats(stats);
                true
            }
            OverlayData::OperationTimer(timer) => {
                let changed = self.operation_time_secs != timer.elapsed_secs;
                self.operation_time_secs = timer.elapsed_secs;
                changed && self.config.visible_stats.contains(&PersonalStat::OperationTime)
            }
            _ => false,
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
