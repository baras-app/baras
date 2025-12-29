//! Counter editing tab
//!
//! CRUD for boss counter definitions.

use dioxus::prelude::*;

use crate::api;
use crate::types::{BossListItem, CounterListItem, CounterTrigger};

use super::triggers::CounterTriggerEditor;

// ─────────────────────────────────────────────────────────────────────────────
// Counters Tab
// ─────────────────────────────────────────────────────────────────────────────

#[component]
pub fn CountersTab(
    boss: BossListItem,
    on_status: EventHandler<(String, bool)>,
) -> Element {
    let mut counters = use_signal(Vec::<CounterListItem>::new);
    let mut loading = use_signal(|| true);
    let mut expanded_counter = use_signal(|| None::<String>);
    let mut show_new_counter = use_signal(|| false);

    let file_path = boss.file_path.clone();
    let boss_id = boss.id.clone();

    // Load counters on mount
    use_effect(move || {
        let file_path = file_path.clone();
        let boss_id = boss_id.clone();
        spawn(async move {
            if let Some(c) = api::get_counters_for_area(&file_path).await {
                let boss_counters: Vec<_> = c.into_iter().filter(|c| c.boss_id == boss_id).collect();
                counters.set(boss_counters);
            }
            loading.set(false);
        });
    });

    rsx! {
        div { class: "counters-tab",
            // Header
            div { class: "flex items-center justify-between mb-sm",
                span { class: "text-sm text-secondary",
                    if loading() { "Loading..." } else { "{counters().len()} counters" }
                }
                button {
                    class: "btn btn-success btn-sm",
                    onclick: move |_| show_new_counter.set(true),
                    "+ New Counter"
                }
            }

            // New counter form
            if show_new_counter() {
                NewCounterForm {
                    boss: boss.clone(),
                    on_create: move |new_counter: CounterListItem| {
                        spawn(async move {
                            if let Some(created) = api::create_counter(&new_counter).await {
                                let mut current = counters();
                                current.push(created);
                                counters.set(current);
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

            // Counter list
            if loading() {
                div { class: "empty-state text-sm", "Loading counters..." }
            } else if counters().is_empty() {
                div { class: "empty-state text-sm", "No counters defined" }
            } else {
                for counter in counters() {
                    {
                        let counter_key = counter.id.clone();
                        let is_expanded = expanded_counter() == Some(counter_key.clone());
                        let counters_for_row = counters();

                        rsx! {
                            CounterRow {
                                key: "{counter_key}",
                                counter: counter.clone(),
                                expanded: is_expanded,
                                on_toggle: move |_| {
                                    expanded_counter.set(if is_expanded { None } else { Some(counter_key.clone()) });
                                },
                                on_change: move |updated: Vec<CounterListItem>| {
                                    counters.set(updated);
                                },
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
                span { class: "font-medium text-mono", "{counter.id}" }
                span { class: "tag", "{trigger_label}" }
                if counter.decrement {
                    span { class: "tag tag-warning", "Decrement" }
                }
            }

            // Expanded content
            if expanded {
                div { class: "list-item-body",
                    CounterEditForm {
                        counter: counter.clone(),
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
    on_save: EventHandler<CounterListItem>,
    on_delete: EventHandler<CounterListItem>,
) -> Element {
    let mut draft = use_signal(|| counter.clone());
    let original = counter.clone();

    let has_changes = use_memo(move || draft() != original);

    let handle_save = move |_| {
        let updated = draft();
        on_save.call(updated);
    };

    let handle_delete = move |_| {
        on_delete.call(counter.clone());
    };

    rsx! {
        div { class: "counter-edit-form",
            // ─── ID ──────────────────────────────────────────────────────────
            div { class: "form-row-hz",
                label { "Counter ID" }
                input {
                    class: "input-inline text-mono",
                    style: "width: 200px;",
                    value: "{draft().id}",
                    oninput: move |e| {
                        let mut d = draft();
                        d.id = e.value();
                        draft.set(d);
                    }
                }
            }

            // ─── Increment Trigger ───────────────────────────────────────────
            div { class: "form-section",
                div { class: "font-bold text-sm mb-xs", "Increment On" }
                CounterTriggerEditor {
                    trigger: draft().increment_on,
                    on_change: move |t| {
                        let mut d = draft();
                        d.increment_on = t;
                        draft.set(d);
                    }
                }
            }

            // ─── Reset Trigger ───────────────────────────────────────────────
            div { class: "form-section",
                div { class: "font-bold text-sm mb-xs", "Reset On" }
                CounterTriggerEditor {
                    trigger: draft().reset_on,
                    on_change: move |t| {
                        let mut d = draft();
                        d.reset_on = t;
                        draft.set(d);
                    }
                }
            }

            // ─── Options ─────────────────────────────────────────────────────
            div { class: "form-section",
                div { class: "font-bold text-sm mb-xs", "Options" }

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

                div { class: "flex items-center gap-sm",
                    label {
                        class: "flex items-center gap-xs cursor-pointer",
                        input {
                            r#type: "checkbox",
                            checked: draft().decrement,
                            onchange: move |_| {
                                let mut d = draft();
                                d.decrement = !d.decrement;
                                draft.set(d);
                            }
                        }
                        "Decrement"
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
    on_create: EventHandler<CounterListItem>,
    on_cancel: EventHandler<()>,
) -> Element {
    let mut id = use_signal(|| "new_counter".to_string());
    let mut increment_on = use_signal(|| CounterTrigger::AbilityCast {
        abilities: vec![],
        source: None,
    });
    let mut reset_on = use_signal(|| CounterTrigger::CombatEnd);

    let handle_create = move |_| {
        let new_counter = CounterListItem {
            id: id(),
            boss_id: boss.id.clone(),
            boss_name: boss.name.clone(),
            file_path: boss.file_path.clone(),
            increment_on: increment_on(),
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
                label { "Counter ID" }
                input {
                    class: "input-inline text-mono",
                    style: "width: 200px;",
                    value: "{id}",
                    oninput: move |e| id.set(e.value())
                }
            }

            div { class: "form-section",
                div { class: "font-bold text-sm mb-xs", "Increment On" }
                CounterTriggerEditor {
                    trigger: increment_on(),
                    on_change: move |t| increment_on.set(t),
                }
            }

            div { class: "form-section",
                div { class: "font-bold text-sm mb-xs", "Reset On" }
                CounterTriggerEditor {
                    trigger: reset_on(),
                    on_change: move |t| reset_on.set(t),
                }
            }

            div { class: "flex gap-xs mt-sm",
                button {
                    class: "btn btn-success btn-sm",
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
