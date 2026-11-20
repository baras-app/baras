//! Source/target list of the two-column views, with same-named NPC spawns grouped.

use dioxus::prelude::*;
use std::collections::HashMap;

use super::ctx::{ExplorerCtx, repaint_with};
use crate::api::EntityBreakdown;
use crate::components::class_icons::get_class_icon;
use crate::types::{DataTab, ViewMode};
use baras_types::formatting;

pub(super) fn entity_sidebar(
    ctx: ExplorerCtx,
    shown: ViewMode,
    european: bool,
    on_source_click: Callback<(String, Option<i64>)>,
) -> Element {
    let ExplorerCtx {
        selected_source,
        selected_source_id,
        mut show_players_only,
        mut entity_collapsed,
        mut expanded_entity_groups,
        time_range,
        timeline,
        entity_list,
        class_icon_lookup,
        overview_stats_lookup,
        entities,
        overview_data,
        ..
    } = ctx;
    repaint_with(entities);
    repaint_with(overview_data);
    let current_tab = if let ViewMode::Detailed(tab) = shown {
        Some(tab)
    } else {
        None
    };
    let format_number = |n: f64| formatting::format_compact_f64(n, european);
    let on_source_click = move |name: String, id: Option<i64>| on_source_click.call((name, id));

    rsx! {
        div { class: if *entity_collapsed.read() { "entity-section collapsed" } else { "entity-section" },
            div { class: "entity-header",
                if !*entity_collapsed.read() {
                    h4 {
                        if current_tab.is_some_and(|t| !t.is_outgoing()) { "Targets" } else { "Sources" }
                    }
                }
                button {
                    class: "sidebar-collapse-btn",
                    title: if *entity_collapsed.read() { "Expand entities" } else { "Collapse entities" },
                    onclick: move |_| { let v = *entity_collapsed.read(); entity_collapsed.set(!v); },
                    i { class: if *entity_collapsed.read() { "fa-solid fa-angles-right" } else { "fa-solid fa-angles-left" } }
                }
            }
            if !*entity_collapsed.read() {
                div { class: "filter-pills seg-fill entity-filter-tabs",
                    button {
                        class: if *show_players_only.read() { "seg active" } else { "seg" },
                        onclick: move |_| show_players_only.set(true),
                        "Player"
                    }
                    button {
                        class: if !*show_players_only.read() { "seg active" } else { "seg" },
                        onclick: move |_| show_players_only.set(false),
                        "NPC"
                    }
                }
                div { class: "entity-list",
                    // Uses memoized entity_list
                    {
                    let tr = *time_range.read();
                    let duration = if tr.start != 0.0 || tr.end != 0.0 {
                        tr.end - tr.start
                    } else {
                        timeline.read().as_ref().map(|t| t.duration_secs).unwrap_or(1.0)
                    } as f64;
                    let duration = if duration > 0.0 { duration } else { 1.0 };
                    let mode = shown;
                    let show_values = !matches!(mode, ViewMode::Charts | ViewMode::Uptime);
                    let overview_stats = overview_stats_lookup();
                    let list = entity_list();
                    let sel_name = selected_source.read().clone();
                    let sel_id = *selected_source_id.read();
                    // Same-named NPC spawns collapse into one group row (aggregate) with
                    // expandable members, so a wave of adds doesn't bury single enemies.
                    let mut items: Vec<(EntityBreakdown, Vec<EntityBreakdown>)> = Vec::new();
                    let mut group_idx: HashMap<&str, usize> = HashMap::new();
                    for e in list.iter() {
                        match (e.entity_type == "Npc").then(|| group_idx.get(e.source_name.as_str())).flatten() {
                            Some(&i) => {
                                let (head, members) = &mut items[i];
                                if members.is_empty() {
                                    members.push(head.clone());
                                }
                                head.total_value += e.total_value;
                                head.abilities_used = head.abilities_used.max(e.abilities_used);
                                members.push(e.clone());
                            }
                            None => {
                                group_idx.insert(e.source_name.as_str(), items.len());
                                items.push((e.clone(), Vec::new()));
                            }
                        }
                    }
                    items.sort_by(|a, b| {
                        b.0.total_value.partial_cmp(&a.0.total_value).unwrap_or(std::cmp::Ordering::Equal)
                    });
                    // Spawns within a group read chronologically (earliest first);
                    // ones never seen hitting sort last
                    for (_, members) in items.iter_mut() {
                        members.sort_by(|a, b| {
                            a.first_hit_secs
                                .unwrap_or(f32::INFINITY)
                                .partial_cmp(&b.first_hit_secs.unwrap_or(f32::INFINITY))
                                .unwrap_or(std::cmp::Ordering::Equal)
                        });
                    }
                    // For Rotation/Usage: overview DPS for non-healers, EHPS for healers
                    // For Detailed tabs: entity total_value / duration
                    let row_values = |entity: &EntityBreakdown, is_npc: bool| -> (f64, &'static str, &'static str) {
                        match mode {
                            ViewMode::Rotation | ViewMode::Usage => {
                                let stats = overview_stats.get(&entity.source_name);
                                let is_healer = stats.map(|s| s.2).unwrap_or(false);
                                if is_healer && !is_npc {
                                    (stats.map(|s| s.1).unwrap_or(0.0), "entity-value value-healing", "EHPS")
                                } else {
                                    (stats.map(|s| s.0).unwrap_or(0.0), "entity-value value-damage", "DPS")
                                }
                            }
                            ViewMode::Detailed(DataTab::Damage) => {
                                (entity.total_value / duration, "entity-value value-damage", "DPS")
                            }
                            ViewMode::Detailed(DataTab::Healing) => {
                                (entity.total_value / duration, "entity-value value-healing", "HPS")
                            }
                            ViewMode::Detailed(DataTab::HealingTaken) => {
                                (entity.total_value / duration, "entity-value value-healing", "HTPS")
                            }
                            ViewMode::Detailed(DataTab::DamageTaken) => {
                                (entity.total_value / duration, "entity-value value-dtps", "DTPS")
                            }
                            _ => (entity.total_value / duration, "entity-value", ""),
                        }
                    };
                    rsx! {
                    for (head, members) in items.iter() {
                        {
                            let name = head.source_name.clone();
                            let is_npc = head.entity_type == "Npc";
                            let is_group = !members.is_empty();
                            let name_selected = sel_name.as_ref() == Some(&name);
                            let member_selected = is_group && name_selected
                                && sel_id.is_some_and(|id| members.iter().any(|m| m.source_id == id));
                            let expanded = is_group
                                && (member_selected || expanded_entity_groups.read().contains(&name));
                            // Group heads select by name (aggregate), lone NPCs by instance, players by name
                            let click_id = (is_npc && !is_group).then_some(head.source_id);
                            let is_selected = name_selected
                                && if is_group { sel_id.is_none() } else { sel_id.is_none_or(|id| id == head.source_id) };
                            let class_icon = class_icon_lookup().get(&name).cloned();
                            let (per_sec, value_class, rate_label) = row_values(head, is_npc);
                            let row_class = match (is_selected, is_group, is_npc) {
                                (true, true, _) => "entity-row npc group selected",
                                (true, _, _) => "entity-row selected",
                                (false, true, _) => "entity-row npc group",
                                (false, false, true) => "entity-row npc",
                                _ => "entity-row",
                            };
                            rsx! {
                                div {
                                    class: "{row_class}",
                                    onclick: {
                                        let name = name.clone();
                                        move |_| on_source_click(name.clone(), click_id)
                                    },
                                    span { class: "entity-name",
                                        if is_group {
                                            span {
                                                class: if expanded { "entity-caret expanded" } else { "entity-caret" },
                                                title: if expanded { "Collapse spawns" } else { "Show spawns" },
                                                onclick: {
                                                    let name = name.clone();
                                                    move |e: MouseEvent| {
                                                        e.stop_propagation();
                                                        if member_selected {
                                                            // A pinned spawn keeps the group open; fall back to the
                                                            // aggregate selection so the group can collapse
                                                            on_source_click(name.clone(), None);
                                                            expanded_entity_groups.write().remove(&name);
                                                            return;
                                                        }
                                                        let mut set = expanded_entity_groups.write();
                                                        if !set.remove(&name) {
                                                            set.insert(name.clone());
                                                        }
                                                    }
                                                },
                                                i { class: "fa-solid fa-chevron-right" }
                                            }
                                        }
                                        if let Some(icon_name) = &class_icon {
                                            if let Some(icon_asset) = get_class_icon(icon_name) {
                                                img {
                                                    class: "entity-class-icon",
                                                    src: *icon_asset,
                                                    alt: ""
                                                }
                                            }
                                        }
                                        "{head.source_name}"
                                        if is_group {
                                            span { class: "entity-group-count", "×{members.len()}" }
                                        }
                                    }
                                    if show_values {
                                        span { class: "{value_class}", "{format_number(per_sec)} {rate_label}" }
                                        span { class: "entity-abilities", "{head.abilities_used} abilities" }
                                    }
                                }
                                if expanded {
                                    for m in members.iter() {
                                        {
                                            let mid = m.source_id;
                                            let m_selected = name_selected && sel_id == Some(mid);
                                            let (ps, vc, rl) = row_values(m, true);
                                            let tag = m.first_hit_secs
                                                .map(|t| format!("@{}:{:02}", (t as i32) / 60, (t as i32) % 60))
                                                .unwrap_or_else(|| "spawn".to_string());
                                            rsx! {
                                                div {
                                                    class: if m_selected { "entity-row npc spawn selected" } else { "entity-row npc spawn" },
                                                    onclick: {
                                                        let name = name.clone();
                                                        move |_| on_source_click(name.clone(), Some(mid))
                                                    },
                                                    span { class: "entity-name", "{tag}" }
                                                    if show_values {
                                                        span { class: "{vc}", "{format_number(ps)} {rl}" }
                                                        span { class: "entity-abilities", "{m.abilities_used} abilities" }
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
