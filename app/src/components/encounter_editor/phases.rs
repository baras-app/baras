//! Phase editing tab
//!
//! CRUD for boss phase definitions.

use dioxus::prelude::*;

use crate::api;
use crate::types::{BossListItem, PhaseListItem, PhaseTrigger};

use super::conditions::CounterConditionEditor;
use super::tabs::EncounterData;
use super::triggers::PhaseTriggerEditor;

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
// Phases Tab
// ─────────────────────────────────────────────────────────────────────────────

#[component]
pub fn PhasesTab(
    boss: BossListItem,
    phases: Vec<PhaseListItem>,
    encounter_data: EncounterData,
    on_change: EventHandler<Vec<PhaseListItem>>,
    on_status: EventHandler<(String, bool)>,
) -> Element {
    let mut expanded_phase = use_signal(|| None::<String>);
    let mut show_new_phase = use_signal(|| false);

    // Get phase IDs for preceded_by dropdown
    let phase_ids: Vec<String> = phases.iter().map(|p| p.id.clone()).collect();

    rsx! {
        div { class: "phases-tab",
            // Header
            div { class: "flex items-center justify-between mb-sm",
                span { class: "text-sm text-secondary", "{phases.len()} phases" }
                button {
                    class: "btn btn-success btn-sm",
                    onclick: move |_| show_new_phase.set(true),
                    "+ New Phase"
                }
            }

            // New phase form
            if show_new_phase() {
                {
                    let phases_for_create = phases.clone();
                    rsx! {
                        NewPhaseForm {
                            boss: boss.clone(),
                            phase_ids: phase_ids.clone(),
                            encounter_data: encounter_data.clone(),
                            on_create: move |new_phase: PhaseListItem| {
                                let phases_clone = phases_for_create.clone();
                                spawn(async move {
                                    if let Some(created) = api::create_phase(&new_phase).await {
                                        let mut current = phases_clone;
                                        current.push(created);
                                        on_change.call(current);
                                        on_status.call(("Created".to_string(), false));
                                    } else {
                                        on_status.call(("Failed to create".to_string(), true));
                                    }
                                });
                                show_new_phase.set(false);
                            },
                            on_cancel: move |_| show_new_phase.set(false),
                        }
                    }
                }
            }

            // Phase list
            if phases.is_empty() {
                div { class: "empty-state text-sm", "No phases defined" }
            } else {
                for phase in phases.clone() {
                    {
                        let phase_key = phase.id.clone();
                        let is_expanded = expanded_phase() == Some(phase_key.clone());
                        let phases_for_row = phases.clone();

                        rsx! {
                            PhaseRow {
                                key: "{phase_key}",
                                phase: phase.clone(),
                                all_phases: phases_for_row,
                                expanded: is_expanded,
                                encounter_data: encounter_data.clone(),
                                on_toggle: move |_| {
                                    expanded_phase.set(if is_expanded { None } else { Some(phase_key.clone()) });
                                },
                                on_change: on_change,
                                on_status: on_status,
                                on_collapse: move |_| expanded_phase.set(None),
                            }
                        }
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Phase Row
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn PhaseRow(
    phase: PhaseListItem,
    all_phases: Vec<PhaseListItem>,
    expanded: bool,
    encounter_data: EncounterData,
    on_toggle: EventHandler<()>,
    on_change: EventHandler<Vec<PhaseListItem>>,
    on_status: EventHandler<(String, bool)>,
    on_collapse: EventHandler<()>,
) -> Element {
    let trigger_label = phase.start_trigger.label();

    rsx! {
        div { class: "list-item",
            // Header row
            div {
                class: "list-item-header",
                onclick: move |_| on_toggle.call(()),
                span { class: "list-item-expand", if expanded { "▼" } else { "▶" } }
                span { class: "font-medium", "{phase.name}" }
                span { class: "text-xs text-mono text-muted", "{phase.id}" }
                span { class: "tag", "{trigger_label}" }
            }

            // Expanded content
            if expanded {
                {
                    let all_phases_for_delete = all_phases.clone();
                    rsx! {
                        div { class: "list-item-body",
                            PhaseEditForm {
                                phase: phase.clone(),
                                all_phases: all_phases,
                                encounter_data: encounter_data,
                                on_save: move |updated: PhaseListItem| {
                                    on_status.call(("Saving...".to_string(), false));
                                    spawn(async move {
                                        if api::update_phase(&updated).await {
                                            on_status.call(("Saved".to_string(), false));
                                        } else {
                                            on_status.call(("Failed to save".to_string(), true));
                                        }
                                    });
                                },
                                on_delete: {
                                    let all_phases = all_phases_for_delete.clone();
                                    move |phase_to_delete: PhaseListItem| {
                                        let all_phases = all_phases.clone();
                                        spawn(async move {
                                            if api::delete_phase(&phase_to_delete.id, &phase_to_delete.boss_id, &phase_to_delete.file_path).await {
                                                let updated: Vec<_> = all_phases.iter()
                                                    .filter(|p| p.id != phase_to_delete.id)
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
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Phase Edit Form
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn PhaseEditForm(
    phase: PhaseListItem,
    all_phases: Vec<PhaseListItem>,
    encounter_data: EncounterData,
    on_save: EventHandler<PhaseListItem>,
    on_delete: EventHandler<PhaseListItem>,
) -> Element {
    // Clone values needed for closures and display
    let phase_id_display = phase.id.clone();
    let phase_for_delete = phase.clone();

    let mut draft = use_signal(|| phase.clone());
    let original = phase.clone();

    let has_changes = use_memo(move || draft() != original);

    // Get phase IDs for preceded_by dropdown (exclude self)
    let phase_ids: Vec<String> = all_phases
        .iter()
        .filter(|p| p.id != phase.id)
        .map(|p| p.id.clone())
        .collect();

    let handle_save = move |_| {
        let updated = draft();
        on_save.call(updated);
    };

    let handle_delete = move |_| {
        on_delete.call(phase_for_delete.clone());
    };

    rsx! {
        div { class: "phase-edit-form",
            // ─── Identity ────────────────────────────────────────────────────
            div { class: "form-row-hz",
                label { "Phase ID" }
                code { class: "tag-muted text-mono text-xs", "{phase_id_display}" }
            }

            div { class: "form-row-hz",
                label { "Name" }
                input {
                    class: "input-inline",
                    style: "width: 200px;",
                    value: "{draft().name}",
                    oninput: move |e| {
                        let mut d = draft();
                        d.name = e.value();
                        draft.set(d);
                    }
                }
            }

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

            // ─── Start Trigger ───────────────────────────────────────────────
            div { class: "form-row-hz", style: "align-items: flex-start;",
                label { style: "padding-top: 6px;", "Trigger" }
                PhaseTriggerEditor {
                    trigger: draft().start_trigger,
                    encounter_data: encounter_data.clone(),
                    on_change: move |t| {
                        let mut d = draft();
                        d.start_trigger = t;
                        draft.set(d);
                    }
                }
            }

            // ─── End Trigger (Optional) ──────────────────────────────────────
            div { class: "form-row-hz", style: "align-items: flex-start;",
                label { style: "padding-top: 6px;", "End On" }
                if let Some(end) = draft().end_trigger.clone() {
                    div { class: "flex-col gap-xs",
                        PhaseTriggerEditor {
                            trigger: end,
                            encounter_data: encounter_data.clone(),
                            on_change: move |t| {
                                let mut d = draft();
                                d.end_trigger = Some(t);
                                draft.set(d);
                            }
                        }
                        button {
                            class: "btn btn-sm",
                            style: "width: fit-content;",
                            onclick: move |_| {
                                let mut d = draft();
                                d.end_trigger = None;
                                draft.set(d);
                            },
                            "Remove End Trigger"
                        }
                    }
                } else {
                    div { class: "flex-col gap-xs",
                        span { class: "text-muted text-sm", "(ends when another phase starts)" }
                        button {
                            class: "btn btn-sm",
                            onclick: move |_| {
                                let mut d = draft();
                                d.end_trigger = Some(PhaseTrigger::CombatStart);
                                draft.set(d);
                            },
                            "+ Add End Trigger"
                        }
                    }
                }
            }

            // ─── Guards ──────────────────────────────────────────────────────
            div { class: "form-row-hz",
                label { "Preceded By" }
                select {
                    class: "select",
                    style: "width: 180px;",
                    value: "{draft().preceded_by.clone().unwrap_or_default()}",
                    onchange: move |e| {
                        let mut d = draft();
                        d.preceded_by = if e.value().is_empty() { None } else { Some(e.value()) };
                        draft.set(d);
                    },
                    option { value: "", "(none)" }
                    for phase_id in &phase_ids {
                        option { value: "{phase_id}", "{phase_id}" }
                    }
                }
            }

            div { class: "form-row-hz",
                label { "Counter" }
                CounterConditionEditor {
                    condition: draft().counter_condition.clone(),
                    counters: encounter_data.counter_ids(),
                    on_change: move |cond| {
                        let mut d = draft();
                        d.counter_condition = cond;
                        draft.set(d);
                    }
                }
            }

            // ─── Resets Counters ─────────────────────────────────────────────
            div { class: "form-row-hz", style: "align-items: flex-start;",
                label { style: "padding-top: 6px;", "Resets" }
                CounterListEditor {
                    counters: draft().resets_counters.clone(),
                    available_counters: encounter_data.counter_ids(),
                    on_change: move |counters| {
                        let mut d = draft();
                        d.resets_counters = counters;
                        draft.set(d);
                    }
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
// New Phase Form
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn NewPhaseForm(
    boss: BossListItem,
    phase_ids: Vec<String>,
    encounter_data: EncounterData,
    on_create: EventHandler<PhaseListItem>,
    on_cancel: EventHandler<()>,
) -> Element {
    let mut name = use_signal(|| "New Phase".to_string());
    let mut start_trigger = use_signal(|| PhaseTrigger::CombatStart);
    let mut preceded_by = use_signal(|| None::<String>);

    // Preview the ID that will be generated
    let boss_id_for_preview = boss.id.clone();
    let generated_id = use_memo(move || preview_id(&boss_id_for_preview, &name()));

    let handle_create = move |_| {
        let new_phase = PhaseListItem {
            id: String::new(), // Generated by backend
            name: name(),
            display_text: None,
            boss_id: boss.id.clone(),
            boss_name: boss.name.clone(),
            file_path: boss.file_path.clone(),
            start_trigger: start_trigger(),
            end_trigger: None,
            preceded_by: preceded_by(),
            counter_condition: None,
            resets_counters: vec![],
        };
        on_create.call(new_phase);
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

            div { class: "form-row-hz",
                label { "Preceded By" }
                select {
                    class: "select",
                    style: "width: 180px;",
                    value: "{preceded_by().unwrap_or_default()}",
                    onchange: move |e| {
                        preceded_by.set(if e.value().is_empty() { None } else { Some(e.value()) });
                    },
                    option { value: "", "(none)" }
                    for phase_id in &phase_ids {
                        option { value: "{phase_id}", "{phase_id}" }
                    }
                }
            }

            div { class: "form-section",
                div { class: "font-bold text-sm mb-xs", "Start Trigger" }
                PhaseTriggerEditor {
                    trigger: start_trigger(),
                    encounter_data: encounter_data.clone(),
                    on_change: move |t| start_trigger.set(t),
                }
            }

            div { class: "flex gap-xs mt-sm",
                button {
                    class: if name().is_empty() { "btn btn-sm" } else { "btn btn-success btn-sm" },
                    disabled: name().is_empty(),
                    onclick: handle_create,
                    "Create Phase"
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

// ─────────────────────────────────────────────────────────────────────────────
// Counter List Editor (for resets_counters)
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn CounterListEditor(
    counters: Vec<String>,
    available_counters: Vec<String>,
    on_change: EventHandler<Vec<String>>,
) -> Element {
    let mut selected_counter = use_signal(String::new);

    // Filter available counters to exclude already selected ones
    let remaining: Vec<_> = available_counters
        .iter()
        .filter(|c| !counters.contains(c))
        .cloned()
        .collect();

    let counters_for_add = counters.clone();

    rsx! {
        div { class: "flex-col gap-xs",
            // Counter chips
            div { class: "flex flex-wrap gap-xs",
                for (idx, counter) in counters.iter().enumerate() {
                    {
                        let counters_clone = counters.clone();
                        rsx! {
                            span { class: "chip",
                                "{counter}"
                                button {
                                    class: "chip-remove",
                                    onclick: move |_| {
                                        let mut new_counters = counters_clone.clone();
                                        new_counters.remove(idx);
                                        on_change.call(new_counters);
                                    },
                                    "×"
                                }
                            }
                        }
                    }
                }
            }

            // Add from dropdown
            if !remaining.is_empty() {
                div { class: "flex gap-xs",
                    select {
                        class: "select",
                        style: "width: 150px;",
                        value: "{selected_counter}",
                        onchange: move |e| selected_counter.set(e.value()),
                        option { value: "", "Select counter..." }
                        for counter_id in &remaining {
                            option { value: "{counter_id}", "{counter_id}" }
                        }
                    }
                    button {
                        class: "btn btn-sm",
                        disabled: selected_counter().is_empty(),
                        onclick: move |_| {
                            let val = selected_counter();
                            if !val.is_empty() && !counters_for_add.contains(&val) {
                                let mut new_counters = counters_for_add.clone();
                                new_counters.push(val);
                                on_change.call(new_counters);
                                selected_counter.set(String::new());
                            }
                        },
                        "Add"
                    }
                }
            } else if available_counters.is_empty() {
                span { class: "text-xs text-muted", "No counters defined" }
            }
        }
    }
}
