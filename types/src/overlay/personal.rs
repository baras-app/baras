//! Personal stats overlay

use crate::appearance::default_font_color;
use crate::serde_defaults::{default_scaling_factor, default_true};
use crate::{Color, overlay_colors};
use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// Personal Stats
// ─────────────────────────────────────────────────────────────────────────────

/// Category for personal stats (used for auto-coloring and grouping)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PersonalStatCategory {
    /// Contextual info (encounter name, difficulty, time, spec, phase)
    Info,
    /// Damage metrics
    Damage,
    /// Healing metrics
    Healing,
    /// Damage taken / mitigation metrics
    Mitigation,
    /// Threat metrics
    Threat,
    /// Defensive tank stats (defense %, shield %)
    Defensive,
    /// Utility metrics (APM)
    Utility,
}

/// Stats that can be displayed on the personal overlay.
///
/// Compound groups (e.g., `DamageGroup`) render multiple values in a single row.
/// Info stats and APM remain as single-value rows.
///
/// Legacy individual metric variants (Dps, Hps, etc.) are preserved for config
/// compatibility but render as no-ops (skipped during display). Users should
/// manually switch to compound groups in their settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PersonalStat {
    // ── Info (single-value rows) ──
    EncounterName,
    Difficulty,
    EncounterTime,
    /// Operation timer elapsed time
    OperationTime,
    EncounterCount,
    ClassDiscipline,

    // ── Compound metric groups ──
    /// Total  |  DPS
    DamageGroup,
    /// Boss Total  |  Boss DPS
    BossDamageGroup,
    /// HPS  |  eHPS
    HealingGroup,
    /// Total  |  Total Eff
    HealingAdvanced,
    /// Effective heal %  |  Heal crit %
    HealPctGroup,
    /// Effective healing received: Total  |  Per second
    HealingTakenGroup,
    /// Total  |  TPS
    ThreatGroup,
    /// Total Taken  |  eDTPS
    MitigationGroup,
    /// Def: X%  |  Shield: X%
    DefensiveGroup,
    /// Phase  |  Time
    PhaseGroup,

    // ── Standalone metrics ──
    Apm,
    /// Damage crit %
    DamageCritPct,

    // ── Layout ──
    /// Visual separator line (not a real stat)
    Separator,

    // ── Legacy variants (no-ops, kept for config compatibility) ──
    // These deserialize from old configs but are skipped during rendering.
    // Not shown in the "add stats" UI. Users should switch to compound groups.
    Dps,
    EDps,
    BossDps,
    TotalDamage,
    BossDamage,
    Hps,
    EHps,
    TotalHealing,
    Dtps,
    Tps,
    TotalThreat,
    HealCritPct,
    EffectiveHealPct,
    Phase,
    PhaseTime,
}

impl PersonalStat {
    /// Get the display label for this stat
    pub fn label(&self) -> &'static str {
        match self {
            Self::EncounterName => "Encounter Name",
            Self::Difficulty => "Difficulty",
            Self::EncounterTime => "Duration",
            Self::OperationTime => "Operation Time",
            Self::EncounterCount => "Encounter",
            Self::ClassDiscipline => "Spec",
            Self::DamageGroup => "Damage",
            Self::BossDamageGroup => "Boss Dmg",
            Self::HealingGroup => "Healing",
            Self::HealingAdvanced => "Heal+",
            Self::HealPctGroup => "Heal %",
            Self::HealingTakenGroup => "Heal Taken",
            Self::ThreatGroup => "Threat",
            Self::MitigationGroup => "DTPS",
            Self::DefensiveGroup => "Defense",
            Self::PhaseGroup => "Phase",
            Self::Apm => "APM",
            Self::DamageCritPct => "Dmg Crit",
            Self::Separator => "── Separator ──",
            // Legacy no-ops
            Self::Dps => "DPS (legacy)",
            Self::EDps => "eDPS (legacy)",
            Self::BossDps => "Boss DPS (legacy)",
            Self::TotalDamage => "Total Damage (legacy)",
            Self::BossDamage => "Boss Damage (legacy)",
            Self::Hps => "HPS (legacy)",
            Self::EHps => "eHPS (legacy)",
            Self::TotalHealing => "Total Healing (legacy)",
            Self::Dtps => "eDTPS (legacy)",
            Self::Tps => "TPS (legacy)",
            Self::TotalThreat => "Total Threat (legacy)",
            Self::HealCritPct => "Heal Crit % (legacy)",
            Self::EffectiveHealPct => "Eff Heal % (legacy)",
            Self::Phase => "Phase (legacy)",
            Self::PhaseTime => "Phase Time (legacy)",
        }
    }

    /// Whether this stat is a legacy no-op (kept for config compatibility only)
    pub fn is_legacy(&self) -> bool {
        matches!(
            self,
            Self::Dps
                | Self::EDps
                | Self::BossDps
                | Self::TotalDamage
                | Self::BossDamage
                | Self::Hps
                | Self::EHps
                | Self::TotalHealing
                | Self::Dtps
                | Self::Tps
                | Self::TotalThreat
                | Self::HealCritPct
                | Self::EffectiveHealPct
                | Self::Phase
                | Self::PhaseTime
        )
    }

    /// Get the category for this stat (used for auto-coloring)
    pub fn category(&self) -> PersonalStatCategory {
        match self {
            Self::EncounterName
            | Self::Difficulty
            | Self::EncounterTime
            | Self::OperationTime
            | Self::EncounterCount
            | Self::ClassDiscipline => PersonalStatCategory::Info,

            Self::DamageGroup | Self::BossDamageGroup | Self::DamageCritPct => PersonalStatCategory::Damage,

            Self::HealingGroup | Self::HealingAdvanced | Self::HealPctGroup | Self::HealingTakenGroup => {
                PersonalStatCategory::Healing
            }

            Self::MitigationGroup => PersonalStatCategory::Mitigation,

            Self::ThreatGroup => PersonalStatCategory::Threat,

            Self::DefensiveGroup => PersonalStatCategory::Defensive,

            Self::PhaseGroup => PersonalStatCategory::Info,

            Self::Apm => PersonalStatCategory::Utility,

            Self::Separator => PersonalStatCategory::Info,

            // Legacy no-ops — map to their original categories
            Self::Dps | Self::EDps | Self::BossDps | Self::TotalDamage | Self::BossDamage => {
                PersonalStatCategory::Damage
            }
            Self::Hps | Self::EHps | Self::TotalHealing | Self::HealCritPct | Self::EffectiveHealPct => {
                PersonalStatCategory::Healing
            }
            Self::Dtps => PersonalStatCategory::Mitigation,
            Self::Tps | Self::TotalThreat => PersonalStatCategory::Threat,
            Self::Phase | Self::PhaseTime => PersonalStatCategory::Info,
        }
    }

    /// Whether this stat is informational context (not a combat metric)
    pub fn is_info(&self) -> bool {
        matches!(
            self,
            Self::EncounterName
                | Self::Difficulty
                | Self::EncounterTime
                | Self::OperationTime
                | Self::EncounterCount
                | Self::ClassDiscipline
        )
    }

    /// Whether this stat renders as a compound multi-value row
    pub fn is_compound(&self) -> bool {
        matches!(
            self,
            Self::DamageGroup
                | Self::BossDamageGroup
                | Self::HealingGroup
                | Self::HealingAdvanced
                | Self::HealPctGroup
                | Self::HealingTakenGroup
                | Self::ThreatGroup
                | Self::MitigationGroup
                | Self::DefensiveGroup
                | Self::PhaseGroup
        )
    }

    /// Get all stats in display order
    pub fn all() -> &'static [PersonalStat] {
        &[
            Self::EncounterName,
            Self::Difficulty,
            Self::EncounterTime,
            Self::OperationTime,
            Self::EncounterCount,
            Self::ClassDiscipline,
            Self::DamageGroup,
            Self::BossDamageGroup,
            Self::DamageCritPct,
            Self::HealingGroup,
            Self::HealingAdvanced,
            Self::HealPctGroup,
            Self::HealingTakenGroup,
            Self::ThreatGroup,
            Self::MitigationGroup,
            Self::DefensiveGroup,
            Self::PhaseGroup,
            Self::Apm,
            Self::Separator,
        ]
    }
}

/// Configuration for the personal stats overlay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonalOverlayConfig {
    #[serde(default = "default_personal_stats")]
    pub visible_stats: Vec<PersonalStat>,
    #[serde(default = "default_font_color")]
    pub font_color: Color,
    #[serde(default = "default_font_color")]
    pub label_color: Color,
    /// Font scale multiplier (0.3 - 3.0, default 1.0)
    #[serde(default = "default_scaling_factor")]
    pub font_scale: f32,
    /// When true, background shrinks to fit content instead of filling the window
    #[serde(default)]
    pub dynamic_background: bool,
    /// When true, value text is automatically colored by stat category
    /// (red for damage, green for healing, orange for mitigation, blue for threat)
    #[serde(default = "default_true")]
    pub auto_color_values: bool,
    /// Line spacing multiplier (0.7 - 1.5, default 1.0)
    #[serde(default = "default_scaling_factor")]
    pub line_spacing: f32,
    /// When true, stats with zero/empty values are hidden
    #[serde(default)]
    pub hide_empty_values: bool,
}

fn default_personal_stats() -> Vec<PersonalStat> {
    vec![
        PersonalStat::EncounterName,
        PersonalStat::Difficulty,
        PersonalStat::EncounterTime,
        PersonalStat::Separator,
        PersonalStat::DamageGroup,
        PersonalStat::HealingGroup,
        PersonalStat::MitigationGroup,
        PersonalStat::Separator,
        PersonalStat::Apm,
    ]
}

impl Default for PersonalOverlayConfig {
    fn default() -> Self {
        Self {
            visible_stats: default_personal_stats(),
            font_color: overlay_colors::WHITE,
            label_color: overlay_colors::WHITE,
            font_scale: 1.0,
            dynamic_background: false,
            auto_color_values: true,
            line_spacing: 1.0,
            hide_empty_values: false,
        }
    }
}
