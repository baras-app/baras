use crate::CombatEvent;
use crate::Entity;
use crate::EntityType;
use crate::log_ids::effect_id;
use crate::swtor_ids::SHIELD_EFFECT_IDS;
use chrono::{NaiveDateTime, TimeDelta};
use hashbrown::HashMap;

#[derive(Debug, Clone, Default, PartialEq)]
pub enum EncounterState {
    #[default]
    NotStarted,
    InCombat,
    PostCombat {
        exit_time: NaiveDateTime,
    },
}

#[derive(Debug, Clone, Default)]
pub struct PlayerInfo {
    pub name: String,
    pub id: i64,
    pub class_id: i64,
    pub class_name: String,
    pub discipline_id: i64,
    pub discipline_name: String,
    pub is_dead: bool,
    pub death_time: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, Default)]
pub struct NpcInfo {
    pub name: String,
    pub entity_type: EntityType,
    pub display_name: Option<String>,
    pub log_id: i64,
    pub class_id: i64,
    pub is_dead: bool,
    pub first_seen_at: Option<NaiveDateTime>,
    pub death_time: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, Default)]
pub struct AreaInfo {
    pub area_name: String,
    pub area_id: i64,
    pub entered_at: Option<NaiveDateTime>,
}

#[derive(Debug, Clone)]
pub struct EntityMetrics {
    pub entity_id: i64,
    pub name: String,
    pub total_damage: i64,
    pub dps: i32,
    pub edps: i32,
    pub hps: i32,
    pub ehps: i32,
    pub dtps: i32,
    pub abs: i32,
    pub total_healing: i64,
    pub apm: f32,
}

#[derive(Debug, Clone, Default)]
pub struct MetricAccumulator {
    damage_dealt: i64,
    damage_dealt_effective: i64,
    damage_received: i64,
    damage_absorbed: i64,
    healing_effective: i64,
    healing_done: i64,
    healing_received: i64,
    hit_count: u32,
    actions: u32,
    shielding_given: i64,
}

#[derive(Debug, Clone)]
pub struct EffectInstance {
    pub effect_id: i64,
    pub source_id: i64,
    pub target_id: i64,
    pub applied_at: NaiveDateTime,
    pub removed_at: Option<NaiveDateTime>,
    pub is_shield: bool,
}

#[derive(Debug, Clone)]
pub struct Encounter {
    pub id: u64,
    pub state: EncounterState,
    pub events: Vec<CombatEvent>,
    pub enter_combat_time: Option<NaiveDateTime>,
    pub exit_combat_time: Option<NaiveDateTime>,
    pub last_combat_activity_time: Option<NaiveDateTime>,
    // Summary fields populated on state transitions
    pub players: HashMap<i64, PlayerInfo>,
    pub npcs: HashMap<i64, NpcInfo>,
    pub all_players_dead: bool,
    pub effects: HashMap<i64, Vec<EffectInstance>>,
    pub accumulated_data: HashMap<i64, MetricAccumulator>,
}

impl Encounter {
    pub fn new(id: u64) -> Self {
        Self {
            id,
            state: EncounterState::NotStarted,
            events: Vec::new(),
            enter_combat_time: None,
            exit_combat_time: None,
            last_combat_activity_time: None,
            players: HashMap::new(),
            npcs: HashMap::new(),
            effects: HashMap::new(),
            all_players_dead: false,
            accumulated_data: HashMap::new(),
        }
    }

    pub fn with_player(id: u64, player: PlayerInfo) -> Self {
        let mut enc = Self::new(id);
        enc.players.insert(player.id, player);
        enc
    }

    // --- Player State

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
        self.all_players_dead = !self.players.is_empty() && self.players.values().all(|p| p.is_dead)
    }
    pub fn track_event_entities(&mut self, event: &CombatEvent) {
        self.try_track_entity(&event.source_entity, event.timestamp);
        self.try_track_entity(&event.target_entity, event.timestamp);
    }

    #[inline]
    fn try_track_entity(&mut self, entity: &Entity, timestamp: NaiveDateTime) {
        match entity.entity_type {
            EntityType::Player => {
                self.players
                    .entry(entity.log_id)
                    .or_insert_with(|| PlayerInfo {
                        id: entity.log_id,
                        name: entity.name.clone(),
                        ..Default::default()
                    });
            }
            EntityType::Npc | EntityType::Companion => {
                self.npcs.entry(entity.log_id).or_insert_with(|| NpcInfo {
                    name: entity.name.clone(),
                    entity_type: entity.entity_type.clone(),
                    log_id: entity.log_id,
                    class_id: entity.class_id,
                    first_seen_at: Some(timestamp),
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

    // -- Time Utils

    pub fn duration_ms(&self) -> Option<i64> {
        match (self.enter_combat_time, self.exit_combat_time) {
            (Some(enter), Some(exit)) => Some(exit.signed_duration_since(enter).num_milliseconds()),
            _ => None,
        }
    }

    fn duration_seconds(&self) -> Option<i64> {
        let enter = self.enter_combat_time?;
        let terminal = match self.exit_combat_time {
            Some(exit) => exit,
            None => chrono::offset::Local::now().naive_local(),
        };

        let mut duration = terminal.signed_duration_since(enter);

        // If negative, we crossed midnight - add 24 hours
        if duration.num_milliseconds().is_negative() {
            duration = duration.checked_add(&TimeDelta::days(1))?;
        }

        Some(duration.num_seconds())
    }

    fn get_entity_name(&self, id: i64) -> Option<String> {
        let name = self.players.get(&id).map(|e| e.name.clone());
        if name.is_none() {
            return self.npcs.get(&id).map(|e| e.name.clone());
        }

        name
    }

    // -- Effect Instance Handling

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
            })
    }

    pub fn remove_effect(&mut self, event: &CombatEvent) {
        // fail gracefully if target not present
        let Some(effects) = self.effects.get_mut(&event.target_entity.log_id) else {
            return;
        };
        for effect_instance in effects.iter_mut().rev() {
            if effect_instance.effect_id == event.effect.effect_id
                && effect_instance.source_id == event.source_entity.log_id
                && effect_instance.removed_at.is_none()
            {
                effect_instance.removed_at = Some(event.timestamp);
                return;
            }
        }
    }

    // ---- Metrics ----

    // Read in a distinct event line and grab the numerators of various metrics for easy access
    // when calculating
    pub fn accumulate_data(&mut self, event: &CombatEvent) {
        {
            let source_accumulator = self
                .accumulated_data
                .entry(event.source_entity.log_id)
                .or_default();
            source_accumulator.damage_dealt += event.details.dmg_amount as i64;
            source_accumulator.damage_dealt_effective += event.details.dmg_effective as i64;
            source_accumulator.hit_count += 1;
            source_accumulator.healing_effective += event.details.heal_effective as i64; // adjust field name
            source_accumulator.healing_done += event.details.heal_amount as i64; // adjust field name
            if event.effect.effect_id == effect_id::ABILITYACTIVATE
                && self.enter_combat_time.is_some_and(|t| event.timestamp >= t)
                && self.exit_combat_time.is_none_or(|t| t >= event.timestamp)
            {
                source_accumulator.actions += 1;
            }

            if event.details.dmg_absorbed > 0
                && (event.details.avoid_type.is_empty() || event.details.avoid_type == "shield")
            {
                // TODO: This code is hacky with an arbitrary time cutoff
                if let Some(effects) = self.effects.get(&event.target_entity.log_id) {
                    let earliest_shield_effect = effects
                        .iter()
                        .filter(|e| {
                            e.is_shield
                                && e.applied_at < event.timestamp
                                && (e.removed_at.is_none_or(|t| t >= event.timestamp)
                                    || e.removed_at
                                        .unwrap()
                                        .signed_duration_since(event.timestamp)
                                        .num_milliseconds()
                                        <= 750)
                        })
                        .min_by_key(|e| e.applied_at);
                    if let Some(shield) = earliest_shield_effect {
                        let shield_source_acc =
                            self.accumulated_data.entry(shield.source_id).or_default();
                        shield_source_acc.shielding_given += event.details.dmg_absorbed as i64;
                    }
                }
            }
        }

        {
            let target_accumulator = self
                .accumulated_data
                .entry(event.target_entity.log_id)
                .or_default();
            target_accumulator.damage_received += event.details.dmg_effective as i64;
            target_accumulator.damage_absorbed += event.details.dmg_absorbed as i64;
            target_accumulator.healing_received += event.details.heal_amount as i64;
        }
    }

    pub fn calculuate_entity_metrics(&self) -> Option<Vec<EntityMetrics>> {
        let duration = self.duration_seconds()?;
        if duration <= 0 {
            return None;
        }

        let accumulators = &self.accumulated_data;

        let mut stats: Vec<EntityMetrics> = accumulators
            .into_iter()
            .map(|(id, acc)| EntityMetrics {
                entity_id: *id,
                name: self.get_entity_name(*id).unwrap_or_default(),
                total_damage: acc.damage_dealt,
                dps: (acc.damage_dealt / duration) as i32,
                edps: (acc.damage_dealt_effective / duration) as i32,
                ehps: (acc.healing_effective / duration) as i32,
                total_healing: acc.healing_done,
                hps: (acc.healing_done / duration) as i32,
                dtps: (acc.damage_received / duration) as i32,
                abs: (acc.shielding_given / duration) as i32,
                apm: (acc.actions as f32 / duration as f32) * 60.0,
            })
            .filter(|e| !e.name.is_empty())
            .collect();

        stats.sort_by(|a, b| b.dps.cmp(&a.dps));
        Some(stats)
    }

    pub fn show_dps(&self) {
        let stats = self.calculuate_entity_metrics().unwrap_or_default();

        for entity in stats {
            println!(
                "      [{}: {} dps | {} edps | {} total_abs || {} total heals | {} hps | {} ehps | {} abs | {} apm] ",
                entity.name,
                entity.dps,
                entity.edps,
                entity.dtps,
                entity.total_healing,
                entity.hps,
                entity.ehps,
                entity.abs,
                entity.apm
            );
        }
    }
}
