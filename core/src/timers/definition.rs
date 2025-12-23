//! Timer definition types
//!
//! Definitions are templates loaded from TOML config files that describe
//! what timers to track and how to display them.

use serde::{Deserialize, Serialize};

use crate::effects::EntityFilter;

/// What triggers a timer to start
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum TimerTrigger {
    /// Combat starts
    CombatStart,

    /// Specific ability is cast
    AbilityCast {
        /// Ability IDs that trigger this timer
        ability_ids: Vec<u64>,
    },

    /// Effect is applied to someone
    EffectApplied {
        /// Effect IDs that trigger this timer
        effect_ids: Vec<u64>,
    },

    /// Effect is removed from someone
    EffectRemoved {
        /// Effect IDs that trigger this timer
        effect_ids: Vec<u64>,
    },

    /// Another timer expires (chaining)
    TimerExpires {
        /// ID of the timer that triggers this one
        timer_id: String,
    },

    /// Boss HP reaches a threshold
    BossHpThreshold {
        /// HP percentage (0.0 - 100.0)
        hp_percent: f32,
    },

    /// Manually triggered (for testing/debug)
    Manual,
}

/// Definition of a timer (loaded from config)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimerDefinition {
    /// Unique identifier for this timer
    pub id: String,

    /// Display name shown in overlays
    pub name: String,

    /// Whether this timer is currently enabled
    #[serde(default = "default_true")]
    pub enabled: bool,

    // ─── Trigger ────────────────────────────────────────────────────────────
    /// What causes this timer to start
    pub trigger: TimerTrigger,

    /// Source filter for trigger events
    #[serde(default)]
    pub source: EntityFilter,

    /// Target filter for trigger events
    #[serde(default)]
    pub target: EntityFilter,

    // ─── Duration ───────────────────────────────────────────────────────────
    /// Timer duration in seconds
    pub duration_secs: f32,

    /// If true, resets duration when triggered again
    #[serde(default)]
    pub can_be_refreshed: bool,

    /// Number of times this repeats after initial trigger (0 = no repeat)
    #[serde(default)]
    pub repeats: u8,

    // ─── Display ────────────────────────────────────────────────────────────
    /// Display color as RGBA
    #[serde(default = "default_timer_color")]
    pub color: [u8; 4],

    /// Show on raid frames instead of timer bar overlay?
    #[serde(default)]
    pub show_on_raid_frames: bool,

    // ─── Alerts ─────────────────────────────────────────────────────────────
    /// Alert when this many seconds remain (None = no alert)
    pub alert_at_secs: Option<f32>,

    /// Custom alert text (None = use timer name)
    pub alert_text: Option<String>,

    /// Audio file to play on alert
    pub audio_file: Option<String>,

    // ─── Chaining ───────────────────────────────────────────────────────────
    /// Timer ID to trigger when this one expires
    pub triggers_timer: Option<String>,

    // ─── Context ────────────────────────────────────────────────────────────
    /// Only active in specific encounters (empty = all)
    #[serde(default)]
    pub encounters: Vec<String>,

    /// Specific boss name (if applicable)
    pub boss: Option<String>,

    /// Active difficulties: "story", "veteran", "master"
    #[serde(default)]
    pub difficulties: Vec<String>,
}

// ═══════════════════════════════════════════════════════════════════════════
// Serde Helpers
// ═══════════════════════════════════════════════════════════════════════════

fn default_true() -> bool {
    true
}

fn default_timer_color() -> [u8; 4] {
    [200, 200, 200, 255] // Light grey
}
