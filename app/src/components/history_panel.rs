//! Encounter History Panel Component
//!
//! Displays a table of all encounters from the current log file session.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use std::collections::HashSet;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    async fn invoke(cmd: &str, args: JsValue) -> JsValue;
}

// ─────────────────────────────────────────────────────────────────────────────
// Data Types (mirrors backend)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerMetrics {
    pub entity_id: i64,
    pub name: String,
    pub dps: i64,
    pub edps: i64,
    pub bossdps: i64,
    pub total_damage: i64,
    pub total_damage_effective: i64,
    pub total_damage_boss: i64,
    pub damage_crit_pct: f32,
    pub hps: i64,
    pub ehps: i64,
    pub total_healing: i64,
    pub total_healing_effective: i64,
    pub heal_crit_pct: f32,
    pub effective_heal_pct: f32,
    pub tps: i64,
    pub total_threat: i64,
    pub dtps: i64,
    pub edtps: i64,
    pub total_damage_taken: i64,
    pub total_damage_taken_effective: i64,
    pub abs: i64,
    pub total_shielding: i64,
    pub apm: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EncounterSummary {
    pub encounter_id: u64,
    pub display_name: String,
    pub phase_type: String,
    pub start_time: Option<String>,
    pub duration_seconds: i64,
    pub success: bool,
    pub area_name: String,
    pub difficulty: Option<String>,
    pub boss_name: Option<String>,
    pub player_metrics: Vec<PlayerMetrics>,
    #[serde(default)]
    pub is_phase_start: bool,
    #[serde(default)]
    pub npc_names: Vec<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper Functions
// ─────────────────────────────────────────────────────────────────────────────

fn format_duration(secs: i64) -> String {
    let mins = secs / 60;
    let secs = secs % 60;
    format!("{}:{:02}", mins, secs)
}

fn format_number(n: i64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

/// Group encounters into sections by area (based on is_phase_start flag)
fn group_by_area(encounters: &[EncounterSummary]) -> Vec<(String, Option<String>, Vec<&EncounterSummary>)> {
    let mut sections: Vec<(String, Option<String>, Vec<&EncounterSummary>)> = Vec::new();

    for enc in encounters.iter() {
        if enc.is_phase_start || sections.is_empty() {
            // Start a new section
            sections.push((enc.area_name.clone(), enc.difficulty.clone(), vec![enc]));
        } else if let Some(section) = sections.last_mut() {
            // Add to current section
            section.2.push(enc);
        }
    }

    sections
}

// ─────────────────────────────────────────────────────────────────────────────
// Components
// ─────────────────────────────────────────────────────────────────────────────

#[component]
pub fn HistoryPanel() -> Element {
    let mut encounters = use_signal(Vec::<EncounterSummary>::new);
    let mut expanded_id = use_signal(|| None::<u64>);
    let mut collapsed_sections = use_signal(HashSet::<String>::new);
    let mut loading = use_signal(|| true);

    // Fetch encounter history
    use_future(move || async move {
        let result = invoke("get_encounter_history", JsValue::NULL).await;
        if let Ok(history) = serde_wasm_bindgen::from_value::<Vec<EncounterSummary>>(result) {
            encounters.set(history);
        }
        loading.set(false);
    });

    // Poll for updates
    use_future(move || async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(3000).await;
            let result = invoke("get_encounter_history", JsValue::NULL).await;
            if let Ok(history) = serde_wasm_bindgen::from_value::<Vec<EncounterSummary>>(result) {
                encounters.set(history);
            }
        }
    });

    let history = encounters();
    let is_loading = loading();
    let selected = expanded_id();
    let collapsed = collapsed_sections();

    // Group encounters by area (ascending order - oldest first)
    let sections = group_by_area(&history);

    rsx! {
        section { class: "history-panel",
            div { class: "history-header",
                h3 {
                    i { class: "fa-solid fa-clock-rotate-left" }
                    " Encounter History"
                }
                span { class: "encounter-count", "{history.len()} encounters" }
            }

            if is_loading {
                div { class: "history-loading",
                    i { class: "fa-solid fa-spinner fa-spin" }
                    " Loading..."
                }
            } else if history.is_empty() {
                div { class: "history-empty",
                    i { class: "fa-solid fa-inbox" }
                    p { "No encounters yet" }
                    p { class: "hint", "Encounters will appear here as combat occurs" }
                }
            } else {
                div { class: "history-table-container",
                    table { class: "history-table",
                        thead {
                            tr {
                                th { class: "col-name", "Encounter" }
                                th { class: "col-type", "Type" }
                                th { class: "col-duration", "Duration" }
                                th { class: "col-result", "Result" }
                            }
                        }
                        tbody {
                            for (idx, (area_name, difficulty, area_encounters)) in sections.iter().enumerate() {
                                {
                                    let section_key = format!("{}_{}", idx, area_name);
                                    let is_collapsed = collapsed.contains(&section_key);
                                    let section_key_toggle = section_key.clone();
                                    let chevron_class = if is_collapsed { "fa-chevron-right" } else { "fa-chevron-down" };

                                    rsx! {
                                        // Area header row (collapsible)
                                        tr {
                                            class: "phase-header-row",
                                            onclick: move |_| {
                                                let mut set = collapsed_sections();
                                                if set.contains(&section_key_toggle) {
                                                    set.remove(&section_key_toggle);
                                                } else {
                                                    set.insert(section_key_toggle.clone());
                                                }
                                                collapsed_sections.set(set);
                                            },
                                            td { colspan: "4",
                                                div { class: "phase-header",
                                                    i { class: "fa-solid {chevron_class} collapse-icon" }
                                                    i { class: "fa-solid fa-map-location-dot" }
                                                    span { class: "phase-area", " {area_name}" }
                                                    if let Some(diff) = difficulty {
                                                        span { class: "phase-difficulty", " • {diff}" }
                                                    }
                                                    span { class: "section-count", " ({area_encounters.len()})" }
                                                }
                                            }
                                        }
                                        // Encounter rows (hidden if collapsed)
                                        if !is_collapsed {
                                            for enc in area_encounters.iter() {
                                                {
                                                    let enc_id = enc.encounter_id;
                                                    let is_expanded = selected == Some(enc_id);
                                                    let row_class = if is_expanded { "expanded" } else { "" };
                                                    let success_class = if enc.success { "success" } else { "wipe" };

                                                    rsx! {
                                                        tr {
                                                            key: "{enc_id}",
                                                            class: "{row_class}",
                                                            onclick: move |_| {
                                                                if selected == Some(enc_id) {
                                                                    expanded_id.set(None);
                                                                } else {
                                                                    expanded_id.set(Some(enc_id));
                                                                }
                                                            },
                                                            td { class: "col-name",
                                                                span { class: "encounter-name", "{enc.display_name}" }
                                                            }
                                                            td { class: "col-type",
                                                                span { class: "phase-type", "{enc.phase_type}" }
                                                            }
                                                            td { class: "col-duration",
                                                                "{format_duration(enc.duration_seconds)}"
                                                            }
                                                            td { class: "col-result",
                                                                span { class: "result-badge {success_class}",
                                                                    if enc.success {
                                                                        i { class: "fa-solid fa-check" }
                                                                    } else {
                                                                        i { class: "fa-solid fa-skull" }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                        // Expanded detail row
                                                        if is_expanded {
                                                            tr { class: "detail-row",
                                                                td { colspan: "4",
                                                                    EncounterDetail { encounter: (*enc).clone() }
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
                }
            }
        }
    }
}

#[component]
fn EncounterDetail(encounter: EncounterSummary) -> Element {
    let metrics = &encounter.player_metrics;

    // Sort by DPS descending
    let mut sorted_metrics = metrics.clone();
    sorted_metrics.sort_by(|a, b| b.dps.cmp(&a.dps));

    // Format NPC list
    let npc_list = encounter.npc_names.join(", ");

    rsx! {
        div { class: "encounter-detail",
            div { class: "detail-header",
                if let Some(time) = &encounter.start_time {
                    span { class: "detail-item",
                        i { class: "fa-solid fa-clock" }
                        " {time}"
                    }
                }
                span { class: "detail-item",
                    i { class: "fa-solid fa-stopwatch" }
                    " {format_duration(encounter.duration_seconds)}"
                }
                if !npc_list.is_empty() {
                    span { class: "detail-item npc-list",
                        i { class: "fa-solid fa-skull-crossbones" }
                        " {npc_list}"
                    }
                }
            }

            if sorted_metrics.is_empty() {
                p { class: "no-metrics", "No player metrics available" }
            } else {
                div { class: "metrics-table-scroll",
                    table { class: "metrics-table",
                        thead {
                            tr {
                                th { class: "col-player", "Player" }
                                th { class: "col-metric", "DPS" }
                                th { class: "col-metric", "eDPS" }
                                th { class: "col-metric", "Boss" }
                                th { class: "col-metric", "HPS" }
                                th { class: "col-metric", "eHPS" }
                                th { class: "col-metric", "ABS" }
                                th { class: "col-metric", "DTPS" }
                                th { class: "col-metric", "TPS" }
                                th { class: "col-metric", "APM" }
                            }
                        }
                        tbody {
                            for player in sorted_metrics.iter() {
                                tr {
                                    td { class: "player-name", "{player.name}" }
                                    td { class: "metric-value dps", "{format_number(player.dps)}" }
                                    td { class: "metric-value dps", "{format_number(player.edps)}" }
                                    td { class: "metric-value dps", "{format_number(player.bossdps)}" }
                                    td { class: "metric-value hps", "{format_number(player.hps)}" }
                                    td { class: "metric-value hps", "{format_number(player.ehps)}" }
                                    td { class: "metric-value hps", "{format_number(player.abs)}" }
                                    td { class: "metric-value dtps", "{format_number(player.edtps)}" }
                                    td { class: "metric-value tps", "{format_number(player.tps)}" }
                                    td { class: "metric-value apm", "{player.apm:.1}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
