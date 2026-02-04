use chrono::NaiveDateTime;

use crate::dsl::BossEncounterDefinition;
use crate::encounter::entity_info::PlayerInfo;
use crate::encounter::summary::{create_encounter_summary, EncounterHistory};
use crate::encounter::{CombatEncounter, EncounterState, OverlayHealthEntry, ProcessingMode};
use crate::game_data::{clear_boss_registry, register_hp_overlay_entity, Difficulty};
use crate::state::info::AreaInfo;
use crate::state::ipc::{
    ParseWorkerOutput, WorkerAreaInfo, WorkerPlayerDiscipline, WorkerPlayerInfo,
};
use hashbrown::HashMap;
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

    // Player discipline registry (session-scoped)
    /// Maps player entity_id -> PlayerInfo with discipline data
    /// This is the source of truth for player disciplines, updated on every DisciplineChanged event
    pub player_disciplines: HashMap<i64, PlayerInfo>,

    // Combat exit grace window tracking
    /// Timestamp of last combat exit - used to detect fake combat splits
    /// (e.g., loot chest "enemies" or Kephess SM walker phase)
    pub last_combat_exit_time: Option<NaiveDateTime>,
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
            player_disciplines: HashMap::new(),
            last_combat_exit_time: None,
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

        if let Some(summary) = create_encounter_summary(
            encounter,
            &self.current_area,
            &mut self.encounter_history,
            &self.player_disciplines,
        ) {
            self.encounter_history.add(summary);
        }
    }

    /// Set the next encounter ID (used after importing subprocess results)
    pub fn set_next_encounter_id(&mut self, id: u64) {
        self.next_encounter_id = id;
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
        let difficulty = Difficulty::from_difficulty_id(self.current_area.difficulty_id);
        let difficulty_id = if self.current_area.difficulty_id != 0 {
            Some(self.current_area.difficulty_id)
        } else {
            None
        };
        let difficulty_name = if !self.current_area.difficulty_name.is_empty() {
            Some(self.current_area.difficulty_name.clone())
        } else {
            None
        };
        encounter.set_difficulty_info(difficulty, difficulty_id, difficulty_name);

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
        let area_entered_line = self.current_area.entered_at_line;
        encounter.set_area(area_id, area_name, area_entered_line);

        // Share boss definitions with the new encounter (Arc clone is cheap)
        encounter.load_boss_definitions(Arc::clone(&self.boss_definitions));

        tracing::info!(
            "[ENCOUNTER] Creating new encounter ID={}, boss_defs={}",
            id,
            self.boss_definitions.len()
        );

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

        // Share definitions with current encounter (Arc clone is cheap)
        // BUT only if the encounter doesn't already have definitions loaded
        // (e.g., when player dies and revives in another area before encounter finalizes,
        // we don't want to clobber the mid-fight definitions)
        if let Some(enc) = self.current_encounter_mut() {
            if enc.boss_definitions().is_empty() {
                enc.load_boss_definitions(definitions);
            }
        }
    }

    /// Try to detect which boss encounter is active based on an NPC class ID.
    /// Only matches entities with `triggers_encounter=true` (defaults to `is_boss`).
    /// This allows non-boss NPCs to trigger encounter detection for areas where
    /// groups may wipe before reaching the actual boss.
    pub fn detect_boss_encounter(&mut self, npc_class_id: i64) -> Option<usize> {
        let enc = self.current_encounter_mut()?;

        // If already tracking a boss, don't switch mid-fight
        if enc.active_boss_idx().is_some() {
            return enc.active_boss_idx();
        }

        // Search definitions for matching trigger entity
        for (idx, def) in enc.boss_definitions().iter().enumerate() {
            if def.encounter_trigger_ids().any(|id| id == npc_class_id) {
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

    // --- IPC Methods ---

    /// Create a ParseWorkerOutput from current cache state.
    ///
    /// Used by parse-worker to serialize state for the main app.
    pub fn to_worker_output(
        &self,
        end_pos: u64,
        line_count: u64,
        event_count: usize,
    ) -> ParseWorkerOutput {
        ParseWorkerOutput {
            end_pos,
            line_count,
            event_count,
            encounter_count: self.encounter_history.summaries().len(),
            encounters: self.encounter_history.summaries().to_vec(),
            player: WorkerPlayerInfo::from_player(&self.player),
            area: WorkerAreaInfo::from_area(&self.current_area),
            player_disciplines: self
                .player_disciplines
                .values()
                .map(WorkerPlayerDiscipline::from_player)
                .collect(),
            elapsed_ms: 0, // Filled in by caller
        }
    }

    /// Restore cache state from ParseWorkerOutput.
    ///
    /// Used by the app to import state from parse-worker subprocess.
    ///
    /// Restores:
    /// - player info
    /// - area info  
    /// - player_disciplines
    /// - encounter_history
    ///
    /// Does NOT handle:
    /// - byte/line position (caller must set session.current_byte and session.current_line)
    /// - encounter creation (caller handles based on filesystem state)
    /// - boss definition loading (caller handles based on area_id)
    ///
    /// Returns the number of area generations (phase boundaries) for restore_generation().
    pub fn restore_from_worker_output(&mut self, output: &ParseWorkerOutput) -> u64 {
        // Import player info
        let has_player = output.player.apply_to(&mut self.player);
        if has_player {
            self.player_initialized = true;
        }

        // Import area info
        output.area.apply_to(&mut self.current_area);

        // Import player disciplines
        for disc in &output.player_disciplines {
            self.player_disciplines
                .insert(disc.entity_id, disc.to_player_info());
        }

        // Count generations before consuming encounters
        let generation_count = output
            .encounters
            .iter()
            .filter(|e| e.is_phase_start)
            .count() as u64;

        // Import encounter summaries
        for summary in &output.encounters {
            self.encounter_history.add(summary.clone());
        }

        // Restore generation counter to prevent raid splitting
        if generation_count > 0 {
            self.encounter_history.restore_generation(generation_count);
        }

        // Rebuild pull counts from imported encounter names
        self.encounter_history.rebuild_pull_counts();

        // Restore area generation counter
        self.current_area.generation = generation_count;

        generation_count
    }
}
