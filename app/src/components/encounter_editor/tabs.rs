//! Tab navigation for boss editing
//!
//! Each boss expands to show tabs: [Timers] [Phases] [Counters] [Challenges] [Entities]
//! All encounter data is loaded here and passed down to tabs.

use dioxus::prelude::*;

use crate::api;
use crate::types::{
    BossListItem, ChallengeListItem, CounterListItem, EntityListItem, PhaseListItem, TimerListItem,
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

/// Centralized encounter data for all tabs
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EncounterData {
    pub timers: Vec<TimerListItem>,
    pub phases: Vec<PhaseListItem>,
    pub counters: Vec<CounterListItem>,
    pub challenges: Vec<ChallengeListItem>,
    pub entities: Vec<EntityListItem>,
}

impl EncounterData {
    /// Get timer IDs for dropdowns
    pub fn timer_ids(&self) -> Vec<String> {
        self.timers.iter().map(|t| t.timer_id.clone()).collect()
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
    boss: BossListItem,
    timers: Vec<TimerListItem>,
    on_timer_change: EventHandler<Vec<TimerListItem>>,
    on_status: EventHandler<(String, bool)>,
) -> Element {
    let mut active_tab = use_signal(|| BossTab::Timers);
    let mut loading = use_signal(|| true);

    // Centralized encounter data
    let mut phases = use_signal(Vec::<PhaseListItem>::new);
    let mut counters = use_signal(Vec::<CounterListItem>::new);
    let mut challenges = use_signal(Vec::<ChallengeListItem>::new);
    let mut entities = use_signal(Vec::<EntityListItem>::new);

    // Filter timers for this boss (timers come from parent)
    let boss_timers: Vec<TimerListItem> = timers
        .iter()
        .filter(|t| t.boss_id == boss.id)
        .cloned()
        .collect();

    // Build encounter data for child components
    let encounter_data = EncounterData {
        timers: boss_timers.clone(),
        phases: phases(),
        counters: counters(),
        challenges: challenges(),
        entities: entities(),
    };

    let timer_count = boss_timers.len();
    let phase_count = phases().len();
    let counter_count = counters().len();
    let challenge_count = challenges().len();
    let entity_count = entities().len();

    // Load all encounter data on mount
    let file_path = boss.file_path.clone();
    let boss_id = boss.id.clone();
    use_effect(move || {
        let file_path = file_path.clone();
        let boss_id = boss_id.clone();
        spawn(async move {
            // Load phases
            if let Some(p) = api::get_phases_for_area(&file_path).await {
                let boss_phases: Vec<_> = p.into_iter().filter(|p| p.boss_id == boss_id).collect();
                phases.set(boss_phases);
            }
            // Load counters
            if let Some(c) = api::get_counters_for_area(&file_path).await {
                let boss_counters: Vec<_> = c.into_iter().filter(|c| c.boss_id == boss_id).collect();
                counters.set(boss_counters);
            }
            // Load challenges
            if let Some(ch) = api::get_challenges_for_area(&file_path).await {
                let boss_challenges: Vec<_> = ch.into_iter().filter(|c| c.boss_id == boss_id).collect();
                challenges.set(boss_challenges);
            }
            // Load entities
            if let Some(e) = api::get_entities_for_area(&file_path).await {
                let boss_entities: Vec<_> = e.into_iter().filter(|e| e.boss_id == boss_id).collect();
                entities.set(boss_entities);
            }
            loading.set(false);
        });
    });

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
                            boss: boss.clone(),
                            timers: boss_timers,
                            encounter_data: encounter_data.clone(),
                            on_change: move |updated| {
                                let mut all_timers = timers.clone();
                                all_timers.retain(|t| t.boss_id != boss.id);
                                all_timers.extend(updated);
                                on_timer_change.call(all_timers);
                            },
                            on_status: on_status,
                        }
                    },
                    BossTab::Phases => rsx! {
                        PhasesTab {
                            boss: boss.clone(),
                            phases: phases(),
                            encounter_data: encounter_data.clone(),
                            on_change: move |updated| phases.set(updated),
                            on_status: on_status,
                        }
                    },
                    BossTab::Counters => rsx! {
                        CountersTab {
                            boss: boss.clone(),
                            counters: counters(),
                            encounter_data: encounter_data.clone(),
                            on_change: move |updated| counters.set(updated),
                            on_status: on_status,
                        }
                    },
                    BossTab::Challenges => rsx! {
                        ChallengesTab {
                            boss: boss.clone(),
                            challenges: challenges(),
                            encounter_data: encounter_data.clone(),
                            on_change: move |updated| challenges.set(updated),
                            on_status: on_status,
                        }
                    },
                    BossTab::Entities => rsx! {
                        EntitiesTab {
                            boss: boss.clone(),
                            entities: entities(),
                            on_change: move |updated| entities.set(updated),
                            on_status: on_status,
                        }
                    },
                }
            }
        }
    }
}
