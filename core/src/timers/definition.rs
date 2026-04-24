//! Timer definition types
//!
//! Definitions are templates loaded from TOML config files that describe
//! what timers to track and how to display them.

use serde::{Deserialize, Serialize};

use crate::dsl::AudioConfig;
use crate::dsl::Condition;
use crate::dsl::CounterCondition;
use crate::dsl::EntityDefinition;
use crate::dsl::Trigger;
use crate::game_data::Difficulty;
use baras_types::AlertTrigger;

// Re-export Trigger as TimerTrigger for backward compatibility during migration
pub use crate::dsl::Trigger as TimerTrigger;

// ═══════════════════════════════════════════════════════════════════════════
// Timer Display Target
// ═══════════════════════════════════════════════════════════════════════════

/// Which overlay should display this timer
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimerDisplayTarget {
    /// Show on Timers A overlay (default for backward compatibility)
    #[default]
    TimersA,
    /// Show on Timers B overlay
    TimersB,
    /// No overlay display (alerts only)
    None,
    /// Show on Ability Queue overlay
    AbilityQueue,
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

    /// Optional ability ID for icon display on the timer bar
    #[serde(default)]
    pub icon_ability_id: Option<u64>,

    /// Only show timer when remaining time is at or below this threshold (0 = always show)
    /// Useful for long timers where you only care about the final countdown
    #[serde(default)]
    pub show_at_secs: f32,

    /// Show on raid frames instead of timer bar overlay?
    #[serde(default)]
    pub show_on_raid_frames: bool,

    /// Which overlay should display this timer (defaults to TimersA)
    #[serde(default)]
    pub display_target: TimerDisplayTarget,

    // ─── Alerts ─────────────────────────────────────────────────────────────
    /// When to fire an alert: on timer start, on timer expire, or never.
    /// For instant alerts (is_alert=true), this defaults to OnApply behavior.
    #[serde(default)]
    pub alert_on: AlertTrigger,

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

    /// Boss definition ID for scoping (e.g., "scyva", "SCYVA_K").
    /// When set, this timer only activates when the detected boss encounter's
    /// definition_id matches. This is more precise than `boss` (display name),
    /// which can collide when multiple definitions share the same display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boss_definition_id: Option<String>,

    /// Active difficulties: "story", "veteran", "master"
    #[serde(default)]
    pub difficulties: Vec<String>,

    /// Group size filter (None = all sizes, Some(8) = 8-man only, Some(16) = 16-man only)
    #[serde(default)]
    pub group_size: Option<u8>,

    // ─── Conditions (state guards) ──────────────────────────────────────────
    /// State conditions that must be satisfied for this timer to be active.
    /// Implicitly AND'd — all conditions must be true.
    #[serde(default)]
    pub conditions: Vec<Condition>,

    /// DEPRECATED: Use `conditions` with `phase_active` instead.
    /// Only active during these phases (empty = all phases).
    #[serde(default)]
    pub phases: Vec<String>,

    /// DEPRECATED: Use `conditions` with `counter_compare` instead.
    /// Only active when counter meets condition.
    #[serde(default)]
    pub counter_condition: Option<CounterCondition>,

    // ─── Instance Behavior ───────────────────────────────────────────────────
    /// If true, create separate timer instances per target. If false, only one
    /// instance can be active at a time (keyed by definition ID only).
    /// Defaults to true for user-created timers, false for boss encounter timers.
    #[serde(default = "crate::serde_defaults::default_true")]
    pub per_target: bool,

    // ─── Ability Queue ──────────────────────────────────────────────────────
    /// GCD duration in seconds. When set, firing this timer also creates
    /// a synthetic GCD countdown bar in the ability queue overlay.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gcd_secs: Option<f32>,

    /// If true, timer holds at zero as "queued/ready" instead of being removed.
    /// The timer stays in active_timers with is_queued = true.
    #[serde(default)]
    pub queue_on_expire: bool,

    /// Sort priority for queued entries in tier 2 (higher = higher priority, 255 = first).
    #[serde(default)]
    pub queue_priority: u8,

    /// Trigger that clears a queued entry. Evaluation stubbed as no-op in v1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue_remove_trigger: Option<Trigger>,

    /// Names of other timers in the same encounter that block this ability
    /// from appearing as "next to cast" when any of them is currently active.
    /// OR semantics — any one active blocker marks this timer as blocked.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queue_blocking_timers: Vec<String>,

    /// When true, render this timer's ability-queue row as a trickling-down
    /// bar (full → empty as the cooldown elapses) instead of the default
    /// filling-up progress bar. Only has an effect when `display_target` is
    /// `AbilityQueue`.
    #[serde(default)]
    pub queue_countdown_bar: bool,

    /// When true, this timer is never a "next cast" candidate even when
    /// eligible by timing. Useful for display-only rows (incoming debuffs,
    /// lockout windows, mechanic timers) that the player doesn't cast.
    #[serde(default)]
    pub queue_hide_from_next: bool,
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
    /// Check if this timer matches a given ability ID and/or name.
    /// Delegates to unified `Trigger::matches_ability`.
    pub fn matches_ability_with_name(&self, ability_id: u64, ability_name: Option<&str>) -> bool {
        self.trigger.matches_ability(ability_id, ability_name)
    }

    /// Check if this timer matches a given effect ID/name for apply triggers.
    /// Delegates to unified `Trigger::matches_effect_applied`.
    pub fn matches_effect_applied(&self, effect_id: u64, effect_name: Option<&str>) -> bool {
        self.trigger.matches_effect_applied(effect_id, effect_name)
    }

    /// Check if this timer matches a given effect ID/name for remove triggers.
    /// Delegates to unified `Trigger::matches_effect_removed`.
    pub fn matches_effect_removed(&self, effect_id: u64, effect_name: Option<&str>) -> bool {
        self.trigger.matches_effect_removed(effect_id, effect_name)
    }

    /// Check if this timer is triggered by another timer expiring.
    /// Delegates to unified `Trigger::matches_timer_expires`.
    pub fn matches_timer_expires(&self, timer_id: &str) -> bool {
        self.trigger.matches_timer_expires(timer_id)
    }

    pub fn matches_timer_canceled(&self, timer_id: &str) -> bool {
        self.trigger.matches_timer_canceled(timer_id)
    }

    /// Check if this timer triggers on combat start.
    pub fn triggers_on_combat_start(&self) -> bool {
        self.trigger.contains_combat_start()
    }

    /// Check if this timer triggers when boss HP crosses below a threshold.
    /// Delegates to unified `Trigger::matches_boss_hp_below`.
    pub fn matches_boss_hp_threshold(
        &self,
        entities: &[EntityDefinition],
        npc_id: i64,
        npc_name: Option<&str>,
        previous_hp: f32,
        current_hp: f32,
    ) -> bool {
        // Provide empty string if no name (trigger will match on ID if selector is empty)
        self.trigger.matches_boss_hp_below(
            entities,
            npc_id,
            npc_name.unwrap_or(""),
            previous_hp,
            current_hp,
        )
    }

    /// Check if this timer triggers on a specific phase being entered.
    /// Delegates to unified `Trigger::matches_phase_entered`.
    pub fn matches_phase_entered(&self, phase_id: &str) -> bool {
        self.trigger.matches_phase_entered(phase_id)
    }

    /// Check if this timer triggers when a specific phase ends.
    /// Delegates to unified `Trigger::matches_phase_ended`.
    pub fn matches_phase_ended(&self, phase_id: &str) -> bool {
        self.trigger.matches_phase_ended(phase_id)
    }

    /// Check if this timer triggers when a counter reaches a value.
    /// Delegates to unified `Trigger::matches_counter_reaches`.
    pub fn matches_counter_reaches(
        &self,
        counter_id: &str,
        old_value: u32,
        new_value: u32,
    ) -> bool {
        self.trigger
            .matches_counter_reaches(counter_id, old_value, new_value)
    }

    /// Check if this timer triggers when a counter value changes (any change).
    /// Delegates to unified `Trigger::matches_counter_changes`.
    pub fn matches_counter_changes(&self, counter_id: &str) -> bool {
        self.trigger.matches_counter_changes(counter_id)
    }

    /// Check if this timer triggers when an NPC first appears.
    /// Delegates to unified `Trigger::matches_npc_appears`.
    pub fn matches_npc_appears(
        &self,
        entities: &[EntityDefinition],
        npc_id: i64,
        entity_name: Option<&str>,
    ) -> bool {
        self.trigger
            .matches_npc_appears(entities, npc_id, entity_name.unwrap_or(""))
    }

    /// Check if this timer triggers on entity death.
    /// Delegates to unified `Trigger::matches_entity_death`.
    pub fn matches_entity_death(
        &self,
        entities: &[EntityDefinition],
        npc_id: i64,
        entity_name: Option<&str>,
    ) -> bool {
        self.trigger
            .matches_entity_death(entities, npc_id, entity_name.unwrap_or(""))
    }

    /// Check if this timer triggers at a specific combat time.
    /// Delegates to unified `Trigger::matches_time_elapsed`.
    pub fn matches_time_elapsed(&self, old_combat_secs: f32, new_combat_secs: f32) -> bool {
        self.trigger
            .matches_time_elapsed(old_combat_secs, new_combat_secs)
    }

    /// Check if this timer triggers when an NPC sets its target.
    /// Delegates to unified `Trigger::matches_target_set`.
    pub fn matches_target_set(
        &self,
        entities: &[crate::dsl::EntityDefinition],
        source_npc_id: i64,
        source_name: Option<&str>,
    ) -> bool {
        self.trigger
            .matches_target_set(entities, source_npc_id, source_name)
    }

    /// Check if this timer triggers when damage is taken from an ability.
    /// Delegates to unified `Trigger::matches_damage_taken`.
    pub fn matches_damage_taken(
        &self,
        ability_id: u64,
        ability_name: Option<&str>,
        defense_type_id: i64,
    ) -> bool {
        self.trigger.matches_damage_taken(ability_id, ability_name, defense_type_id)
    }

    /// Check if this timer triggers on a threat modification event.
    pub fn matches_threat_modified(&self, ability_id: u64, ability_name: Option<&str>, threat: f32) -> bool {
        self.trigger.matches_threat_modified(ability_id, ability_name, threat)
    }

    /// Check if this timer triggers when healing is taken from an ability.
    pub fn matches_healing_taken(&self, ability_id: u64, ability_name: Option<&str>) -> bool {
        self.trigger.matches_healing_taken(ability_id, ability_name)
    }

    /// Check if this timer is active for a given encounter context
    pub fn is_active_for_context(
        &self,
        area_id: Option<i64>,
        encounter: Option<&str>,
        boss: Option<&str>,
        boss_def_id: Option<&str>,
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

        // Check boss filter — prefer definition ID (unique) over display name (can collide)
        if let Some(timer_boss_def_id) = &self.boss_definition_id {
            if let Some(current_def_id) = boss_def_id {
                if !timer_boss_def_id.eq_ignore_ascii_case(current_def_id) {
                    return false;
                }
            } else {
                return false;
            }
        } else if let Some(timer_boss) = &self.boss {
            // Legacy fallback: match by display name when no definition ID is set
            if let Some(current_boss) = boss {
                if !timer_boss.eq_ignore_ascii_case(current_boss) {
                    return false;
                }
            } else {
                return false;
            }
        }

        // Check difficulty tier filter
        if !self.difficulties.is_empty() {
            let Some(diff) = difficulty else {
                return false;
            };
            if !self.difficulties.iter().any(|d| diff.matches_config_key(d)) {
                return false;
            }
        }

        // Check group size filter (independent of difficulty tier)
        if let Some(required_size) = self.group_size {
            let Some(diff) = difficulty else {
                return false;
            };
            if diff.group_size() != required_size {
                return false;
            }
        }

        true
    }
}

// NOTE: Trigger matching functions have been moved to `impl Trigger` in dsl/triggers/mod.rs.
// TimerDefinition methods now delegate to those unified methods.
