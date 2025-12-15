use crate::CombatEvent;
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
            all_players_dead: false,
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
