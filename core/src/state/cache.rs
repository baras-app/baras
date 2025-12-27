use crate::boss::{BossEncounterDefinition, BossEncounterState};
use crate::encounter::{Encounter, EncounterState, BossHealthEntry};
use crate::encounter::entity_info::PlayerInfo;
use crate::encounter::summary::{EncounterHistory, create_summary};
use crate::state::info::AreaInfo;
use crate::game_data::{lookup_boss, register_boss_npcs, clear_boss_registry};
use std::collections::{HashSet, VecDeque};

const CACHE_DEFAULT_CAPACITY: usize = 3;

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
    encounters: VecDeque<Encounter>,
    next_encounter_id: u64,

    // Full encounter history for current file
    pub encounter_history: EncounterHistory,

    // Boss encounter system
    /// Boss encounter definitions for the current area (loaded on AreaEntered)
    pub boss_definitions: Vec<BossEncounterDefinition>,
    /// Index into boss_definitions for the currently active encounter (None = no boss detected)
    pub active_boss_idx: Option<usize>,
    /// Runtime state for the active boss encounter
    pub boss_state: BossEncounterState,

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
            boss_definitions: Vec::new(),
            active_boss_idx: None,
            boss_state: BossEncounterState::new(),
            seen_npc_instances: HashSet::new(),
        };
        cache.push_new_encounter();
        cache
    }

    // --- Encounter Management ---

    /// Finalize the current encounter and add it to history (if it had combat)
    pub fn finalize_current_encounter(&mut self) {
        let Some(encounter) = self.encounters.back() else { return };
        if encounter.state == EncounterState::NotStarted {
            return;
        }

        if let Some(summary) = create_summary(
            encounter,
            &self.current_area,
            &mut self.encounter_history,
        ) {
            self.encounter_history.add(summary);
        }
    }

    pub fn push_new_encounter(&mut self) -> u64 {
        // Finalize the current encounter before creating a new one
        self.finalize_current_encounter();

        // Reset boss encounter state for the new encounter
        self.reset_boss_encounter();

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

    // --- Boss Health ---

    /// Get current health of all bosses from boss_state (realtime tracking)
    pub fn get_boss_health(&self) -> Vec<BossHealthEntry> {
        let mut entries: Vec<_> = self.boss_state.hp_raw
            .iter()
            .filter_map(|(&npc_id, &(current, max))| {
                lookup_boss(npc_id).map(|info| BossHealthEntry {
                    name: info.boss.to_string(),
                    current: current as i32,
                    max: max as i32,
                    first_seen_at: self.boss_state.first_seen.get(&npc_id).copied(),
                })
            })
            .filter(|b| b.max > 0)
            .collect();

        // Sort by encounter order (first_seen_at)
        entries.sort_by_key(|e| e.first_seen_at);
        entries
    }

    // --- Boss Encounter Management ---

    /// Load boss definitions for the current area.
    /// Called when entering a new area (AreaEntered signal).
    /// Also registers NPC IDs in the global boss registry for is_boss() checks.
    #[allow(deprecated)]
    pub fn load_boss_definitions(&mut self, definitions: Vec<BossEncounterDefinition>) {
        // Register boss NPC IDs in the global registry
        // Only entities with is_boss = true are registered (for detection)
        for def in &definitions {
            // New format: use entity roster (boss entities only)
            let boss_ids: Vec<i64> = def.boss_npc_ids().collect();
            if !boss_ids.is_empty() {
                register_boss_npcs(&boss_ids);
            } else {
                // Legacy format: use flat npc_ids
                register_boss_npcs(&def.npc_ids);
            }
        }
        self.boss_definitions = definitions;
        self.active_boss_idx = None;
        // Don't reset boss_state here - that happens on CombatEnded
    }

    /// Clear boss definitions (e.g., when leaving an instance).
    /// Also clears the global boss registry.
    pub fn clear_boss_definitions(&mut self) {
        clear_boss_registry();
        self.boss_definitions.clear();
        self.active_boss_idx = None;
    }

    /// Try to detect which boss encounter is active based on an NPC class ID.
    /// Returns the definition index if a match is found.
    pub fn detect_boss_encounter(&mut self, npc_class_id: i64) -> Option<usize> {
        // If already tracking a boss, don't switch mid-fight
        if self.active_boss_idx.is_some() {
            return self.active_boss_idx;
        }

        // Search definitions for matching NPC ID (checks entity roster)
        for (idx, def) in self.boss_definitions.iter().enumerate() {
            if def.matches_npc_id(npc_class_id) {
                self.active_boss_idx = Some(idx);
                return Some(idx);
            }
        }

        None
    }

    /// Get the currently active boss encounter definition (if any).
    pub fn active_boss_definition(&self) -> Option<&BossEncounterDefinition> {
        self.active_boss_idx.and_then(|idx| self.boss_definitions.get(idx))
    }

    /// Reset boss encounter state (on combat end).
    /// Clears active_boss_idx and resets boss_state, but keeps definitions.
    pub fn reset_boss_encounter(&mut self) {
        self.active_boss_idx = None;
        self.boss_state.reset();
    }
}
