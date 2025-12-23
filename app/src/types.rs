//! Frontend type definitions
//!
//! Contains types used by the Dioxus frontend, including re-exports from
//! baras-types and frontend-specific types that mirror backend structures.

use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// Re-exports from baras-types (shared with backend)
// ─────────────────────────────────────────────────────────────────────────────

pub use baras_types::{
    AppConfig, BossHealthConfig, Color, OverlayAppearanceConfig,
    OverlaySettings, PersonalOverlayConfig, PersonalStat, RaidOverlaySettings,
    MAX_PROFILES,
};

// ─────────────────────────────────────────────────────────────────────────────
// Frontend-Only Types (mirror backend structures)
// ─────────────────────────────────────────────────────────────────────────────

/// Session information from the backend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub player_name: Option<String>,
    pub player_class: Option<String>,
    pub player_discipline: Option<String>,
    pub area_name: Option<String>,
    pub in_combat: bool,
    pub encounter_count: usize,
}

/// Overlay status response from backend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayStatus {
    pub running: Vec<String>,
    pub enabled: Vec<String>,
    pub personal_running: bool,
    pub personal_enabled: bool,
    pub raid_running: bool,
    pub raid_enabled: bool,
    pub boss_health_running: bool,
    pub boss_health_enabled: bool,
    pub overlays_visible: bool,
    pub move_mode: bool,
    pub rearrange_mode: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// Metric Types
// ─────────────────────────────────────────────────────────────────────────────

/// Available metric overlay types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MetricType {
    Dps,
    EDps,
    BossDps,
    Hps,
    EHps,
    Abs,
    Dtps,
    Tps,
}

impl MetricType {
    /// Human-readable label for display
    pub fn label(&self) -> &'static str {
        match self {
            MetricType::Dps => "Damage",
            MetricType::EDps => "Effective Damage",
            MetricType::BossDps => "Boss Damage",
            MetricType::Hps => "Healing",
            MetricType::EHps => "Effective Healing",
            MetricType::Tps => "Threat",
            MetricType::Dtps => "Damage Taken",
            MetricType::Abs => "Shielding Given",
        }
    }

    /// Config key used for persistence
    pub fn config_key(&self) -> &'static str {
        match self {
            MetricType::Dps => "dps",
            MetricType::EDps => "edps",
            MetricType::BossDps => "bossdps",
            MetricType::Hps => "hps",
            MetricType::EHps => "ehps",
            MetricType::Tps => "tps",
            MetricType::Dtps => "dtps",
            MetricType::Abs => "abs",
        }
    }

    /// All metric overlay types (for iteration)
    pub fn all() -> &'static [MetricType] {
        &[
            MetricType::Dps,
            MetricType::EDps,
            MetricType::BossDps,
            MetricType::Hps,
            MetricType::EHps,
            MetricType::Abs,
            MetricType::Dtps,
            MetricType::Tps,
        ]
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Type Enum
// ─────────────────────────────────────────────────────────────────────────────

/// Unified overlay kind - matches backend OverlayType
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(tag = "type", content = "value")]
pub enum OverlayType {
    Metric(MetricType),
    Personal,
    Raid,
    BossHealth,
}
