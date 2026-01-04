//! Counter editing tab
//!
//! CRUD for boss counter definitions.
//! Uses CounterDefinition DSL type directly.

use dioxus::prelude::*;

use crate::api;
use crate::types::{BossWithPath, CounterDefinition, EncounterItem, EntityFilter, Trigger};

use super::tabs::EncounterData;
use super::triggers::ComposableTriggerEditor;
use super::InlineNameCreator;

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

/// Create a default counter definition
fn default_counter(name: String) -> CounterDefinition {
    CounterDefinition {
        id: String::new(), // Backend will generate
        name,
        display_text: None,
        increment_on: Trigger::AbilityCast {
            abilities: vec![],
            source: EntityFilter::default(),
        },
        decrement_on: None,
        reset_on: Trigger::CombatEnd,
        initial_value: 0,
        decrement: false,
        set_value: None,
    }
}

#[component]
pub fn CountersTab(
    boss_with_path: BossWithPath,
    encounter_data: EncounterData,
    on_change: EventHandler<Vec<CounterDefinition>>,
    on_status: EventHandler<(String, bool)>,
) -> Element {
    let mut expanded_counter = use_signal(|| None::<String>);

    // Extract counters from BossWithPath
    let counters = boss_with_path.boss.counters.clone();

    rsx! {
        div { class: "counters-tab",
            // Header
            div { class: "flex items-center justify-between mb-sm",
                span { class: "text-sm text-secondary", "{counters.len()} counters" }
                {
                    let bwp = boss_with_path.clone();
                    let counters_for_create = counters.clone();
                    rsx! {
                        InlineNameCreator {
                            button_label: "+ New Counter",
                            placeholder: "Counter name...",
                            on_create: move |name: String| {
                                let counters_clone = counters_for_create.clone();
                                let boss_id = bwp.boss.id.clone();
                                let file_path = bwp.file_path.clone();
                                let counter = default_counter(name);
                                let item = EncounterItem::Counter(counter);
                                spawn(async move {
                                    match api::create_encounter_item(&boss_id, &file_path, &item).await {
                                        Ok(EncounterItem::Counter(created)) => {
                                            let created_id = created.id.clone();
                                            let mut current = counters_clone;
                                            current.push(created);
                                            on_change.call(current);
                                            expanded_counter.set(Some(created_id));
                                            on_status.call(("Created".to_string(), false));
                                        }
                                        Ok(_) => on_status.call(("Unexpected response type".to_string(), true)),
                                        Err(e) => on_status.call((e, true)),
                                    }
                                });
                            }
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
                                boss_with_path: boss_with_path.clone(),
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
    counter: CounterDefinition,
    boss_with_path: BossWithPath,
    expanded: bool,
    all_counters: Vec<CounterDefinition>,
    encounter_data: EncounterData,
    on_toggle: EventHandler<()>,
    on_change: EventHandler<Vec<CounterDefinition>>,
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
                {
                    let bwp_for_save = boss_with_path.clone();
                    let bwp_for_delete = boss_with_path.clone();
                    rsx! {
                        div { class: "list-item-body",
                            CounterEditForm {
                                counter: counter.clone(),
                                encounter_data: encounter_data,
                                on_save: move |updated: CounterDefinition| {
                                    on_status.call(("Saving...".to_string(), false));
                                    let boss_id = bwp_for_save.boss.id.clone();
                                    let file_path = bwp_for_save.file_path.clone();
                                    let item = EncounterItem::Counter(updated);
                                    spawn(async move {
                                        match api::update_encounter_item(&boss_id, &file_path, &item, None).await {
                                            Ok(_) => on_status.call(("Saved".to_string(), false)),
                                            Err(_) => on_status.call(("Failed to save".to_string(), true)),
                                        }
                                    });
                                },
                                on_delete: {
                                    let all_counters = all_counters.clone();
                                    move |counter_to_delete: CounterDefinition| {
                                        let all_counters = all_counters.clone();
                                        let boss_id = bwp_for_delete.boss.id.clone();
                                        let file_path = bwp_for_delete.file_path.clone();
                                        spawn(async move {
                                            match api::delete_encounter_item("counter", &counter_to_delete.id, &boss_id, &file_path).await {
                                                Ok(_) => {
                                                    let updated: Vec<_> = all_counters.iter()
                                                        .filter(|c| c.id != counter_to_delete.id)
                                                        .cloned()
                                                        .collect();
                                                    on_change.call(updated);
                                                    on_collapse.call(());
                                                    on_status.call(("Deleted".to_string(), false));
                                                }
                                                Err(err) => {
                                                    on_status.call((err, true));
                                                }
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
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Counter Edit Form
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn CounterEditForm(
    counter: CounterDefinition,
    encounter_data: EncounterData,
    on_save: EventHandler<CounterDefinition>,
    on_delete: EventHandler<CounterDefinition>,
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
                ComposableTriggerEditor {
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
                                    Some(Trigger::AbilityCast {
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
                        ComposableTriggerEditor {
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
                ComposableTriggerEditor {
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

