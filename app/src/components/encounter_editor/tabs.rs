//! Tab navigation for boss editing
//!
//! Each boss expands to show tabs: [Timers] [Phases] [Counters] [Challenges] [Entities]

use dioxus::prelude::*;

use crate::types::{BossListItem, TimerListItem};

use super::timers::TimersTab;
use super::phases::PhasesTab;
use super::counters::CountersTab;
use super::challenges::ChallengesTab;
use super::entities::EntitiesTab;

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

/// Tab container for a single boss
#[component]
pub fn BossTabs(
    boss: BossListItem,
    timers: Vec<TimerListItem>,
    on_timer_change: EventHandler<Vec<TimerListItem>>,
    on_status: EventHandler<(String, bool)>,
) -> Element {
    let mut active_tab = use_signal(|| BossTab::Timers);

    // Filter timers for this boss
    let boss_timers: Vec<TimerListItem> = timers
        .iter()
        .filter(|t| t.boss_id == boss.id)
        .cloned()
        .collect();

    let timer_count = boss_timers.len();

    rsx! {
        div { class: "boss-tabs",
            // Tab bar
            div { class: "tab-nav",
                for tab in BossTab::all() {
                    {
                        let is_active = active_tab() == *tab;
                        let tab_copy = *tab;
                        let count_label = match tab {
                            BossTab::Timers => format!(" ({})", timer_count),
                            _ => String::new(),
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
                            on_status: on_status,
                        }
                    },
                    BossTab::Counters => rsx! {
                        CountersTab {
                            boss: boss.clone(),
                            on_status: on_status,
                        }
                    },
                    BossTab::Challenges => rsx! {
                        ChallengesTab {
                            boss: boss.clone(),
                            on_status: on_status,
                        }
                    },
                    BossTab::Entities => rsx! {
                        EntitiesTab {
                            boss: boss.clone(),
                            on_status: on_status,
                        }
                    },
                }
            }
        }
    }
}
