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
    Apm,
    Interrupts,
    /// Serialized form must match `config_key()` — the frontend compares
    /// status lists against config keys
    #[serde(rename = "incoming_damage")]
    IncomingDamage,
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
            MetricType::Apm => "APM",
            MetricType::Interrupts => "Interrupts",
            MetricType::IncomingDamage => "Incoming Damage",
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
            MetricType::Apm => "baras-apm",
            MetricType::Interrupts => "baras-interrupts",
            MetricType::IncomingDamage => "baras-incoming-damage",
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
            MetricType::Apm => (350, 510),
            MetricType::Interrupts => (350, 740),
            MetricType::IncomingDamage => (50, 740),
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
            MetricType::Apm,
            MetricType::Interrupts,
            MetricType::IncomingDamage,
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
            MetricType::Apm => "apm",
            MetricType::Interrupts => "interrupts",
            MetricType::IncomingDamage => "incoming_damage",
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
            "apm" => Some(MetricType::Apm),
            "interrupts" => Some(MetricType::Interrupts),
            "incoming_damage" => Some(MetricType::IncomingDamage),
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
    /// Effects C overlay (personal effects)
    EffectsC,
    /// Ability cooldowns
    Cooldowns,
    /// Ability cooldowns (second tracker)
    CooldownsB,
    /// DOTs on enemy targets
    DotTracker,
    /// Encounter notes overlay (Markdown)
    Notes,
    /// Standalone combat time display
    CombatTime,
    /// Persistent operation timer (tracks entire raid run)
    OperationTimer,
    /// Ability queue overlay (GCD bar + queued/ready + active countdowns)
    AbilityQueue,
    /// Enemy HP frames (PvP)
    EnemyFrames,
    /// Recently cast abilities (local player)
    AbilityCast,
}

impl OverlayType {
    /// Overlays with live countdowns, redrawn at `OverlaySettings::overlay_fps`
    /// while locked; the rest poll at a fixed 10 per second
    pub fn is_animated(&self) -> bool {
        matches!(
            self,
            Self::TimersA
                | Self::TimersB
                | Self::Alerts
                | Self::EffectsA
                | Self::EffectsB
                | Self::EffectsC
                | Self::Raid
                | Self::Cooldowns
                | Self::CooldownsB
                | Self::DotTracker
        )
    }

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
            OverlayType::EffectsC => "effects_c",
            OverlayType::Cooldowns => "cooldowns",
            OverlayType::CooldownsB => "cooldowns_b",
            OverlayType::DotTracker => "dot_tracker",
            OverlayType::Notes => "notes",
            OverlayType::CombatTime => "combat_time",
            OverlayType::OperationTimer => "operation_timer",
            OverlayType::AbilityQueue => "ability_queue",
            OverlayType::EnemyFrames => "enemy_frames",
            OverlayType::AbilityCast => "ability_cast",
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
            OverlayType::EffectsC => "baras-effects-c".to_string(),
            OverlayType::Cooldowns => "baras-cooldowns".to_string(),
            OverlayType::CooldownsB => "baras-cooldowns-b".to_string(),
            OverlayType::DotTracker => "baras-dot-tracker".to_string(),
            OverlayType::Notes => "baras-notes".to_string(),
            OverlayType::CombatTime => "baras-combat-time".to_string(),
            OverlayType::OperationTimer => "baras-operation-timer".to_string(),
            OverlayType::AbilityQueue => "baras-ability-queue".to_string(),
            OverlayType::EnemyFrames => "baras-enemy-frames".to_string(),
            OverlayType::AbilityCast => "baras-ability-cast".to_string(),
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
            OverlayType::EffectsC => (350, 360),
            OverlayType::Cooldowns => (50, 500),
            OverlayType::CooldownsB => (250, 500),
            OverlayType::DotTracker => (50, 650),
            OverlayType::Notes => (950, 550),
            OverlayType::CombatTime => (400, 100),
            OverlayType::OperationTimer => (400, 160),
            OverlayType::AbilityQueue => (650, 850),
            OverlayType::EnemyFrames => (900, 200),
            OverlayType::AbilityCast => (50, 800),
        }
    }
}
