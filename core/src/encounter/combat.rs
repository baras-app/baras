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
    pub last_combat_activity_time: Option<NaiveDateTime>,

    // ─── Entity Tracking ────────────────────────────────────────────────────
    /// Players in this encounter
    pub players: HashMap<i64, PlayerInfo>,
    /// NPCs in this encounter
    pub npcs: HashMap<i64, NpcInfo>,
    /// Whether all players are dead
    pub all_players_dead: bool,

    // ─── Effect Instances (for shield attribution) ──────────────────────────
    /// Active effects by target ID
    pub effects: HashMap<i64, Vec<EffectInstance>>,

    // ─── Metrics ────────────────────────────────────────────────────────────
    /// Accumulated damage/healing/etc. data by entity ID
    pub accumulated_data: HashMap<i64, MetricAccumulator>,
    /// Challenge metrics for boss encounters
    pub challenge_tracker: ChallengeTracker,
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
            last_combat_activity_time: None,

            // Entity tracking
            players: HashMap::new(),
            npcs: HashMap::new(),
            all_players_dead: false,

            // Effects
            effects: HashMap::new(),

            // Metrics
            accumulated_data: HashMap::new(),
            challenge_tracker: ChallengeTracker::new(),
        }
    }

    /// Create with a pre-registered local player
    pub fn with_player(id: u64, mode: ProcessingMode, player: PlayerInfo) -> Self {
        let mut enc = Self::new(id, mode);
        enc.players.insert(player.id, player);
        enc
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

    /// Set the encounter difficulty
    pub fn set_difficulty(&mut self, difficulty: Option<Difficulty>) {
        self.difficulty = difficulty;
    }

    /// Set the area context for this encounter
    pub fn set_area(&mut self, area_id: Option<i64>, area_name: Option<String>) {
        self.area_id = area_id;
        self.area_name = area_name;
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
        if (old_percent - new_pct).abs() > 0.01 {
            Some((old_percent, new_pct))
        } else {
            None
        }
    }

    /// Get HP percentage for a specific NPC ID
    pub fn get_npc_hp_pct(&self, npc_id: i64) -> Option<f32> {
        self.npcs.get(&npc_id).map(|n| n.hp_percent())
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
            .filter(|npc| entity_class_ids.contains(&npc.class_id))
            .map(|npc| OverlayHealthEntry {
                name: crate::context::resolve(npc.name).to_string(),
                target_name: self
                    .players
                    .get(&npc.current_target_id)
                    .map(|p| crate::context::resolve(p.name).to_string()),
                current: npc.current_hp,
                max: npc.max_hp,
                first_seen_at: npc.first_seen_at,
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

    /// Decrement a counter (saturates at 0)
    pub fn decrement_counter(&mut self, counter_id: &str) -> u32 {
        let count = self.counters.entry(counter_id.to_string()).or_insert(0);
        *count = count.saturating_sub(1);
        *count
    }

    /// Get the current value of a counter
    pub fn get_counter(&self, counter_id: &str) -> u32 {
        self.counters.get(counter_id).copied().unwrap_or(0)
    }

    /// Set a counter to a specific value
    pub fn set_counter(&mut self, counter_id: &str, value: u32) {
        self.counters.insert(counter_id.to_string(), value);
    }

    /// Reset a counter to 0
    pub fn reset_counter(&mut self, counter_id: &str) {
        self.counters.insert(counter_id.to_string(), 0);
    }

    /// Reset multiple counters to 0
    pub fn reset_counters(&mut self, counter_ids: &[String]) {
        for id in counter_ids {
            self.counters.insert(id.clone(), 0);
        }
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
    // Combat Time
    // ═══════════════════════════════════════════════════════════════════════

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
    pub fn duration_seconds(&self) -> Option<i64> {
        Some(self.duration_ms()? / 1000)
    }

    /// Get combat duration in milliseconds
    pub fn duration_ms(&self) -> Option<i64> {
        use chrono::TimeDelta;

        let enter = self.enter_combat_time?;
        let terminal = self
            .exit_combat_time
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
                    player.is_dead = false;
                    player.death_time = None;
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

    pub fn check_all_players_dead(&mut self) {
        self.all_players_dead =
            !self.players.is_empty() && self.players.values().all(|p| p.is_dead);
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
                    .or_insert_with(|| PlayerInfo {
                        id: entity.log_id,
                        name: entity.name,
                        ..Default::default()
                    });
            }
            EntityType::Npc | EntityType::Companion => {
                // Only register companions during active combat to avoid stale entries
                // from mount/dismount respawns in the "dead zone" between encounters
                if entity.entity_type == EntityType::Companion
                    && self.state != EncounterState::InCombat
                {
                    return;
                }

                self.npcs.entry(entity.log_id).or_insert_with(|| NpcInfo {
                    name: entity.name,
                    entity_type: entity.entity_type,
                    log_id: entity.log_id,
                    class_id: entity.class_id,
                    first_seen_at: Some(timestamp),
                    current_hp: entity.health.0,
                    max_hp: entity.health.1,
                    is_boss: is_boss(entity.class_id),
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
        } else if let Some(npc) = self.npcs.get(&entity_id) {
            if npc.current_target_id != 0 {
                return Some(npc.current_target_id);
            }
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
        use crate::is_boss;

        let defense_type = event.details.defense_type_id;
        let is_defense = matches!(
            defense_type,
            defense_type::DODGE
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

            if event.details.dmg_amount > 0 {
                target.damage_received += event.details.dmg_amount as i64;
                target.damage_received_effective += event.details.dmg_effective as i64;
                target.damage_absorbed += event.details.dmg_absorbed as i64;
                target.attacks_received += 1;

                if is_defense {
                    target.defense_count += 1;
                }

                if is_natural_shield {
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

    pub fn calculate_entity_metrics(
        &self,
        player_disciplines: &hashbrown::HashMap<i64, super::entity_info::PlayerInfo>,
    ) -> Option<Vec<super::metrics::EntityMetrics>> {
        use super::metrics::EntityMetrics;

        let duration_ms = self.duration_ms()?;
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
                let shield_pct = if acc.attacks_received > 0 {
                    (acc.shield_roll_count as f32 / acc.attacks_received as f32) * 100.0
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
                    ehps: ((acc.healing_effective + acc.shielding_given) * 1000 / duration_ms) as i32,
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
