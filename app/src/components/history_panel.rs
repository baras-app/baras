//! Encounter History Panel Component
//!
//! Displays a table of all encounters from the current log file session.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local as spawn;

use crate::api;
use crate::components::class_icons::{get_class_icon, get_role_icon};

// ─────────────────────────────────────────────────────────────────────────────
// Data Types (mirrors backend)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerMetrics {
    pub entity_id: i64,
    pub name: String,
    #[serde(default)]
    pub discipline_name: Option<String>,
    #[serde(default)]
    pub class_name: Option<String>,
    #[serde(default)]
    pub class_icon: Option<String>,
    #[serde(default)]
    pub role_icon: Option<String>,
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
    pub encounter_type: String,
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
fn group_by_area(
    encounters: &[EncounterSummary],
) -> Vec<(String, Option<String>, Vec<&EncounterSummary>)> {
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
    let mut show_only_bosses = use_signal(|| false);

    // Fetch encounter history
    use_future(move || async move {
        if let Some(history) = api::get_encounter_history().await {
            encounters.set(history);
        }
        loading.set(false);
    });

    // Listen for session updates (refresh on combat end, file change, etc.)
    use_future(move || async move {
        let closure = Closure::new(move |event: JsValue| {
            // Extract payload from event object (Tauri events have { payload: "..." } structure)
            if let Ok(payload) = js_sys::Reflect::get(&event, &JsValue::from_str("payload"))
                && let Some(event_type) = payload.as_string()
                && (event_type.contains("CombatEnded")
                    || event_type.contains("TailingModeChanged")
                    || event_type.contains("FileLoaded"))
            {
                spawn(async move {
                    if let Some(history) = api::get_encounter_history().await {
                        // Use try_write to handle signal being dropped when component unmounts
                        let _ = encounters.try_write().map(|mut w| *w = history);
                    }
                });
            }
        });
        api::tauri_listen("session-updated", &closure).await;
        closure.forget();
    });

    let history = encounters();
    let is_loading = loading();
    let selected = expanded_id();
    let collapsed = collapsed_sections();
    let bosses_only = show_only_bosses();

    // Filter encounters if boss-only mode is enabled
    let filtered_history: Vec<_> = if bosses_only {
        history
            .iter()
            .filter(|e| e.boss_name.is_some())
            .cloned()
            .collect()
    } else {
        history.clone()
    };

    // Group encounters by area (ascending order - oldest first)
    let sections = group_by_area(&filtered_history)
        .into_iter()
        .map(|(area, diff, encs)| {
            let rev_encs: Vec<_> = encs.into_iter().rev().collect();
            (area, diff, rev_encs)
        })
        .rev()
        .collect::<Vec<_>>();

    rsx! {
        section { class: "history-panel",
            div { class: "history-header",
                h3 {
                    i { class: "fa-solid fa-clock-rotate-left" }
                    " Encounter History"
                }
                div { class: "history-controls",
                    label { class: "boss-filter-toggle",
                        input {
                            r#type: "checkbox",
                            checked: bosses_only,
                            onchange: move |e| show_only_bosses.set(e.checked())
                        }
                        span { "Bosses only" }
                    }
                    span { class: "encounter-count",
                        "{filtered_history.len()}"
                        if bosses_only { " / {history.len()}" }
                    }
                }
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

// ─────────────────────────────────────────────────────────────────────────────
// Sortable Metrics Table
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortColumn {
    Player,
    Dps,
    TotalDamage,
    Tps,
    DamageTaken,
    Dtps,
    Hps,
    Ehps,
    EffectiveHealPct,
    Abs,
    Apm,
}

impl SortColumn {
    fn label(&self) -> &'static str {
        match self {
            Self::Player => "Player",
            Self::Dps => "DPS",
            Self::TotalDamage => "Total Dmg",
            Self::Tps => "TPS",
            Self::DamageTaken => "Dmg Taken",
            Self::Dtps => "DTPS",
            Self::Hps => "HPS",
            Self::Ehps => "eHPS",
            Self::EffectiveHealPct => "Eff Heal%",
            Self::Abs => "ABS",
            Self::Apm => "APM",
        }
    }
}

fn sort_metrics(metrics: &mut [PlayerMetrics], column: SortColumn, ascending: bool) {
    metrics.sort_by(|a, b| {
        let cmp = match column {
            SortColumn::Player => a.name.cmp(&b.name),
            SortColumn::Dps => a.dps.cmp(&b.dps),
            SortColumn::TotalDamage => a.total_damage.cmp(&b.total_damage),
            SortColumn::Tps => a.tps.cmp(&b.tps),
            SortColumn::DamageTaken => a.total_damage_taken.cmp(&b.total_damage_taken),
            SortColumn::Dtps => a.dtps.cmp(&b.dtps),
            SortColumn::Hps => a.hps.cmp(&b.hps),
            SortColumn::Ehps => a.ehps.cmp(&b.ehps),
            SortColumn::EffectiveHealPct => a
                .effective_heal_pct
                .partial_cmp(&b.effective_heal_pct)
                .unwrap_or(std::cmp::Ordering::Equal),
            SortColumn::Abs => a.abs.cmp(&b.abs),
            SortColumn::Apm => a
                .apm
                .partial_cmp(&b.apm)
                .unwrap_or(std::cmp::Ordering::Equal),
        };
        if ascending { cmp } else { cmp.reverse() }
    });
}

#[component]
fn EncounterDetail(encounter: EncounterSummary) -> Element {
    let mut sort_column = use_signal(|| SortColumn::Dps);
    let mut sort_ascending = use_signal(|| false); // Default descending for metrics

    let metrics = &encounter.player_metrics;

    // Sort metrics based on current sort state
    let mut sorted_metrics = metrics.clone();
    sort_metrics(&mut sorted_metrics, sort_column(), sort_ascending());

    // Format NPC list
    let npc_list = encounter.npc_names.join(", ");

    // Column definitions for the table
    let columns = [
        SortColumn::Player,
        SortColumn::Dps,
        SortColumn::TotalDamage,
        SortColumn::Tps,
        SortColumn::DamageTaken,
        SortColumn::Dtps,
        SortColumn::Hps,
        SortColumn::Ehps,
        SortColumn::EffectiveHealPct,
        SortColumn::Abs,
        SortColumn::Apm,
    ];

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
                    table { class: "metrics-table sortable",
                        thead {
                            tr {
                                for col in columns {
                                    {
                                        let is_active = sort_column() == col;
                                        let is_asc = sort_ascending();
                                        let header_class = if col == SortColumn::Player { "col-player sortable-header" } else { "col-metric sortable-header" };
                                        let sort_icon = if is_active {
                                            if is_asc { "fa-sort-up" } else { "fa-sort-down" }
                                        } else {
                                            "fa-sort"
                                        };
                                        let active_class = if is_active { "active" } else { "" };

                                        rsx! {
                                            th {
                                                class: "{header_class} {active_class}",
                                                onclick: move |_| {
                                                    if sort_column() == col {
                                                        sort_ascending.set(!sort_ascending());
                                                    } else {
                                                        sort_column.set(col);
                                                        // Default to descending for numeric columns, ascending for player name
                                                        sort_ascending.set(col == SortColumn::Player);
                                                    }
                                                },
                                                span { "{col.label()}" }
                                                i { class: "fa-solid {sort_icon} sort-icon" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        tbody {
                            for player in sorted_metrics.iter() {
                                tr {
                                    td { class: "player-name",
                                        span { class: "name-with-icon",
                                            if let Some(role_name) = &player.role_icon {
                                                if let Some(role_asset) = get_role_icon(role_name) {
                                                    img {
                                                        class: "role-icon",
                                                        src: *role_asset,
                                                        alt: ""
                                                    }
                                                }
                                            }
                                            if let Some(icon_name) = &player.class_icon {
                                                if let Some(icon_asset) = get_class_icon(icon_name) {
                                                    {
                                                        let class_css = icon_name.trim_end_matches(".png");
                                                        rsx! {
                                                            img {
                                                                class: "class-icon class-{class_css}",
                                                                src: *icon_asset,
                                                                title: "{player.discipline_name.as_deref().unwrap_or(\"\")}",
                                                                alt: ""
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            "{player.name}"
                                        }
                                    }
                                    td { class: "metric-value dps", "{format_number(player.dps)}" }
                                    td { class: "metric-value dps", "{format_number(player.total_damage)}" }
                                    td { class: "metric-value tps", "{format_number(player.tps)}" }
                                    td { class: "metric-value dtps", "{format_number(player.total_damage_taken)}" }
                                    td { class: "metric-value dtps", "{format_number(player.dtps)}" }
                                    td { class: "metric-value hps", "{format_number(player.hps)}" }
                                    td { class: "metric-value hps", "{format_number(player.ehps)}" }
                                    td { class: "metric-value hps", "{player.effective_heal_pct:.1}%" }
                                    td { class: "metric-value hps", "{format_number(player.abs)}" }
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
