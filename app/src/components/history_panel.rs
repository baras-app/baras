//! Encounter History Panel Component
//!
//! Displays a table of all encounters from the current log file session.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local as spawn;

use crate::api;
use crate::components::class_icons::{get_class_icon, get_role_icon};
use crate::components::{ToastSeverity, use_parsely_upload, use_toast};
use baras_types::formatting;

// ─────────────────────────────────────────────────────────────────────────────
// Upload State Tracking
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum UploadState {
    Idle,
    Uploading,
    Success(String), // Contains the Parsely link
    Error(String),   // Contains the error message
}

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
    #[serde(default)]
    pub end_time: Option<String>,
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
    #[serde(default)]
    pub challenges: Vec<ChallengeSummary>,
    // Line number tracking for per-encounter Parsely uploads
    #[serde(default)]
    pub area_entered_line: Option<u64>,
    #[serde(default)]
    pub event_start_line: Option<u64>,
    #[serde(default)]
    pub event_end_line: Option<u64>,
    // Parsely link (set after successful upload)
    #[serde(default)]
    pub parsely_link: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChallengeSummary {
    pub name: String,
    pub metric: String,
    pub total_value: i64,
    pub event_count: u32,
    pub duration_secs: f32,
    pub per_second: Option<f32>,
    pub by_player: Vec<ChallengePlayerSummary>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChallengePlayerSummary {
    pub entity_id: i64,
    pub name: String,
    pub value: i64,
    pub percent: f32,
    pub per_second: Option<f32>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper Functions
// ─────────────────────────────────────────────────────────────────────────────



/// Group encounters into sections by area (based on is_phase_start flag or area change)
fn group_by_area(
    encounters: &[EncounterSummary],
) -> Vec<(String, Option<String>, Vec<&EncounterSummary>)> {
    let mut sections: Vec<(String, Option<String>, Vec<&EncounterSummary>)> = Vec::new();

    for enc in encounters.iter() {
        // Start new section if: phase start, no sections yet, or area/difficulty changed
        let area_changed = sections
            .last()
            .map_or(false, |s| s.0 != enc.area_name || s.1 != enc.difficulty);

        if enc.is_phase_start || sections.is_empty() || area_changed {
            sections.push((enc.area_name.clone(), enc.difficulty.clone(), vec![enc]));
        } else if let Some(section) = sections.last_mut() {
            section.2.push(enc);
        }
    }

    sections
}

// ─────────────────────────────────────────────────────────────────────────────
// Components
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
pub struct HistoryPanelProps {
    pub state: Signal<crate::types::UiSessionState>,
}

#[component]
pub fn HistoryPanel(mut props: HistoryPanelProps) -> Element {
    let mut encounters = use_signal(Vec::<EncounterSummary>::new);
    let mut expanded_id = use_signal(|| None::<u64>);
    let mut collapsed_sections = use_signal(HashSet::<String>::new);
    let mut loading = use_signal(|| true);
    
    // Create local signal from ui_state (same pattern as DataExplorer)
    let mut show_only_bosses = use_signal(|| props.state.read().data_explorer.show_only_bosses);
    
    // Sync show_only_bosses from parent state when it changes (e.g., config loaded at startup)
    // This handles the race condition where the component mounts before async config loading completes
    // NOTE: We only sync parent→local here. Local→parent sync happens in the onchange handler
    // to avoid a bidirectional sync loop that causes "sticky" toggle behavior.
    use_effect(move || {
        let parent_bosses = props.state.read().data_explorer.show_only_bosses;
        if *show_only_bosses.read() != parent_bosses {
            show_only_bosses.set(parent_bosses);
        }
    });
    
    // Track upload state per encounter_id
    let mut upload_states = use_signal(HashMap::<u64, UploadState>::new);
    // Get parsely upload manager for event handlers
    let mut parsely_upload = use_parsely_upload();

    // Fetch encounter history
    use_future(move || async move {
        if let Some(history) = api::get_encounter_history().await {
            // Auto-expand the most recent encounter (last in list = newest)
            if let Some(latest) = history.last() {
                expanded_id.set(Some(latest.encounter_id));
            }
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
                let is_combat_ended = event_type.contains("CombatEnded");
                spawn(async move {
                    if let Some(history) = api::get_encounter_history().await {
                        // Auto-expand the newest encounter on CombatEnded
                        if is_combat_ended {
                            if let Some(latest) = history.last() {
                                let _ = expanded_id
                                    .try_write()
                                    .map(|mut w| *w = Some(latest.encounter_id));
                            }
                        }
                        let _ = encounters.try_write().map(|mut w| *w = history);
                    }
                });
            }
        });
        api::tauri_listen("session-updated", &closure).await;
        closure.forget();
    });

    // Listen for parsely upload success (to refresh encounter links)
    use_future(move || async move {
        if let Some(window) = web_sys::window() {
            let closure = Closure::<dyn Fn(web_sys::Event)>::new(move |_event: web_sys::Event| {
                spawn(async move {
                    if let Some(history) = api::get_encounter_history().await {
                        let _ = encounters.try_write().map(|mut w| *w = history);
                    }
                });
            });
            let _ = window.add_event_listener_with_callback(
                "parsely-upload-success",
                closure.as_ref().unchecked_ref()
            );
            closure.forget();
        }
    });

    let history = encounters();
    let is_loading = loading();
    let selected = expanded_id();
    let collapsed = collapsed_sections();
    let bosses_only = show_only_bosses();

    // Filter encounters if boss-only mode is enabled
    // When filtering, propagate is_phase_start to the next visible encounter
    // so phase boundaries aren't lost when trash encounters are hidden
    let filtered_history: Vec<_> = if bosses_only {
        let mut pending_phase_start = false;
        history
            .iter()
            .filter_map(|e| {
                if e.is_phase_start {
                    pending_phase_start = true;
                }
                if e.boss_name.is_some() {
                    let mut enc = e.clone();
                    if pending_phase_start {
                        enc.is_phase_start = true;
                        pending_phase_start = false;
                    }
                    Some(enc)
                } else {
                    None
                }
            })
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
                label { class: "toggle-switch-label boss-filter-toggle",
                    span { class: "toggle-switch",
                        input {
                            r#type: "checkbox",
                            checked: *show_only_bosses.read(),
                            onchange: move |e| {
                                let checked = e.checked();
                                show_only_bosses.set(checked);
                                // Update parent state immediately (avoids bidirectional sync loop)
                                if let Ok(mut state) = props.state.try_write() {
                                    state.data_explorer.show_only_bosses = checked;
                                }
                                let mut toast = use_toast();
                                spawn(async move {
                                    if let Some(mut cfg) = api::get_config().await {
                                        cfg.show_only_bosses = checked;
                                        if let Err(err) = api::update_config(&cfg).await {
                                            toast.show(format!("Failed to save settings: {}", err), ToastSeverity::Normal);
                                        }
                                    }
                                });
                            }
                        }
                        span { class: "toggle-slider" }
                    }
                    span { class: "toggle-text", "Bosses only" }
                }
                span { class: "encounter-count",
                    "{filtered_history.len()}"
                    if *show_only_bosses.read() { " / {history.len()}" }
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
                                th { class: "col-upload", "" }
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
                                            td { colspan: "5",
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
                                                    let npc_list = enc.npc_names.join(", ");
                                                    
                                                    // Check if already uploaded (persisted in backend)
                                                    let persisted_link = enc.parsely_link.clone();
                                                    
                                                    // Get transient upload state (for uploading/error states)
                                                    let current_upload_state = upload_states()
                                                        .get(&enc_id)
                                                        .cloned()
                                                        .unwrap_or(UploadState::Idle);
                                                    
                                                    // Line numbers for upload (always present for parsed encounters)
                                                    let start_line = enc.event_start_line.unwrap_or(0);
                                                    let end_line = enc.event_end_line.unwrap_or(0);
                                                    let area_line = enc.area_entered_line;

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
                                                                div { class: "encounter-info",
                                                                    span { class: "encounter-name", "{enc.display_name}" }
                                                                    if !npc_list.is_empty() {
                                                                        span { class: "encounter-npcs", "{npc_list}" }
                                                                    }
                                                                }
                                                            }
                                                            td { class: "col-duration",
                                                                "{formatting::format_duration(enc.duration_seconds)}"
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
                                                            td { class: "col-upload",
                                                                // If already uploaded (persisted), show link
                                                                if let Some(link) = persisted_link {
                                                                    a {
                                                                        class: "parsely-upload-btn success",
                                                                        href: "{link}",
                                                                        target: "_blank",
                                                                        title: "View on Parsely",
                                                                        onclick: |e| e.stop_propagation(),
                                                                        i { class: "fa-solid fa-external-link" }
                                                                    }
                                                                } else {
                                                                    // Otherwise show upload button or transient state
                                                                    match current_upload_state {
                                                                        UploadState::Idle => rsx! {
                                                                            button {
                                                                                class: "parsely-upload-btn",
                                                                                title: "Upload to Parsely",
                                                                                onclick: {
                                                                                    let encounter_name = enc.display_name.clone();
                                                                                    move |e| {
                                                                                        e.stop_propagation();
                                                                                        
                                                                                        let name = encounter_name.clone();
                                                                                        
                                                                                        // Get the active file path
                                                                                        spawn(async move {
                                                                                            if let Some(path) = api::get_active_file().await {
                                                                                                parsely_upload.open_encounter(
                                                                                                    path,
                                                                                                    name,
                                                                                                    enc_id,
                                                                                                    start_line,
                                                                                                    end_line,
                                                                                                    area_line,
                                                                                                );
                                                                                            }
                                                                                        });
                                                                                    }
                                                                                },
                                                                                i { class: "fa-solid fa-upload" }
                                                                            }
                                                                        },
                                                                        UploadState::Uploading => rsx! {
                                                                            span { class: "parsely-upload-btn uploading",
                                                                                i { class: "fa-solid fa-spinner fa-spin" }
                                                                            }
                                                                        },
                                                                        UploadState::Success(_) => rsx! {
                                                                            // This shouldn't happen - success saves to backend
                                                                            // But handle it gracefully
                                                                            i { class: "fa-solid fa-check" }
                                                                        },
                                                                        UploadState::Error(ref err) => rsx! {
                                                                            button {
                                                                                class: "parsely-upload-btn error",
                                                                                title: "Error: {err}. Click to retry.",
                                                                                onclick: move |e| {
                                                                                    e.stop_propagation();
                                                                                    upload_states.with_mut(|states| {
                                                                                        states.insert(enc_id, UploadState::Idle);
                                                                                    });
                                                                                },
                                                                                i { class: "fa-solid fa-triangle-exclamation" }
                                                                            }
                                                                        },
                                                                    }
                                                                }
                                                            }
                                                        }
                                                        // Expanded detail row
                                                        if is_expanded {
                                                            tr { class: "detail-row",
                                                                td { colspan: "5",
                                                                    EncounterDetail {
                                                                        encounter: (*enc).clone(),
                                                                        state: props.state,
                                                                        encounter_idx: enc_id as u32,
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
fn EncounterDetail(
    encounter: EncounterSummary,
    state: Signal<crate::types::UiSessionState>,
    encounter_idx: u32,
) -> Element {
    let mut sort_column = use_signal(|| SortColumn::Dps);
    let mut sort_ascending = use_signal(|| false); // Default descending for metrics

    let metrics = &encounter.player_metrics;

    // Sort metrics based on current sort state
    let mut sorted_metrics = metrics.clone();
    sort_metrics(&mut sorted_metrics, sort_column(), sort_ascending());

    // Format NPC list
    let npc_list = encounter.npc_names.join(", ");

    let eu = state.read().european_number_format;
    let format_number = |n: i64| formatting::format_compact(n, eu);

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
                    " {formatting::format_duration(encounter.duration_seconds)}"
                }
                if let Some(end_time) = &encounter.end_time {
                    span { class: "detail-item",
                        i { class: "fa-solid fa-flag-checkered" }
                        " {end_time}"
                    }
                }
                button {
                    class: "btn btn-view-explorer",
                    title: "View detailed breakdown in Data Explorer",
                    onclick: move |_| {
                        let mut s = state.write();
                        s.active_tab = crate::types::MainTab::DataExplorer;
                        s.data_explorer.selected_encounter = Some(encounter_idx);
                    },
                    i { class: "fa-solid fa-magnifying-glass-chart" }
                    " View in Explorer"
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
                                                    img {
                                                        class: "class-icon",
                                                        src: *icon_asset,
                                                        title: "{player.discipline_name.as_deref().unwrap_or(\"\")}",
                                                        alt: ""
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
                                    td { class: "metric-value hps", "{formatting::format_pct_f32(player.effective_heal_pct, eu)}" }
                                    td { class: "metric-value hps", "{format_number(player.abs)}" }
                                    td { class: "metric-value apm", "{formatting::format_f32_1(player.apm, eu)}" }
                                }
                            }
                        }
                        {
                            let tot_dps: i64 = sorted_metrics.iter().map(|p| p.dps).sum();
                            let tot_dmg: i64 = sorted_metrics.iter().map(|p| p.total_damage).sum();
                            let tot_tps: i64 = sorted_metrics.iter().map(|p| p.tps).sum();
                            let tot_taken: i64 = sorted_metrics.iter().map(|p| p.total_damage_taken).sum();
                            let tot_dtps: i64 = sorted_metrics.iter().map(|p| p.dtps).sum();
                            let tot_hps: i64 = sorted_metrics.iter().map(|p| p.hps).sum();
                            let tot_ehps: i64 = sorted_metrics.iter().map(|p| p.ehps).sum();
                            let tot_heal: i64 = sorted_metrics.iter().map(|p| p.total_healing).sum();
                            let tot_eheal: i64 = sorted_metrics.iter().map(|p| p.total_healing_effective).sum();
                            let eff_pct = if tot_heal > 0 { tot_eheal as f32 / tot_heal as f32 * 100.0 } else { 0.0 };
                            let tot_abs: i64 = sorted_metrics.iter().map(|p| p.abs).sum();
                            rsx! {
                                tfoot {
                                    tr { class: "totals-row",
                                        td { class: "player-name totals-label", "Total" }
                                        td { class: "metric-value dps", "{format_number(tot_dps)}" }
                                        td { class: "metric-value dps", "{format_number(tot_dmg)}" }
                                        td { class: "metric-value tps", "{format_number(tot_tps)}" }
                                        td { class: "metric-value dtps", "{format_number(tot_taken)}" }
                                        td { class: "metric-value dtps", "{format_number(tot_dtps)}" }
                                        td { class: "metric-value hps", "{format_number(tot_hps)}" }
                                        td { class: "metric-value hps", "{format_number(tot_ehps)}" }
                                        td { class: "metric-value hps", "{formatting::format_pct_f32(eff_pct, eu)}" }
                                        td { class: "metric-value hps", "{format_number(tot_abs)}" }
                                        td { class: "metric-value apm", "—" }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Challenge results
            if !encounter.challenges.is_empty() {
                div { class: "challenge-results",
                    h4 { class: "challenge-results-header",
                        i { class: "fa-solid fa-trophy" }
                        " Challenges"
                    }
                    div { class: "challenge-cards",
                        for challenge in encounter.challenges.iter() {
                            {
                                let duration_str = formatting::format_duration(challenge.duration_secs as i64);
                                let per_sec_str = challenge.per_second
                                    .map(|ps| format_number(ps as i64))
                                    .unwrap_or_default();

                                rsx! {
                                    div { class: "challenge-card",
                                        div { class: "challenge-card-header",
                                            span { class: "challenge-name", "{challenge.name}" }
                                            span { class: "challenge-total",
                                                "{format_number(challenge.total_value)}"
                                                if !per_sec_str.is_empty() {
                                                    span { class: "text-muted", " ({per_sec_str}/s)" }
                                                }
                                            }
                                            span { class: "challenge-duration text-muted", "{duration_str}" }
                                        }
                                        if !challenge.by_player.is_empty() {
                                            div { class: "challenge-players",
                                                for player in challenge.by_player.iter() {
                                                    {
                                                        let ps_str = player.per_second
                                                            .map(|ps| format_number(ps as i64))
                                                            .unwrap_or_default();
                                                        rsx! {
                                                            div { class: "challenge-player-row",
                                                                div {
                                                                    class: "challenge-bar-fill",
                                                                    style: "width: {player.percent:.1}%",
                                                                }
                                                                span { class: "challenge-player-name", "{player.name}" }
                                                                if !ps_str.is_empty() {
                                                                    span { class: "challenge-player-ps", "{ps_str}/s" }
                                                                }
                                                                span { class: "challenge-player-value", "{format_number(player.value)}" }
                                                                span { class: "challenge-player-pct", "{formatting::format_pct_f32(player.percent, eu)}" }
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
