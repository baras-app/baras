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
                        span { class: "encounter-count",
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
                                                let (result_class, result_icon) = match enc.pvp_outcome.as_deref() {
                                                    Some("win") => ("success", "fa-solid fa-check"),
                                                    Some("loss") => ("wipe", "fa-solid fa-skull"),
                                                    Some(_) => ("unknown", "fa-solid fa-question"),
                                                    None if enc.success => ("success", "fa-solid fa-check"),
                                                    None => ("wipe", "fa-solid fa-skull"),
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
                                                            span { class: "encounter-name", "{enc.display_name}" }
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
                                                                span { class: "result-indicator {result_class}",
                                                                    i { class: "{result_icon}" }
                                                                }
                                                            }
                                                        }
                                                        // NPC names (if available)
                                                        if !npc_list.is_empty() {
                                                            div { class: "encounter-npcs",
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
