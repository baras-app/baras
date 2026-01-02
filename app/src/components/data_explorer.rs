//! Data Explorer Panel Component
//!
//! Displays detailed ability breakdown and DPS analysis for encounters.
//! Uses DataFusion SQL queries over parquet files for historical data.

use dioxus::prelude::*;
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

    rsx! {
        div { class: "data-explorer",
            // Encounter Selector
            div { class: "encounter-selector",
                h3 { "Select Encounter" }
                div { class: "encounter-list",
                    if encounters.read().is_empty() {
                        p { class: "hint", "No encounters available. Load a log file first." }
                    }
                    for (idx, enc) in encounters.read().iter().enumerate() {
                        {
                            let enc_idx = idx as u32;
                            let is_selected = *selected_encounter.read() == Some(enc_idx);
                            rsx! {
                                div {
                                    class: if is_selected { "encounter-item selected" } else { "encounter-item" },
                                    onclick: move |_| selected_encounter.set(Some(enc_idx)),
                                    span { class: "encounter-name", "{enc.display_name}" }
                                    span { class: "encounter-area", "{enc.area_name}" }
                                    if enc.success {
                                        span { class: "encounter-success", "✓" }
                                    } else {
                                        span { class: "encounter-wipe", "✗" }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Data Panel (only show when encounter selected)
            if selected_encounter.read().is_some() {
                div { class: "data-panel",
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
