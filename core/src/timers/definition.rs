//! Timer definition types
//!
//! Definitions are templates loaded from TOML config files that describe
//! what timers to track and how to display them.

use serde::{Deserialize, Serialize};

use crate::audio::AudioConfig;
use crate::boss::CounterCondition;
use crate::game_data::Difficulty;
use crate::triggers::Trigger;

// Re-export Trigger as TimerTrigger for backward compatibility during migration
pub use crate::triggers::Trigger as TimerTrigger;

/// Definition of a timer (loaded from config)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimerDefinition {
    /// Unique identifier for this timer
    pub id: String,

    /// Display name shown in overlays
    pub name: String,

    /// Whether this timer is currently enabled
    #[serde(default = "crate::serde_defaults::default_true")]
    pub enabled: bool,

    // ─── Trigger ────────────────────────────────────────────────────────────

    /// What causes this timer to start (includes source/target filters)
    pub trigger: Trigger,

    // ─── Duration ───────────────────────────────────────────────────────────

    /// Timer duration in seconds (0 = instant, use with is_alert)
    #[serde(default)]
    pub duration_secs: f32,

    /// If true, fires as instant alert (no countdown bar)
    /// When set, duration_secs defaults to 0 and timer won't appear in countdown overlay
    #[serde(default)]
    pub is_alert: bool,

    /// If true, resets duration when triggered again
    #[serde(default)]
    pub can_be_refreshed: bool,

    /// Number of times this repeats after initial trigger (0 = no repeat)
    #[serde(default)]
    pub repeats: u8,

    // ─── Display ────────────────────────────────────────────────────────────

    /// Display color as RGBA
    #[serde(default = "crate::serde_defaults::default_timer_color")]
    pub color: [u8; 4],

    /// Only show timer when remaining time is at or below this threshold (0 = always show)
    /// Useful for long timers where you only care about the final countdown
    #[serde(default)]
    pub show_at_secs: f32,

    /// Show on raid frames instead of timer bar overlay?
    #[serde(default)]
    pub show_on_raid_frames: bool,

    // ─── Alerts ─────────────────────────────────────────────────────────────

    /// Alert when this many seconds remain (None = no alert)
    pub alert_at_secs: Option<f32>,

    /// Custom alert text (None = use timer name)
    pub alert_text: Option<String>,

    // ─── Audio ───────────────────────────────────────────────────────────────

    /// Audio configuration (alerts, countdown, custom sounds)
    #[serde(default)]
    pub audio: AudioConfig,

    // ─── Chaining & Cancellation ────────────────────────────────────────────

    /// Timer ID to trigger when this one expires
    pub triggers_timer: Option<String>,

    /// Cancel this timer when this trigger fires
    pub cancel_trigger: Option<Trigger>,

    // ─── Context ────────────────────────────────────────────────────────────

    /// Area IDs for matching (primary key - more reliable than names)
    #[serde(default)]
    pub area_ids: Vec<i64>,

    /// Only active in specific encounters by name (fallback when area_ids empty)
    #[serde(default)]
    pub encounters: Vec<String>,

    /// Specific boss name (if applicable)
    pub boss: Option<String>,

    /// Active difficulties: "story", "veteran", "master"
    #[serde(default)]
    pub difficulties: Vec<String>,

    // ─── Phase/Counter Conditions (optional) ─────────────────────────────────

    /// Only active during these phases (empty = all phases)
    #[serde(default)]
    pub phases: Vec<String>,

    /// Only active when counter meets condition
    #[serde(default)]
    pub counter_condition: Option<CounterCondition>,
}

// ═══════════════════════════════════════════════════════════════════════════
// Config File Structure
// ═══════════════════════════════════════════════════════════════════════════

/// Root structure for timer config files (TOML)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TimerConfig {
    /// Timer definitions in this file
    #[serde(default, rename = "timer")]
    pub timers: Vec<TimerDefinition>,
}

impl TimerDefinition {
    /// Check if this timer matches a given ability ID
    pub fn matches_ability(&self, ability_id: u64) -> bool {
        trigger_matches_ability(&self.trigger, ability_id)
    }

    /// Check if this timer matches a given ability ID and/or name
    pub fn matches_ability_with_name(&self, ability_id: u64, ability_name: Option<&str>) -> bool {
        trigger_matches_ability_with_name(&self.trigger, ability_id, ability_name)
    }

    /// Check if this timer matches a given effect ID for apply triggers
    pub fn matches_effect_applied(&self, effect_id: u64) -> bool {
        trigger_matches_effect_applied(&self.trigger, effect_id)
    }

    /// Check if this timer matches a given effect ID for remove triggers
    pub fn matches_effect_removed(&self, effect_id: u64) -> bool {
        trigger_matches_effect_removed(&self.trigger, effect_id)
    }

    /// Check if this timer is triggered by another timer expiring
    pub fn matches_timer_expires(&self, timer_id: &str) -> bool {
        trigger_matches_timer_expires(&self.trigger, timer_id)
    }

    /// Check if this timer triggers on combat start
    pub fn triggers_on_combat_start(&self) -> bool {
        self.trigger.contains_combat_start()
    }

    /// Check if this timer triggers when boss HP crosses below a threshold
    pub fn matches_boss_hp_threshold(
        &self,
        npc_id: i64,
        npc_name: Option<&str>,
        previous_hp: f32,
        current_hp: f32,
    ) -> bool {
        trigger_matches_boss_hp(&self.trigger, npc_id, npc_name, previous_hp, current_hp)
    }

    /// Check if this timer triggers on a specific phase being entered
    pub fn matches_phase_entered(&self, phase_id: &str) -> bool {
        trigger_matches_phase_entered(&self.trigger, phase_id)
    }

    /// Check if this timer triggers when a specific phase ends
    pub fn matches_phase_ended(&self, phase_id: &str) -> bool {
        trigger_matches_phase_ended(&self.trigger, phase_id)
    }

    /// Check if this timer triggers when a counter reaches a value
    pub fn matches_counter_reaches(&self, counter_id: &str, old_value: u32, new_value: u32) -> bool {
        trigger_matches_counter_reaches(&self.trigger, counter_id, old_value, new_value)
    }

    /// Check if this timer triggers when an NPC first appears
    pub fn matches_npc_appears(&self, npc_id: i64, entity_name: Option<&str>) -> bool {
        trigger_matches_npc_appears(&self.trigger, npc_id, entity_name)
    }

    /// Check if this timer triggers on entity death
    pub fn matches_entity_death(&self, npc_id: i64, entity_name: Option<&str>) -> bool {
        trigger_matches_entity_death(&self.trigger, npc_id, entity_name)
    }

    /// Check if this timer triggers at a specific combat time
    pub fn matches_time_elapsed(&self, old_combat_secs: f32, new_combat_secs: f32) -> bool {
        trigger_matches_time_elapsed(&self.trigger, old_combat_secs, new_combat_secs)
    }

    /// Check if this timer triggers when an NPC sets its target
    pub fn matches_target_set(&self, source_npc_id: i64, source_name: Option<&str>) -> bool {
        trigger_matches_target_set(&self.trigger, source_npc_id, source_name)
    }

    /// Check if this timer triggers when damage is taken from an ability
    pub fn matches_damage_taken(&self, ability_id: u64, ability_name: Option<&str>) -> bool {
        trigger_matches_damage_taken(&self.trigger, ability_id, ability_name)
    }

    /// Check if this timer is active for a given encounter context
    pub fn is_active_for_context(
        &self,
        area_id: Option<i64>,
        encounter: Option<&str>,
        boss: Option<&str>,
        difficulty: Option<Difficulty>,
    ) -> bool {
        // Check area filter - prefer area_ids (numeric) over encounters (string)
        if !self.area_ids.is_empty() {
            if let Some(id) = area_id {
                if !self.area_ids.contains(&id) {
                    return false;
                }
            } else {
                return false;
            }
        } else if !self.encounters.is_empty() {
            if let Some(enc) = encounter {
                if !self.encounters.iter().any(|e| e.eq_ignore_ascii_case(enc)) {
                    return false;
                }
            } else {
                return false;
            }
        }

        // Check boss filter
        if let Some(timer_boss) = &self.boss {
            if let Some(current_boss) = boss {
                if !timer_boss.eq_ignore_ascii_case(current_boss) {
                    return false;
                }
            } else {
                return false;
            }
        }

        // Check difficulty filter
        if !self.difficulties.is_empty()
            && let Some(diff) = difficulty
        {
            if !self.difficulties.iter().any(|d| diff.matches_config_key(d)) {
                return false;
            }
        }

        true
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Trigger Matching Functions
// ═══════════════════════════════════════════════════════════════════════════

/// Check if trigger matches ability cast (handles AnyOf recursively)
pub fn trigger_matches_ability(trigger: &Trigger, ability_id: u64) -> bool {
    trigger_matches_ability_with_name(trigger, ability_id, None)
}

/// Check if trigger matches ability cast with optional name (handles AnyOf recursively)
pub fn trigger_matches_ability_with_name(
    trigger: &Trigger,
    ability_id: u64,
    ability_name: Option<&str>,
) -> bool {
    match trigger {
        Trigger::AbilityCast { abilities, .. } => {
            abilities.is_empty() || abilities.iter().any(|s| s.matches(ability_id, ability_name))
        }
        Trigger::AnyOf { conditions } => conditions
            .iter()
            .any(|c| trigger_matches_ability_with_name(c, ability_id, ability_name)),
        _ => false,
    }
}

/// Check if trigger matches effect applied (handles AnyOf recursively)
pub fn trigger_matches_effect_applied(trigger: &Trigger, effect_id: u64) -> bool {
    trigger_matches_effect_applied_with_name(trigger, effect_id, None)
}

/// Check if trigger matches effect applied with optional name (handles AnyOf recursively)
pub fn trigger_matches_effect_applied_with_name(
    trigger: &Trigger,
    effect_id: u64,
    effect_name: Option<&str>,
) -> bool {
    match trigger {
        Trigger::EffectApplied { effects, .. } => {
            effects.is_empty() || effects.iter().any(|s| s.matches(effect_id, effect_name))
        }
        Trigger::AnyOf { conditions } => conditions
            .iter()
            .any(|c| trigger_matches_effect_applied_with_name(c, effect_id, effect_name)),
        _ => false,
    }
}

/// Check if trigger matches effect removed (handles AnyOf recursively)
pub fn trigger_matches_effect_removed(trigger: &Trigger, effect_id: u64) -> bool {
    trigger_matches_effect_removed_with_name(trigger, effect_id, None)
}

/// Check if trigger matches effect removed with optional name (handles AnyOf recursively)
pub fn trigger_matches_effect_removed_with_name(
    trigger: &Trigger,
    effect_id: u64,
    effect_name: Option<&str>,
) -> bool {
    match trigger {
        Trigger::EffectRemoved { effects, .. } => {
            effects.is_empty() || effects.iter().any(|s| s.matches(effect_id, effect_name))
        }
        Trigger::AnyOf { conditions } => conditions
            .iter()
            .any(|c| trigger_matches_effect_removed_with_name(c, effect_id, effect_name)),
        _ => false,
    }
}

/// Check if trigger matches timer expiration (handles AnyOf recursively)
pub fn trigger_matches_timer_expires(trigger: &Trigger, timer_id: &str) -> bool {
    match trigger {
        Trigger::TimerExpires { timer_id: trigger_id } => trigger_id == timer_id,
        Trigger::AnyOf { conditions } => {
            conditions.iter().any(|c| trigger_matches_timer_expires(c, timer_id))
        }
        _ => false,
    }
}

/// Check if trigger matches boss HP threshold crossing (handles AnyOf recursively)
pub fn trigger_matches_boss_hp(
    trigger: &Trigger,
    npc_id: i64,
    npc_name: Option<&str>,
    previous_hp: f32,
    current_hp: f32,
) -> bool {
    match trigger {
        Trigger::BossHpBelow { hp_percent, entity } => {
            // Check HP threshold crossing
            let crossed = previous_hp > *hp_percent && current_hp <= *hp_percent;
            if !crossed {
                return false;
            }

            // Check entity filter (if specified)
            if entity.is_empty() {
                return true; // No filter = any boss
            }

            // Check NPC ID first, then name
            if entity.matches_npc_id(npc_id) {
                return true;
            }
            if let Some(name) = npc_name {
                return entity.matches_name(name);
            }
            false
        }
        Trigger::AnyOf { conditions } => conditions
            .iter()
            .any(|c| trigger_matches_boss_hp(c, npc_id, npc_name, previous_hp, current_hp)),
        _ => false,
    }
}

/// Check if trigger matches phase entered (handles AnyOf recursively)
pub fn trigger_matches_phase_entered(trigger: &Trigger, phase_id: &str) -> bool {
    match trigger {
        Trigger::PhaseEntered { phase_id: trigger_phase } => trigger_phase == phase_id,
        Trigger::AnyOf { conditions } => {
            conditions.iter().any(|c| trigger_matches_phase_entered(c, phase_id))
        }
        _ => false,
    }
}

/// Check if trigger matches phase ended (handles AnyOf recursively)
pub fn trigger_matches_phase_ended(trigger: &Trigger, phase_id: &str) -> bool {
    match trigger {
        Trigger::PhaseEnded { phase_id: trigger_phase } => trigger_phase == phase_id,
        Trigger::AnyOf { conditions } => {
            conditions.iter().any(|c| trigger_matches_phase_ended(c, phase_id))
        }
        _ => false,
    }
}

/// Check if trigger matches counter reaching a value (handles AnyOf recursively)
pub fn trigger_matches_counter_reaches(
    trigger: &Trigger,
    counter_id: &str,
    old_value: u32,
    new_value: u32,
) -> bool {
    match trigger {
        Trigger::CounterReaches { counter_id: trigger_counter, value } => {
            trigger_counter == counter_id && old_value < *value && new_value >= *value
        }
        Trigger::AnyOf { conditions } => conditions
            .iter()
            .any(|c| trigger_matches_counter_reaches(c, counter_id, old_value, new_value)),
        _ => false,
    }
}

/// Check if trigger matches NPC appears (handles AnyOf recursively)
pub fn trigger_matches_npc_appears(
    trigger: &Trigger,
    npc_id: i64,
    entity_name: Option<&str>,
) -> bool {
    match trigger {
        Trigger::NpcAppears { entity } => {
            if entity.is_empty() {
                return false; // Require explicit filter
            }
            if entity.matches_npc_id(npc_id) {
                return true;
            }
            if let Some(name) = entity_name {
                return entity.matches_name(name);
            }
            false
        }
        Trigger::AnyOf { conditions } => conditions
            .iter()
            .any(|c| trigger_matches_npc_appears(c, npc_id, entity_name)),
        _ => false,
    }
}

/// Check if trigger matches entity death (handles AnyOf recursively)
pub fn trigger_matches_entity_death(
    trigger: &Trigger,
    npc_id: i64,
    entity_name: Option<&str>,
) -> bool {
    match trigger {
        Trigger::EntityDeath { entity } => {
            if entity.is_empty() {
                return true; // No filter = any death
            }
            if entity.matches_npc_id(npc_id) {
                return true;
            }
            if let Some(name) = entity_name {
                return entity.matches_name(name);
            }
            false
        }
        Trigger::AnyOf { conditions } => conditions
            .iter()
            .any(|c| trigger_matches_entity_death(c, npc_id, entity_name)),
        _ => false,
    }
}

/// Check if trigger matches time elapsed (handles AnyOf recursively)
pub fn trigger_matches_time_elapsed(trigger: &Trigger, old_secs: f32, new_secs: f32) -> bool {
    match trigger {
        Trigger::TimeElapsed { secs } => old_secs < *secs && new_secs >= *secs,
        Trigger::AnyOf { conditions } => {
            conditions.iter().any(|c| trigger_matches_time_elapsed(c, old_secs, new_secs))
        }
        _ => false,
    }
}

/// Check if trigger matches target set (handles AnyOf recursively)
pub fn trigger_matches_target_set(
    trigger: &Trigger,
    source_npc_id: i64,
    source_name: Option<&str>,
) -> bool {
    match trigger {
        Trigger::TargetSet { entity, .. } => {
            if entity.is_empty() {
                return false; // Require explicit filter
            }
            if entity.matches_npc_id(source_npc_id) {
                return true;
            }
            if let Some(name) = source_name {
                return entity.matches_name(name);
            }
            false
        }
        Trigger::AnyOf { conditions } => {
            conditions.iter().any(|c| trigger_matches_target_set(c, source_npc_id, source_name))
        }
        _ => false,
    }
}

/// Check if trigger matches timer started (handles AnyOf recursively)
pub fn trigger_matches_timer_started(trigger: &Trigger, timer_id: &str) -> bool {
    match trigger {
        Trigger::TimerStarted { timer_id: trigger_id } => trigger_id == timer_id,
        Trigger::AnyOf { conditions } => {
            conditions.iter().any(|c| trigger_matches_timer_started(c, timer_id))
        }
        _ => false,
    }
}

/// Check if trigger matches damage taken (handles AnyOf recursively)
pub fn trigger_matches_damage_taken(
    trigger: &Trigger,
    ability_id: u64,
    ability_name: Option<&str>,
) -> bool {
    match trigger {
        Trigger::DamageTaken { abilities, .. } => {
            abilities.is_empty() || abilities.iter().any(|s| s.matches(ability_id, ability_name))
        }
        Trigger::AnyOf { conditions } => conditions
            .iter()
            .any(|c| trigger_matches_damage_taken(c, ability_id, ability_name)),
        _ => false,
    }
}
