//! Unified combat encounter state
//!
//! CombatEncounter merges the previous Encounter (metrics, entity tracking) and
//! BossEncounterState (phases, counters, HP) into a single source of truth.
//!
//! This simplifies the architecture by:
//! - Eliminating state duplication between SessionCache and TimerManager
//! - Providing clean historical mode support (phases work without Timer/Effect managers)
//! - Centralizing all combat state in one place

use std::sync::Arc;

use arrow::array::ArrowNativeTypeOp;
use chrono::NaiveDateTime;
use hashbrown::{HashMap, HashSet};

use crate::combat_log::{CombatEvent, Entity, EntityType};
use crate::context::IStr;
use crate::dsl::{BossEncounterDefinition, CounterCondition, CounterDefinition};
use crate::game_data::{Difficulty, Discipline, SHIELD_EFFECT_IDS, defense_type, effect_id};
use crate::{effect_type_id, is_boss};

use super::challenge::ChallengeTracker;
use super::effect_instance::EffectInstance;
use super::entity_info::{NpcInfo, PlayerInfo};
use super::metrics::MetricAccumulator;
use super::{EncounterState, OverlayHealthEntry};
use crate::dsl::ChallengeContext;

/// Processing mode for the encounter
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProcessingMode {
    /// Full processing - emit all signals for Timer/Effect managers
    #[default]
    Live,
    /// Historical mode - metrics and phases only, no timer/effect signals
    Historical,
}

/// Information about the currently active boss
#[derive(Debug, Clone)]
pub struct ActiveBoss {
    /// Definition ID (e.g., "apex_vanguard")
    pub definition_id: String,
    /// Display name
    pub name: String,
    /// Entity ID in the combat log
    pub entity_id: i64,
    /// Maximum HP
    pub max_hp: i64,
    /// Current HP
    pub current_hp: i64,
}

/// Unified combat encounter tracking all state, metrics, and boss information
#[derive(Debug, Clone)]
pub struct CombatEncounter {
    // ─── Identity ───────────────────────────────────────────────────────────
    /// Unique encounter ID
    pub id: u64,
    /// Processing mode (Live vs Historical)
    pub mode: ProcessingMode,
    /// Encounter difficulty (set from current area)
    pub difficulty: Option<Difficulty>,
    /// Area ID from game (primary matching key for timers)
    pub area_id: Option<i64>,
    /// Area name from game (for display/logging)
    pub area_name: Option<String>,
    /// Difficulty ID from game (for encounter classification)
    pub difficulty_id: Option<i64>,
    /// Difficulty name from game (for display)
    pub difficulty_name: Option<String>,
    /// Line number of the AreaEntered event for this encounter's area.
    /// Used for per-encounter Parsely uploads to include server info.
    pub area_entered_line: Option<u64>,

    // ─── Boss Definitions (loaded on area enter) ────────────────────────────
    /// Boss definitions for current area (Arc for zero-copy sharing)
    boss_definitions: Arc<Vec<BossEncounterDefinition>>,
    /// Index into boss_definitions for active boss (if detected)
    active_boss_idx: Option<usize>,

    // ─── Boss State (from BossEncounterState) ───────────────────────────────
    /// Currently detected boss info
    pub active_boss: Option<ActiveBoss>,
    /// Current phase ID (e.g., "walker_1", "kephess_2", "burn")
    pub current_phase: Option<String>,
    /// Previous phase ID (for preceded_by checks)
    pub previous_phase: Option<String>,
    /// When the current phase started
    pub phase_started_at: Option<NaiveDateTime>,
    /// Counter values
    pub counters: HashMap<String, u32>,
    /// Elapsed combat time in seconds
    pub combat_time_secs: f32,
    /// Previous combat time (for TimeElapsed threshold detection)
    pub prev_combat_time_secs: f32,

    // ─── Combat State (from Encounter) ──────────────────────────────────────
    /// Current encounter state
    pub state: EncounterState,
    /// When combat started
    pub enter_combat_time: Option<NaiveDateTime>,
    /// When combat ended
    pub exit_combat_time: Option<NaiveDateTime>,
    /// Last combat activity timestamp
    pub last_damage_time: Option<NaiveDateTime>,

    // ─── Entity Tracking ────────────────────────────────────────────────────
    /// Players in this encounter
    pub players: HashMap<i64, PlayerInfo>,
    /// NPCs in this encounter
    pub npcs: HashMap<i64, NpcInfo>,
    /// Buffered NPC targets from TargetSet events that arrived before InCombat
    pending_npc_targets: HashMap<i64, i64>,
    /// NPC log_ids that were dead at the end of the prior encounter.
    /// Used to prevent stale dead NPCs from being re-registered when a new
    /// encounter starts quickly after the previous one.
    prior_dead_npc_log_ids: HashSet<i64>,
    /// Whether all players are dead (sticky - once true, stays true)
    pub all_players_dead: bool,
    /// Whether the victory trigger has fired (for has_victory_trigger encounters).
    /// Once true, ExitCombat events will be honored.
    pub victory_triggered: bool,
    /// Timestamp when the victory trigger fired (used as encounter end time)
    pub victory_triggered_at: Option<NaiveDateTime>,
    /// Timestamp when local player received RECENTLY_REVIVED effect (medcenter/probe revive)
    /// Used to trigger soft-timeout wipe detection for boss encounters
    pub local_player_revive_immunity_time: Option<NaiveDateTime>,
    /// Battle rez is being cast targeting the local player (activated, not yet interrupted/completed)
    pub battle_rez_pending: bool,
    /// Local player revived out of combat (no battle rez) — triggers immediate combat end
    pub local_player_ooc_revive_time: Option<NaiveDateTime>,

    // ─── Boss Shield State ────────────────────────────────────────────────
    /// Active boss shield state: (npc_class_id, shield_def_index) → remaining HP
    pub boss_shields: HashMap<(i64, usize), i64>,

    // ─── Effect Stack Tracking (for effect stack counters) ────────────────
    /// Per-entity effect stack counts: effect_id → (entity_id → stack_count)
    /// Used by counters with `track_effect_stacks` config.
    pub effect_stacks: HashMap<i64, HashMap<i64, u8>>,

    // ─── Effect Instances (for shield attribution) ──────────────────────────
    /// Active effects by target ID
    pub effects: HashMap<i64, Vec<EffectInstance>>,

    // ─── Metrics ────────────────────────────────────────────────────────────
    /// Accumulated damage/healing/etc. data by entity ID
    pub accumulated_data: HashMap<i64, MetricAccumulator>,
    /// Challenge metrics for boss encounters
    pub challenge_tracker: ChallengeTracker,

    // ─── Timer Snapshot (for timer_time_remaining conditions) ──────────────
    /// Snapshot of active timer remaining seconds, keyed by definition_id.
    /// Updated by TimerManager before each signal dispatch cycle and refreshed
    /// internally during process_expirations so that TimerExpires-triggered
    /// conditions see up-to-date timer state.
    /// Updated by TimerManager before each signal dispatch cycle.
    /// Absent entries mean the timer is not active (treated as 0.0).
    pub timer_remaining: HashMap<String, f32>,

    // ─── Line Number Tracking (for per-encounter Parsely uploads) ────────────
    /// Line number of the first event accumulated for this encounter
    pub first_event_line: Option<u64>,
    /// Line number of the last event accumulated (includes grace period events)
    pub last_event_line: Option<u64>,
}

impl CombatEncounter {
    /// Create a new combat encounter
    pub fn new(id: u64, mode: ProcessingMode) -> Self {
        Self {
            id,
            mode,
            difficulty: None,
            area_id: None,
            area_name: None,
            difficulty_id: None,
            difficulty_name: None,
            area_entered_line: None,

            // Boss definitions
            boss_definitions: Arc::new(Vec::new()),
            active_boss_idx: None,

            // Boss state
            active_boss: None,
            current_phase: None,
            previous_phase: None,
            phase_started_at: None,
            counters: HashMap::new(),
            combat_time_secs: 0.0,
            prev_combat_time_secs: 0.0,

            // Combat state
            state: EncounterState::NotStarted,
            enter_combat_time: None,
            exit_combat_time: None,
            last_damage_time: None,

            // Entity tracking
            players: HashMap::new(),
            npcs: HashMap::new(),
            pending_npc_targets: HashMap::new(),
            prior_dead_npc_log_ids: HashSet::new(),
            all_players_dead: false,
            victory_triggered: false,
            victory_triggered_at: None,
            local_player_revive_immunity_time: None,
            battle_rez_pending: false,
            local_player_ooc_revive_time: None,

            // Boss shields
            boss_shields: HashMap::new(),

            // Effect stack tracking
            effect_stacks: HashMap::new(),

            // Effects
            effects: HashMap::new(),

            // Metrics
            accumulated_data: HashMap::new(),
            challenge_tracker: ChallengeTracker::new(),

            // Timer snapshot
            timer_remaining: HashMap::new(),

            // Line number tracking
            first_event_line: None,
            last_event_line: None,
        }
    }

    /// Create with a pre-registered local player
    pub fn with_player(id: u64, mode: ProcessingMode, mut player: PlayerInfo) -> Self {
        let mut enc = Self::new(id, mode);
        // Clear per-encounter flags when starting a new encounter
        tracing::debug!(
            "[ENCOUNTER] Creating new encounter {} with player {} - clearing revive_immunity (was: {})",
            id,
            player.id,
            player.received_revive_immunity
        );
        player.received_revive_immunity = false;
        player.is_dead = false;
        player.death_time = None;
        enc.players.insert(player.id, player);
        enc
    }

    /// Set the log_ids of NPCs that were dead at the end of the prior encounter.
    /// These NPCs will be excluded from registration in this encounter.
    pub fn set_prior_dead_npcs(&mut self, ids: HashSet<i64>) {
        self.prior_dead_npc_log_ids = ids;
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Boss Definitions
    // ═══════════════════════════════════════════════════════════════════════

    /// Load boss definitions for the current area (takes Arc for zero-copy sharing)
    pub fn load_boss_definitions(&mut self, definitions: Arc<Vec<BossEncounterDefinition>>) {
        self.boss_definitions = definitions;
        self.active_boss_idx = None;
    }

    /// Get the currently loaded boss definitions
    pub fn boss_definitions(&self) -> &[BossEncounterDefinition] {
        &self.boss_definitions
    }

    /// Get the Arc to boss definitions (for cheap cloning in hot paths)
    pub fn boss_definitions_arc(&self) -> Arc<Vec<BossEncounterDefinition>> {
        Arc::clone(&self.boss_definitions)
    }

    /// Get the active boss definition (if a boss is detected)
    pub fn active_boss_definition(&self) -> Option<&BossEncounterDefinition> {
        self.active_boss_idx.map(|idx| &self.boss_definitions[idx])
    }

    /// Set the active boss by definition index
    pub fn set_active_boss_idx(&mut self, idx: Option<usize>) {
        self.active_boss_idx = idx;
    }

    /// Get the active boss definition index
    pub fn active_boss_idx(&self) -> Option<usize> {
        self.active_boss_idx
    }

    /// Check if the active boss has a victory trigger that applies to the current difficulty.
    /// Returns false when `victory_trigger_difficulties` is set and the current difficulty
    /// doesn't match (e.g., Trandoshan Squad only has a victory trigger on Master).
    pub fn has_active_victory_trigger(&self) -> bool {
        let Some(boss) = self.active_boss_definition() else {
            return false;
        };
        if !boss.has_victory_trigger {
            return false;
        }
        if boss.victory_trigger_difficulties.is_empty() {
            return true;
        }
        self.difficulty
            .as_ref()
            .map(|d| {
                boss.victory_trigger_difficulties
                    .iter()
                    .any(|vd| d.matches_config_key(vd))
            })
            .unwrap_or(true) // default to true if difficulty unknown
    }

    /// Set the encounter difficulty
    pub fn set_difficulty(&mut self, difficulty: Option<Difficulty>) {
        self.difficulty = difficulty;
    }

    /// Set the encounter difficulty with full info (ID, name, parsed enum)
    pub fn set_difficulty_info(
        &mut self,
        difficulty: Option<Difficulty>,
        difficulty_id: Option<i64>,
        difficulty_name: Option<String>,
    ) {
        self.difficulty = difficulty;
        self.difficulty_id = difficulty_id;
        self.difficulty_name = difficulty_name;
    }

    /// Set the area context for this encounter (including AreaEntered line number)
    pub fn set_area(
        &mut self,
        area_id: Option<i64>,
        area_name: Option<String>,
        area_entered_line: Option<u64>,
    ) {
        self.area_id = area_id;
        self.area_name = area_name;
        self.area_entered_line = area_entered_line;
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Boss State
    // ═══════════════════════════════════════════════════════════════════════

    /// Set the active boss
    pub fn set_boss(&mut self, boss: ActiveBoss) {
        self.active_boss = Some(boss);
    }

    /// Clear the active boss
    pub fn clear_boss(&mut self) {
        self.active_boss = None;
    }

    /// Update HP for a specific entity
    /// Returns `Some((old_hp, new_hp))` if HP changed significantly
    pub fn update_entity_hp(&mut self, npc_id: i64, current: i32, max: i32) -> Option<(f32, f32)> {
        let npc = self.npcs.get_mut(&npc_id)?;

        // Use current HP as "old" for first readings - prevents false threshold crossings
        let old_percent = npc.hp_percent();

        // Track by all identifiers
        npc.current_hp = current;
        npc.max_hp = max;

        let new_pct = npc.hp_percent();
        if old_percent != new_pct {
            Some((old_percent, new_pct))
        } else {
            None
        }
    }

    /// Get HP percentage for a specific NPC ID
    pub fn get_npc_hp_pct(&self, npc_id: i64) -> Option<f32> {
        self.npcs.get(&npc_id).map(|n| n.hp_percent())
    }

    /// Check if an NPC already has a recorded HP value (max_hp > 0).
    /// Used to decide whether to accept source-entity HP snapshots:
    /// only accept them for first-sighting fallback when no prior HP exists.
    pub fn npc_has_hp(&self, log_id: i64) -> bool {
        self.npcs.get(&log_id).is_some_and(|n| n.max_hp > 0)
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Boss Shield Management
    // ═══════════════════════════════════════════════════════════════════════

    /// Activate a boss shield with its full HP value
    pub fn activate_shield(&mut self, npc_class_id: i64, shield_idx: usize, total: i64) {
        self.boss_shields.insert((npc_class_id, shield_idx), total);
    }

    /// Deactivate a specific boss shield
    pub fn deactivate_shield(&mut self, npc_class_id: i64, shield_idx: usize) {
        self.boss_shields.remove(&(npc_class_id, shield_idx));
    }

    /// Absorb damage across all active shields for a given NPC class.
    /// Decrements remaining HP and removes depleted shields.
    pub fn absorb_shield_damage(&mut self, npc_class_id: i64, amount: i64) {
        let keys: Vec<(i64, usize)> = self
            .boss_shields
            .keys()
            .filter(|(id, _)| *id == npc_class_id)
            .copied()
            .collect();

        for key in keys {
            if let Some(remaining) = self.boss_shields.get_mut(&key) {
                *remaining = (*remaining - amount).max(0);
                if *remaining == 0 {
                    self.boss_shields.remove(&key);
                }
            }
        }
    }

    /// Get boss health entries for overlay display
    pub fn get_boss_health(&self) -> Vec<OverlayHealthEntry> {
        let Some(def) = self.active_boss_definition() else {
            return Vec::new();
        };

        let entity_class_ids: HashSet<i64> = def
            .entities
            .iter()
            .filter(|e| e.shows_on_hp_overlay())
            .flat_map(|e| e.ids.iter().copied())
            .collect();

        let mut entries: Vec<OverlayHealthEntry> = self
            .npcs
            .values()
            // Only show NPCs that have taken damage (under 100% HP) to avoid
            // cluttering the overlay with spawned-but-inactive enemies
            .filter(|npc| entity_class_ids.contains(&npc.class_id) && npc.current_hp < npc.max_hp)
            .map(|npc| {
                // Look up entity definition for hp_markers and shields
                let entity_def = def.entity_for_id(npc.class_id);
                let hp_markers = entity_def
                    .map(|e| e.hp_markers.clone())
                    .unwrap_or_default();
                let active_shields = entity_def
                    .map(|e| {
                        e.shields
                            .iter()
                            .enumerate()
                            .filter_map(|(idx, shield_def)| {
                                self.boss_shields
                                    .get(&(npc.class_id, idx))
                                    .map(|&remaining| super::ActiveShield {
                                        label: shield_def.label.clone(),
                                        remaining,
                                        total: shield_def.total,
                                    })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let pushes_at = entity_def.and_then(|e| e.pushes_at);

                OverlayHealthEntry {
                    name: crate::context::resolve(npc.name).to_string(),
                    target_name: self
                        .players
                        .get(&npc.current_target_id)
                        .map(|p| crate::context::resolve(p.name).to_string()),
                    current: npc.current_hp,
                    max: npc.max_hp,
                    first_seen_at: npc.first_seen_at,
                    hp_markers,
                    active_shields,
                    pushes_at,
                }
            })
            .collect();

        entries.sort_by(|a, b| a.first_seen_at.cmp(&b.first_seen_at));
        entries
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Phase Management
    // ═══════════════════════════════════════════════════════════════════════

    /// Set the current phase with timestamp
    pub fn set_phase(&mut self, phase_id: &str, timestamp: NaiveDateTime) {
        self.previous_phase = self.current_phase.take();
        self.current_phase = Some(phase_id.to_string());
        self.phase_started_at = Some(timestamp);
    }

    /// Get the current phase ID
    pub fn phase(&self) -> Option<&str> {
        self.current_phase.as_deref()
    }

    /// Get how long we've been in the current phase (in seconds)
    pub fn phase_duration_secs(&self, current_time: NaiveDateTime) -> f32 {
        self.phase_started_at
            .map(|start| (current_time - start).num_milliseconds() as f32 / 1000.0)
            .unwrap_or(0.0)
    }

    /// Check if currently in a specific phase
    pub fn is_in_phase(&self, phase_id: &str) -> bool {
        self.current_phase.as_deref() == Some(phase_id)
    }

    /// Check if currently in any of the specified phases
    pub fn is_in_any_phase(&self, phase_ids: &[String]) -> bool {
        if phase_ids.is_empty() {
            return true;
        }
        if let Some(current) = &self.current_phase {
            phase_ids.iter().any(|p| p == current)
        } else {
            false
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Counter Management
    // ═══════════════════════════════════════════════════════════════════════

    /// Increment a counter and return the new value
    pub fn increment_counter(&mut self, counter_id: &str) -> u32 {
        let count = self.counters.entry(counter_id.to_string()).or_insert(0);
        *count += 1;
        *count
    }

    /// Modify a counter (increment, decrement, or set_value)
    /// Returns (old_value, new_value)
    pub fn modify_counter(
        &mut self,
        counter_id: &str,
        decrement: bool,
        set_value: Option<u32>,
    ) -> (u32, u32) {
        let old_value = self.get_counter(counter_id);
        let new_value = if let Some(val) = set_value {
            val
        } else if decrement {
            old_value.saturating_sub(1)
        } else {
            old_value + 1
        };
        self.counters.insert(counter_id.to_string(), new_value);
        (old_value, new_value)
    }

    /// Get the current value of a counter
    pub fn get_counter(&self, counter_id: &str) -> u32 {
        self.counters.get(counter_id).copied().unwrap_or(0)
    }

    /// Set a counter to a specific value
    pub fn set_counter(&mut self, counter_id: &str, value: u32) {
        self.counters.insert(counter_id.to_string(), value);
    }

    /// Reset multiple counters to their initial values
    pub fn reset_counters_to_initial(
        &mut self,
        counter_ids: &[String],
        definitions: &[CounterDefinition],
    ) {
        for id in counter_ids {
            let initial = definitions
                .iter()
                .find(|d| d.id == *id)
                .map(|d| d.initial_value)
                .unwrap_or(0);
            self.counters.insert(id.clone(), initial);
        }
    }

    /// Reset all counters
    pub fn reset_all_counters(&mut self) {
        self.counters.clear();
    }

    /// Check a counter condition
    pub fn check_counter_condition(&self, cond: &CounterCondition) -> bool {
        let value = self.get_counter(&cond.counter_id);
        cond.operator.evaluate(value, cond.value)
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Effect Stack Tracking
    // ═══════════════════════════════════════════════════════════════════════

    /// Update the stack count for an effect on a specific entity.
    pub fn update_effect_stacks(&mut self, effect_id: i64, entity_id: i64, stacks: u8) {
        self.effect_stacks
            .entry(effect_id)
            .or_default()
            .insert(entity_id, stacks);
    }

    /// Remove the stack entry for an effect on a specific entity (effect was removed).
    pub fn remove_effect_stacks(&mut self, effect_id: i64, entity_id: i64) {
        if let Some(entities) = self.effect_stacks.get_mut(&effect_id) {
            entities.remove(&entity_id);
            if entities.is_empty() {
                self.effect_stacks.remove(&effect_id);
            }
        }
    }

    /// Get all entities' stack counts for a specific effect.
    pub fn get_effect_stacks(&self, effect_id: i64) -> Option<&HashMap<i64, u8>> {
        self.effect_stacks.get(&effect_id)
    }

    /// Clear all effect stack state (on combat end).
    pub fn clear_effect_stacks(&mut self) {
        self.effect_stacks.clear();
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Timer Snapshot (for timer_time_remaining conditions)
    // ═══════════════════════════════════════════════════════════════════════

    /// Replace the timer remaining snapshot with a fresh one.
    /// Called by the parser before dispatching signals so conditions see current timer state.
    /// Takes `&self` (not `&mut self`) because this uses interior mutability via RefCell,
    /// allowing the timer manager to refresh the snapshot mid-processing through the
    /// immutable `&CombatEncounter` reference provided by the SignalHandler trait.
    pub fn update_timer_snapshot(&mut self, snapshot: HashMap<String, f32>) {
        self.timer_remaining = snapshot;
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Condition Evaluation
    // ═══════════════════════════════════════════════════════════════════════

    /// Evaluate a single state condition against current encounter state.
    pub fn evaluate_condition(&self, condition: &crate::dsl::Condition) -> bool {
        use crate::dsl::Condition;
        match condition {
            Condition::PhaseActive { phase_ids } => self.is_in_any_phase(phase_ids),

            Condition::CounterCompare {
                counter_id,
                operator,
                value,
            } => {
                let current = self.get_counter(counter_id);
                operator.evaluate(current, *value)
            }

            Condition::CounterCompareCounter {
                counter_id,
                operator,
                other_counter_id,
            } => {
                let left = self.get_counter(counter_id);
                let right = self.get_counter(other_counter_id);
                operator.evaluate(left, right)
            }

            Condition::TimerTimeRemaining {
                timer_id,
                operator,
                value,
            } => {
                // If the timer is not active, the condition is always false
                match self.timer_remaining.get(timer_id) {
                    Some(&remaining) => operator.evaluate_f32(remaining, *value),
                    None => false,
                }
            }

            Condition::AllOf { conditions } => {
                conditions.iter().all(|c| self.evaluate_condition(c))
            }

            Condition::AnyOf { conditions } => {
                conditions.iter().any(|c| self.evaluate_condition(c))
            }

            Condition::Not { condition } => !self.evaluate_condition(condition),
        }
    }

    /// Evaluate a single condition using an overridden timer_remaining snapshot.
    ///
    /// This is used by the timer manager during `process_expirations` to evaluate
    /// conditions against fresh timer state rather than the potentially stale
    /// snapshot cached on the encounter from before signal dispatch.
    fn evaluate_condition_with_timer_snapshot(
        &self,
        condition: &crate::dsl::Condition,
        timer_snapshot: &hashbrown::HashMap<String, f32>,
    ) -> bool {
        use crate::dsl::Condition;
        match condition {
            Condition::PhaseActive { phase_ids } => self.is_in_any_phase(phase_ids),

            Condition::CounterCompare {
                counter_id,
                operator,
                value,
            } => {
                let current = self.get_counter(counter_id);
                operator.evaluate(current, *value)
            }

            Condition::CounterCompareCounter {
                counter_id,
                operator,
                other_counter_id,
            } => {
                let left = self.get_counter(counter_id);
                let right = self.get_counter(other_counter_id);
                operator.evaluate(left, right)
            }

            Condition::TimerTimeRemaining {
                timer_id,
                operator,
                value,
            } => {
                // Use the overridden snapshot instead of self.timer_remaining
                match timer_snapshot.get(timer_id) {
                    Some(&remaining) => operator.evaluate_f32(remaining, *value),
                    None => false,
                }
            }

            Condition::AllOf { conditions } => conditions
                .iter()
                .all(|c| self.evaluate_condition_with_timer_snapshot(c, timer_snapshot)),

            Condition::AnyOf { conditions } => conditions
                .iter()
                .any(|c| self.evaluate_condition_with_timer_snapshot(c, timer_snapshot)),

            Condition::Not { condition } => {
                !self.evaluate_condition_with_timer_snapshot(condition, timer_snapshot)
            }
        }
    }

    /// Evaluate all conditions (implicitly AND'd). Returns true if all are met.
    /// An empty conditions list always returns true.
    pub fn evaluate_conditions(&self, conditions: &[crate::dsl::Condition]) -> bool {
        conditions.iter().all(|c| self.evaluate_condition(c))
    }

    /// Evaluate merged conditions: combines new `conditions` field with legacy
    /// `phases` and `counter_condition` fields for backward compatibility.
    pub fn evaluate_merged_conditions(
        &self,
        conditions: &[crate::dsl::Condition],
        phases: &[String],
        counter_condition: Option<&CounterCondition>,
    ) -> bool {
        // Check new-style conditions
        if !conditions.iter().all(|c| self.evaluate_condition(c)) {
            return false;
        }

        // Check legacy phases
        if !phases.is_empty() && !self.is_in_any_phase(phases) {
            return false;
        }

        // Check legacy counter condition
        if let Some(cond) = counter_condition {
            if !self.check_counter_condition(cond) {
                return false;
            }
        }

        true
    }

    /// Evaluate merged conditions with an overridden timer_remaining snapshot.
    ///
    /// Used by the timer manager during `process_expirations` so that
    /// `TimerTimeRemaining` conditions see up-to-date timer state rather than
    /// the potentially stale snapshot cached before signal dispatch.
    pub fn evaluate_merged_conditions_with_timer_snapshot(
        &self,
        conditions: &[crate::dsl::Condition],
        phases: &[String],
        counter_condition: Option<&CounterCondition>,
        timer_snapshot: &hashbrown::HashMap<String, f32>,
    ) -> bool {
        // Check new-style conditions using overridden timer snapshot
        if !conditions
            .iter()
            .all(|c| self.evaluate_condition_with_timer_snapshot(c, timer_snapshot))
        {
            return false;
        }

        // Check legacy phases
        if !phases.is_empty() && !self.is_in_any_phase(phases) {
            return false;
        }

        // Check legacy counter condition
        if let Some(cond) = counter_condition {
            if !self.check_counter_condition(cond) {
                return false;
            }
        }

        true
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Combat Time
    // ═══════════════════════════════════════════════════════════════════════

    /// Compute the `combat_time_secs` value to store for a given event timestamp.
    ///
    /// Once combat has ended (either via kill-target death, victory trigger, wipe, or
    /// timeout), the value is capped at `effective_end_time()` so that grace-period
    /// events accumulated after the exit do not inflate `MAX(combat_time_secs)` beyond
    /// the actual encounter end.  Without this cap the query layer derives an inflated
    /// duration (e.g. 6:21 instead of 6:17), skewing DPS/HPS denominators and the
    /// timeline scrubber end position.
    ///
    /// While combat is still active (`effective_end_time()` is `None`) the raw elapsed
    /// time is returned unchanged.
    ///
    /// Returns `None` when combat has not yet started (`enter_combat_time` is unset).
    pub fn compute_combat_time_secs(&self, event_timestamp: NaiveDateTime) -> Option<f32> {
        self.enter_combat_time.map(|start| {
            // Cap at the effective end time when set so grace-period events don't
            // extend the duration seen by the query layer.
            let effective_ts = self
                .effective_end_time()
                .map(|end| event_timestamp.min(end))
                .unwrap_or(event_timestamp);
            (effective_ts - start).num_milliseconds() as f32 / 1000.0
        })
    }

    /// Update combat time and return (old_time, new_time) for threshold checking
    pub fn update_combat_time(&mut self, current_timestamp: NaiveDateTime) -> (f32, f32) {
        let old_time = self.combat_time_secs;
        if let Some(start) = self.enter_combat_time {
            let duration = current_timestamp - start;
            self.combat_time_secs = duration.num_milliseconds() as f32 / 1000.0;
        }
        self.prev_combat_time_secs = old_time;
        (old_time, self.combat_time_secs)
    }

    /// Get combat duration in seconds (truncated)
    pub fn duration_seconds(&self, current_time: Option<chrono::NaiveDateTime>) -> Option<i64> {
        Some(self.duration_ms(current_time)? / 1000)
    }

    /// Get the effective end time of the encounter.
    /// For victory-trigger encounters, this is when the victory trigger fired.
    /// Otherwise, it's the exit_combat_time.
    pub fn effective_end_time(&self) -> Option<NaiveDateTime> {
        // Victory trigger time takes precedence (that's when the boss actually died)
        self.victory_triggered_at.or(self.exit_combat_time)
    }

    /// Get combat duration in milliseconds
    ///
    /// For completed encounters, uses the effective end time.
    /// For in-progress encounters, uses the provided `current_time`
    /// (interpolated game time) instead of the system clock to avoid
    /// clock skew between SWTOR's timestamps and the OS clock.
    pub fn duration_ms(&self, current_time: Option<chrono::NaiveDateTime>) -> Option<i64> {
        use chrono::TimeDelta;

        let enter = self.enter_combat_time?;
        let terminal = self
            .effective_end_time()
            .or(current_time)
            .unwrap_or_else(|| chrono::offset::Local::now().naive_local());

        let mut duration = terminal.signed_duration_since(enter);

        // Handle midnight crossing
        if duration.num_milliseconds().is_negative() {
            duration = duration.checked_add(&TimeDelta::days(1))?;
        }

        Some(duration.num_milliseconds())
    }

    /// Build a ChallengeContext snapshot
    pub fn challenge_context(&self, boss_npc_ids: &[i64]) -> ChallengeContext {
        ChallengeContext {
            current_phase: self.current_phase.clone(),
            counters: self.counters.iter().map(|(k, v)| (k.clone(), *v)).collect(),
            hp_by_npc_id: self
                .npcs
                .iter()
                .map(|(k, v)| (*k, v.hp_percent()))
                .collect(),
            boss_npc_ids: boss_npc_ids.to_vec(),
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Entity Filter Context Helpers
    // ═══════════════════════════════════════════════════════════════════════

    /// Build the set of runtime entity IDs for boss NPCs.
    /// Used for `EntityFilter::Boss` / `NpcExceptBoss` matching in source/target filters.
    pub fn boss_entity_ids(&self) -> std::collections::HashSet<i64> {
        self.npcs
            .values()
            .filter(|n| n.is_boss)
            .map(|n| n.log_id)
            .collect()
    }

    /// Get the local player's current target entity ID.
    /// Returns `None` if the player isn't tracked or has no target.
    pub fn local_player_target_id(&self, local_player_id: i64) -> Option<i64> {
        self.players
            .get(&local_player_id)
            .map(|p| p.current_target_id)
            .filter(|&id| id != 0)
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Entity State
    // ═══════════════════════════════════════════════════════════════════════

    pub fn set_entity_death(
        &mut self,
        entity_id: i64,
        entity_type: &EntityType,
        timestamp: NaiveDateTime,
    ) {
        match entity_type {
            EntityType::Player => {
                if let Some(player) = self.players.get_mut(&entity_id) {
                    player.is_dead = true;
                    player.death_time = Some(timestamp);
                }
            }
            EntityType::Npc | EntityType::Companion => {
                if let Some(npc) = self.npcs.get_mut(&entity_id) {
                    npc.is_dead = true;
                    npc.death_time = Some(timestamp);
                }
            }
            _ => {}
        }
    }

    pub fn set_entity_alive(&mut self, entity_id: i64, entity_type: &EntityType) {
        match entity_type {
            EntityType::Player => {
                if let Some(player) = self.players.get_mut(&entity_id) {
                    // Only allow revival if player hasn't used medcenter/probe revive
                    if !player.received_revive_immunity {
                        player.is_dead = false;
                        player.death_time = None;
                    }
                }
            }
            EntityType::Npc | EntityType::Companion => {
                if let Some(npc) = self.npcs.get_mut(&entity_id) {
                    npc.is_dead = false;
                    npc.death_time = None;
                }
            }
            _ => {}
        }
    }

    pub fn set_player_revive_immunity(&mut self, entity_id: i64) {
        // Only set revive immunity if player is already tracked
        // Don't create incomplete player entries (missing name, etc.)
        if let Some(player) = self.players.get_mut(&entity_id) {
            player.received_revive_immunity = true;
        }
    }

    pub fn check_all_players_dead(&mut self) {
        if self.all_players_dead {
            return;
        }
        // Only check during active combat - deaths during NotStarted are pre-combat
        // and shouldn't affect the upcoming encounter
        if self.state != EncounterState::InCombat {
            return;
        }
        // Only consider players seen during actual combat (after enter_combat_time)
        // This filters out players who were tracked pre-combat but left/switched characters
        let dominated_players: Vec<_> = self
            .players
            .values()
            .filter(|p| {
                self.enter_combat_time.is_none_or(|combat_start| {
                    p.last_seen_at.is_some_and(|seen| seen >= combat_start)
                })
            })
            .collect();

        self.all_players_dead =
            !dominated_players.is_empty() && dominated_players.iter().all(|p| p.is_dead);
    }

    /// Check if this encounter is likely a wipe.
    /// For boss encounters with kill targets: if any kill target is still alive, it's a wipe.
    /// Falls back to all_players_dead for encounters without kill targets.
    pub fn is_likely_wipe(&self) -> bool {
        // If all_players_dead is set, it's definitely a wipe
        if self.all_players_dead {
            return true;
        }
        
        // For boss encounters with kill targets defined
        if let Some(def_idx) = self.active_boss_idx() {
            let def = &self.boss_definitions()[def_idx];
            let kill_target_ids: HashSet<i64> = def.kill_targets()
                .flat_map(|e| e.ids.iter().copied())
                .collect();
            
            if !kill_target_ids.is_empty() {
                // Find all NPC instances that match kill target class IDs
                let kill_target_instances: Vec<_> = self.npcs.values()
                    .filter(|npc| kill_target_ids.contains(&npc.class_id))
                    .collect();
                
                // If we've seen kill targets and any are still alive, it's a wipe
                if !kill_target_instances.is_empty() {
                    // Consider a kill target dead if either:
                    // - is_dead flag is set (received death event), OR
                    // - current_hp <= 0 (handles game race condition where death event is never logged)
                    let all_kill_targets_dead = kill_target_instances.iter().all(|npc| {
                        npc.is_dead || npc.current_hp <= 0
                    });
                    return !all_kill_targets_dead;
                }
            }
        }
        
        // Fall back to all_players_dead for non-boss encounters
        false
    }

    pub fn track_event_entities(&mut self, event: &CombatEvent) {
        if event.effect.type_id == effect_type_id::REMOVEEFFECT {
            return;
        }

        // For TARGETSET/TARGETCLEARED, track the source entity (player, NPC, or companion)
        // so we can set their current target before the entity lookup
        if event.effect.effect_id == effect_id::TARGETSET
            || event.effect.effect_id == effect_id::TARGETCLEARED
        {
            self.try_track_entity(&event.source_entity, event.timestamp);
            return;
        }

        self.try_track_entity(&event.source_entity, event.timestamp);
        self.try_track_entity(&event.target_entity, event.timestamp);
    }

    #[inline]
    fn try_track_entity(&mut self, entity: &Entity, timestamp: NaiveDateTime) {
        // Dont register zero health entities
        if entity.health.0.is_zero() {
            return;
        }

        match entity.entity_type {
            EntityType::Player => {
                self.players
                    .entry(entity.log_id)
                    .and_modify(|p| p.last_seen_at = Some(timestamp))
                    .or_insert_with(|| PlayerInfo {
                        id: entity.log_id,
                        name: entity.name,
                        last_seen_at: Some(timestamp),
                        ..Default::default()
                    });
            }
            EntityType::Npc | EntityType::Companion => {
                // Only register NPCs/companions during active combat to avoid stale entries
                // from targeting nearby enemies during grace period (e.g., boss jumping down
                // after trash) or mount/dismount respawns between encounters
                if self.state != EncounterState::InCombat {
                    return;
                }

                // Skip NPCs that were dead at the end of the prior encounter.
                // Prevents stale dead NPCs from bleeding into a new encounter when
                // encounters transition quickly (e.g., wipe and immediate repull).
                if self.prior_dead_npc_log_ids.contains(&entity.log_id) {
                    tracing::trace!(
                        "[ENCOUNTER] Skipping NPC registration for log_id={} (dead in prior encounter)",
                        entity.log_id
                    );
                    return;
                }

                let pending_target =
                    self.pending_npc_targets.remove(&entity.log_id).unwrap_or(0);
                self.npcs.entry(entity.log_id).or_insert_with(|| NpcInfo {
                    name: entity.name,
                    entity_type: entity.entity_type,
                    log_id: entity.log_id,
                    class_id: entity.class_id,
                    first_seen_at: Some(timestamp),
                    current_hp: entity.health.0,
                    max_hp: entity.health.1,
                    is_boss: is_boss(entity.class_id),
                    current_target_id: pending_target,
                    ..Default::default()
                });
            }
            _ => {}
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(
            self.state,
            EncounterState::InCombat | EncounterState::PostCombat { .. }
        )
    }

    fn get_entity_name(&self, id: i64) -> Option<IStr> {
        self.players
            .get(&id)
            .map(|e| e.name)
            .or_else(|| self.npcs.get(&id).map(|e| e.name))
    }

    fn get_entity_type(&self, id: i64) -> Option<EntityType> {
        if self.players.contains_key(&id) {
            Some(EntityType::Player)
        } else {
            self.npcs.get(&id).map(|e| e.entity_type)
        }
    }

    /// Set an entity's current target (works for both players and NPCs)
    pub fn set_entity_target(&mut self, entity_id: i64, target_id: i64) {
        if let Some(player) = self.players.get_mut(&entity_id) {
            player.current_target_id = target_id;
        } else if let Some(npc) = self.npcs.get_mut(&entity_id) {
            npc.current_target_id = target_id;
        } else {
            // Buffer target for NPCs not yet registered (TargetSet arrived before InCombat)
            self.pending_npc_targets.insert(entity_id, target_id);
        }
    }

    /// Clear an entity's current target (works for both players and NPCs)
    pub fn clear_entity_target(&mut self, entity_id: i64) {
        if let Some(player) = self.players.get_mut(&entity_id) {
            player.current_target_id = 0;
        } else if let Some(npc) = self.npcs.get_mut(&entity_id) {
            npc.current_target_id = 0;
        }
    }

    /// Get an entity's current target (works for both players and NPCs)
    pub fn get_current_target(&self, entity_id: i64) -> Option<i64> {
        if let Some(player) = self.players.get(&entity_id) {
            if player.current_target_id != 0 {
                return Some(player.current_target_id);
            }
        } else if let Some(npc) = self.npcs.get(&entity_id)
            && npc.current_target_id != 0
        {
            return Some(npc.current_target_id);
        }
        None
    }
    // ═══════════════════════════════════════════════════════════════════════
    // Effect Instances
    // ═══════════════════════════════════════════════════════════════════════

    pub fn apply_effect(&mut self, event: &CombatEvent) {
        let is_shield = SHIELD_EFFECT_IDS.contains(&event.effect.effect_id);
        self.effects
            .entry(event.target_entity.log_id)
            .or_default()
            .push(EffectInstance {
                effect_id: event.effect.effect_id,
                source_id: event.source_entity.log_id,
                target_id: event.target_entity.log_id,
                applied_at: event.timestamp,
                is_shield,
                removed_at: None,
            });
    }

    pub fn remove_effect(&mut self, event: &CombatEvent) {
        let target_id = event.target_entity.log_id;
        let Some(effects) = self.effects.get_mut(&target_id) else {
            return;
        };

        for effect_instance in effects.iter_mut().rev() {
            if effect_instance.effect_id == event.effect.effect_id
                && effect_instance.source_id == event.source_entity.log_id
                && effect_instance.removed_at.is_none()
            {
                effect_instance.removed_at = Some(event.timestamp);
                break;
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Metrics Accumulation
    // ═══════════════════════════════════════════════════════════════════════

    pub fn accumulate_data(&mut self, event: &CombatEvent) {
        // Only accumulate metrics once combat has started. Pre-combat events
        // (before EnterCombat sets enter_combat_time) are intentionally dropped
        // so that healing, threat, taunts, etc. don't inflate encounter totals.
        if self.enter_combat_time.is_none() {
            return;
        }

        use crate::is_boss;

        let defense_type = event.details.defense_type_id;
        let is_defense = matches!(
            defense_type,
            defense_type::MISS
                | defense_type::DODGE
                | defense_type::PARRY
                | defense_type::RESIST
                | defense_type::DEFLECT
        );
        let is_natural_shield = defense_type == defense_type::SHIELD
            && event.details.dmg_effective == event.details.dmg_amount;

        // Source accumulation
        {
            let source = self
                .accumulated_data
                .entry(event.source_entity.log_id)
                .or_default();

            if event.details.dmg_amount > 0
                && event.source_entity.log_id != event.target_entity.log_id
            {
                source.damage_dealt += event.details.dmg_amount as i64;
                source.damage_dealt_effective += event.details.dmg_effective as i64;
                source.damage_hit_count += 1;
                if event.details.is_crit {
                    source.damage_crit_count += 1;
                }
                if is_boss(event.target_entity.class_id) {
                    source.damge_dealt_boss += event.details.dmg_amount as i64;
                }
            }

            if event.details.heal_amount > 0 {
                source.healing_done += event.details.heal_amount as i64;
                source.healing_effective += event.details.heal_effective as i64;
                source.heal_count += 1;
                if event.details.is_crit {
                    source.heal_crit_count += 1;
                }
            }

            source.threat_generated += event.details.threat as f64;

            if event.effect.effect_id == effect_id::ABILITYACTIVATE
                && self.enter_combat_time.is_some_and(|t| event.timestamp >= t)
                && self.exit_combat_time.is_none_or(|t| t >= event.timestamp)
            {
                source.actions += 1;
            }

            if event.effect.effect_id == effect_id::TAUNT {
                source.taunt_count += 1;
            }

            if event.details.dmg_absorbed > 0 && !is_natural_shield {
                self.attribute_shield_absorption(event);
            }
        }

        // Target accumulation
        {
            let target = self
                .accumulated_data
                .entry(event.target_entity.log_id)
                .or_default();

            // Count all incoming attacks (hits + full avoidances) for defense %.
            if is_defense || event.details.dmg_amount > 0 {
                target.attacks_received += 1;
            }
            if is_defense {
                target.defense_count += 1;
            }

            if event.details.dmg_amount > 0 {
                target.damage_received += event.details.dmg_amount as i64;
                target.damage_received_effective += event.details.dmg_effective as i64;
                target.damage_absorbed += event.details.dmg_absorbed as i64;
                // Only hits that land can proc a shield roll, so this is
                // the correct denominator for shield %.
                target.hits_received += 1;

                if defense_type == defense_type::SHIELD {
                    target.shield_roll_count += 1;
                    target.shield_roll_absorbed += event.details.dmg_absorbed as i64;
                }
            }

            if event.details.heal_amount > 0 {
                target.healing_received += event.details.heal_amount as i64;
                target.healing_received_effective += event.details.heal_effective as i64;
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Line Number Tracking
    // ═══════════════════════════════════════════════════════════════════════

    /// Track an event's line number for per-encounter Parsely uploads.
    /// Sets first_event_line if not yet set, always updates last_event_line.
    #[inline]
    pub fn track_event_line(&mut self, line_number: u64) {
        if self.first_event_line.is_none() {
            self.first_event_line = Some(line_number);
        }
        self.last_event_line = Some(line_number);
    }

    /// Calculate per-entity DPS/HPS/etc. metrics.
    ///
    /// `current_time` is used as the duration fallback while combat is still active.
    /// Pass `interpolated_game_time()` from the live service path so the DPS/HPS
    /// denominator stays in sync with the overlay display timer (both game-clock
    /// anchored).  Pass `None` at finalization — `effective_end_time()` is already
    /// set by then and provides the correct frozen end timestamp.
    pub fn calculate_entity_metrics(
        &self,
        player_disciplines: &hashbrown::HashMap<i64, super::entity_info::PlayerInfo>,
        current_time: Option<NaiveDateTime>,
    ) -> Option<Vec<super::metrics::EntityMetrics>> {
        use super::metrics::EntityMetrics;

        let duration_ms = self.duration_ms(current_time)?;
        if duration_ms <= 0 {
            return None;
        }

        let mut stats: Vec<EntityMetrics> = self
            .accumulated_data
            .iter()
            .filter_map(|(id, acc)| {
                let name = self.get_entity_name(*id)?;
                let entity_type = self.get_entity_type(*id)?;

                let damage_crit_pct = if acc.damage_hit_count > 0 {
                    (acc.damage_crit_count as f32 / acc.damage_hit_count as f32) * 100.0
                } else {
                    0.0
                };
                let heal_crit_pct = if acc.heal_count > 0 {
                    (acc.heal_crit_count as f32 / acc.heal_count as f32) * 100.0
                } else {
                    0.0
                };
                let effective_heal_pct = if acc.healing_done > 0 {
                    (acc.healing_effective as f32 / acc.healing_done as f32) * 100.0
                } else {
                    0.0
                };
                let defense_pct = if acc.attacks_received > 0 {
                    (acc.defense_count as f32 / acc.attacks_received as f32) * 100.0
                } else {
                    0.0
                };
                let shield_pct = if acc.hits_received > 0 {
                    (acc.shield_roll_count as f32 / acc.hits_received as f32) * 100.0
                } else {
                    0.0
                };

                // Look up discipline info from session-level registry (source of truth)
                let (discipline, discipline_name, class_name) =
                    if let Some(player) = player_disciplines.get(id) {
                        let disc = Discipline::from_guid(player.discipline_id);
                        let disc_name = if player.discipline_name.is_empty() {
                            None
                        } else {
                            Some(player.discipline_name.clone())
                        };
                        // Derive class_name from Discipline enum (English) for CSS matching
                        let cls_name = disc.map(|d| format!("{:?}", d.class()));
                        (disc, disc_name, cls_name)
                    } else {
                        (None, None, None)
                    };

                Some(EntityMetrics {
                    entity_id: *id,
                    entity_type,
                    name,
                    discipline,
                    discipline_name,
                    class_name,
                    total_damage: acc.damage_dealt,
                    total_damage_boss: acc.damge_dealt_boss,
                    total_damage_effective: acc.damage_dealt_effective,
                    dps: (acc.damage_dealt * 1000 / duration_ms) as i32,
                    edps: (acc.damage_dealt_effective * 1000 / duration_ms) as i32,
                    bossdps: (acc.damge_dealt_boss * 1000 / duration_ms) as i32,
                    damage_crit_pct,
                    total_healing: acc.healing_done + acc.shielding_given,
                    total_healing_effective: acc.healing_effective + acc.shielding_given,
                    hps: ((acc.healing_done + acc.shielding_given) * 1000 / duration_ms) as i32,
                    ehps: ((acc.healing_effective + acc.shielding_given) * 1000 / duration_ms)
                        as i32,
                    heal_crit_pct,
                    effective_heal_pct,
                    abs: (acc.shielding_given * 1000 / duration_ms) as i32,
                    total_shielding: acc.shielding_given,
                    total_damage_taken: acc.damage_received,
                    total_damage_taken_effective: acc.damage_received_effective,
                    dtps: (acc.damage_received * 1000 / duration_ms) as i32,
                    edtps: (acc.damage_received_effective * 1000 / duration_ms) as i32,
                    htps: (acc.healing_received * 1000 / duration_ms) as i32,
                    ehtps: (acc.healing_received_effective * 1000 / duration_ms) as i32,
                    total_healing_received: acc.healing_received,
                    total_healing_received_effective: acc.healing_received_effective,
                    defense_pct,
                    shield_pct,
                    total_shield_absorbed: acc.shield_roll_absorbed,
                    taunt_count: acc.taunt_count,
                    apm: (acc.actions as f32 * 60000.0 / duration_ms as f32),
                    tps: (acc.threat_generated * 1000.0 / duration_ms as f64) as i32,
                    total_threat: acc.threat_generated as i64,
                })
            })
            .collect();

        stats.sort_by(|a, b| b.dps.cmp(&a.dps));
        Some(stats)
    }
}
