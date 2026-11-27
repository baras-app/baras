//! One row of the timers list (name, id, trigger, overlays, toggles)
//! and its columns

use dioxus::prelude::*;

use crate::api;
use crate::components::ability_icon::ColorIcon;
use crate::components::def_list::{Column, NoteMark};
use crate::components::editor_common::{Origin, OriginBadge};
use crate::types::{BossTimerDefinition, BossWithPath, EncounterItem};

use super::tabs::EncounterData;
use super::timer_form::TimerEditForm;

/// Column names, in the order of the rows' grid
pub(super) const COLUMNS: [Column; 7] = [
    Column::BLANK,
    Column::new("Name", "", "Sort by name"),
    Column::new("ID", "col-id col-drop-2", "The timer's unique ID: names can repeat"),
    Column::new("Trigger", "col-dim col-drop-1", "The game event that starts the timer"),
    Column::new("Overlays", "", "Overlays the timer is shown on"),
    Column::BLANK,
    Column::BLANK,
];

/// What `t` sorts by in column `col` of [`COLUMNS`]
pub(super) fn sort_text(col: usize, t: &BossTimerDefinition) -> String {
    match col {
        2 => t.id.clone(),
        3 => t.trigger.label().to_string(),
        4 if t.is_alert => "Alert".to_string(),
        4 => t
            .display_targets
            .iter()
            .map(|d| d.label())
            .collect::<Vec<_>>()
            .join(","),
        _ => t.name.clone(),
    }
}

/// (letter, role as saved, who it is shown to, class when on)
const ROLES: [(&str, &str, &str, &str); 3] = [
    ("T", "Tank", "Tanks", "role-toggle-tank"),
    ("H", "Healer", "Healers", "role-toggle-healer"),
    ("D", "Dps", "DPS", "role-toggle-dps"),
];

#[component]
pub(super) fn TimerRow(
    timer: BossTimerDefinition,
    all_timers: Vec<BossTimerDefinition>,
    is_builtin: bool,
    is_modified: bool,
    boss_with_path: BossWithPath,
    encounter_data: EncounterData,
    expanded: bool,
    on_toggle: EventHandler<()>,
    on_change: EventHandler<Vec<BossTimerDefinition>>,
    on_refetch: EventHandler<()>,
    on_status: EventHandler<(String, bool)>,
    on_collapse: EventHandler<()>,
) -> Element {
    let mut is_dirty = use_signal(|| false);
    let color_hex = format!("#{:02x}{:02x}{:02x}", timer.color[0], timer.color[1], timer.color[2]);
    let shown = !timer.roles.is_empty();

    // The row's toggles save straight away, without opening the timer
    let (all, bwp) = (all_timers.clone(), boss_with_path.clone());
    let save = use_callback(move |updated: BossTimerDefinition| {
        let mut current = all.clone();
        if let Some(slot) = current.iter_mut().find(|t| t.id == updated.id) {
            *slot = updated.clone();
            on_change.call(current);
        }
        let (boss_id, file_path) = (bwp.boss.id.clone(), bwp.file_path.clone());
        spawn(async move {
            let item = EncounterItem::Timer(updated);
            if let Err(e) = api::update_encounter_item(&boss_id, &file_path, &item, None).await {
                on_status.call((format!("Failed to save: {e}"), true));
            }
            on_refetch.call(());
        });
    });
    let (for_shown, for_audio) = (timer.clone(), timer.clone());

    rsx! {
        div { class: "list-item", class: if expanded { "expanded" }, class: if !timer.enabled { "item-disabled" },
            div {
                class: "list-item-header def-row timer-cols",
                onclick: move |_| on_toggle.call(()),

                ColorIcon { icon_id: timer.icon_ability_id, color: color_hex }

                span { class: "col-name",
                    span { class: "def-name truncate", "{timer.name}" }
                    NoteMark { note: timer.note.clone() }
                    if expanded && is_dirty() {
                        span { class: "unsaved-indicator", title: "Unsaved changes" }
                    }
                }

                span { class: "col-id col-drop-2 truncate", title: "{timer.id}", "{timer.id}" }

                span { class: "col-dim col-drop-1 truncate", "{timer.trigger.label()}" }

                span { class: "col-tags",
                    if timer.is_alert {
                        span { class: "tag tag-alert", "Alert" }
                    } else if timer.display_targets.is_empty() {
                        span { class: "text-muted", title: "Not shown on any overlay", "—" }
                    } else {
                        for target in timer.display_targets.iter() {
                            span { class: "tag", "{target.label()}" }
                        }
                    }
                }

                OriginBadge { origin: Origin::of(is_builtin, is_modified), kind: "timer" }

                div { class: "def-row-toggles",
                    span {
                        class: "row-toggle",
                        title: if shown { "Hide for all roles" } else { "Show for all roles" },
                        onclick: move |e| {
                            e.stop_propagation();
                            let mut updated = for_shown.clone();
                            updated.roles = if shown { vec![] } else { ROLES.iter().map(|r| r.1.to_string()).collect() };
                            save(updated);
                        },
                        i { class: if shown { "fa-solid fa-eye text-success" } else { "fa-solid fa-eye-slash text-muted" } }
                    }
                    span {
                        class: "row-toggle",
                        class: if !timer.audio.enabled { "row-toggle-quiet" },
                        title: if timer.audio.enabled { "Disable audio" } else { "Enable audio" },
                        onclick: move |e| {
                            e.stop_propagation();
                            let mut updated = for_audio.clone();
                            updated.audio.enabled = !updated.audio.enabled;
                            save(updated);
                        },
                        i { class: if timer.audio.enabled { "fa-solid fa-volume-high text-primary" } else { "fa-solid fa-volume-xmark text-muted" } }
                    }
                    div { class: "flex items-center role-toggle-group",
                        for (letter, role, who, on_class) in ROLES {
                            {
                                let active = timer.roles.iter().any(|r| r == role);
                                let for_role = timer.clone();
                                rsx! {
                                    span {
                                        class: "role-toggle",
                                        class: if active { "{on_class} active" },
                                        title: if active { "Hide for {who}" } else { "Show for {who}" },
                                        onclick: move |e| {
                                            e.stop_propagation();
                                            let mut updated = for_role.clone();
                                            if active {
                                                updated.roles.retain(|r| r != role);
                                            } else {
                                                updated.roles.push(role.to_string());
                                            }
                                            save(updated);
                                        },
                                        "{letter}"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if expanded {
                TimerEditForm {
                    timer: timer.clone(),
                    all_timers,
                    is_builtin,
                    is_modified,
                    boss_with_path,
                    encounter_data,
                    on_change,
                    on_refetch,
                    on_status,
                    on_collapse,
                    on_dirty: move |dirty: bool| is_dirty.set(dirty),
                }
            }
        }
    }
}
