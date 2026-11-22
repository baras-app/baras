//! Encounter list sidebar: boss filter, live button and per-area encounter sections.

use dioxus::prelude::*;

use super::ctx::{ExplorerCtx, repaint_with};
use crate::api;
use crate::components::overview_table::pct_tier;
use crate::components::toggle_switch::ToggleSwitch;
use crate::components::{ToastSeverity, use_parsely_upload, use_toast};
use baras_types::formatting;

pub(super) fn encounter_sidebar(ctx: ExplorerCtx) -> Element {
    let ExplorerCtx {
        mut state,
        mut encounters,
        mut selected_encounter,
        mut auto_live,
        mut auto_follow,
        mut live_query_active,
        mut live_poll_tick,
        mut show_only_bosses,
        mut collapsed_sections,
        mut sidebar_collapsed,
        filtered_history,
        sections,
        ttk_ratings,
        ..
    } = ctx;
    repaint_with(encounters);

    // Toast notifications
    let mut toast = use_toast();

    // Which raid-summary run each pull belongs to; refetched when the log's
    // encounters change or the backend writes a history file
    let mut history_version = use_signal(|| 0u32);
    api::use_tauri_event("history-updated", move |_| {
        let _ = history_version.try_write().map(|mut w| *w += 1);
    });
    let run_links = use_resource(move || {
        let (_, _) = (history_version(), encounters.read().len());
        async move { api::get_session_run_links().await }
    });

    // Parsely upload state for per-encounter uploads
    let mut parsely_upload = use_parsely_upload();

    rsx! {
        // Sidebar with encounter list
        aside { class: if *sidebar_collapsed.read() { "explorer-sidebar collapsed" } else { "explorer-sidebar" },
            div { class: "sidebar-header",
                div { class: "sidebar-header-row",
                    if !*sidebar_collapsed.read() {
                        h3 {
                            i { class: "fa-solid fa-list" }
                            " Encounters"
                        }
                    }
                    button {
                        class: "sidebar-collapse-btn",
                        title: if *sidebar_collapsed.read() { "Expand encounters (S)" } else { "Collapse encounters (S)" },
                        "data-shortcut": "s",
                        onclick: move |_| { let v = *sidebar_collapsed.read(); sidebar_collapsed.set(!v); },
                        i { class: if *sidebar_collapsed.read() { "fa-solid fa-angles-right" } else { "fa-solid fa-angles-left" } }
                    }
                }
                if !*sidebar_collapsed.read() {
                    div { class: "history-controls",
                        ToggleSwitch {
                            label: "Bosses Only",
                            title: "Hide trash pulls and other non-boss fights",
                            class: "boss-filter-toggle",
                            checked: *show_only_bosses.read(),
                            on_change: move |checked: bool| {
                                show_only_bosses.set(checked);
                                // Update parent state immediately (avoids bidirectional sync loop)
                                if let Ok(mut state) = state.try_write() {
                                    state.data_explorer.show_only_bosses = checked;
                                }
                                spawn(async move {
                                    if let Err(err) = api::patch_config(|cfg| {
                                        cfg.show_only_bosses = checked;
                                    }).await {
                                        toast.show(format!("Failed to save settings: {}", err), ToastSeverity::Normal);
                                    }
                                });
                            },
                        }
                        ToggleSwitch {
                            label: "Percentiles",
                            title: "Show Parsely percentile badges",
                            class: "boss-filter-toggle",
                            checked: !state.read().data_explorer.hide_percentiles,
                            on_change: move |checked: bool| {
                                if let Ok(mut state) = state.try_write() {
                                    state.data_explorer.hide_percentiles = !checked;
                                }
                                spawn(async move {
                                    if let Err(err) = api::patch_config(|cfg| {
                                        cfg.hide_percentiles = !checked;
                                    }).await {
                                        toast.show(format!("Failed to save settings: {}", err), ToastSeverity::Normal);
                                    }
                                });
                            },
                        }
                        span { class: "encounter-count", title: "Encounters shown / total in this log",
                            "{filtered_history().len()}"
                            if *show_only_bosses.read() { " / {encounters().len()}" }
                        }
                    }
                }
            }

            if !*sidebar_collapsed.read() {
                // Live button — pinned above scrollable list, always visible
                div { class: "sidebar-live-button",
                    div {
                        class: if *auto_follow.read() { "sidebar-encounter-item selected live-query-item" } else { "sidebar-encounter-item live-query-item" },
                        onclick: move |_| {
                            auto_follow.set(true);
                            spawn(async move {
                                if let Some(info) = api::get_session_info().await {
                                    if info.in_combat {
                                        // In combat: go to live buffer
                                        live_query_active.set(true);
                                        selected_encounter.set(None);
                                        live_poll_tick.set(0);
                                    } else {
                                        // Not in combat: select latest encounter
                                        live_query_active.set(false);
                                        if let Some(list) = api::get_encounter_history().await {
                                            let latest_id = list.last().map(|e| e.encounter_id as u32);
                                            encounters.set(list);
                                            if let Some(id) = latest_id {
                                                selected_encounter.set(Some(id));
                                            }
                                        }
                                    }
                                }
                            });
                        },
                        i { class: "fa-solid fa-play live-play-icon" }
                        span { class: "encounter-name",
                            if *live_query_active.read() { " Live Encounter" } else { " Live" }
                        }
                        // Auto-live toggle — nested in the live button row
                        ToggleSwitch {
                            label: "auto",
                            title: "Switch to each new fight as it starts",
                            class: "live-query-toggle",
                            stop_propagation: true,
                            checked: *auto_live.read(),
                            on_change: move |checked: bool| {
                                auto_live.set(checked);
                                if checked {
                                    auto_follow.set(true);
                                } else {
                                    auto_follow.set(false);
                                    live_query_active.set(false);
                                }
                                if let Ok(mut state) = state.try_write() {
                                    state.data_explorer.auto_live = checked;
                                }
                                spawn(async move {
                                    if let Err(err) = api::patch_config(|cfg| {
                                        cfg.data_explorer_auto_live = checked;
                                    }).await {
                                        toast.show(format!("Failed to save settings: {}", err), ToastSeverity::Normal);
                                    }
                                });
                            },
                        }
                    }
                }
                div { class: "sidebar-encounter-list",
                    if encounters().is_empty() {
                        div { class: "sidebar-empty",
                            i { class: "fa-solid fa-inbox" }
                            p { "No encounters" }
                            p { class: "hint", "Load a log file to see encounters" }
                        }
                    } else {
                        for (idx, (area_name, difficulty, area_encounters)) in sections().iter().enumerate() {
                            {
                                let section_key = format!("{}_{}", idx, area_name);
                                let is_collapsed = collapsed_sections().contains(&section_key);
                                let section_key_toggle = section_key.clone();
                                let chevron_class = if is_collapsed { "fa-chevron-right" } else { "fa-chevron-down" };
                                // The run holding this section's most recent recorded pull
                                let run_link = run_links.read().as_ref().and_then(|links| {
                                    area_encounters
                                        .iter()
                                        .rev()
                                        .find_map(|e| links.iter().find(|l| l.encounter_id == e.encounter_id))
                                        .cloned()
                                });

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
                                        span { class: "section-area", title: "{area_name}", "{area_name}" }
                                        if let Some(diff) = difficulty {
                                            span { class: "section-difficulty", title: "{diff}", " • {baras_types::history::short_difficulty(&diff)}" }
                                        }
                                        if let Some(secs) = crate::components::encounter_types::time_in_instance(area_encounters) {
                                            span { class: "section-difficulty", title: "Time in this instance",
                                                " • "
                                                i { class: "fa-regular fa-clock" }
                                                " {formatting::format_duration(secs)}"
                                            }
                                        }
                                        span { class: "section-count", " ({area_encounters.len()})" }
                                        if let Some(link) = run_link {
                                            button {
                                                class: "section-run-link",
                                                title: "Open this run's raid summary",
                                                onclick: move |e| {
                                                    e.stop_propagation();
                                                    let mut s = state.write();
                                                    s.history.selected_run = Some((link.filename.clone(), link.start_ms));
                                                    // Filters that would hide the run from the list
                                                    s.history.run_search.clear();
                                                    if link.is_flashpoint {
                                                        s.history.filter.flashpoints = true;
                                                    }
                                                    s.active_tab = crate::types::MainTab::RaidSummaries;
                                                },
                                                i { class: "fa-solid fa-dungeon" }
                                            }
                                        }
                                    }

                                    // Encounter items (hidden if collapsed)
                                    if !is_collapsed {
                                        for enc in area_encounters.iter() {
                                            {
                                                // Use actual encounter_id for parquet file lookup
                                                let enc_idx = enc.encounter_id as u32;
                                                let enc_id = enc.encounter_id;
                                                let is_selected = *selected_encounter.read() == Some(enc_idx);
                                                // Arena rounds carry a tri-state outcome; everything else uses success
                                                let (result_class, result_icon, result_title) = match enc.pvp_outcome.as_deref() {
                                                    Some("win") => ("success", "fa-solid fa-check", "Win"),
                                                    Some("loss") => ("wipe", "fa-solid fa-skull", "Loss"),
                                                    Some(_) => ("unknown", "fa-solid fa-question", "Outcome unknown"),
                                                    None if enc.success => ("success", "fa-solid fa-check", "Kill"),
                                                    None => ("wipe", "fa-solid fa-skull", "Wipe"),
                                                };
                                                let npc_list = enc.npc_names.iter()
                                                    .filter(|n| !n.trim().is_empty())
                                                    .cloned()
                                                    .collect::<Vec<_>>()
                                                    .join(", ");
                                                let persisted_link = enc.parsely_link.clone();
                                                let start_line = enc.event_start_line.unwrap_or(0);
                                                let end_line = enc.event_end_line.unwrap_or(0);
                                                let area_line = enc.area_entered_line;

                                                rsx! {
                                                    div {
                                                        class: if is_selected { "sidebar-encounter-item selected" } else { "sidebar-encounter-item" },
                                                        onclick: move |_| {
                                                            auto_follow.set(false);
                                                            live_query_active.set(false);
                                                            selected_encounter.set(Some(enc_idx));
                                                        },
                                                        div { class: "encounter-main",
                                                            span { class: "encounter-name", title: "{enc.display_name}", "{enc.display_name}" }
                                                        }
                                                        div { class: "encounter-meta",
                                                            span { class: "encounter-duration", title: "Fight duration", "{formatting::format_duration(enc.duration_seconds)}" }
                                                            if let Some(time) = &enc.start_time {
                                                                {
                                                                    // Format ISO timestamp "2026-03-07T21:30:31" → "21:30:31"
                                                                    let display_time = time.split('T').nth(1).unwrap_or(time);
                                                                    // Inside an instance: time since it was entered, clock time on hover
                                                                    let since_entry = enc
                                                                        .instance_entered
                                                                        .as_deref()
                                                                        .and_then(baras_types::history::iso_to_ms)
                                                                        .zip(baras_types::history::iso_to_ms(time))
                                                                        .map(|(entered, start)| (start - entered) / 1000)
                                                                        .filter(|s| *s >= 0);
                                                                    match since_entry {
                                                                        Some(secs) => rsx! {
                                                                            span { class: "encounter-time", title: "Started {display_time}, this long after entering the instance", "(+{formatting::format_duration(secs)})" }
                                                                        },
                                                                        None => rsx! { span { class: "encounter-time", "({display_time})" } },
                                                                    }
                                                                }
                                                            }
                                                            // Kill time percentile
                                                            if let Some(r) = ttk_ratings.read().get(&enc_id) {
                                                                span {
                                                                    class: "pct-badge {pct_tier(r.pct)}",
                                                                    title: "{baras_types::percentile_label(r.pct)}\n~{r.source.total} total kills\n{r.source.updated_note()}",
                                                                    "{r.pct}"
                                                                }
                                                            }
                                                            div { class: "encounter-main-right",
                                                                // Parsely upload / link button
                                                                {
                                                                    if let Some(url) = persisted_link.clone() {
                                                                        rsx! {
                                                                            button {
                                                                                class: "parsely-upload-btn success",
                                                                                title: "View on Parsely",
                                                                                onclick: move |e| {
                                                                                    e.stop_propagation();
                                                                                    let u = url.clone();
                                                                                    spawn(async move { api::open_url(&u).await; });
                                                                                },
                                                                                i { class: "fa-solid fa-external-link" }
                                                                            }
                                                                        }
                                                                    } else {
                                                                        rsx! {
                                                                            button {
                                                                                class: "parsely-upload-btn",
                                                                                title: "Upload to Parsely",
                                                                                onclick: {
                                                                                    let encounter_name = enc.display_name.clone();
                                                                                    move |e| {
                                                                                        e.stop_propagation();
                                                                                        let name = encounter_name.clone();
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
                                                                        }
                                                                    }
                                                                }
                                                                // Result indicator (success/wipe/unknown)
                                                                span { class: "result-indicator {result_class}", title: result_title,
                                                                    i { class: "{result_icon}" }
                                                                }
                                                            }
                                                        }
                                                        // NPC names (if available)
                                                        if !npc_list.is_empty() {
                                                            div { class: "encounter-npcs", title: "{npc_list}",
                                                                i { class: "fa-solid fa-skull-crossbones" }
                                                                " {npc_list}"
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
