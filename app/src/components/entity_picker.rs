//! Stand-in for the Data Explorer's entity sidebar while it is folded: the
//! selected entity (with its discipline icon) as a button dropping the list.

use dioxus::prelude::*;
use std::collections::HashMap;

use crate::api::EntityBreakdown;
use crate::components::class_icons::get_class_icon;

#[component]
pub fn EntityPicker(
    /// The sidebar's entity list (already filtered to players or NPCs)
    entities: Memo<Vec<EntityBreakdown>>,
    /// Name → discipline icon, from the overview
    icons: Memo<HashMap<String, String>>,
    selected: Option<String>,
    players_only: Signal<bool>,
    /// (name, instance id): same contract as the sidebar's row click
    on_pick: EventHandler<(String, Option<i64>)>,
) -> Element {
    let mut open = use_signal(|| false);
    let icons = icons.read();
    let icon_for = |name: &str| icons.get(name).and_then(|i| get_class_icon(i)).copied();

    // One row per name: same-named NPC spawns pick the aggregate, a lone NPC its instance
    let mut rows: Vec<(String, Option<i64>, f64, usize)> = Vec::new();
    for e in entities.read().iter() {
        match rows.iter_mut().find(|r| r.0 == e.source_name) {
            Some(r) => {
                r.1 = None;
                r.2 += e.total_value;
                r.3 += 1;
            }
            None => {
                let id = (e.entity_type == "Npc").then_some(e.source_id);
                rows.push((e.source_name.clone(), id, e.total_value, 1));
            }
        }
    }
    rows.sort_by(|a, b| b.2.total_cmp(&a.2));

    let label = selected.clone().unwrap_or_else(|| "All sources".to_string());
    let selected_icon = selected.as_deref().and_then(icon_for);

    rsx! {
        div { class: "nav-menu entity-picker",
            button {
                class: "entity-picker-btn",
                title: "Choose source",
                onclick: move |_| open.toggle(),
                if let Some(src) = selected_icon {
                    img { class: "entity-class-icon", src, alt: "" }
                } else {
                    i { class: "fa-solid fa-user" }
                }
                span { class: "entity-picker-name", "{label}" }
                i { class: "fa-solid fa-chevron-down entity-picker-caret" }
            }
            if open() {
                div { class: "nav-menu-backdrop", onclick: move |_| open.set(false) }
                div { class: "nav-menu-list entity-picker-list",
                    div { class: "filter-pills seg-fill entity-filter-tabs",
                        button {
                            class: if players_only() { "seg active" } else { "seg" },
                            onclick: move |_| players_only.set(true),
                            "Player"
                        }
                        button {
                            class: if !players_only() { "seg active" } else { "seg" },
                            onclick: move |_| players_only.set(false),
                            "NPC"
                        }
                    }
                    div { class: "entity-picker-items",
                        for (name, id, _, count) in rows {
                            {
                                let icon = icon_for(&name);
                                let active = selected.as_deref() == Some(name.as_str());
                                let pick = name.clone();
                                rsx! {
                                    button {
                                        key: "{name}",
                                        class: if active { "nav-menu-item active" } else { "nav-menu-item" },
                                        onclick: move |_| {
                                            on_pick.call((pick.clone(), id));
                                            open.set(false);
                                        },
                                        if let Some(src) = icon {
                                            img { class: "entity-class-icon", src, alt: "" }
                                        }
                                        "{name}"
                                        if count > 1 {
                                            span { class: "entity-group-count", "×{count}" }
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
