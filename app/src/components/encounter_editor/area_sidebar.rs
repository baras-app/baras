//! Encounter editor sidebar: areas by category, and under the open area its
//! bosses. Bosses load per area on selection, so only the open area has any.

use dioxus::prelude::*;

use crate::types::AreaListItem;

/// What the sidebar shows of a boss: id, name, enabled, final boss
pub type BossRow = (String, String, bool, bool);

#[component]
pub fn AreaCategory(
    name: &'static str,
    areas: Vec<AreaListItem>,
    selected: Option<AreaListItem>,
    /// Bosses of the selected area (empty while loading)
    bosses: Vec<BossRow>,
    loading: bool,
    /// The open area's boss list is shown
    bosses_open: bool,
    selected_boss: Option<String>,
    on_select: EventHandler<AreaListItem>,
    on_select_boss: EventHandler<String>,
    on_new_boss: EventHandler<()>,
) -> Element {
    let mut collapsed = use_signal(|| false);
    let chevron = if collapsed() { "fa-chevron-right" } else { "fa-chevron-down" };

    rsx! {
        div {
            class: "sidebar-section-header",
            onclick: move |_| collapsed.set(!collapsed()),
            i { class: "fa-solid {chevron} collapse-icon" }
            span { class: "section-area", "{name}" }
            span { class: "section-count", "{areas.len()}" }
        }

        if !collapsed() {
            for area in areas {
                {
                    let is_active = selected.as_ref().is_some_and(|s| s.file_path == area.file_path);
                    let area_clone = area.clone();
                    rsx! {
                        div {
                            key: "{area.file_path}",
                            class: if is_active { "sidebar-encounter-item selected" } else { "sidebar-encounter-item" },
                            onclick: move |_| on_select.call(area_clone.clone()),
                            div { class: "encounter-main",
                                span { class: "encounter-name", "{area.name}" }
                                span { class: "encounter-count", title: "bosses", "{area.boss_count}" }
                            }
                        }
                        if is_active && bosses_open {
                            div { class: "sidebar-boss-list",
                                if loading {
                                    div { class: "sidebar-boss-item muted", "Loading…" }
                                }
                                for (id, boss_name, enabled, is_final) in bosses.iter().cloned() {
                                    {
                                        let is_sel = selected_boss.as_deref() == Some(id.as_str());
                                        let class = match (is_sel, enabled) {
                                            (true, _) => "sidebar-boss-item selected",
                                            (false, true) => "sidebar-boss-item",
                                            (false, false) => "sidebar-boss-item disabled",
                                        };
                                        rsx! {
                                            div {
                                                key: "{id}",
                                                class,
                                                title: if enabled { "{id}" } else { "{id} · disabled" },
                                                onclick: move |_| on_select_boss.call(id.clone()),
                                                span { class: "truncate", "{boss_name}" }
                                                if is_final {
                                                    i { class: "fa-solid fa-flag boss-final", title: "Final boss" }
                                                }
                                            }
                                        }
                                    }
                                }
                                if !loading {
                                    div { class: "sidebar-boss-item add", onclick: move |_| on_new_boss.call(()),
                                        i { class: "fa-solid fa-plus" }
                                        " New boss"
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
