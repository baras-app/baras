pub mod metrics;
pub mod effect_instance;
pub mod shielding;
pub mod entity_info;
pub mod summary;
use crate::is_boss;
use crate::combat_log::{CombatEvent, Entity, EntityType};
use crate::context::{resolve, IStr};
use entity_info::PlayerInfo;
use crate::game_data::effect_id;
use crate::game_data::SHIELD_EFFECT_IDS;
use chrono::{NaiveDateTime, TimeDelta};
use hashbrown::HashMap;
use metrics::{MetricAccumulator, EntityMetrics};
use entity_info::NpcInfo;
use effect_instance::EffectInstance;


#[derive(Debug, Clone, Default, PartialEq)]
pub enum EncounterState {
    #[default]
    NotStarted,
    InCombat,
    PostCombat { exit_time: NaiveDateTime },
}

/// Classification of the phase/content type where an encounter occurred
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
pub enum PhaseType {
    #[default]
    OpenWorld,
    Raid,
    Flashpoint,
    PvP,
    DummyParse,
}

/// Real-time boss health data for overlay display
#[derive(Debug, Clone, serde::Serialize)]
pub struct BossHealthEntry {
    pub name: String,
    pub current: i32,
    pub max: i32,
    /// Used for sorting by encounter order (not serialized)
    #[serde(skip)]
    pub first_seen_at: Option<NaiveDateTime>,
}

impl BossHealthEntry {
    pub fn percent(&self) -> f32 {
        if self.max > 0 {
            (self.current as f32 / self.max as f32) * 100.0
        } else {
            0.0
        }
    }
}



#[derive(Debug, Clone)]
pub struct Encounter {
    pub id: u64,
    pub state: EncounterState,
    pub events: Vec<CombatEvent>,
    pub enter_combat_time: Option<NaiveDateTime>,
    pub exit_combat_time: Option<NaiveDateTime>,
    pub last_combat_activity_time: Option<NaiveDateTime>,
    pub players: HashMap<i64, PlayerInfo>,
    pub npcs: HashMap<i64, NpcInfo>,
    pub all_players_dead: bool,
    pub effects: HashMap<i64, Vec<EffectInstance>>,
    pub accumulated_data: HashMap<i64, MetricAccumulator>,
    /// Pending shield absorptions waiting for resolution (target_id -> pending events)
    pub pending_absorptions: HashMap<i64, Vec<shielding::PendingAbsorption>>,
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
            pending_absorptions: HashMap::new(),
        }
    }

    pub fn with_player(id: u64, player: PlayerInfo) -> Self {
        let mut enc = Self::new(id);
        enc.players.insert(player.id, player);
        enc
    }

    // --- Entity State ---

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
            !self.players.is_empty() && self.players.values().all(|p| p.is_dead)
    }

    pub fn track_event_entities(&mut self, event: &CombatEvent) {
        // Guard against entities not in combat that the player
        // targeted from being added
        if event.effect.type_id == effect_id::TARGETSET ||
            event.effect.type_id == effect_id::TARGETCLEARED {
            return;
    }
        self.try_track_entity(&event.source_entity, event.timestamp);
        self.try_track_entity(&event.target_entity, event.timestamp);

        // Update health for NPCs on every event
        self.update_npc_health(&event.source_entity);
        self.update_npc_health(&event.target_entity);
    }

    /// Update NPC health from entity data (called on every combat event)
    #[inline]
    fn update_npc_health(&mut self, entity: &Entity) {
        if let Some(npc) = self.npcs.get_mut(&entity.log_id) {
            npc.health = entity.health;
        }
    }

    #[inline]
    fn try_track_entity(&mut self, entity: &Entity, timestamp: NaiveDateTime) {
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
                self.npcs.entry(entity.log_id).or_insert_with(|| NpcInfo {
                    name: entity.name,
                    entity_type: entity.entity_type,
                    log_id: entity.log_id,
                    class_id: entity.class_id,
                    first_seen_at: Some(timestamp),
                    health: entity.health,
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

    // --- Time Utils ---

     pub fn duration_seconds(&self) -> Option<i64> {
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
            self.npcs
                .get(&id)
                .map(|e| e.entity_type.clone())
        }
    }

    // --- Effect Instance Handling ---

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
                has_absorbed: false,
            })
    }

    pub fn remove_effect(&mut self, event: &CombatEvent) {
        let target_id = event.target_entity.log_id;
        let Some(effects) = self.effects.get_mut(&target_id) else {
            return;
        };

        let mut removed_shield: Option<EffectInstance> = None;
        for effect_instance in effects.iter_mut().rev() {
            if effect_instance.effect_id == event.effect.effect_id
                && effect_instance.source_id == event.source_entity.log_id
                && effect_instance.removed_at.is_none()
            {
                effect_instance.removed_at = Some(event.timestamp);
                if effect_instance.is_shield {
                    removed_shield = Some(effect_instance.clone());
                }
                break;
            }
        }

        // Resolve pending absorptions when a shield ends
        if let Some(shield) = removed_shield {
            self.resolve_pending_absorptions(target_id, &shield);
        }
    }

    pub fn accumulate_data(&mut self, event: &CombatEvent) {
        let avoid = resolve(event.details.avoid_type);
        let is_defense = matches!(avoid, "dodge" | "parry" | "resist" | "deflect");
        let is_natural_shield = avoid == "shield";

        // Source accumulation (damage/healing dealt)
        {
            let source = self
                .accumulated_data
                .entry(event.source_entity.log_id)
                .or_default();

            // Damage dealt
            if event.details.dmg_amount > 0 {
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

            // Healing dealt
            if event.details.heal_amount > 0 {
                source.healing_done += event.details.heal_amount as i64;
                source.healing_effective += event.details.heal_effective as i64;
                source.heal_count += 1;
                if event.details.is_crit {
                    source.heal_crit_count += 1;
                }
            }

            source.threat_generated += event.details.threat as f64;

            // Actions (APM tracking)
            if event.effect.effect_id == effect_id::ABILITYACTIVATE
                && self.enter_combat_time.is_some_and(|t| event.timestamp >= t)
                && self.exit_combat_time.is_none_or(|t| t >= event.timestamp)
            {
                source.actions += 1;
            }

            // Taunt tracking
            if event.effect.effect_id == effect_id::TAUNT {
                source.taunt_count += 1;
            }

            // Effect shield absorption (Static Barrier, etc.)
            if event.details.dmg_absorbed > 0 && !is_natural_shield {
                self.attribute_shield_absorption(event);
            }
        }

        // Target accumulation (damage/healing received)
        {
            let target = self
                .accumulated_data
                .entry(event.target_entity.log_id)
                .or_default();

            // Damage received
            if event.details.dmg_amount > 0 {
                target.damage_received += event.details.dmg_amount as i64;
                target.damage_received_effective += event.details.dmg_effective as i64;
                target.damage_absorbed += event.details.dmg_absorbed as i64;
                target.attacks_received += 1;

                // Defense stats
                if is_defense {
                    target.defense_count += 1;
                }

                // Natural shield roll (tank stat)
                if is_natural_shield {
                    target.shield_roll_count += 1;
                    target.shield_roll_absorbed += event.details.dmg_absorbed as i64;
                }
            }

            // Healing received
            if event.details.heal_amount > 0 {
                target.healing_received += event.details.heal_amount as i64;
                target.healing_received_effective += event.details.heal_effective as i64;
            }
        }
    }

    pub fn calculate_entity_metrics(&self) -> Option<Vec<EntityMetrics>> {
        let duration = self.duration_seconds()?;
        if duration <= 0 {
            return None;
        }

        let accumulators = &self.accumulated_data;

        let mut stats: Vec<EntityMetrics> = accumulators
            .iter()
            .filter_map(|(id, acc)| {
                let name = self.get_entity_name(*id)?;
                let entity_type = self.get_entity_type(*id)?;

                // Crit percentages (avoid division by zero)
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

                // Effective heal percentage
                let effective_heal_pct = if acc.healing_done > 0 {
                    (acc.healing_effective as f32 / acc.healing_done as f32) * 100.0
                } else {
                    0.0
                };

                // Defense percentage (dodge/parry/resist/deflect)
                let defense_pct = if acc.attacks_received > 0 {
                    (acc.defense_count as f32 / acc.attacks_received as f32) * 100.0
                } else {
                    0.0
                };

                // Shield percentage (natural shield rolls)
                let shield_pct = if acc.attacks_received > 0 {
                    (acc.shield_roll_count as f32 / acc.attacks_received as f32) * 100.0
                } else {
                    0.0
                };

                Some(EntityMetrics {
                    entity_id: *id,
                    entity_type,
                    name,

                    // Damage dealing
                    total_damage: acc.damage_dealt,
                    total_damage_boss: acc.damge_dealt_boss,
                    total_damage_effective: acc.damage_dealt_effective,
                    dps: (acc.damage_dealt / duration) as i32,
                    edps: (acc.damage_dealt_effective / duration) as i32,
                    bossdps: (acc.damge_dealt_boss / duration) as i32,
                    damage_crit_pct,

                    // Healing dealing
                    total_healing: acc.healing_done,
                    total_healing_effective: acc.healing_effective,
                    hps: (acc.healing_done / duration) as i32,
                    ehps: ((acc.healing_effective + acc.shielding_given) / duration) as i32,
                    heal_crit_pct,
                    effective_heal_pct,

                    // Shielding
                    abs: (acc.shielding_given / duration) as i32,
                    total_shielding: acc.shielding_given,

                    // Damage taken
                    total_damage_taken: acc.damage_received,
                    total_damage_taken_effective: acc.damage_received_effective,
                    dtps: (acc.damage_received / duration) as i32,
                    edtps: (acc.damage_received_effective / duration) as i32,

                    // Healing received
                    htps: (acc.healing_received / duration) as i32,
                    ehtps: (acc.healing_received_effective / duration) as i32,

                    // Tank stats
                    defense_pct,
                    shield_pct,
                    total_shield_absorbed: acc.shield_roll_absorbed,
                    taunt_count: acc.taunt_count,

                    // General
                    apm: (acc.actions as f32 / duration as f32) * 60.0,
                    tps: (acc.threat_generated / duration as f64) as i32,
                    total_threat: acc.threat_generated as i64,
                })
            })
            .collect();

        stats.sort_by(|a, b| b.dps.cmp(&a.dps));
        Some(stats)
    }

}
