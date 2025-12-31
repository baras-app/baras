//! Counter editing tab
//!
//! CRUD for boss counter definitions.

use dioxus::prelude::*;

use crate::api;
use crate::types::{BossListItem, CounterListItem, CounterTrigger, EntityFilter};

use super::tabs::EncounterData;
use super::triggers::CounterTriggerEditor;

/// Generate a preview of the ID that will be created (mirrors backend logic)
fn preview_id(boss_id: &str, name: &str) -> String {
    let name_part: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .split('_')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_");
    format!("{}_{}", boss_id, name_part)
}

// ─────────────────────────────────────────────────────────────────────────────
// Counters Tab
// ─────────────────────────────────────────────────────────────────────────────

#[component]
pub fn CountersTab(
    boss: BossListItem,
    counters: Vec<CounterListItem>,
    encounter_data: EncounterData,
    on_change: EventHandler<Vec<CounterListItem>>,
    on_status: EventHandler<(String, bool)>,
) -> Element {
    let mut expanded_counter = use_signal(|| None::<String>);
    let mut show_new_counter = use_signal(|| false);

    rsx! {
        div { class: "counters-tab",
            // Header
            div { class: "flex items-center justify-between mb-sm",
                span { class: "text-sm text-secondary", "{counters.len()} counters" }
                button {
                    class: "btn btn-success btn-sm",
                    onclick: move |_| show_new_counter.set(true),
                    "+ New Counter"
                }
            }

            // New counter form
            if show_new_counter() {
                {
                    let counters_for_create = counters.clone();
                    rsx! {
                        NewCounterForm {
                            boss: boss.clone(),
                            encounter_data: encounter_data.clone(),
                            on_create: move |new_counter: CounterListItem| {
                                let counters_clone = counters_for_create.clone();
                                spawn(async move {
                                    if let Some(created) = api::create_counter(&new_counter).await {
                                        let mut current = counters_clone;
                                        current.push(created);
                                        on_change.call(current);
                                        on_status.call(("Created".to_string(), false));
                                    } else {
                                        on_status.call(("Failed to create".to_string(), true));
                                    }
                                });
                                show_new_counter.set(false);
                            },
                            on_cancel: move |_| show_new_counter.set(false),
                        }
                    }
                }
            }

            // Counter list
            if counters.is_empty() {
                div { class: "empty-state text-sm", "No counters defined" }
            } else {
                for counter in counters.clone() {
                    {
                        let counter_key = counter.id.clone();
                        let is_expanded = expanded_counter() == Some(counter_key.clone());
                        let counters_for_row = counters.clone();

                        rsx! {
                            CounterRow {
                                key: "{counter_key}",
                                counter: counter.clone(),
                                expanded: is_expanded,
                                encounter_data: encounter_data.clone(),
                                on_toggle: move |_| {
                                    expanded_counter.set(if is_expanded { None } else { Some(counter_key.clone()) });
                                },
                                on_change: on_change,
                                on_status: on_status,
                                on_collapse: move |_| expanded_counter.set(None),
                                all_counters: counters_for_row,
                            }
                        }
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Counter Row
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn CounterRow(
    counter: CounterListItem,
    expanded: bool,
    all_counters: Vec<CounterListItem>,
    encounter_data: EncounterData,
    on_toggle: EventHandler<()>,
    on_change: EventHandler<Vec<CounterListItem>>,
    on_status: EventHandler<(String, bool)>,
    on_collapse: EventHandler<()>,
) -> Element {
    let trigger_label = counter.increment_on.label();

    rsx! {
        div { class: "list-item",
            // Header row
            div {
                class: "list-item-header",
                onclick: move |_| on_toggle.call(()),
                span { class: "list-item-expand", if expanded { "▼" } else { "▶" } }
                span { class: "font-medium", "{counter.name}" }
                span { class: "tag", "{trigger_label}" }
                if counter.decrement_on.is_some() {
                    span { class: "tag tag-info", "↓ Decrement" }
                } else if counter.decrement {
                    span { class: "tag tag-warning", "Decrement" }
                }
            }

            // Expanded content
            if expanded {
                div { class: "list-item-body",
                    CounterEditForm {
                        counter: counter.clone(),
                        encounter_data: encounter_data,
                        on_save: move |updated: CounterListItem| {
                            on_status.call(("Saving...".to_string(), false));
                            spawn(async move {
                                if api::update_counter(&updated).await {
                                    on_status.call(("Saved".to_string(), false));
                                } else {
                                    on_status.call(("Failed to save".to_string(), true));
                                }
                            });
                        },
                        on_delete: {
                            let all_counters = all_counters.clone();
                            move |counter_to_delete: CounterListItem| {
                                let all_counters = all_counters.clone();
                                spawn(async move {
                                    if api::delete_counter(
                                        &counter_to_delete.id,
                                        &counter_to_delete.boss_id,
                                        &counter_to_delete.file_path
                                    ).await {
                                        let updated: Vec<_> = all_counters.iter()
                                            .filter(|c| c.id != counter_to_delete.id)
                                            .cloned()
                                            .collect();
                                        on_change.call(updated);
                                        on_collapse.call(());
                                        on_status.call(("Deleted".to_string(), false));
                                    } else {
                                        on_status.call(("Failed to delete".to_string(), true));
                                    }
                                });
                            }
                        },
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Counter Edit Form
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn CounterEditForm(
    counter: CounterListItem,
    encounter_data: EncounterData,
    on_save: EventHandler<CounterListItem>,
    on_delete: EventHandler<CounterListItem>,
) -> Element {
    // Clone values needed for closures and display
    let counter_id_display = counter.id.clone();
    let counter_for_delete = counter.clone();

    let mut draft = use_signal(|| counter.clone());
    let original = counter.clone();

    let has_changes = use_memo(move || draft() != original);

    let handle_save = move |_| {
        let updated = draft();
        on_save.call(updated);
    };

    let handle_delete = move |_| {
        on_delete.call(counter_for_delete.clone());
    };

    rsx! {
        div { class: "counter-edit-form",
            // ─── ID (read-only) ─────────────────────────────────────────────
            div { class: "form-row-hz",
                label { "Counter ID" }
                code { class: "tag-muted text-mono text-xs", "{counter_id_display}" }
            }

            // ─── Name ────────────────────────────────────────────────────────
            div { class: "form-row-hz",
                label { "Name" }
                input {
                    class: "input-inline",
                    style: "width: 200px;",
                    value: "{draft().name.clone()}",
                    oninput: move |e| {
                        let mut d = draft();
                        d.name = e.value();
                        draft.set(d);
                    }
                }
            }

            // ─── Display Text ────────────────────────────────────────────────
            div { class: "form-row-hz",
                label { "Display Text" }
                input {
                    class: "input-inline",
                    style: "width: 200px;",
                    placeholder: "(defaults to name)",
                    value: "{draft().display_text.clone().unwrap_or_default()}",
                    oninput: move |e| {
                        let mut d = draft();
                        d.display_text = if e.value().is_empty() { None } else { Some(e.value()) };
                        draft.set(d);
                    }
                }
            }

            // ─── Increment Trigger ───────────────────────────────────────────
            div { class: "form-row-hz", style: "align-items: flex-start;",
                label { style: "padding-top: 6px;", "Increment On" }
                CounterTriggerEditor {
                    trigger: draft().increment_on,
                    encounter_data: encounter_data.clone(),
                    on_change: move |t| {
                        let mut d = draft();
                        d.increment_on = t;
                        draft.set(d);
                    }
                }
            }

            // ─── Decrement Trigger (optional) ────────────────────────────────
            div { class: "form-row-hz", style: "align-items: flex-start;",
                label { style: "padding-top: 6px;", "Decrement On" }
                div { class: "flex-col gap-xs",
                    div { class: "flex items-center gap-xs",
                        input {
                            r#type: "checkbox",
                            checked: draft().decrement_on.is_some(),
                            onchange: move |_| {
                                let mut d = draft();
                                d.decrement_on = if d.decrement_on.is_some() {
                                    None
                                } else {
                                    Some(CounterTrigger::AbilityCast {
                                        abilities: vec![],
                                        source: EntityFilter::default(),
                                    })
                                };
                                draft.set(d);
                            }
                        }
                        span { class: "text-xs text-muted", "(enable separate decrement trigger)" }
                    }
                    if let Some(ref decrement_trigger) = draft().decrement_on {
                        CounterTriggerEditor {
                            trigger: decrement_trigger.clone(),
                            encounter_data: encounter_data.clone(),
                            on_change: move |t| {
                                let mut d = draft();
                                d.decrement_on = Some(t);
                                draft.set(d);
                            }
                        }
                    }
                }
            }

            // ─── Reset Trigger ───────────────────────────────────────────────
            div { class: "form-row-hz", style: "align-items: flex-start;",
                label { style: "padding-top: 6px;", "Reset On" }
                CounterTriggerEditor {
                    trigger: draft().reset_on,
                    encounter_data: encounter_data.clone(),
                    on_change: move |t| {
                        let mut d = draft();
                        d.reset_on = t;
                        draft.set(d);
                    }
                }
            }

            // ─── Options ─────────────────────────────────────────────────────
            div { class: "form-row-hz",
                label { "Initial Value" }
                input {
                    r#type: "number",
                    min: "0",
                    class: "input-inline",
                    style: "width: 70px;",
                    value: "{draft().initial_value}",
                    oninput: move |e| {
                        if let Ok(val) = e.value().parse::<u32>() {
                            let mut d = draft();
                            d.initial_value = val;
                            draft.set(d);
                        }
                    }
                }
            }

            div { class: "form-row-hz",
                label { "Set Value" }
                div { class: "flex items-center gap-xs",
                    input {
                        r#type: "checkbox",
                        checked: draft().set_value.is_some(),
                        onchange: move |_| {
                            let mut d = draft();
                            d.set_value = if d.set_value.is_some() { None } else { Some(1) };
                            draft.set(d);
                        }
                    }
                    if draft().set_value.is_some() {
                        input {
                            r#type: "number",
                            min: "0",
                            class: "input-inline",
                            style: "width: 70px;",
                            value: "{draft().set_value.unwrap_or(1)}",
                            oninput: move |e| {
                                if let Ok(val) = e.value().parse::<u32>() {
                                    let mut d = draft();
                                    d.set_value = Some(val);
                                    draft.set(d);
                                }
                            }
                        }
                    }
                }
                span { class: "text-xs text-muted", "(set to specific value instead of increment)" }
            }

            div { class: "form-row-hz",
                label { "Decrement" }
                div { class: "flex items-center gap-xs",
                    input {
                        r#type: "checkbox",
                        checked: draft().decrement,
                        onchange: move |_| {
                            let mut d = draft();
                            d.decrement = !d.decrement;
                            draft.set(d);
                        }
                    }
                    span { class: "text-xs text-muted", "(count down instead of up)" }
                }
            }

            // ─── Actions ─────────────────────────────────────────────────────
            div { class: "form-actions",
                button {
                    class: if has_changes() { "btn btn-success btn-sm" } else { "btn btn-sm" },
                    disabled: !has_changes(),
                    onclick: handle_save,
                    "Save"
                }
                button {
                    class: "btn btn-danger btn-sm",
                    onclick: handle_delete,
                    "Delete"
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// New Counter Form
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn NewCounterForm(
    boss: BossListItem,
    encounter_data: EncounterData,
    on_create: EventHandler<CounterListItem>,
    on_cancel: EventHandler<()>,
) -> Element {
    let mut name = use_signal(|| "New Counter".to_string());
    let mut increment_on = use_signal(|| CounterTrigger::AbilityCast {
        abilities: vec![],
        source: EntityFilter::default(),
    });
    let mut decrement_on = use_signal(|| None::<CounterTrigger>);
    let mut reset_on = use_signal(|| CounterTrigger::CombatEnd);

    // Preview the ID that will be generated
    let boss_id_for_preview = boss.id.clone();
    let generated_id = use_memo(move || preview_id(&boss_id_for_preview, &name()));

    let handle_create = move |_| {
        let new_counter = CounterListItem {
            id: String::new(), // Backend will generate
            name: name(),
            display_text: None,
            boss_id: boss.id.clone(),
            boss_name: boss.name.clone(),
            file_path: boss.file_path.clone(),
            increment_on: increment_on(),
            decrement_on: decrement_on(),
            reset_on: reset_on(),
            initial_value: 0,
            decrement: false,
            set_value: None,
        };
        on_create.call(new_counter);
    };

    rsx! {
        div { class: "new-item-form mb-md",
            div { class: "form-row-hz",
                label { "Name" }
                input {
                    class: "input-inline",
                    style: "width: 200px;",
                    value: "{name}",
                    oninput: move |e| name.set(e.value())
                }
            }

            div { class: "form-row-hz",
                label { "ID" }
                code { class: "tag-muted text-mono text-xs", "{generated_id}" }
                span { class: "text-xs text-muted ml-xs", "(auto-generated)" }
            }

            div { class: "form-row-hz", style: "align-items: flex-start;",
                label { style: "padding-top: 6px;", "Increment On" }
                CounterTriggerEditor {
                    trigger: increment_on(),
                    encounter_data: encounter_data.clone(),
                    on_change: move |t| increment_on.set(t),
                }
            }

            div { class: "form-row-hz", style: "align-items: flex-start;",
                label { style: "padding-top: 6px;", "Decrement On" }
                div { class: "flex-col gap-xs",
                    div { class: "flex items-center gap-xs",
                        input {
                            r#type: "checkbox",
                            checked: decrement_on().is_some(),
                            onchange: move |_| {
                                decrement_on.set(if decrement_on().is_some() {
                                    None
                                } else {
                                    Some(CounterTrigger::AbilityCast {
                                        abilities: vec![],
                                        source: EntityFilter::default(),
                                    })
                                });
                            }
                        }
                        span { class: "text-xs text-muted", "(enable separate decrement trigger)" }
                    }
                    if let Some(ref trigger) = decrement_on() {
                        CounterTriggerEditor {
                            trigger: trigger.clone(),
                            encounter_data: encounter_data.clone(),
                            on_change: move |t| decrement_on.set(Some(t)),
                        }
                    }
                }
            }

            div { class: "form-row-hz", style: "align-items: flex-start;",
                label { style: "padding-top: 6px;", "Reset On" }
                CounterTriggerEditor {
                    trigger: reset_on(),
                    encounter_data: encounter_data.clone(),
                    on_change: move |t| reset_on.set(t),
                }
            }

            div { class: "flex gap-xs mt-sm",
                button {
                    class: if name().is_empty() { "btn btn-sm" } else { "btn btn-success btn-sm" },
                    disabled: name().is_empty(),
                    onclick: handle_create,
                    "Create Counter"
                }
                button {
                    class: "btn btn-sm",
                    onclick: move |_| on_cancel.call(()),
                    "Cancel"
                }
            }
        }
    }
}
