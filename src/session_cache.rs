use crate::{
    combat_event::*,
    encounter::*,
    log_ids::{effect_id, effect_type_id},
};
use std::collections::VecDeque;
use time::{Date, PrimitiveDateTime, Time};

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

    // Track last exit combat time for damage-based combat start detection
    last_exit_combat_time: Option<Time>,
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
            last_exit_combat_time: None,
        };
        cache.push_new_encounter();
        cache
    }

    /// Process an incoming event and route it appropriately
    pub fn process_event(&mut self, event: CombatEvent) {
        // 1. Update player info on DisciplineChanged
        // Allow first event to initialize player, subsequent ones must match player id
        if event.effect.type_id == effect_type_id::DISCIPLINECHANGED {
            if !self.player_initialized || event.source_entity.log_id == self.player.id {
                self.update_primary_player(&event);
            }
            self.add_player_to_encounter(&event);
        }
        if event.effect.effect_id == effect_id::DEATH {
            let enc = self
                .current_encounter_mut()
                .expect("tried to call invalid enc");
            enc.set_entity_death(
                event.target_entity.log_id,
                &event.target_entity.entity_type,
                event.timestamp,
            );
            enc.check_all_players_dead();
        } else if event.effect.effect_id == effect_id::REVIVED {
            let enc = self
                .current_encounter_mut()
                .expect("tried to call invalid enc");
            enc.set_entity_alive(event.source_entity.log_id, &event.source_entity.entity_type);
            enc.check_all_players_dead();
        }
        // 2. Update area on AreaEntered
        if event.effect.type_id == effect_type_id::AREAENTERED {
            self.update_area_from_event(&event);
        }
        // 3. Route event to appropriate encounter
        self.route_event_to_encounter(event);
    }

    pub fn add_player_to_encounter(&mut self, event: &CombatEvent) {
        let enc = self
            .current_encounter_mut()
            .expect("attempting to add players to non-existant encounter");

        enc.players
            .entry(event.source_entity.log_id)
            .or_insert(PlayerInfo {
                id: event.source_entity.log_id,
                name: event.source_entity.name.clone(),
                class_id: event.effect.effect_id,
                class_name: event.effect.effect_name.clone(),
                discipline_id: event.effect.discipline_id,
                discipline_name: event.effect.discipline_name.clone(),
                is_dead: false,
                death_time: None,
            });
    }

    fn update_primary_player(&mut self, event: &CombatEvent) {
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
        let effect_type_id = event.effect.type_id;
        let timestamp = event.timestamp;

        // Get current encounter state
        let current_state = self
            .current_encounter()
            .map(|e| e.state.clone())
            .unwrap_or_default();

        // Add or update effect instances. Apply/Remove effect lines do not change the encounter
        // state
        match event.effect.type_id {
            effect_type_id::APPLYEFFECT => {
                // ignore if there is no target entity
                if event.target_entity.entity_type == EntityType::Empty {
                    return;
                }
                if let Some(enc) = self.current_encounter_mut() {
                    enc.apply_effect(&event);
                }
            }
            effect_type_id::REMOVEEFFECT => {
                // ignore if there is no source entity
                if event.source_entity.entity_type == EntityType::Empty {
                    return;
                }

                if let Some(enc) = self.current_encounter_mut() {
                    enc.remove_effect(&event);
                }
            }
            _ => {}
        }

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
                    // Start combat on damage if >15s since last exit (or no prior exit)
                    let should_start = match self.last_exit_combat_time {
                        None => true,
                        Some(last_exit) => timestamp.duration_since(last_exit).whole_seconds() > 15,
                    };
                    if should_start && let Some(enc) = self.current_encounter_mut() {
                        enc.state = EncounterState::InCombat;
                        enc.enter_combat_time = Some(timestamp);
                        enc.events.push(event);
                    }
                    // Otherwise discard - likely trailing damage from previous encounter
                } else {
                    // Buffer non-damage events for the upcoming encounter
                    if let Some(enc) = self.current_encounter_mut() {
                        enc.events.push(event);
                    }
                }
            }

            EncounterState::InCombat => {
                let enc_ref = self
                    .current_encounter()
                    .expect("failed to get current encounter");

                // timeout combat if >120 seconds have passed since a heal or damage observed
                if let Some(enc) = self.current_encounter()
                    && let Some(last_activity) = enc.last_combat_activity_time
                {
                    let elapsed = timestamp.duration_since(last_activity).whole_seconds();
                    if elapsed >= 120 {
                        // End combat at last_activity_time, not current timestamp
                        if let Some(enc) = self.current_encounter_mut() {
                            enc.exit_combat_time = Some(last_activity);
                            enc.state = EncounterState::PostCombat {
                                exit_time: last_activity,
                            };
                        }
                        self.last_exit_combat_time = Some(last_activity);
                        self.finalize_and_start_new();
                        // Re-process this event in the new encounter
                        self.route_event_to_encounter(event);
                        return;
                    }
                }
                // if this happens something has gone wrong. terminate encounter immediately and
                // start another
                if effect_id == effect_id::ENTERCOMBAT {
                    if let Some(enc) = self.current_encounter_mut() {
                        enc.exit_combat_time = Some(timestamp);
                        enc.state = EncounterState::PostCombat {
                            exit_time: timestamp,
                        };
                        self.last_exit_combat_time = Some(timestamp);
                        self.finalize_and_start_new();
                        //reroute event to new encounter
                        self.route_event_to_encounter(event);
                    }
                    // ExitCombat event recorded or all players in the encounter are dead
                } else if effect_id == effect_id::EXITCOMBAT || enc_ref.all_players_dead {
                    // Transition to PostCombat
                    if let Some(enc) = self.current_encounter_mut() {
                        enc.exit_combat_time = Some(timestamp);
                        enc.state = EncounterState::PostCombat {
                            exit_time: timestamp,
                        };
                        enc.events.push(event);
                    }
                    self.last_exit_combat_time = Some(timestamp);
                //always terminate combat on area entered
                } else if effect_type_id == effect_type_id::AREAENTERED {
                    if let Some(enc) = self.current_encounter_mut() {
                        enc.exit_combat_time = Some(timestamp);
                        enc.state = EncounterState::PostCombat {
                            exit_time: timestamp,
                        };
                        self.last_exit_combat_time = Some(timestamp);
                        self.finalize_and_start_new();
                    }
                } else {
                    // Collect all events during combat
                    if let Some(enc) = self.current_encounter_mut() {
                        enc.track_event_entities(&event);
                        enc.events.push(event);
                        if effect_id == effect_id::DAMAGE || effect_id == effect_id::HEAL {
                            enc.last_combat_activity_time = Some(timestamp);
                        }
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

        let encounter = if self.player_initialized {
            Encounter::with_player(id, self.player.clone())
        } else {
            Encounter::new(id)
        };

        self.next_encounter_id += 1;
        self.encounters.push_back(encounter);
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

    /// Get last encounter that is in combat
    pub fn last_combat_encounter(&self) -> Option<&Encounter> {
        self.encounters
            .iter()
            .rfind(|e| e.state != EncounterState::NotStarted)
    }

    /// --- Utility Methods ---

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
            println!("    All Players Dead: {}", enc.all_players_dead);
            println!("    Event count: {}", enc.events.len());
            println!("    Players ({}):", enc.players.len());
            for (id, player) in &enc.players {
                println!(
                    "      [{}: {}] alive={}, death_time={}",
                    player.name,
                    id,
                    !player.is_dead,
                    player
                        .death_time
                        .map(|t| t.to_string())
                        .unwrap_or_else(|| "N/A".to_string())
                );
            }
            println!("    Npcs ({}):", enc.npcs.len());
            for (id, player) in &enc.npcs {
                println!(
                    "      [{}: {}] alive={}, death_time={}",
                    player.name,
                    id,
                    !player.is_dead,
                    player
                        .death_time
                        .map(|t| t.to_string())
                        .unwrap_or_else(|| "N/A".to_string())
                );
            }
        }
    }
}
