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
use crate::game_data::{BossInfo, ContentType, is_pvp_area, lookup_area_content_type, lookup_boss};
use crate::state::info::AreaInfo;

/// Summary of a completed encounter with computed metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncounterSummary {
    pub encounter_id: u64,
    pub display_name: String,
    pub encounter_type: PhaseType,
    /// ISO 8601 formatted start time (or None if unknown)
    pub start_time: Option<String>,
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
    current_area_name: Option<String>,
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
        self.current_area_name = None;
    }

    /// Check if area changed and update tracking
    pub fn check_area_change(&mut self, area_name: &str) -> bool {
        let changed = self
            .current_area_name
            .as_ref()
            .is_none_or(|prev| prev != area_name);
        if changed {
            self.current_area_name = Some(area_name.to_string());
            // Reset trash count on area change
            self.trash_pull_count = 0;
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
}

/// Classify an encounter's phase type and find the primary boss (if any)
/// Checks loaded boss definitions first, then falls back to static data
pub fn classify_encounter(
    encounter: &CombatEncounter,
    area: &AreaInfo,
) -> (PhaseType, Option<&'static BossInfo>) {
    // guard boss condition -
    if encounter.npcs.values().any(|v| v.is_boss) {
        // 1. Check loaded boss definitions first (priority source of truth)
        if let Some(def) = encounter.active_boss_definition() {
            let phase = match def.area_type {
                crate::dsl::AreaType::Operation => PhaseType::Raid,
                crate::dsl::AreaType::Flashpoint => PhaseType::Flashpoint,
                crate::dsl::AreaType::LairBoss | crate::dsl::AreaType::OpenWorld => {
                    PhaseType::OpenWorld
                }
                crate::dsl::AreaType::TrainingDummy => PhaseType::DummyParse,
            };
            // Try to find matching static BossInfo for backwards compatibility
            let boss_info = encounter
                .npcs
                .values()
                .find_map(|npc| lookup_boss(npc.class_id));
            return (phase, boss_info);
        }

        // 2. Fall back to static data lookup
        let mut boss_npcs: Vec<_> = encounter
            .npcs
            .values()
            .filter_map(|npc| lookup_boss(npc.class_id).map(|info| (npc, info)))
            .collect();

        // Sort by first_seen_at to get the primary boss (first encountered)
        boss_npcs.sort_by_key(|(npc, _)| npc.first_seen_at);

        if let Some((_, boss_info)) = boss_npcs.first() {
            let phase = match boss_info.content_type {
                ContentType::TrainingDummy => PhaseType::DummyParse,
                ContentType::Operation => PhaseType::Raid,
                ContentType::Flashpoint => PhaseType::Flashpoint,
                ContentType::LairBoss | ContentType::OpenWorld => PhaseType::OpenWorld,
            };
            return (phase, Some(*boss_info));
        }
    }

    // 3. No boss found - check PvP area
    if is_pvp_area(area.area_id) {
        return (PhaseType::PvP, None);
    }

    // 4. Check if area name matches a known operation/flashpoint
    if let Some(content_type) = lookup_area_content_type(&area.area_name) {
        let phase = match content_type {
            ContentType::TrainingDummy => PhaseType::DummyParse,
            ContentType::Operation => PhaseType::Raid,
            ContentType::Flashpoint => PhaseType::Flashpoint,
            ContentType::LairBoss | ContentType::OpenWorld => PhaseType::OpenWorld,
        };
        return (phase, None);
    }

    // 5. Default to OpenWorld
    (PhaseType::OpenWorld, None)
}

/// Determine if an encounter was successful (clean exit, not a wipe)
pub fn determine_success(encounter: &CombatEncounter) -> bool {
    !encounter.all_players_dead && encounter.exit_combat_time.is_some()
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

    // Check if this is a new phase (area change)
    let is_phase_start = history.check_area_change(&area.area_name);

    // Classify using area info
    let (encounter_type, boss_info) = classify_encounter(encounter, area);

    // Get boss name: prefer definition name, fall back to static data
    let boss_name = if encounter.npcs.values().any(|v| v.is_boss) {
        encounter
            .active_boss_definition()
            .map(|def| def.name.clone())
            .or_else(|| boss_info.map(|b| b.boss.to_string()))
    } else {
        None
    };

    let display_name = history.generate_name(encounter_type, boss_name.as_deref());

    // Calculate metrics and filter to players only
    let player_metrics: Vec<PlayerMetrics> = encounter
        .calculate_entity_metrics(player_disciplines)
        .unwrap_or_default()
        .into_iter()
        .filter(|m| m.entity_type != EntityType::Npc)
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
