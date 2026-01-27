//! Encounter history tracking and classification
//!
//! Provides persistence of encounter metrics across the current log file session,
//! classification of encounters by phase type, and human-readable naming.

use hashbrown::HashMap;
use serde::{Deserialize, Serialize};

use super::CombatEncounter;
use super::PhaseType;
use super::entity_info::PlayerInfo;
use super::metrics::PlayerMetrics;
use crate::combat_log::EntityType;
use crate::context::resolve;
use crate::debug_log;
use crate::game_data::{BossInfo, ContentType, Difficulty, is_pvp_area, lookup_boss};
use crate::state::info::AreaInfo;

/// Summary of a completed encounter with computed metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncounterSummary {
    pub encounter_id: u64,
    pub display_name: String,
    pub encounter_type: PhaseType,
    /// ISO 8601 formatted start time (or None if unknown)
    pub start_time: Option<String>,
    /// ISO 8601 formatted end time (or None if unknown)
    pub end_time: Option<String>,
    pub duration_seconds: i64,
    pub success: bool,
    pub area_name: String,
    pub difficulty: Option<String>,
    pub boss_name: Option<String>,
    pub player_metrics: Vec<PlayerMetrics>,
    /// True if this encounter starts a new phase (area change)
    pub is_phase_start: bool,
    /// Names of NPC enemies in the encounter
    pub npc_names: Vec<String>,
}

/// Tracks encounter history for the current log file session
#[derive(Debug, Clone, Default)]
pub struct EncounterHistory {
    summaries: Vec<EncounterSummary>,
    boss_pull_counts: HashMap<String, u32>,
    trash_pull_count: u32,
    /// Generation counter from AreaInfo, used to detect phase boundaries
    /// (including re-entering the same area).
    current_generation: Option<u64>,
}

impl EncounterHistory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, summary: EncounterSummary) {
        self.summaries.push(summary);
    }

    pub fn summaries(&self) -> &[EncounterSummary] {
        &self.summaries
    }

    pub fn clear(&mut self) {
        self.summaries.clear();
        self.boss_pull_counts.clear();
        self.trash_pull_count = 0;
        self.current_generation = None;
    }

    /// Check if area changed and update tracking.
    /// Uses the area generation counter so re-entering the same area
    /// (e.g., running the same flashpoint twice) is detected as a new phase.
    pub fn check_area_change(&mut self, generation: u64) -> bool {
        let changed = self.current_generation != Some(generation);
        if changed {
            self.current_generation = Some(generation);
            // Reset pull counts on area change
            self.trash_pull_count = 0;
            self.boss_pull_counts.clear();
        }
        changed
    }

    /// Generate a human-readable name for an encounter based on its type and boss
    pub fn generate_name(&mut self, encounter_type: PhaseType, boss_name: Option<&str>) -> String {
        match (encounter_type, boss_name) {
            // Boss encounter: "Brontes - 7"
            (_, Some(name)) => {
                let count = self.boss_pull_counts.entry(name.to_string()).or_insert(0);
                *count += 1;
                format!("{} - {}", name, count)
            }
            (PhaseType::Raid, None) => {
                self.trash_pull_count += 1;
                format!("Raid Trash {}", self.trash_pull_count)
            }
            (PhaseType::Flashpoint, None) => {
                self.trash_pull_count += 1;
                format!("Flashpoint Trash {}", self.trash_pull_count)
            }
            (PhaseType::DummyParse, None) => {
                self.trash_pull_count += 1;
                format!("Dummy Parse {}", self.trash_pull_count)
            }
            (PhaseType::PvP, None) => {
                self.trash_pull_count += 1;
                format!("PvP Match {}", self.trash_pull_count)
            }
            (PhaseType::OpenWorld, None) => {
                self.trash_pull_count += 1;
                format!("Open World {}", self.trash_pull_count)
            }
        }
    }

    /// Peek the current pull count for a boss without incrementing.
    /// Returns what the pull number would be for an in-progress encounter.
    /// Used for live overlay display before the encounter is finalized.
    pub fn peek_pull_count(&self, boss_name: &str) -> u32 {
        self.boss_pull_counts.get(boss_name).copied().unwrap_or(0) + 1
    }

    /// Peek the current trash pull count without incrementing.
    pub fn peek_trash_count(&self) -> u32 {
        self.trash_pull_count + 1
    }
}

/// Classify an encounter's phase type and find the primary boss (if any)
/// Uses difficulty ID for phase classification, with training dummy override
pub fn classify_encounter(
    encounter: &CombatEncounter,
    area: &AreaInfo,
) -> (PhaseType, Option<&'static BossInfo>) {
    // 1. Find boss info if present (sorted by first_seen_at for primary boss)
    let boss_info = if encounter.npcs.values().any(|v| v.is_boss) {
        let mut boss_npcs: Vec<_> = encounter
            .npcs
            .values()
            .filter_map(|npc| lookup_boss(npc.class_id).map(|info| (npc, info)))
            .collect();
        boss_npcs.sort_by_key(|(npc, _)| npc.first_seen_at);
        boss_npcs.first().map(|(_, info)| *info)
    } else {
        None
    };

    // 2. Check for training dummy (overrides all other classification)
    if let Some(info) = boss_info
        && info.content_type == ContentType::TrainingDummy
    {
        return (PhaseType::DummyParse, Some(info));
    }
    if let Some(def) = encounter.active_boss_definition()
        && def.area_type == crate::dsl::AreaType::TrainingDummy
    {
        return (PhaseType::DummyParse, boss_info);
    }

    // 3. Check PvP area
    if is_pvp_area(area.area_id) {
        return (PhaseType::PvP, boss_info);
    }

    // 4. Classify by difficulty ID
    let phase = if let Some(difficulty) = Difficulty::from_difficulty_id(area.difficulty_id) {
        match difficulty.group_size() {
            8 | 16 => PhaseType::Raid,
            4 => PhaseType::Flashpoint,
            _ => PhaseType::OpenWorld,
        }
    } else {
        PhaseType::OpenWorld
    };

    (phase, boss_info)
}

/// Determine if an encounter was successful (not a wipe)
/// Returns false (wipe) if either all players died OR the local player died
pub fn determine_success(encounter: &CombatEncounter) -> bool {
    !encounter.all_players_dead && !encounter.local_player_died
}

/// Create an EncounterSummary from a completed CombatEncounter
pub fn create_encounter_summary(
    encounter: &CombatEncounter,
    area: &AreaInfo,
    history: &mut EncounterHistory,
    player_disciplines: &HashMap<i64, PlayerInfo>,
) -> Option<EncounterSummary> {
    // Skip encounters that never started combat
    #[allow(clippy::question_mark)]
    if encounter.enter_combat_time.is_none() {
        return None;
    }

    // DEBUG: Log wipe detection state with player details
    let combat_start = encounter.enter_combat_time;
    let player_states: Vec<String> = encounter
        .players
        .values()
        .map(|p| {
            let in_combat = combat_start.is_none_or(|start| {
                p.last_seen_at.is_some_and(|seen| seen >= start)
            });
            format!("{}:dead={},in_combat={}", resolve(p.name), p.is_dead, in_combat)
        })
        .collect();
    debug_log!(
        "create_encounter_summary: all_dead={}, local_died={}, players={}, states=[{}]",
        encounter.all_players_dead,
        encounter.local_player_died,
        encounter.players.len(),
        player_states.join(", ")
    );

    // Check if this is a new phase (area change)
    let is_phase_start = history.check_area_change(area.generation);

    // Classify using area info
    let (encounter_type, boss_info) = classify_encounter(encounter, area);

    // Get boss name: prefer active definition, fall back to detected boss NPC
    // This allows non-boss trigger entities to classify the encounter
    let boss_name = encounter
        .active_boss_definition()
        .map(|def| def.name.clone())
        .or_else(|| {
            // Only fall back to hardcoded data if a boss NPC was actually seen
            if encounter.npcs.values().any(|v| v.is_boss) {
                boss_info.map(|b| b.boss.to_string())
            } else {
                None
            }
        });

    let display_name = history.generate_name(encounter_type, boss_name.as_deref());

    // Calculate metrics and filter to players seen during actual combat
    let combat_start = encounter.enter_combat_time;
    let player_metrics: Vec<PlayerMetrics> = encounter
        .calculate_entity_metrics(player_disciplines)
        .unwrap_or_default()
        .into_iter()
        .filter(|m| {
            // Filter out NPCs
            if m.entity_type == EntityType::Npc {
                return false;
            }
            // Filter out players not seen during combat (e.g., character switches)
            encounter.players.get(&m.entity_id).is_some_and(|p| {
                combat_start.is_none_or(|start| {
                    p.last_seen_at.is_some_and(|seen| seen >= start)
                })
            })
        })
        .map(|m| m.to_player_metrics())
        .collect();

    // Use area difficulty directly from AreaEntered event
    let difficulty = if area.difficulty_name.is_empty() {
        None
    } else {
        Some(area.difficulty_name.clone())
    };

    // Collect NPC names with counts (show count only if > 1)
    // Filter out companions - they're friendly NPCs, not enemies
    let mut npc_counts: HashMap<String, u32> = HashMap::new();
    for npc in encounter.npcs.values() {
        if npc.entity_type != EntityType::Companion {
            *npc_counts.entry(resolve(npc.name).to_string()).or_insert(0) += 1;
        }
    }
    let mut npc_names: Vec<String> = npc_counts
        .into_iter()
        .map(|(name, count)| {
            if count > 1 {
                format!("{} ({})", name, count)
            } else {
                name
            }
        })
        .collect();
    npc_names.sort();

    Some(EncounterSummary {
        encounter_id: encounter.id,
        display_name,
        encounter_type,
        start_time: encounter
            .enter_combat_time
            .map(|t| t.format("%Y-%m-%dT%H:%M:%S").to_string()),
        end_time: encounter
            .exit_combat_time
            .map(|t| t.format("%Y-%m-%dT%H:%M:%S").to_string()),
        duration_seconds: encounter.duration_seconds().unwrap_or(0),
        success: determine_success(encounter),
        area_name: area.area_name.clone(),
        difficulty,
        boss_name,
        player_metrics,
        is_phase_start,
        npc_names,
    })
}
