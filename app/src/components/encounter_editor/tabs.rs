//! Tab navigation for boss editing
//!
//! Each boss expands to show tabs: [Timers] [Phases] [Counters] [Challenges] [Entities]
//! All encounter data comes from BossWithPath - no additional loading needed.

use dioxus::prelude::*;

use crate::types::{
    BossTimerDefinition, BossWithPath, ChallengeDefinition, CounterDefinition, EntityDefinition,
    PhaseDefinition,
};

use super::challenges::ChallengesTab;
use super::counters::CountersTab;
use super::entities::EntitiesTab;
use super::phases::PhasesTab;
use super::timers::TimersTab;

/// Available tabs for boss editing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BossTab {
    Timers,
    Phases,
    Counters,
    Challenges,
    Entities,
}

impl BossTab {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Timers => "Timers",
            Self::Phases => "Phases",
            Self::Counters => "Counters",
            Self::Challenges => "Challenges",
            Self::Entities => "Entities",
        }
    }

    pub fn all() -> &'static [BossTab] {
        &[
            Self::Timers,
            Self::Phases,
            Self::Counters,
            Self::Challenges,
            Self::Entities,
        ]
    }
}

/// Encounter data for child components (references into BossWithPath)
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EncounterData {
    pub timers: Vec<BossTimerDefinition>,
    pub phases: Vec<PhaseDefinition>,
    pub counters: Vec<CounterDefinition>,
    pub challenges: Vec<ChallengeDefinition>,
    pub entities: Vec<EntityDefinition>,
}

impl EncounterData {
    /// Build from BossWithPath
    pub fn from_boss(bwp: &BossWithPath) -> Self {
        Self {
            timers: bwp.boss.timers.clone(),
            phases: bwp.boss.phases.clone(),
            counters: bwp.boss.counters.clone(),
            challenges: bwp.boss.challenges.clone(),
            entities: bwp.boss.entities.clone(),
        }
    }

    /// Get timer IDs for dropdowns
    pub fn timer_ids(&self) -> Vec<String> {
        self.timers.iter().map(|t| t.id.clone()).collect()
    }

    /// Get phase IDs for dropdowns
    pub fn phase_ids(&self) -> Vec<String> {
        self.phases.iter().map(|p| p.id.clone()).collect()
    }

    /// Get counter IDs for dropdowns
    pub fn counter_ids(&self) -> Vec<String> {
        self.counters.iter().map(|c| c.id.clone()).collect()
    }

    /// Get entity names marked as boss
    pub fn boss_entity_names(&self) -> Vec<String> {
        self.entities
            .iter()
            .filter(|e| e.is_boss)
            .map(|e| e.name.clone())
            .collect()
    }
}

/// Tab container for a single boss
#[component]
pub fn BossTabs(
    boss_with_path: BossWithPath,
    on_boss_change: EventHandler<BossWithPath>,
    on_status: EventHandler<(String, bool)>,
) -> Element {
    let mut active_tab = use_signal(|| BossTab::Timers);

    // Build encounter data from BossWithPath (no async loading needed!)
    let encounter_data = EncounterData::from_boss(&boss_with_path);

    let timer_count = boss_with_path.boss.timers.len();
    let phase_count = boss_with_path.boss.phases.len();
    let counter_count = boss_with_path.boss.counters.len();
    let challenge_count = boss_with_path.boss.challenges.len();
    let entity_count = boss_with_path.boss.entities.len();

    rsx! {
        div { class: "boss-tabs",
            // Tab bar with counts
            div { class: "tab-nav",
                for tab in BossTab::all() {
                    {
                        let is_active = active_tab() == *tab;
                        let tab_copy = *tab;
                        let count_label = match tab {
                            BossTab::Timers => format!(" ({})", timer_count),
                            BossTab::Phases => format!(" ({})", phase_count),
                            BossTab::Counters => format!(" ({})", counter_count),
                            BossTab::Challenges => format!(" ({})", challenge_count),
                            BossTab::Entities => format!(" ({})", entity_count),
                        };

                        rsx! {
                            button {
                                class: if is_active { "tab-btn active" } else { "tab-btn" },
                                onclick: move |_| active_tab.set(tab_copy),
                                "{tab.label()}{count_label}"
                            }
                        }
                    }
                }
            }

            // Tab content
            div { class: "p-sm",
                match active_tab() {
                    BossTab::Timers => rsx! {
                        TimersTab {
                            boss_with_path: boss_with_path.clone(),
                            encounter_data: encounter_data.clone(),
                            on_change: move |updated_timers: Vec<BossTimerDefinition>| {
                                let mut bwp = boss_with_path.clone();
                                bwp.boss.timers = updated_timers;
                                on_boss_change.call(bwp);
                            },
                            on_status: on_status,
                        }
                    },
                    BossTab::Phases => rsx! {
                        PhasesTab {
                            boss_with_path: boss_with_path.clone(),
                            encounter_data: encounter_data.clone(),
                            on_change: move |updated_phases: Vec<PhaseDefinition>| {
                                let mut bwp = boss_with_path.clone();
                                bwp.boss.phases = updated_phases;
                                on_boss_change.call(bwp);
                            },
                            on_status: on_status,
                        }
                    },
                    BossTab::Counters => rsx! {
                        CountersTab {
                            boss_with_path: boss_with_path.clone(),
                            encounter_data: encounter_data.clone(),
                            on_change: move |updated_counters: Vec<CounterDefinition>| {
                                let mut bwp = boss_with_path.clone();
                                bwp.boss.counters = updated_counters;
                                on_boss_change.call(bwp);
                            },
                            on_status: on_status,
                        }
                    },
                    BossTab::Challenges => rsx! {
                        ChallengesTab {
                            boss_with_path: boss_with_path.clone(),
                            encounter_data: encounter_data.clone(),
                            on_change: move |updated_challenges: Vec<ChallengeDefinition>| {
                                let mut bwp = boss_with_path.clone();
                                bwp.boss.challenges = updated_challenges;
                                on_boss_change.call(bwp);
                            },
                            on_status: on_status,
                        }
                    },
                    BossTab::Entities => rsx! {
                        EntitiesTab {
                            boss_with_path: boss_with_path.clone(),
                            on_change: move |updated_entities: Vec<EntityDefinition>| {
                                let mut bwp = boss_with_path.clone();
                                bwp.boss.entities = updated_entities;
                                on_boss_change.call(bwp);
                            },
                            on_status: on_status,
                        }
                    },
                }
            }
        }
    }
}
