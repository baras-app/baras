//! New area and boss creation forms

use dioxus::prelude::*;

use crate::components::editor_common::FormCard;
use crate::components::form_fields::{Field, TextField, TogglePill};
use crate::types::{AreaListItem, BossEditItem, NewAreaRequest};

// ─────────────────────────────────────────────────────────────────────────────
// New Area Form
// ─────────────────────────────────────────────────────────────────────────────

#[component]
pub fn NewAreaForm(
    on_create: EventHandler<NewAreaRequest>,
    on_cancel: EventHandler<()>,
) -> Element {
    let mut name = use_signal(String::new);
    let mut area_id = use_signal(String::new);
    let mut area_type = use_signal(|| "operation".to_string());

    let can_create = !name().is_empty() && !area_id().is_empty();

    rsx! {
        div { class: "modal-overlay", onclick: move |_| on_cancel.call(()),

            div {
                class: "modal-content",
                onclick: move |e| e.stop_propagation(),

                div { class: "modal-header",
                    h3 { "Create New Area" }
                }

                // Name
                div { class: "form-field",
                    label { class: "form-label", "Area Name" }
                    input {
                        r#type: "text",
                        class: "input w-full",
                        placeholder: "e.g., The Ravagers",
                        value: "{name}",
                        oninput: move |e| name.set(e.value()),
                    }
                }

                // Area ID
                div { class: "form-field",
                    label { class: "form-label", "Area ID (from game data)" }
                    input {
                        r#type: "text",
                        class: "input w-full",
                        placeholder: "e.g., 833571547775799",
                        value: "{area_id}",
                        oninput: move |e| area_id.set(e.value()),
                    }
                    span { class: "hint", "Find in combat log: AreaEntered with area ID" }
                }

                // Area Type
                div { class: "form-field",
                    label { class: "form-label", "Area Type" }
                    select {
                        class: "select w-full",
                        value: "{area_type}",
                        onchange: move |e| area_type.set(e.value()),
                        option { value: "operation", "Operation (Raid)" }
                        option { value: "flashpoint", "Flashpoint" }
                        option { value: "lair_boss", "Lair Boss" }
                        option { value: "other", "Other" }
                    }
                }

                // Actions
                div { class: "form-actions",
                    button {
                        class: "btn btn-ghost",
                        onclick: move |_| on_cancel.call(()),
                        "Cancel"
                    }

                    button {
                        class: if can_create { "btn btn-success" } else { "btn" },
                        disabled: !can_create,
                        onclick: move |_| {
                            if let Ok(id) = area_id().parse::<i64>() {
                                on_create
                                    .call(NewAreaRequest {
                                        name: name(),
                                        area_id: id,
                                        area_type: area_type(),
                                    });
                            }
                        },
                        "Create Area"
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// New Boss Form
// ─────────────────────────────────────────────────────────────────────────────

#[component]
pub fn NewBossForm(
    area: AreaListItem,
    on_create: EventHandler<BossEditItem>,
    on_cancel: EventHandler<()>,
) -> Element {
    let mut id = use_signal(String::new);
    let mut name = use_signal(String::new);
    let mut difficulties = use_signal(|| {
        vec![
            "story".to_string(),
            "veteran".to_string(),
            "master".to_string(),
        ]
    });

    let can_create = !id().is_empty() && !name().is_empty();

    rsx! {
        FormCard {
            icon: "fa-skull",
            title: "New Boss",
            action: rsx! {
                button {
                    class: "btn btn-ghost btn-xs",
                    title: "Cancel",
                    onclick: move |_| on_cancel.call(()),
                    i { class: "fa-solid fa-xmark" }
                }
            },

            TextField {
                label: "Boss ID",
                value: id(),
                placeholder: "e.g., apex_vanguard (snake_case)",
                on_input: move |v| id.set(v),
            }
            TextField { label: "Name", value: name(), placeholder: "e.g., Apex Vanguard", on_input: move |v| name.set(v) }
            Field { label: "Difficulties",
                for (diff, label) in [("story", "Story"), ("veteran", "Veteran"), ("master", "Master")] {
                    TogglePill {
                        label,
                        active: difficulties().iter().any(|d| d == diff),
                        on_click: move |_| {
                            let mut d = difficulties.write();
                            if d.iter().any(|x| x == diff) {
                                d.retain(|x| x != diff);
                            } else {
                                d.push(diff.to_string());
                            }
                        },
                    }
                }
            }

            // Actions
            div { class: "form-actions",
                button {
                    class: if can_create { "btn btn-success btn-sm" } else { "btn btn-sm" },
                    disabled: !can_create,
                    onclick: move |_| {
                        on_create
                            .call(BossEditItem {
                                id: id(),
                                name: name(),
                                area_name: area.name.clone(),
                                area_id: area.area_id,
                                file_path: area.file_path.clone(),
                                difficulties: difficulties(),
                            });
                    },
                    "Create Boss"
                }

                button {
                    class: "btn btn-ghost btn-sm",
                    onclick: move |_| on_cancel.call(()),
                    "Cancel"
                }
            }
        }
    }
}
