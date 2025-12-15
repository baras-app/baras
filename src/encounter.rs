use crate::{
    CombatEvent,
    log_ids::{effect_id, effect_type_id},
};
use std::collections::VecDeque;
use time::{Date, PrimitiveDateTime, Time};

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
    // Summary fields populated on state transitions
    pub participant_ids: Vec<i64>, // unique source/target log_ids (NPCs)
}

impl Encounter {
    pub fn new(id: u64) -> Self {
        Self {
            id,
            state: EncounterState::NotStarted,
            events: Vec::new(),
            enter_combat_time: None,
            exit_combat_time: None,
            participant_ids: Vec::new(),
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
}

pub struct SessionCache {
    // Session metadata
    pub session_date: Date, // from filename, used for timestamp date assignment

    // Player state
    pub player: PlayerInfo,
    pub player_initialized: bool,

    // Area state
    pub current_area: AreaInfo,

    // Encounter tracking - fixed-size window
    encounters: VecDeque<Encounter>, // most recent at back, max 3
    next_encounter_id: u64,

    // Post-combat grace period for trailing damage
    post_combat_threshold_ms: i64,
}

impl SessionCache {
    pub fn new(session_date: Date) -> Self {
        let mut cache = Self {
            session_date,
            player: PlayerInfo::default(),
            player_initialized: false,
            current_area: AreaInfo::default(),
            encounters: VecDeque::with_capacity(3),
            next_encounter_id: 0,
            post_combat_threshold_ms: 5000,
        };
        cache.push_new_encounter();
        cache
    }

    /// Process an incoming event and route it appropriately
    pub fn process_event(&mut self, event: CombatEvent) {
        // 1. Update player info on DisciplineChanged
        // Allow first event to initialize player, subsequent ones must match player id
        if event.effect.type_id == effect_type_id::DISCIPLINECHANGED
            && (!self.player_initialized || event.source_entity.log_id == self.player.id)
        {
            self.update_player_from_event(&event);
        }

        // 2. Update area on AreaEntered
        if event.effect.type_id == effect_type_id::AREAENTERED {
            self.update_area_from_event(&event);
        }

        // 3. Route event to appropriate encounter
        self.route_event_to_encounter(event);
    }

    fn update_player_from_event(&mut self, event: &CombatEvent) {
        // First DisciplineChanged sets player identity, subsequent ones update discipline
        if !self.player_initialized {
            self.player.name = event.source_entity.name.clone();
            self.player.id = event.source_entity.log_id;
            self.player_initialized = true;
        }
        // Always update discipline from the event
        self.player.class_name = event.effect.effect_name.clone();
        self.player.class_id = event.effect.effect_id;
        self.player.discipline_id = event.effect.discipline_id;
        self.player.discipline_name = event.effect.discipline_name.clone();
    }

    fn update_area_from_event(&mut self, event: &CombatEvent) {
        // Extract area info from the event - adjust field access based on actual data
        self.current_area.area_name = event.effect.effect_name.clone();
        self.current_area.area_id = event.effect.effect_id;
        self.current_area.entered_at = Some(event.timestamp);
    }

    fn route_event_to_encounter(&mut self, event: CombatEvent) {
        let effect_id = event.effect.effect_id;
        let timestamp = event.timestamp;

        // Get current encounter state
        let current_state = self
            .current_encounter()
            .map(|e| e.state.clone())
            .unwrap_or_default();

        match current_state {
            EncounterState::NotStarted => {
                if effect_id == effect_id::ENTERCOMBAT {
                    // Transition to InCombat
                    if let Some(enc) = self.current_encounter_mut() {
                        enc.state = EncounterState::InCombat;
                        enc.enter_combat_time = Some(timestamp);
                        enc.events.push(event);
                    }
                } else if effect_id == effect_id::DAMAGE {
                    // Damage before combat starts - discard or ignore
                    // (shouldn't normally happen unless log is mid-combat)
                } else {
                    // Buffer non-damage events for the upcoming encounter
                    if let Some(enc) = self.current_encounter_mut() {
                        enc.events.push(event);
                    }
                }
            }

            EncounterState::InCombat => {
                if effect_id == effect_id::EXITCOMBAT {
                    // Transition to PostCombat
                    if let Some(enc) = self.current_encounter_mut() {
                        enc.exit_combat_time = Some(timestamp);
                        enc.state = EncounterState::PostCombat {
                            exit_time: timestamp,
                        };
                        enc.events.push(event);
                    }
                } else {
                    // Collect all events during combat
                    if let Some(enc) = self.current_encounter_mut() {
                        enc.events.push(event);
                    }
                }
            }

            EncounterState::PostCombat { exit_time } => {
                if effect_id == effect_id::ENTERCOMBAT {
                    // New combat starting - finalize current, start new
                    self.finalize_and_start_new();
                    if let Some(enc) = self.current_encounter_mut() {
                        enc.state = EncounterState::InCombat;
                        enc.enter_combat_time = Some(timestamp);
                        enc.events.push(event);
                    }
                } else if effect_id == effect_id::DAMAGE {
                    // Damage in post-combat - check if within grace period
                    let elapsed = timestamp.duration_since(exit_time).whole_milliseconds();
                    if elapsed <= self.post_combat_threshold_ms as i128 {
                        // Assign to current (ending) encounter
                        if let Some(enc) = self.current_encounter_mut() {
                            enc.events.push(event);
                        }
                    } else {
                        // Beyond grace period - finalize encounter
                        // Discard this damage event (orphaned)
                        self.finalize_and_start_new();
                    }
                } else {
                    // Non-damage event after combat - goes to next encounter
                    self.finalize_and_start_new();
                    if let Some(enc) = self.current_encounter_mut() {
                        enc.events.push(event);
                    }
                }
            }
        }
    }

    /// Finalize current encounter and push a new NotStarted one
    fn finalize_and_start_new(&mut self) {
        self.push_new_encounter();
        self.trim_old_encounters();
    }

    fn push_new_encounter(&mut self) {
        let id = self.next_encounter_id;
        self.next_encounter_id += 1;
        self.encounters.push_back(Encounter::new(id));
    }

    /// Keep only the last 3 encounters
    fn trim_old_encounters(&mut self) {
        while self.encounters.len() > 3 {
            self.encounters.pop_front();
        }
    }

    // --- Accessors ---

    /// Current (most recent) encounter
    pub fn current_encounter(&self) -> Option<&Encounter> {
        self.encounters.back()
    }

    pub fn current_encounter_mut(&mut self) -> Option<&mut Encounter> {
        self.encounters.back_mut()
    }

    /// All tracked encounters (up to 3), oldest first
    pub fn encounters(&self) -> impl Iterator<Item = &Encounter> {
        self.encounters.iter()
    }

    /// Get encounter by id if still in cache
    pub fn encounter_by_id(&self, id: u64) -> Option<&Encounter> {
        self.encounters.iter().find(|e| e.id == id)
    }

    /// All events across all cached encounters
    pub fn all_cached_events(&self) -> impl Iterator<Item = &CombatEvent> {
        self.encounters.iter().flat_map(|e| e.events.iter())
    }

    /// Resolve a Time to full PrimitiveDateTime using session date
    /// Handles midnight rollover by checking if time < previous time
    pub fn resolve_datetime(&self, time: Time, previous: Option<Time>) -> PrimitiveDateTime {
        let date = match previous {
            Some(prev) if time < prev => self.session_date.next_day().unwrap_or(self.session_date),
            _ => self.session_date,
        };
        PrimitiveDateTime::new(date, time)
    }

    /// Print session and encounter metadata (excludes event lists)
    pub fn print_metadata(&self) {
        println!("=== Session Metadata ===");
        println!("Session date: {}", self.session_date);
        println!();

        println!("--- Player Info ---");
        println!("  Name: {}", self.player.name);
        println!("  ID: {}", self.player.id);
        println!(
            "  Class: {} (id: {})",
            self.player.class_name, self.player.class_id
        );
        println!(
            "  Discipline: {} (id: {})",
            self.player.discipline_name, self.player.discipline_id
        );
        println!("  Initialized: {}", self.player_initialized);
        println!();

        println!("--- Current Area ---");
        println!("  Name: {}", self.current_area.area_name);
        println!("  ID: {}", self.current_area.area_id);
        println!(
            "  Entered at: {}",
            self.current_area
                .entered_at
                .map(|t| t.to_string())
                .unwrap_or_else(|| "N/A".to_string())
        );
        println!();

        println!("--- Encounters ({} cached) ---", self.encounters.len());
        for enc in &self.encounters {
            println!("  Encounter #{}", enc.id);
            println!("    State: {:?}", enc.state);
            println!(
                "    Enter combat: {}",
                enc.enter_combat_time
                    .map(|t| t.to_string())
                    .unwrap_or_else(|| "N/A".to_string())
            );
            println!(
                "    Exit combat: {}",
                enc.exit_combat_time
                    .map(|t| t.to_string())
                    .unwrap_or_else(|| "N/A".to_string())
            );
            println!(
                "    Duration: {}",
                enc.duration_ms()
                    .map(|ms| format!("{}ms", ms))
                    .unwrap_or_else(|| "N/A".to_string())
            );
            println!("    Event count: {}", enc.events.len());
            println!("    Participant IDs: {:?}", enc.participant_ids);
        }
    }
}
