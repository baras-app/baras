use crate::CombatEvent;
use crate::Entity;
use crate::EntityType;
use hashbrown::HashMap;
use time::Time;

#[derive(Debug, Clone, Default, PartialEq)]
pub enum EncounterState {
    #[default]
    NotStarted,
    InCombat,
    PostCombat {
        exit_time: Time,
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
    pub death_time: Option<Time>,
}

#[derive(Debug, Clone, Default)]
pub struct NpcInfo {
    pub name: String,
    pub entity_type: EntityType,
    pub display_name: Option<String>,
    pub log_id: i64,
    pub class_id: i64,
    pub is_dead: bool,
    pub first_seen_at: Option<Time>,
    pub death_time: Option<Time>,
}

#[derive(Debug, Clone, Default)]
pub struct AreaInfo {
    pub area_name: String,
    pub area_id: i64,
    pub entered_at: Option<Time>,
}

#[derive(Debug, Clone)]
pub struct Encounter {
    pub id: u64,
    pub state: EncounterState,
    pub events: Vec<CombatEvent>,
    pub enter_combat_time: Option<Time>,
    pub exit_combat_time: Option<Time>,
    pub last_combat_activity_time: Option<Time>,
    // Summary fields populated on state transitions
    pub players: HashMap<i64, PlayerInfo>,
    pub npcs: HashMap<i64, NpcInfo>,
    pub all_players_dead: bool,
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
            all_players_dead: false,
        }
    }

    pub fn with_player(id: u64, player: PlayerInfo) -> Self {
        let mut enc = Self::new(id);
        enc.players.insert(player.id, player);
        enc
    }

    pub fn set_entity_death(&mut self, entity_id: i64, entity_type: &EntityType, timestamp: Time) {
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
    pub fn track_event_entities(&mut self, event: &CombatEvent) {
        self.try_track_entity(&event.source_entity, event.timestamp);
        self.try_track_entity(&event.target_entity, event.timestamp);
    }

    #[inline]
    fn try_track_entity(&mut self, entity: &Entity, timestamp: Time) {
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

    pub fn duration_ms(&self) -> Option<i64> {
        match (self.enter_combat_time, self.exit_combat_time) {
            (Some(enter), Some(exit)) => {
                Some(exit.duration_since(enter).whole_milliseconds() as i64)
            }
            _ => None,
        }
    }
    pub fn check_all_players_dead(&mut self) {
        self.all_players_dead = !self.players.is_empty() && self.players.values().all(|p| p.is_dead)
    }
}
