//! Timer definition types
//!
//! Definitions are templates loaded from TOML config files that describe
//! what timers to track and how to display them.

use serde::{Deserialize, Serialize};

use crate::effects::EntityFilter;
use crate::boss::CounterCondition;
use crate::game_data::Difficulty;

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
        /// Specific NPC ID to monitor (most reliable for multi-boss encounters)
        #[serde(default)]
        npc_id: Option<i64>,
        /// Boss name fallback (may vary by locale)
        #[serde(default)]
        boss_name: Option<String>,
    },

    /// Phase is entered (for boss encounters with phases)
    PhaseEntered {
        /// Phase ID that triggers this timer
        phase_id: String,
    },

    /// Phase ends (transitions to another phase)
    PhaseEnded {
        /// Phase ID that triggers this timer when it ends
        phase_id: String,
    },

    /// Counter reaches a specific value
    CounterReaches {
        /// Counter ID to monitor
        counter_id: String,
        /// Value that triggers the timer
        value: u32,
    },

    /// Entity is first seen (spawned/detected)
    EntityFirstSeen {
        /// Entity reference from roster (preferred)
        #[serde(default)]
        entity: Option<String>,
        /// NPC ID to watch for (legacy/fallback)
        #[serde(default)]
        npc_id: Option<i64>,
        /// Entity name fallback (runtime matching)
        #[serde(default)]
        entity_name: Option<String>,
    },

    /// Entity dies
    EntityDeath {
        /// Entity reference from roster (preferred)
        #[serde(default)]
        entity: Option<String>,
        /// Specific NPC ID to watch for (None = any entity death)
        #[serde(default)]
        npc_id: Option<i64>,
        /// Entity name fallback
        #[serde(default)]
        entity_name: Option<String>,
    },

    /// Time elapsed since combat start (for timed mechanics)
    TimeElapsed {
        /// Seconds into combat when this triggers
        secs: f32,
    },

    /// Manually triggered (for testing/debug)
    Manual,

    /// Another timer starts (for cancel_trigger chaining)
    TimerStarted {
        /// ID of the timer whose start cancels this one
        timer_id: String,
    },

    /// NPC sets its target to someone (e.g., sphere targeting player)
    TargetSet {
        /// Entity reference from roster (preferred)
        #[serde(default)]
        entity: Option<String>,
        /// NPC ID of the entity doing the targeting
        #[serde(default)]
        npc_id: Option<i64>,
        /// Entity name fallback for the targeter
        #[serde(default)]
        entity_name: Option<String>,
    },

    // ─── Logical Composition ─────────────────────────────────────────────────

    /// Any condition suffices (OR logic)
    /// For AND logic, use the `phases` and `counter_condition` fields as filters
    AnyOf {
        conditions: Vec<TimerTrigger>,
    },
}

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
    /// What causes this timer to start
    pub trigger: TimerTrigger,

    /// Source filter for trigger events
    #[serde(default)]
    pub source: EntityFilter,

    /// Target filter for trigger events
    #[serde(default)]
    pub target: EntityFilter,

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

    /// Show on raid frames instead of timer bar overlay?
    #[serde(default)]
    pub show_on_raid_frames: bool,

    // ─── Alerts ─────────────────────────────────────────────────────────────
    // TODO: Wire up alert functionality - should_alert() exists but is never called
    /// Alert when this many seconds remain (None = no alert)
    pub alert_at_secs: Option<f32>,

    /// Custom alert text (None = use timer name)
    pub alert_text: Option<String>,

    /// Audio file to play on alert (TODO: audio playback not yet implemented)
    pub audio_file: Option<String>,

    // ─── Chaining & Cancellation ────────────────────────────────────────────
    /// Timer ID to trigger when this one expires
    pub triggers_timer: Option<String>,

    /// Cancel this timer when this trigger fires
    /// Uses the same trigger types as the start trigger, including:
    /// - effect_removed, phase_ended, ability_cast, etc.
    /// - timer_started: cancel when another timer starts (replaces cancel_on_timer)
    pub cancel_trigger: Option<TimerTrigger>,

    // ─── Context ────────────────────────────────────────────────────────────
    /// Only active in specific encounters (empty = all)
    #[serde(default)]
    pub encounters: Vec<String>,

    /// Specific boss name (if applicable)
    pub boss: Option<String>,

    /// Active difficulties: "story", "veteran", "master"
    #[serde(default)]
    pub difficulties: Vec<String>,

    // ─── Phase/Counter Conditions (optional) ─────────────────────────────────
    /// Only active during these phases (empty = all phases)
    /// Only applies when fighting a boss with phase definitions
    #[serde(default)]
    pub phases: Vec<String>,

    /// Only active when counter meets condition
    /// Only applies when fighting a boss with counter definitions
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
    /// Check if this timer matches a given ability ID (handles compound conditions)
    pub fn matches_ability(&self, ability_id: u64) -> bool {
        trigger_matches_ability(&self.trigger, ability_id)
    }

    /// Check if this timer matches a given effect ID for apply triggers (handles compound conditions)
    pub fn matches_effect_applied(&self, effect_id: u64) -> bool {
        trigger_matches_effect_applied(&self.trigger, effect_id)
    }

    /// Check if this timer matches a given effect ID for remove triggers (handles compound conditions)
    pub fn matches_effect_removed(&self, effect_id: u64) -> bool {
        trigger_matches_effect_removed(&self.trigger, effect_id)
    }

    /// Check if this timer is triggered by another timer expiring (handles compound conditions)
    pub fn matches_timer_expires(&self, timer_id: &str) -> bool {
        trigger_matches_timer_expires(&self.trigger, timer_id)
    }

    /// Check if this timer triggers on combat start (handles compound conditions)
    pub fn triggers_on_combat_start(&self) -> bool {
        trigger_matches_combat_start(&self.trigger)
    }

    /// Check if this timer triggers when boss HP crosses below a threshold (handles compound conditions)
    /// Parameters:
    /// - `npc_id`: The NPC ID whose HP changed
    /// - `npc_name`: The NPC name (for fallback matching)
    /// - `previous_hp`: HP percentage before the change
    /// - `current_hp`: HP percentage after the change
    pub fn matches_boss_hp_threshold(&self, npc_id: i64, npc_name: Option<&str>, previous_hp: f32, current_hp: f32) -> bool {
        trigger_matches_boss_hp(&self.trigger, npc_id, npc_name, previous_hp, current_hp)
    }

    /// Check if this timer triggers on a specific phase being entered (handles compound conditions)
    pub fn matches_phase_entered(&self, phase_id: &str) -> bool {
        trigger_matches_phase_entered(&self.trigger, phase_id)
    }

    /// Check if this timer triggers when a specific phase ends (handles compound conditions)
    pub fn matches_phase_ended(&self, phase_id: &str) -> bool {
        trigger_matches_phase_ended(&self.trigger, phase_id)
    }

    /// Check if this timer triggers when a counter reaches a value (handles compound conditions)
    pub fn matches_counter_reaches(&self, counter_id: &str, old_value: u32, new_value: u32) -> bool {
        trigger_matches_counter_reaches(&self.trigger, counter_id, old_value, new_value)
    }

    /// Check if this timer triggers when an entity is first seen (handles compound conditions)
    pub fn matches_entity_first_seen(&self, npc_id: i64, entity_name: Option<&str>) -> bool {
        trigger_matches_entity_first_seen(&self.trigger, npc_id, entity_name)
    }

    /// Check if this timer triggers on entity death (handles compound conditions)
    pub fn matches_entity_death(&self, npc_id: i64, entity_name: Option<&str>) -> bool {
        trigger_matches_entity_death(&self.trigger, npc_id, entity_name)
    }

    /// Check if this timer triggers at a specific combat time (handles compound conditions)
    /// Returns true if combat_time just crossed the threshold
    pub fn matches_time_elapsed(&self, old_combat_secs: f32, new_combat_secs: f32) -> bool {
        trigger_matches_time_elapsed(&self.trigger, old_combat_secs, new_combat_secs)
    }

    /// Check if this timer triggers when an NPC sets its target (handles compound conditions)
    pub fn matches_target_set(&self, source_npc_id: i64, source_name: Option<&str>) -> bool {
        trigger_matches_target_set(&self.trigger, source_npc_id, source_name)
    }

    /// Check if this timer is active for a given encounter context
    pub fn is_active_for_context(&self, encounter: Option<&str>, boss: Option<&str>, difficulty: Option<Difficulty>) -> bool {
        // Check encounter filter
        if !self.encounters.is_empty() {
            if let Some(enc) = encounter {
                if !self.encounters.iter().any(|e| e.eq_ignore_ascii_case(enc)) {
                    return false;
                }
            } else {
                return false; // Timer requires specific encounter but none provided
            }
        }

        // Check boss filter
        if let Some(timer_boss) = &self.boss {
            if let Some(current_boss) = boss {
                if !timer_boss.eq_ignore_ascii_case(current_boss) {
                    return false;
                }
            } else {
                return false; // Timer requires specific boss but none provided
            }
        }

        // Check difficulty filter
        if !self.difficulties.is_empty()
            && let Some(diff) = difficulty {
                // Match if the difficulty's config key matches any in the list
                if !self.difficulties.iter().any(|d| diff.matches_config_key(d)) {
                    return false;
                }
            // If no difficulty provided but timer has filter, allow it (be permissive)
        }

        true
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Recursive Trigger Matching (for compound conditions)
// ═══════════════════════════════════════════════════════════════════════════

/// Check if trigger matches ability ID (handles AnyOf recursively)
fn trigger_matches_ability(trigger: &TimerTrigger, ability_id: u64) -> bool {
    match trigger {
        TimerTrigger::AbilityCast { ability_ids } => ability_ids.contains(&ability_id),
        TimerTrigger::AnyOf { conditions } => {
            conditions.iter().any(|c| trigger_matches_ability(c, ability_id))
        }
        _ => false,
    }
}

/// Check if trigger matches effect applied (handles AnyOf recursively)
fn trigger_matches_effect_applied(trigger: &TimerTrigger, effect_id: u64) -> bool {
    match trigger {
        TimerTrigger::EffectApplied { effect_ids } => effect_ids.contains(&effect_id),
        TimerTrigger::AnyOf { conditions } => {
            conditions.iter().any(|c| trigger_matches_effect_applied(c, effect_id))
        }
        _ => false,
    }
}

/// Check if trigger matches effect removed (handles AnyOf recursively)
fn trigger_matches_effect_removed(trigger: &TimerTrigger, effect_id: u64) -> bool {
    match trigger {
        TimerTrigger::EffectRemoved { effect_ids } => effect_ids.contains(&effect_id),
        TimerTrigger::AnyOf { conditions } => {
            conditions.iter().any(|c| trigger_matches_effect_removed(c, effect_id))
        }
        _ => false,
    }
}

/// Check if trigger matches timer expiration (handles AnyOf recursively)
fn trigger_matches_timer_expires(trigger: &TimerTrigger, timer_id: &str) -> bool {
    match trigger {
        TimerTrigger::TimerExpires { timer_id: trigger_id } => trigger_id == timer_id,
        TimerTrigger::AnyOf { conditions } => {
            conditions.iter().any(|c| trigger_matches_timer_expires(c, timer_id))
        }
        _ => false,
    }
}

/// Check if trigger matches combat start (handles AnyOf recursively)
fn trigger_matches_combat_start(trigger: &TimerTrigger) -> bool {
    match trigger {
        TimerTrigger::CombatStart => true,
        TimerTrigger::AnyOf { conditions } => {
            conditions.iter().any(trigger_matches_combat_start)
        }
        _ => false,
    }
}

/// Check if trigger matches boss HP threshold crossing (handles AnyOf recursively)
/// Matches when HP crosses below threshold for the specified NPC (or any NPC if no filter)
fn trigger_matches_boss_hp(trigger: &TimerTrigger, npc_id: i64, npc_name: Option<&str>, previous_hp: f32, current_hp: f32) -> bool {
    match trigger {
        TimerTrigger::BossHpThreshold { hp_percent, npc_id: filter_npc_id, boss_name } => {
            // Check NPC ID filter first (most reliable)
            if let Some(required_npc_id) = filter_npc_id
                && *required_npc_id != npc_id {
                    return false;
            }
            // Fall back to name filter
            if let Some(required_name) = boss_name {
                if let Some(actual_name) = npc_name {
                    if !required_name.eq_ignore_ascii_case(actual_name) {
                        return false;
                    }
                } else {
                    return false; // Name required but not provided
                }
            }
            // Check HP threshold crossing
            previous_hp > *hp_percent && current_hp <= *hp_percent
        }
        TimerTrigger::AnyOf { conditions } => {
            conditions.iter().any(|c| trigger_matches_boss_hp(c, npc_id, npc_name, previous_hp, current_hp))
        }
        _ => false,
    }
}

/// Check if trigger matches phase entered (handles AnyOf recursively)
fn trigger_matches_phase_entered(trigger: &TimerTrigger, phase_id: &str) -> bool {
    match trigger {
        TimerTrigger::PhaseEntered { phase_id: trigger_phase } => trigger_phase == phase_id,
        TimerTrigger::AnyOf { conditions } => {
            conditions.iter().any(|c| trigger_matches_phase_entered(c, phase_id))
        }
        _ => false,
    }
}

/// Check if trigger matches phase ended (handles AnyOf recursively)
fn trigger_matches_phase_ended(trigger: &TimerTrigger, phase_id: &str) -> bool {
    match trigger {
        TimerTrigger::PhaseEnded { phase_id: trigger_phase } => trigger_phase == phase_id,
        TimerTrigger::AnyOf { conditions } => {
            conditions.iter().any(|c| trigger_matches_phase_ended(c, phase_id))
        }
        _ => false,
    }
}

/// Check if trigger matches counter reaching a value (handles AnyOf recursively)
/// Triggers when counter value crosses from below to at/above the target
fn trigger_matches_counter_reaches(trigger: &TimerTrigger, counter_id: &str, old_value: u32, new_value: u32) -> bool {
    match trigger {
        TimerTrigger::CounterReaches { counter_id: trigger_counter, value } => {
            trigger_counter == counter_id && old_value < *value && new_value >= *value
        }
        TimerTrigger::AnyOf { conditions } => {
            conditions.iter().any(|c| trigger_matches_counter_reaches(c, counter_id, old_value, new_value))
        }
        _ => false,
    }
}

/// Check if trigger matches entity first seen (handles AnyOf recursively)
/// Note: `entity` roster resolution happens at a higher level (TimerManager)
fn trigger_matches_entity_first_seen(trigger: &TimerTrigger, npc_id: i64, entity_name: Option<&str>) -> bool {
    match trigger {
        TimerTrigger::EntityFirstSeen { entity: _, npc_id: filter_npc_id, entity_name: filter_name } => {
            // Check NPC ID filter first (most reliable)
            if let Some(required_npc_id) = filter_npc_id {
                return *required_npc_id == npc_id;
            }
            // Check entity name filter
            if let Some(required_name) = filter_name {
                if let Some(actual_name) = entity_name {
                    return required_name.eq_ignore_ascii_case(actual_name);
                }
                return false;
            }
            // No filter specified = match nothing (require at least one filter)
            false
        }
        TimerTrigger::AnyOf { conditions } => {
            conditions.iter().any(|c| trigger_matches_entity_first_seen(c, npc_id, entity_name))
        }
        _ => false,
    }
}

/// Check if trigger matches entity death (handles AnyOf recursively)
/// Note: `entity` roster resolution happens at a higher level (TimerManager)
fn trigger_matches_entity_death(trigger: &TimerTrigger, npc_id: i64, entity_name: Option<&str>) -> bool {
    match trigger {
        TimerTrigger::EntityDeath { entity: _, npc_id: filter_npc_id, entity_name: filter_name } => {
            // Check NPC ID filter first
            if let Some(required_npc_id) = filter_npc_id
                && *required_npc_id != npc_id {
                    return false;
            }
            // Check name filter
            if let Some(required_name) = filter_name {
                if let Some(actual_name) = entity_name {
                    if !required_name.eq_ignore_ascii_case(actual_name) {
                        return false;
                    }
                } else {
                    return false;
                }
            }
            true
        }
        TimerTrigger::AnyOf { conditions } => {
            conditions.iter().any(|c| trigger_matches_entity_death(c, npc_id, entity_name))
        }
        _ => false,
    }
}

/// Check if trigger matches time elapsed (handles AnyOf recursively)
/// Triggers when combat time crosses from below to at/above the threshold
fn trigger_matches_time_elapsed(trigger: &TimerTrigger, old_secs: f32, new_secs: f32) -> bool {
    match trigger {
        TimerTrigger::TimeElapsed { secs } => {
            old_secs < *secs && new_secs >= *secs
        }
        TimerTrigger::AnyOf { conditions } => {
            conditions.iter().any(|c| trigger_matches_time_elapsed(c, old_secs, new_secs))
        }
        _ => false,
    }
}

/// Check if trigger matches target set (handles AnyOf recursively)
/// Triggers when an NPC sets its target to someone
/// Note: `entity` roster resolution happens at a higher level (TimerManager)
fn trigger_matches_target_set(trigger: &TimerTrigger, source_npc_id: i64, source_name: Option<&str>) -> bool {
    match trigger {
        TimerTrigger::TargetSet { entity: _, npc_id: filter_npc_id, entity_name: filter_name } => {
            // Check NPC ID filter first (most reliable)
            if let Some(required_npc_id) = filter_npc_id {
                return *required_npc_id == source_npc_id;
            }
            // Check entity name filter
            if let Some(required_name) = filter_name {
                if let Some(actual_name) = source_name {
                    return required_name.eq_ignore_ascii_case(actual_name);
                }
                return false;
            }
            // No filter specified = match nothing (require at least one filter)
            false
        }
        TimerTrigger::AnyOf { conditions } => {
            conditions.iter().any(|c| trigger_matches_target_set(c, source_npc_id, source_name))
        }
        _ => false,
    }
}
