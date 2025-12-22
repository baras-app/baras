use crate::encounter::{Encounter, EncounterState,};
use crate::encounter::entity_info::PlayerInfo;
use crate::session::info::{AreaInfo};
use std::collections::VecDeque;

const CACHE_DEFAULT_CAPACITY: usize = 3;

/// Pure storage for session state.
/// Routing logic lives in EventProcessor.
#[derive(Debug, Clone, Default)]
pub struct SessionCache {
    // Player state
    pub player: PlayerInfo,
    pub player_initialized: bool,

    // Area state
    pub current_area: AreaInfo,

    // Encounter tracking - fixed-size window
    encounters: VecDeque<Encounter>,
    next_encounter_id: u64,
}

impl SessionCache {
    pub fn new() -> Self {
        let mut cache = Self {
            player: PlayerInfo::default(),
            player_initialized: false,
            current_area: AreaInfo::default(),
            encounters: VecDeque::with_capacity(CACHE_DEFAULT_CAPACITY),
            next_encounter_id: 0,
        };
        cache.push_new_encounter();
        cache
    }

    // --- Encounter Management ---

    pub fn push_new_encounter(&mut self) -> u64 {
        let id = self.next_encounter_id;

        let encounter = if self.player_initialized {
            Encounter::with_player(id, self.player.clone())
        } else {
            Encounter::new(id)
        };

        self.next_encounter_id += 1;
        self.encounters.push_back(encounter);
        self.trim_old_encounters();
        id
    }

    fn trim_old_encounters(&mut self) {
        while self.encounters.len() > CACHE_DEFAULT_CAPACITY {
            self.encounters.pop_front();
        }
    }

    // --- Accessors ---

    pub fn current_encounter(&self) -> Option<&Encounter> {
        self.encounters.back()
    }

    pub fn current_encounter_mut(&mut self) -> Option<&mut Encounter> {
        self.encounters.back_mut()
    }

    pub fn encounters(&self) -> impl Iterator<Item = &Encounter> {
        self.encounters.iter()
    }

    pub fn encounter_by_id(&self, id: u64) -> Option<&Encounter> {
        self.encounters.iter().find(|e| e.id == id)
    }

    pub fn last_combat_encounter(&self) -> Option<&Encounter> {
        self.encounters
            .iter()
            .rfind(|e| e.state != EncounterState::NotStarted)
    }

    pub fn last_combat_encounter_mut(&mut self) -> Option<&mut Encounter> {
        self.encounters
            .iter_mut()
            .rfind(|e| e.state != EncounterState::NotStarted)
    }

    pub fn encounter_count(&self) -> usize {
        self.encounters.len()
    }

    // --- Debug/Display ---

    /// Print session and encounter metadata (excludes event lists)
    pub fn print_metadata(&self) {
        print!("function deprecated")
    }
}
