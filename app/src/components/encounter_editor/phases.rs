//! Phase editing tab
//!
//! CRUD for boss phase definitions.

use dioxus::prelude::*;

use crate::api;
use crate::types::{BossListItem, PhaseListItem, PhaseTrigger};

use super::conditions::CounterConditionEditor;
use super::triggers::PhaseTriggerEditor;

// ─────────────────────────────────────────────────────────────────────────────
// Phases Tab
// ─────────────────────────────────────────────────────────────────────────────

#[component]
pub fn PhasesTab(
    boss: BossListItem,
    on_status: EventHandler<(String, bool)>,
) -> Element {
    let mut phases = use_signal(Vec::<PhaseListItem>::new);
    let mut loading = use_signal(|| true);
    let mut expanded_phase = use_signal(|| None::<String>);
    let mut show_new_phase = use_signal(|| false);

    let file_path = boss.file_path.clone();
    let boss_id = boss.id.clone();

    // Load phases on mount
    use_effect(move || {
        let file_path = file_path.clone();
        let boss_id = boss_id.clone();
        spawn(async move {
            if let Some(p) = api::get_phases_for_area(&file_path).await {
                let boss_phases: Vec<_> = p.into_iter().filter(|p| p.boss_id == boss_id).collect();
                phases.set(boss_phases);
            }
            loading.set(false);
        });
    });

    // Get counter IDs for condition editor
    let counter_ids: Vec<String> = vec![]; // TODO: Load from counters

    // Get phase IDs for preceded_by dropdown
    let phase_ids: Vec<String> = phases().iter().map(|p| p.id.clone()).collect();

    rsx! {
        div { class: "phases-tab",
            // Header
            div { class: "flex items-center justify-between mb-sm",
                span { class: "text-sm text-secondary",
                    if loading() { "Loading..." } else { "{phases().len()} phases" }
                }
                button {
                    class: "btn btn-success btn-sm",
                    onclick: move |_| show_new_phase.set(true),
                    "+ New Phase"
                }
            }

            // New phase form
            if show_new_phase() {
                NewPhaseForm {
                    boss: boss.clone(),
                    phase_ids: phase_ids.clone(),
                    counter_ids: counter_ids.clone(),
                    on_create: move |new_phase: PhaseListItem| {
                        spawn(async move {
                            if let Some(created) = api::create_phase(&new_phase).await {
                                let mut current = phases();
                                current.push(created);
                                phases.set(current);
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

            // Phase list
            if loading() {
                div { class: "empty-state text-sm", "Loading phases..." }
            } else if phases().is_empty() {
                div { class: "empty-state text-sm", "No phases defined" }
            } else {
                for phase in phases() {
                    {
                        let phase_key = phase.id.clone();
                        let is_expanded = expanded_phase() == Some(phase_key.clone());
                        let phases_for_row = phases();

                        rsx! {
                            PhaseRow {
                                key: "{phase_key}",
                                phase: phase.clone(),
                                all_phases: phases_for_row,
                                expanded: is_expanded,
                                counter_ids: counter_ids.clone(),
                                on_toggle: move |_| {
                                    expanded_phase.set(if is_expanded { None } else { Some(phase_key.clone()) });
                                },
                                on_change: move |updated: Vec<PhaseListItem>| {
                                    phases.set(updated);
                                },
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
    counter_ids: Vec<String>,
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
                                counter_ids: counter_ids,
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
    counter_ids: Vec<String>,
    on_save: EventHandler<PhaseListItem>,
    on_delete: EventHandler<PhaseListItem>,
) -> Element {
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
        on_delete.call(phase.clone());
    };

    rsx! {
        div { class: "phase-edit-form",
            // ─── Main Fields ─────────────────────────────────────────────────
            div { class: "form-row-hz",
                label { "Phase ID" }
                input {
                    class: "input-inline text-mono",
                    style: "width: 200px;",
                    disabled: true,
                    value: "{draft().id}",
                }
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

            // ─── Start Trigger ───────────────────────────────────────────────
            div { class: "form-section",
                div { class: "font-bold text-sm mb-xs", "Start Trigger" }
                PhaseTriggerEditor {
                    trigger: draft().start_trigger,
                    on_change: move |t| {
                        let mut d = draft();
                        d.start_trigger = t;
                        draft.set(d);
                    }
                }
            }

            // ─── End Trigger (Optional) ──────────────────────────────────────
            div { class: "form-section",
                div { class: "flex items-center gap-xs mb-xs",
                    span { class: "font-bold text-sm", "End Trigger" }
                    span { class: "text-xs text-muted", "(optional)" }
                }

                if draft().end_trigger.is_some() {
                    PhaseTriggerEditor {
                        trigger: draft().end_trigger.clone().unwrap(),
                        on_change: move |t| {
                            let mut d = draft();
                            d.end_trigger = Some(t);
                            draft.set(d);
                        }
                    }
                    button {
                        class: "btn btn-sm btn-danger mt-xs",
                        onclick: move |_| {
                            let mut d = draft();
                            d.end_trigger = None;
                            draft.set(d);
                        },
                        "Remove End Trigger"
                    }
                } else {
                    button {
                        class: "btn-dashed text-sm",
                        onclick: move |_| {
                            let mut d = draft();
                            d.end_trigger = Some(PhaseTrigger::CombatStart);
                            draft.set(d);
                        },
                        "+ Add End Trigger"
                    }
                }
            }

            // ─── Guards ──────────────────────────────────────────────────────
            div { class: "form-section",
                div { class: "font-bold text-sm mb-xs", "Guards" }

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
                        counters: counter_ids.clone(),
                        on_change: move |cond| {
                            let mut d = draft();
                            d.counter_condition = cond;
                            draft.set(d);
                        }
                    }
                }
            }

            // ─── Resets Counters ─────────────────────────────────────────────
            div { class: "form-section",
                div { class: "font-bold text-sm mb-xs", "Resets Counters" }
                CounterListEditor {
                    counters: draft().resets_counters.clone(),
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
    counter_ids: Vec<String>,
    on_create: EventHandler<PhaseListItem>,
    on_cancel: EventHandler<()>,
) -> Element {
    let mut name = use_signal(|| "New Phase".to_string());
    let mut start_trigger = use_signal(|| PhaseTrigger::CombatStart);
    let mut preceded_by = use_signal(|| None::<String>);

    let handle_create = move |_| {
        let new_phase = PhaseListItem {
            id: String::new(), // Generated by backend
            name: name(),
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
                    on_change: move |t| start_trigger.set(t),
                }
            }

            div { class: "flex gap-xs mt-sm",
                button {
                    class: "btn btn-success btn-sm",
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
    on_change: EventHandler<Vec<String>>,
) -> Element {
    let mut new_counter = use_signal(String::new);

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

            // Add new counter
            div { class: "flex gap-xs",
                input {
                    r#type: "text",
                    class: "input-inline",
                    style: "width: 150px;",
                    placeholder: "counter_id",
                    value: "{new_counter}",
                    oninput: move |e| new_counter.set(e.value())
                }
                button {
                    class: "btn btn-sm",
                    onclick: move |_| {
                        let val = new_counter();
                        if !val.is_empty() && !counters_for_add.contains(&val) {
                            let mut new_counters = counters_for_add.clone();
                            new_counters.push(val);
                            on_change.call(new_counters);
                            new_counter.set(String::new());
                        }
                    },
                    "Add"
                }
            }
        }
    }
}
