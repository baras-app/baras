//! Overlay type definitions
//!
//! Core enums that identify overlay types and their properties.

use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// Metric Types
// ─────────────────────────────────────────────────────────────────────────────

/// Specific metric types (DPS, HPS, etc.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MetricType {
    Dps,
    EDps,
    BossDps,
    Hps,
    EHps,
    #[serde(rename = "abs", alias = "htps")]
    Htps,
    Dtps,
    Tps,
}

impl MetricType {
    /// Display title for this overlay
    pub fn title(&self) -> &'static str {
        match self {
            MetricType::Dps => "Damage",
            MetricType::EDps => "Effective Damage",
            MetricType::BossDps => "Boss Damage",
            MetricType::Hps => "Healing",
            MetricType::EHps => "Effective Healing",
            MetricType::Tps => "Threat",
            MetricType::Dtps => "Damage Taken",
            MetricType::Htps => "Healing Taken",
        }
    }

    /// Window namespace for platform identification
    pub fn namespace(&self) -> &'static str {
        match self {
            MetricType::Dps => "baras-dps",
            MetricType::EDps => "baras-edps",
            MetricType::BossDps => "baras-boss-dps",
            MetricType::Hps => "baras-hps",
            MetricType::EHps => "baras-ehps",
            MetricType::Tps => "baras-tps",
            MetricType::Dtps => "baras-dtps",
            MetricType::Htps => "baras-abs",
        }
    }

    /// Default screen position for this overlay type
    pub fn default_position(&self) -> (i32, i32) {
        match self {
            MetricType::Dps => (50, 50),
            MetricType::EDps => (50, 50),
            MetricType::BossDps => (50, 50),
            MetricType::Hps => (50, 280),
            MetricType::EHps => (50, 280),
            MetricType::Tps => (50, 510),
            MetricType::Dtps => (350, 50),
            MetricType::Htps => (350, 280),
        }
    }

    /// All overlay types
    pub fn all() -> &'static [MetricType] {
        &[
            MetricType::Dps,
            MetricType::EDps,
            MetricType::BossDps,
            MetricType::Hps,
            MetricType::EHps,
            MetricType::Htps,
            MetricType::Dtps,
            MetricType::Tps,
        ]
    }

    /// Config key for position/settings storage
    pub fn config_key(&self) -> &'static str {
        match self {
            MetricType::Dps => "dps",
            MetricType::EDps => "edps",
            MetricType::BossDps => "bossdps",
            MetricType::Hps => "hps",
            MetricType::EHps => "ehps",
            MetricType::Tps => "tps",
            MetricType::Dtps => "dtps",
            MetricType::Htps => "abs",
        }
    }

    /// Parse from config key string
    pub fn from_config_key(key: &str) -> Option<Self> {
        match key {
            "dps" => Some(MetricType::Dps),
            "edps" => Some(MetricType::EDps),
            "bossdps" => Some(MetricType::BossDps),
            "hps" => Some(MetricType::Hps),
            "ehps" => Some(MetricType::EHps),
            "tps" => Some(MetricType::Tps),
            "dtps" => Some(MetricType::Dtps),
            "abs" => Some(MetricType::Htps),
            _ => None,
        }
    }

    /// Get default appearance config with the correct bar color for this type.
    /// Uses baras_core::context::overlay_colors as the single source of truth.
    pub fn default_appearance(&self) -> baras_core::context::OverlayAppearanceConfig {
        baras_core::context::OverlayAppearanceConfig::default_for_type(self.config_key())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Unified Overlay Kind
// ─────────────────────────────────────────────────────────────────────────────

/// Unified overlay kind - covers all overlay types including personal
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum OverlayType {
    /// A metric overlay (DPS, HPS, etc.)
    Metric(MetricType),
    /// The personal stats overlay
    Personal,
    /// The raid frames overlay (shows effects/HoTs on group members)
    Raid,
    /// The boss health bar overlay
    BossHealth,
    /// Timer A countdown bars (default)
    TimersA,
    /// Timer B countdown bars
    TimersB,
    /// Challenge metrics overlay
    Challenges,
    /// Alert text notifications
    Alerts,
    /// Effects A overlay (personal effects)
    EffectsA,
    /// Effects B overlay (personal effects)
    EffectsB,
    /// Ability cooldowns
    Cooldowns,
    /// DOTs on enemy targets
    DotTracker,
    /// Encounter notes overlay (Markdown)
    Notes,
    /// Standalone combat time display
    CombatTime,
    /// Persistent operation timer (tracks entire raid run)
    OperationTimer,
}

impl OverlayType {
    /// Get the config key for this overlay kind
    pub fn config_key(&self) -> &'static str {
        match self {
            OverlayType::Metric(ot) => ot.config_key(),
            OverlayType::Personal => "personal",
            OverlayType::Raid => "raid",
            OverlayType::BossHealth => "boss_health",
            OverlayType::TimersA => "timers_a",
            OverlayType::TimersB => "timers_b",
            OverlayType::Challenges => "challenges",
            OverlayType::Alerts => "alerts",
            OverlayType::EffectsA => "effects_a",
            OverlayType::EffectsB => "effects_b",
            OverlayType::Cooldowns => "cooldowns",
            OverlayType::DotTracker => "dot_tracker",
            OverlayType::Notes => "notes",
            OverlayType::CombatTime => "combat_time",
            OverlayType::OperationTimer => "operation_timer",
        }
    }

    /// Get the namespace for window identification
    pub fn namespace(&self) -> String {
        match self {
            OverlayType::Metric(ot) => ot.namespace().to_string(),
            OverlayType::Personal => "baras-personal".to_string(),
            OverlayType::Raid => "baras-raid".to_string(),
            OverlayType::BossHealth => "baras-boss-health".to_string(),
            OverlayType::TimersA => "baras-timers".to_string(), // Keep "baras-timers" for backward compat
            OverlayType::TimersB => "baras-timers-b".to_string(),
            OverlayType::Challenges => "baras-challenges".to_string(),
            OverlayType::Alerts => "baras-alerts".to_string(),
            OverlayType::EffectsA => "baras-effects-a".to_string(),
            OverlayType::EffectsB => "baras-effects-b".to_string(),
            OverlayType::Cooldowns => "baras-cooldowns".to_string(),
            OverlayType::DotTracker => "baras-dot-tracker".to_string(),
            OverlayType::Notes => "baras-notes".to_string(),
            OverlayType::CombatTime => "baras-combat-time".to_string(),
            OverlayType::OperationTimer => "baras-operation-timer".to_string(),
        }
    }

    /// Get default position
    pub fn default_position(&self) -> (i32, i32) {
        match self {
            OverlayType::Metric(ot) => ot.default_position(),
            OverlayType::Personal => (350, 510),
            OverlayType::Raid => (650, 50),
            OverlayType::BossHealth => (650, 400),
            OverlayType::TimersA => (650, 550),
            OverlayType::TimersB => (650, 700),
            OverlayType::Challenges => (950, 50),
            OverlayType::Alerts => (950, 400),
            OverlayType::EffectsA => (350, 200),
            OverlayType::EffectsB => (350, 280),
            OverlayType::Cooldowns => (50, 500),
            OverlayType::DotTracker => (50, 650),
            OverlayType::Notes => (950, 550),
            OverlayType::CombatTime => (400, 100),
            OverlayType::OperationTimer => (400, 160),
        }
    }
}
