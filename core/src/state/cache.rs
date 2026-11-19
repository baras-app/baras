use baras_types::history::{InstanceVisit, TimerStart};
use chrono::NaiveDateTime;

use crate::combat_log::EntityType;
use crate::dsl::{BossEncounterDefinition, OperationTimerStart};
use crate::encounter::entity_info::PlayerInfo;
use crate::encounter::summary::{EncounterHistory, create_encounter_summary};
use crate::encounter::{CombatEncounter, EncounterState, OverlayHealthEntry, ProcessingMode};
use crate::game_data::{Difficulty, clear_boss_registry, register_hp_overlay_entity};
use crate::history::InstanceLineage;
use crate::state::info::AreaInfo;
use crate::state::ipc::{ParseWorkerOutput, WorkerAreaInfo, WorkerPlayerDiscipline, WorkerPlayerInfo};
use hashbrown::{HashMap, HashSet as HashbrownSet};
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

    /// Region-based operation timer start for `current_area` (`None` for
    /// areas without one). `area_id` guards against stale definitions
    /// between an area change and its definition reload.
    timer_start: Option<(i64, OperationTimerStart)>,
    /// Set once the start signal fires; cleared when a new area is loaded.
    timer_start_fired: bool,
    /// Every timer start of this session, persisted with the pull history
    pub timer_starts: Vec<TimerStart>,
    /// Instanced-area stays in this log, oldest first (see `InstanceVisit`)
    pub visits: Vec<InstanceVisit>,
    /// The instance being played. A visit that is not a return to it (a
    /// reset) starts a new encounter section and restarts the run clock.
    lineage: Option<LiveInstance>,
    /// A reset was judged and `GameSignal::InstanceReset` is still owed
    instance_reset: bool,

    // NPC tracking (session-scoped)
    /// NPC instance log IDs that have been seen in this session (for NpcFirstSeen signals)
    /// Tracks by log_id (instance) not class_id (template) so each spawn is detected
    pub seen_npc_instances: HashSet<i64>,

    // Player discipline registry (session-scoped)
    /// Maps player entity_id -> PlayerInfo with discipline data
    /// This is the source of truth for player disciplines, updated on every DisciplineChanged event
    pub player_disciplines: HashMap<i64, PlayerInfo>,
    /// Set on area change; ability-inferred disciplines are evicted at the next
    /// encounter boundary. Deferred (not done at AreaEntered directly) so the
    /// outgoing encounter's summary snapshots disciplines before they're wiped.
    pub inferred_discipline_eviction_pending: bool,

    // Combat exit grace window tracking
    /// Timestamp of last combat exit - used to detect fake combat splits
    /// (e.g., loot chest "enemies" or Kephess SM walker phase)
    pub last_combat_exit_time: Option<NaiveDateTime>,

    // Log file corruption detection
    /// True if a different character's events were detected in this log file
    /// (e.g., hibernation caused a second login to append to the same file)
    pub character_mismatch: bool,
    /// True if the log file started without an AreaEntered event
    /// (e.g., crash/disconnect caused SWTOR to resume logging without area context)
    pub missing_area: bool,
}

/// The instance being played, as its visits in this log have shown it.
#[derive(Debug, Clone)]
struct LiveInstance {
    /// Index of the last visit judged
    judged: usize,
    facts: InstanceLineage,
    /// When the instance was first entered (ISO 8601)
    entered: String,
    /// Judged a new instance; its next encounter opens the section
    opens_section: bool,
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
            timer_start: None,
            timer_start_fired: false,
            timer_starts: Vec::new(),
            visits: Vec::new(),
            lineage: None,
            instance_reset: false,
            seen_npc_instances: HashSet::new(),
            player_disciplines: HashMap::new(),
            inferred_discipline_eviction_pending: false,
            last_combat_exit_time: None,
            character_mismatch: false,
            missing_area: false,
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

        // Trash kills identify the instance: they never respawn without a reset
        let visit_idx = self.visits.iter().rposition(|v| Some(v.area_id) == encounter.area_id);
        if encounter.active_boss_idx().is_none()
            && let Some(idx) = visit_idx
        {
            let visit = &mut self.visits[idx];
            for npc in encounter
                .npcs
                .values()
                .filter(|n| n.is_dead && n.entity_type == EntityType::Npc)
            {
                if !visit.trash.contains(&npc.class_id) {
                    visit.trash.push(npc.class_id);
                }
            }
        }

        let mut new_instance = false;
        if let Some(idx) = visit_idx {
            Self::judge_visit(&self.visits, &mut self.lineage, &mut self.instance_reset, idx);
            if let Some(live) = &mut self.lineage {
                new_instance = std::mem::take(&mut live.opens_section);
            }
        }

        if let Some(summary) = create_encounter_summary(
            encounter,
            &self.current_area,
            &mut self.encounter_history,
            &self.player_disciplines,
            self.player.id,
            new_instance,
            visit_idx.and(self.lineage.as_ref()).map(|live| live.entered.as_str()),
        ) {
            if summary.success
                && summary.boss_name.is_some()
                && let Some(live) = &mut self.lineage
            {
                live.facts.has_kill = true;
            }
            self.encounter_history.add(summary);
        }
    }

    /// Judge a visit once: a return to the instance being played continues
    /// it, a reset replaces it. Later calls for the same visit only fold in
    /// what it has shown since.
    fn judge_visit(
        visits: &[InstanceVisit],
        lineage: &mut Option<LiveInstance>,
        instance_reset: &mut bool,
        idx: usize,
    ) {
        let visit = &visits[idx];
        match lineage {
            Some(live) if visits[live.judged].area_id == visit.area_id => {
                if live.judged != idx {
                    let evidence = visit.prefix != 0 && live.facts.prefix != 0;
                    if evidence
                        && (visits[live.judged].difficulty_id != visit.difficulty_id
                            || !live.facts.continues(visit, true))
                    {
                        live.facts = InstanceLineage::default();
                        live.entered.clone_from(&visit.entered);
                        live.opens_section = true;
                        *instance_reset = true;
                    }
                    live.judged = idx;
                }
                live.facts.absorb(visit);
            }
            // Another area: the area change already starts a section
            lineage => {
                let mut facts = InstanceLineage::default();
                facts.absorb(visit);
                *lineage = Some(LiveInstance {
                    judged: idx,
                    facts,
                    entered: visit.entered.clone(),
                    opens_section: false,
                });
            }
        }
    }

    /// Fold an NPC id into the current visit. An id from another server
    /// process than the instance being played settles the visit on the spot,
    /// without waiting for its first encounter to end.
    pub fn observe_npc(&mut self, log_id: i64) {
        let area = &self.current_area;
        if area.difficulty_id == 0 {
            return;
        }
        let Some(idx) = self
            .visits
            .len()
            .checked_sub(1)
            .filter(|i| self.visits[*i].area_id == area.area_id)
        else {
            return;
        };
        self.visits[idx].observe_npc(log_id);
        if let Some(live) = &self.lineage
            && live.judged != idx
            && live.facts.prefix != 0
            && live.facts.prefix != self.visits[idx].prefix
        {
            Self::judge_visit(&self.visits, &mut self.lineage, &mut self.instance_reset, idx);
        }
    }

    /// A reset judged since the last call, with the run clock start already
    /// recorded in the new instance (None: its clock has not started).
    pub fn take_instance_reset(&mut self) -> Option<Option<NaiveDateTime>> {
        if !std::mem::take(&mut self.instance_reset) {
            return None;
        }
        let live = self.lineage.as_ref()?;
        let area_id = self.visits[live.judged].area_id;
        Some(
            self.timer_starts
                .iter()
                .find(|t| t.area_id == area_id && t.time >= live.entered)
                .and_then(|t| NaiveDateTime::parse_from_str(&t.time, "%Y-%m-%dT%H:%M:%S").ok()),
        )
    }

    /// Rebuild the instance being played from imported visits and summaries,
    /// so a reset is still recognised after the app restarts mid-run.
    fn restore_lineage(&mut self) {
        let summaries = self.encounter_history.summaries();
        let Some(last) = summaries.last() else { return };
        let (Some(entered), Some(last_start)) = (&last.instance_entered, &last.start_time) else {
            return;
        };
        let Some(first) = self.visits.iter().position(|v| v.entered == *entered) else {
            return;
        };
        let area_id = self.visits[first].area_id;
        let mut live = LiveInstance {
            judged: first,
            facts: InstanceLineage::default(),
            entered: entered.clone(),
            opens_section: false,
        };
        // Visits after the last encounter have not been judged yet
        for (idx, visit) in self.visits.iter().enumerate().skip(first) {
            if visit.area_id == area_id && visit.entered <= *last_start {
                live.facts.absorb(visit);
                live.judged = idx;
            }
        }
        live.facts.has_kill = summaries
            .iter()
            .any(|s| s.instance_entered.as_ref() == Some(entered) && s.success && s.boss_name.is_some());
        self.lineage = Some(live);
    }

    /// Set the next encounter ID (used after importing subprocess results)
    pub fn set_next_encounter_id(&mut self, id: u64) {
        self.next_encounter_id = id;
    }

    /// Evict ability-inferred disciplines (deferred from area change): players
    /// can respec between PvP matches and enemies never emit a correcting
    /// DisciplineChanged, so the next area re-detects from scratch.
    pub fn evict_inferred_disciplines(&mut self) {
        self.inferred_discipline_eviction_pending = false;
        for player in self.player_disciplines.values_mut() {
            if player.discipline_inferred {
                player.discipline_id = 0;
                player.discipline_name.clear();
                player.discipline_inferred = false;
            }
        }
    }

    pub fn push_new_encounter(&mut self) -> u64 {
        // Finalize the current encounter before creating a new one
        self.finalize_current_encounter();

        // Collect log_ids of dead NPCs from the outgoing encounter so the next
        // encounter can reject stale registrations for the same NPC instances.
        let dead_npc_ids: HashbrownSet<i64> = self
            .current_encounter()
            .map(|enc| {
                enc.npcs
                    .values()
                    .filter(|npc| npc.is_dead)
                    .map(|npc| npc.log_id)
                    .collect()
            })
            .unwrap_or_default();

        // Clear NPC instance tracking for fresh detection in new encounter
        self.seen_npc_instances.clear();

        let id = self.next_encounter_id;

        let mut encounter = if self.player_initialized {
            CombatEncounter::with_player(id, ProcessingMode::Live, self.player.clone())
        } else {
            CombatEncounter::new(id, ProcessingMode::Live)
        };

        // Carry forward dead NPC log_ids from the prior encounter to prevent
        // stale dead NPCs from being re-registered in rapid encounter transitions
        if !dead_npc_ids.is_empty() {
            tracing::debug!(
                "[ENCOUNTER] Carrying {} dead NPC log_ids from prior encounter into new encounter ID={}",
                dead_npc_ids.len(),
                id
            );
            encounter.set_prior_dead_npcs(dead_npc_ids);
        }

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
        self.encounters.iter().rfind(|e| e.state != EncounterState::NotStarted)
    }

    pub fn last_combat_encounter_mut(&mut self) -> Option<&mut CombatEncounter> {
        self.encounters
            .iter_mut()
            .rfind(|e| e.state != EncounterState::NotStarted)
    }

    pub fn encounter_count(&self) -> usize {
        self.encounters.len()
    }

    /// Returns true if the current encounter is in the PostCombat grace window state.
    /// Used to skip accumulation/time updates for events during the grace window.
    pub fn is_in_grace_window(&self) -> bool {
        self.current_encounter()
            .is_some_and(|enc| matches!(enc.state, EncounterState::PostCombat { .. }))
    }

    // --- Boss Health ---

    /// Get current health of all bosses from the current encounter
    pub fn get_boss_health(&self) -> Vec<OverlayHealthEntry> {
        self.current_encounter()
            .map(|enc| enc.get_boss_health(&self.player_disciplines))
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
        self.timer_start = None;
    }

    /// Install the area's region-based timer start (called alongside
    /// `load_boss_definitions` on area entry).
    pub fn set_timer_start(&mut self, area_id: i64, timer_start: Option<OperationTimerStart>) {
        self.timer_start = timer_start.map(|t| (area_id, t));
        self.timer_start_fired = false;
    }

    /// Pending timer start for the current area, if it hasn't fired yet.
    pub fn pending_timer_start(&self) -> Option<&OperationTimerStart> {
        if self.timer_start_fired {
            return None;
        }
        self.timer_start
            .as_ref()
            .filter(|(area_id, _)| *area_id == self.current_area.area_id)
            .map(|(_, t)| t)
    }

    pub fn mark_timer_start_fired(&mut self, at: chrono::NaiveDateTime) {
        self.timer_start_fired = true;
        self.record_timer_start(at);
    }

    /// Record a run clock start for the current area without touching the
    /// region trigger (flashpoints start on entry).
    pub fn record_timer_start(&mut self, at: chrono::NaiveDateTime) {
        self.timer_starts.push(TimerStart {
            area_id: self.current_area.area_id,
            difficulty_id: self.current_area.difficulty_id,
            time: at.format("%Y-%m-%dT%H:%M:%S").to_string(),
        });
    }

    /// Open a visit for the instanced area just entered.
    pub fn begin_visit(&mut self, at: chrono::NaiveDateTime) {
        self.visits.push(InstanceVisit {
            area_id: self.current_area.area_id,
            difficulty_id: self.current_area.difficulty_id,
            entered: at.format("%Y-%m-%dT%H:%M:%S").to_string(),
            area_name: self.current_area.area_name.clone(),
            difficulty: self.current_area.difficulty_name.clone(),
            server: self.current_area.server.clone(),
            ..Default::default()
        });
    }

    /// Load boss definitions for the current area.
    /// Replaces any existing definitions and registers HP overlay entities.
    /// Also updates the current encounter with the new definitions.
    ///
    /// When `force` is true, the current encounter's definitions are always replaced
    /// (used during hot-reload so that phases, counters, etc. are updated immediately).
    /// When `force` is false, the current encounter is only updated if it has no
    /// definitions yet OR if the encounter hasn't started combat yet.
    /// This avoids clobbering mid-fight definitions (e.g., player dies and revives in
    /// another area) while ensuring area transitions between two defined areas (e.g.,
    /// Dread Fortress → Scum and Villainy) correctly update the pre-combat encounter
    /// with fresh definitions.
    pub fn load_boss_definitions(&mut self, definitions: Vec<BossEncounterDefinition>, force: bool) {
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
        // Safe to update when:
        // - force=true: hot-reload, always replace
        // - encounter has no definitions yet: first load for this encounter
        // - encounter hasn't started combat: stale definitions from a prior area
        //   can be safely replaced (no active boss fight to disrupt)
        // NOT safe to update when encounter is InCombat/PostCombat with existing
        // definitions (e.g., player died and revived in another area mid-fight)
        if let Some(enc) = self.current_encounter_mut()
            && (force || enc.boss_definitions().is_empty() || enc.state == EncounterState::NotStarted)
        {
            enc.load_boss_definitions(definitions);
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

        // Search definitions for matching trigger entity (skip disabled bosses)
        for (idx, def) in enc.boss_definitions().iter().enumerate() {
            if def.enabled && def.encounter_trigger_ids().any(|id| id == npc_class_id) {
                enc.set_active_boss_idx(Some(idx));
                return Some(idx);
            }
        }

        None
    }

    /// Activate a boss encounter by definition index (for trigger-based detection).
    /// Returns true if activated, false if a boss is already active or no encounter exists.
    pub fn activate_boss_by_idx(&mut self, idx: usize) -> bool {
        let Some(enc) = self.current_encounter_mut() else {
            return false;
        };
        if enc.active_boss_idx().is_some() {
            return false;
        }
        enc.set_active_boss_idx(Some(idx));
        true
    }

    /// Get the currently active boss encounter definition (if any).
    pub fn active_boss_definition(&self) -> Option<&BossEncounterDefinition> {
        self.current_encounter()?.active_boss_definition()
    }

    // --- IPC Methods ---

    /// Create a ParseWorkerOutput from current cache state.
    ///
    /// Used by parse-worker to serialize state for the main app.
    pub fn to_worker_output(&self, end_pos: u64, line_count: u64, event_count: usize) -> ParseWorkerOutput {
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
            character_mismatch: self.character_mismatch,
            missing_area: self.missing_area,
            timer_starts: self.timer_starts.clone(),
            visits: self.visits.clone(),
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
            self.player_disciplines.insert(disc.entity_id, disc.to_player_info());
        }

        // Count generations before consuming encounters
        let generation_count = output.encounters.iter().filter(|e| e.is_phase_start).count() as u64;

        // Import encounter summaries
        for summary in &output.encounters {
            self.encounter_history.add(summary.clone());
        }
        self.timer_starts.extend(output.timer_starts.iter().cloned());
        self.visits.extend(output.visits.iter().cloned());
        self.restore_lineage();

        // Restore generation counter to prevent raid splitting
        if generation_count > 0 {
            self.encounter_history.restore_generation(generation_count);
        }

        // Rebuild pull counts from imported encounter names
        self.encounter_history.rebuild_pull_counts();

        // Restore area generation counter
        self.current_area.generation = generation_count;

        // Restore log file corruption flags
        if output.character_mismatch {
            self.character_mismatch = true;
        }
        if output.missing_area {
            self.missing_area = true;
        }

        generation_count
    }
}
