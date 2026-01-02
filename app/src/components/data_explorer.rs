//! Data Explorer Panel Component
//!
//! Displays detailed ability breakdown and DPS analysis for encounters.
//! Uses DataFusion SQL queries over parquet files for historical data.

use dioxus::prelude::*;
use std::collections::HashSet;
use wasm_bindgen_futures::spawn_local as spawn;

use crate::api::{self, AbilityBreakdown, EntityBreakdown};
use crate::components::history_panel::EncounterSummary;

// ─────────────────────────────────────────────────────────────────────────────
// Helper Functions
// ─────────────────────────────────────────────────────────────────────────────

fn format_number(n: f64) -> String {
    let n = n as i64;
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

fn format_pct(n: f64) -> String {
    format!("{:.1}%", n)
}

fn format_duration(secs: i64) -> String {
    let mins = secs / 60;
    let secs = secs % 60;
    format!("{}:{:02}", mins, secs)
}

/// Group encounters into sections by area (based on is_phase_start flag)
fn group_by_area(encounters: &[EncounterSummary]) -> Vec<(String, Option<String>, Vec<&EncounterSummary>)> {
    let mut sections: Vec<(String, Option<String>, Vec<&EncounterSummary>)> = Vec::new();

    for enc in encounters.iter() {
        if enc.is_phase_start || sections.is_empty() {
            sections.push((enc.area_name.clone(), enc.difficulty.clone(), vec![enc]));
        } else if let Some(section) = sections.last_mut() {
            section.2.push(enc);
        }
    }

    sections
}

// ─────────────────────────────────────────────────────────────────────────────
// Component
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
pub struct DataExplorerProps {
    /// Initial encounter index (None = show selector)
    #[props(default)]
    pub encounter_idx: Option<u32>,
}

#[component]
pub fn DataExplorerPanel(props: DataExplorerProps) -> Element {
    // Encounter selection state
    let mut encounters = use_signal(Vec::<EncounterSummary>::new);
    let mut selected_encounter = use_signal(|| props.encounter_idx);

    // Sidebar state
    let mut show_only_bosses = use_signal(|| false);
    let mut collapsed_sections = use_signal(HashSet::<String>::new);

    // Query result state
    let mut abilities = use_signal(Vec::<AbilityBreakdown>::new);
    let mut entities = use_signal(Vec::<EntityBreakdown>::new);
    let mut selected_source = use_signal(|| None::<String>);
    let mut loading = use_signal(|| false);
    let mut error_msg = use_signal(|| None::<String>);

    // Load encounter list on mount
    use_effect(move || {
        spawn(async move {
            if let Some(list) = api::get_encounter_history().await {
                encounters.set(list);
            }
        });
    });

    // Load data when encounter selection changes
    use_effect(move || {
        let idx = *selected_encounter.read();
        spawn(async move {
            // Clear previous data
            abilities.set(Vec::new());
            entities.set(Vec::new());
            selected_source.set(None);
            error_msg.set(None);

            if idx.is_none() {
                return; // No encounter selected
            }

            loading.set(true);

            // Load entity breakdown first
            match api::query_entity_breakdown(idx).await {
                Some(data) => entities.set(data),
                None => {
                    error_msg.set(Some("No data available for this encounter".to_string()));
                    loading.set(false);
                    return;
                }
            }

            // Load ability breakdown (all sources initially)
            match api::query_damage_by_ability(idx, None).await {
                Some(data) => abilities.set(data),
                None => error_msg.set(Some("Failed to load ability breakdown".to_string())),
            }

            loading.set(false);
        });
    });

    // Filter by source when selected
    let mut on_source_click = move |name: String| {
        let idx = *selected_encounter.read();
        let current = selected_source.read().clone();

        // Toggle selection
        let new_source = if current.as_ref() == Some(&name) {
            None
        } else {
            Some(name.clone())
        };

        selected_source.set(new_source.clone());

        spawn(async move {
            loading.set(true);
            if let Some(data) = api::query_damage_by_ability(idx, new_source.as_deref()).await {
                abilities.set(data);
            }
            loading.set(false);
        });
    };

    // Prepare data for rendering
    let history = encounters();
    let bosses_only = show_only_bosses();
    let collapsed = collapsed_sections();

    // Filter encounters based on boss-only toggle
    let filtered_history: Vec<_> = if bosses_only {
        history.iter().filter(|e| e.boss_name.is_some()).cloned().collect()
    } else {
        history.clone()
    };

    // Group encounters by area
    let sections = group_by_area(&filtered_history);

    rsx! {
        div { class: "data-explorer",
            // Sidebar with encounter list
            aside { class: "explorer-sidebar",
                div { class: "sidebar-header",
                    h3 {
                        i { class: "fa-solid fa-list" }
                        " Encounters"
                    }
                    div { class: "history-controls",
                        label { class: "boss-filter-toggle",
                            input {
                                r#type: "checkbox",
                                checked: bosses_only,
                                onchange: move |e| show_only_bosses.set(e.checked())
                            }
                            span { "Trash" }
                        }
                        span { class: "encounter-count",
                            "{filtered_history.len()}"
                            if bosses_only { " / {history.len()}" }
                        }
                    }
                }

                div { class: "sidebar-encounter-list",
                    if history.is_empty() {
                        div { class: "sidebar-empty",
                            i { class: "fa-solid fa-inbox" }
                            p { "No encounters" }
                            p { class: "hint", "Load a log file to see encounters" }
                        }
                    } else {
                        for (idx, (area_name, difficulty, area_encounters)) in sections.iter().enumerate() {
                            {
                                let section_key = format!("{}_{}", idx, area_name);
                                let is_collapsed = collapsed.contains(&section_key);
                                let section_key_toggle = section_key.clone();
                                let chevron_class = if is_collapsed { "fa-chevron-right" } else { "fa-chevron-down" };

                                rsx! {
                                    // Area header (collapsible)
                                    div {
                                        class: "sidebar-section-header",
                                        onclick: move |_| {
                                            let mut set = collapsed_sections();
                                            if set.contains(&section_key_toggle) {
                                                set.remove(&section_key_toggle);
                                            } else {
                                                set.insert(section_key_toggle.clone());
                                            }
                                            collapsed_sections.set(set);
                                        },
                                        i { class: "fa-solid {chevron_class} collapse-icon" }
                                        span { class: "section-area", "{area_name}" }
                                        if let Some(diff) = difficulty {
                                            span { class: "section-difficulty", " • {diff}" }
                                        }
                                        span { class: "section-count", " ({area_encounters.len()})" }
                                    }

                                    // Encounter items (hidden if collapsed)
                                    if !is_collapsed {
                                        for (enc_offset, enc) in area_encounters.iter().enumerate() {
                                            {
                                                // Calculate global index for this encounter
                                                let global_idx = filtered_history.iter()
                                                    .position(|e| e.encounter_id == enc.encounter_id)
                                                    .map(|i| i as u32);
                                                let enc_idx = global_idx.unwrap_or(enc_offset as u32);
                                                let is_selected = *selected_encounter.read() == Some(enc_idx);
                                                let success_class = if enc.success { "success" } else { "wipe" };

                                                rsx! {
                                                    div {
                                                        class: if is_selected { "sidebar-encounter-item selected" } else { "sidebar-encounter-item" },
                                                        onclick: move |_| selected_encounter.set(Some(enc_idx)),
                                                        div { class: "encounter-main",
                                                            span { class: "encounter-name", "{enc.display_name}" }
                                                            span { class: "result-indicator {success_class}",
                                                                if enc.success {
                                                                    i { class: "fa-solid fa-check" }
                                                                } else {
                                                                    i { class: "fa-solid fa-skull" }
                                                                }
                                                            }
                                                        }
                                                        div { class: "encounter-meta",
                                                            if let Some(time) = &enc.start_time {
                                                                span { class: "encounter-time", "{time}" }
                                                            }
                                                            span { class: "encounter-duration", "({format_duration(enc.duration_seconds)})" }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Data Panel (main content area)
            div { class: "data-panel",
                if selected_encounter.read().is_none() {
                    div { class: "panel-placeholder",
                        i { class: "fa-solid fa-chart-bar" }
                        p { "Select an encounter" }
                        p { class: "hint", "Choose an encounter from the sidebar to view detailed breakdown" }
                    }
                } else {
                    // Header
                    div { class: "explorer-header",
                        h3 {
                            if let Some(idx) = *selected_encounter.read() {
                                if let Some(enc) = encounters.read().get(idx as usize) {
                                    "{enc.display_name}"
                                } else {
                                    "Encounter #{idx}"
                                }
                            }
                        }
                        if *loading.read() {
                            span { class: "loading-indicator", "Loading..." }
                        }
                    }

                    // Error display
                    if let Some(err) = error_msg.read().as_ref() {
                        div { class: "error-message", "{err}" }
                    }

                    // Two-column layout
                    div { class: "explorer-content",
                        // Entity breakdown (source filter)
                        div { class: "entity-section",
                            h4 { "Damage Sources" }
                            div { class: "entity-list",
                                for entity in entities.read().iter() {
                                    {
                                        let name = entity.source_name.clone();
                                        let is_selected = selected_source.read().as_ref() == Some(&name);
                                        rsx! {
                                            div {
                                                class: if is_selected { "entity-row selected" } else { "entity-row" },
                                                onclick: {
                                                    let name = name.clone();
                                                    move |_| on_source_click(name.clone())
                                                },
                                                span { class: "entity-name", "{entity.source_name}" }
                                                span { class: "entity-value", "{format_number(entity.total_value)}" }
                                                span { class: "entity-abilities", "{entity.abilities_used} abilities" }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Ability breakdown table
                        div { class: "ability-section",
                            h4 {
                                if let Some(src) = selected_source.read().as_ref() {
                                    "Abilities - {src}"
                                } else {
                                    "All Abilities"
                                }
                            }
                            table { class: "ability-table",
                                thead {
                                    tr {
                                        th { "Ability" }
                                        th { class: "num", "Total" }
                                        th { class: "num", "Hits" }
                                        th { class: "num", "Avg" }
                                        th { class: "num", "Max" }
                                        th { class: "num", "Crit%" }
                                    }
                                }
                                tbody {
                                    for ability in abilities.read().iter() {
                                        tr {
                                            td { "{ability.ability_name}" }
                                            td { class: "num", "{format_number(ability.total_value)}" }
                                            td { class: "num", "{ability.hit_count}" }
                                            td { class: "num", "{format_number(ability.avg_hit)}" }
                                            td { class: "num", "{format_number(ability.max_hit)}" }
                                            td { class: "num", "{format_pct(ability.crit_rate)}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
