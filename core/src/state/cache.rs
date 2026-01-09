use crate::dsl::BossEncounterDefinition;
use crate::encounter::entity_info::PlayerInfo;
use crate::encounter::summary::{EncounterHistory, create_encounter_summary};
use crate::encounter::{OverlayHealthEntry, CombatEncounter, EncounterState, ProcessingMode};
use crate::game_data::{Difficulty, clear_boss_registry, register_hp_overlay_entity};
use crate::state::info::AreaInfo;
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;

const CACHE_DEFAULT_CAPACITY: usize = 2;

/// Pure storage for session state.
/// Routing logic lives in EventProcessor.
#[derive(Debug, Clone)]
pub struct SessionCache {
    // Player state
    pub player: PlayerInfo,
    pub player_initialized: bool,

    // Area state
    pub current_area: AreaInfo,

    // Encounter tracking - fixed-size window for live encounters
    encounters: VecDeque<CombatEncounter>,
    next_encounter_id: u64,

    // Full encounter history for current file
    pub encounter_history: EncounterHistory,

    // Boss encounter definitions (area-scoped, copied into each encounter)
    boss_definitions: Arc<Vec<BossEncounterDefinition>>,

    // NPC tracking (session-scoped)
    /// NPC instance log IDs that have been seen in this session (for NpcFirstSeen signals)
    /// Tracks by log_id (instance) not class_id (template) so each spawn is detected
    pub seen_npc_instances: HashSet<i64>,
}

impl Default for SessionCache {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionCache {
    pub fn new() -> Self {
        let mut cache = Self {
            player: PlayerInfo::default(),
            player_initialized: false,
            current_area: AreaInfo::default(),
            encounters: VecDeque::with_capacity(CACHE_DEFAULT_CAPACITY),
            next_encounter_id: 0,
            encounter_history: EncounterHistory::new(),
            boss_definitions: Arc::new(Vec::new()),
            seen_npc_instances: HashSet::new(),
        };
        cache.push_new_encounter();
        cache
    }

    // --- Encounter Management ---

    /// Finalize the current encounter and add it to history (if it had combat)
    pub fn finalize_current_encounter(&mut self) {
        let Some(encounter) = self.encounters.back() else {
            return;
        };
        if encounter.state == EncounterState::NotStarted {
            return;
        }

        if let Some(summary) =
            create_encounter_summary(encounter, &self.current_area, &mut self.encounter_history)
        {
            self.encounter_history.add(summary);
        }
    }

    pub fn push_new_encounter(&mut self) -> u64 {
        // Finalize the current encounter before creating a new one
        self.finalize_current_encounter();

        // Clear NPC instance tracking for fresh detection in new encounter
        self.seen_npc_instances.clear();

        let id = self.next_encounter_id;

        let mut encounter = if self.player_initialized {
            CombatEncounter::with_player(id, ProcessingMode::Live, self.player.clone())
        } else {
            CombatEncounter::new(id, ProcessingMode::Live)
        };

        // Set context from current area (use ID for language independence)
        encounter.set_difficulty(Difficulty::from_difficulty_id(
            self.current_area.difficulty_id,
        ));
        let area_id = if self.current_area.area_id != 0 {
            Some(self.current_area.area_id)
        } else {
            None
        };
        let area_name = if self.current_area.area_name.is_empty() {
            None
        } else {
            Some(self.current_area.area_name.clone())
        };
        encounter.set_area(area_id, area_name);

        // Copy boss definitions into the new encounter
        encounter.load_boss_definitions(self.boss_definitions.to_vec());

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

    pub fn current_encounter(&self) -> Option<&CombatEncounter> {
        self.encounters.back()
    }

    pub fn current_encounter_mut(&mut self) -> Option<&mut CombatEncounter> {
        self.encounters.back_mut()
    }

    pub fn encounters(&self) -> impl Iterator<Item = &CombatEncounter> {
        self.encounters.iter()
    }

    pub fn encounters_mut(&mut self) -> impl Iterator<Item = &mut CombatEncounter> {
        self.encounters.iter_mut()
    }

    pub fn encounter_by_id(&self, id: u64) -> Option<&CombatEncounter> {
        self.encounters.iter().find(|e| e.id == id)
    }

    pub fn last_combat_encounter(&self) -> Option<&CombatEncounter> {
        self.encounters
            .iter()
            .rfind(|e| e.state != EncounterState::NotStarted)
    }

    pub fn last_combat_encounter_mut(&mut self) -> Option<&mut CombatEncounter> {
        self.encounters
            .iter_mut()
            .rfind(|e| e.state != EncounterState::NotStarted)
    }

    pub fn encounter_count(&self) -> usize {
        self.encounters.len()
    }

    // --- Boss Health ---

    /// Get current health of all bosses from the current encounter
    pub fn get_boss_health(&self) -> Vec<OverlayHealthEntry> {
        self.current_encounter()
            .map(|enc| enc.get_boss_health())
            .unwrap_or_default()
    }

    // --- Boss Encounter Management ---

    /// Get the boss definitions (area-scoped)
    pub fn boss_definitions(&self) -> &[BossEncounterDefinition] {
        &self.boss_definitions
    }

    /// Clear boss definitions (e.g., when leaving an instance).
    /// Also clears the global boss registry.
    pub fn clear_boss_definitions(&mut self) {
        clear_boss_registry();
        self.boss_definitions = Arc::new(Vec::new());
    }

    /// Load boss definitions for the current area.
    /// Replaces any existing definitions and registers HP overlay entities.
    /// Also updates the current encounter with the new definitions.
    pub fn load_boss_definitions(&mut self, definitions: Vec<BossEncounterDefinition>) {
        // Register HP overlay entities for name lookup
        for def in &definitions {
            for entity in def.hp_overlay_entities() {
                for &npc_id in &entity.ids {
                    register_hp_overlay_entity(npc_id, &entity.name);
                }
            }
        }
        let definitions = Arc::new(definitions);
        self.boss_definitions = Arc::clone(&definitions);

        // Update current encounter with the definitions (clone from Arc)
        if let Some(enc) = self.current_encounter_mut() {
            enc.load_boss_definitions(definitions.to_vec());
        }
    }

    /// Try to detect which boss encounter is active based on an NPC class ID.
    /// Delegates to the current encounter.
    pub fn detect_boss_encounter(&mut self, npc_class_id: i64) -> Option<usize> {
        let enc = self.current_encounter_mut()?;

        // If already tracking a boss, don't switch mid-fight
        if enc.active_boss_idx().is_some() {
            return enc.active_boss_idx();
        }

        // Search definitions for matching NPC ID
        for (idx, def) in enc.boss_definitions().iter().enumerate() {
            if def.matches_npc_id(npc_class_id) {
                enc.set_active_boss_idx(Some(idx));
                return Some(idx);
            }
        }

        None
    }

    /// Get the currently active boss encounter definition (if any).
    pub fn active_boss_definition(&self) -> Option<&BossEncounterDefinition> {
        self.current_encounter()?.active_boss_definition()
    }
}
