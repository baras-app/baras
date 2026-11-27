//! Phase edit form, shown in the editor drawer

use dioxus::prelude::*;

use crate::components::editor_common::{FormCard, ItemActions, Origin};
use crate::components::editor_drawer::EditorDrawer;
use crate::components::form_fields::{CheckField, Field, NoteField, SubSection, TextField};
use crate::types::{Condition, PhaseDefinition, Trigger};

use super::DifficultiesEditor;
use super::conditions::{ConditionsEditor, CounterConditionEditor};
use super::optional_trigger::OptionalTrigger;
use super::tabs::EncounterData;
use super::triggers::ComposableTriggerEditor;

#[component]
pub(super) fn PhaseEditForm(
    phase: PhaseDefinition,
    all_phases: Vec<PhaseDefinition>,
    #[props(default)] is_builtin: bool,
    #[props(default)] is_modified: bool,
    encounter_data: EncounterData,
    on_save: EventHandler<PhaseDefinition>,
    on_delete: EventHandler<PhaseDefinition>,
    on_close: EventHandler<()>,
    #[props(default)] on_dirty: EventHandler<bool>,
) -> Element {
    let phase_id_display = phase.id.clone();
    let phase_for_delete = phase.clone();
    let phase_for_draft = phase.clone();
    let original = phase.clone();

    let mut draft = use_signal(|| phase_for_draft);
    let mut just_saved = use_signal(|| false);

    // Reset just_saved when user makes new changes after saving
    let original_for_effect = original.clone();
    use_effect(move || {
        if draft() != original_for_effect && just_saved() {
            just_saved.set(false);
        }
    });

    let has_changes = use_memo(move || !just_saved() && draft() != original);

    // Notify parent when dirty state changes
    use_effect(move || {
        on_dirty.call(has_changes());
    });

    // Phase IDs for the preceded_by dropdown (excluding self)
    let phase_ids: Vec<String> = all_phases
        .iter()
        .filter(|p| p.id != phase_id_display)
        .map(|p| p.id.clone())
        .collect();

    let handle_save = move |_: ()| {
        just_saved.set(true);
        on_save.call(draft());
    };

    let handle_delete = move |_: ()| {
        on_delete.call(phase_for_delete.clone());
    };

    let body = rsx! {
        div { class: "phase-edit-form",
            div { class: "edit-grid form-grid-2",
                // Left: what it is
                div { class: "edit-grid-left",
                    FormCard { icon: "fa-tag", title: "Basics",
                        TextField { label: "Name", value: draft().name, on_input: move |v| draft.write().name = v }
                        Field { label: "Phase ID",
                            code { class: "id-display", "{phase_id_display}" }
                        }
                        TextField {
                            label: "Display Text",
                            value: draft().display_text.unwrap_or_default(),
                            placeholder: "(defaults to name)",
                            on_input: move |v: String| draft.write().display_text = (!v.is_empty()).then_some(v),
                        }
                        Field { label: "Difficulties", help: "Only activate this phase on selected difficulties. Empty = all difficulties.",
                            DifficultiesEditor {
                                difficulties: draft().difficulties.clone(),
                                on_change: move |updated: Vec<String>| draft.write().difficulties = updated,
                            }
                        }
                        CheckField { label: "Enabled", checked: draft().enabled, on_change: move |v| draft.write().enabled = v }
                        if !draft().enabled {
                            div { class: "text-xs text-warning mt-xs",
                                i { class: "fa-solid fa-triangle-exclamation" }
                                " Disabling a phase may break timers or other phases that depend on it."
                            }
                        }
                        NoteField { value: draft().note, on_input: move |v| draft.write().note = v }
                    }
                    FormCard { icon: "fa-rotate-left", title: "On Start",
                        SubSection { title: "Resets Counters", help: "Counters to reset when this phase starts",
                            CounterListEditor {
                                counters: draft().resets_counters.clone(),
                                available: encounter_data.counter_options(),
                                on_change: move |counters| draft.write().resets_counters = counters,
                            }
                        }
                    }
                }
                // Right: the mechanics, what starts and ends it and the state it needs
                div { class: "edit-grid-right",
                    FormCard { icon: "fa-bolt", title: "Trigger",
                        SubSection { title: "Start On", help: "The game event that starts this phase",
                            ComposableTriggerEditor {
                                trigger: draft().start_trigger,
                                encounter_data: encounter_data.clone(),
                                on_change: move |t| draft.write().start_trigger = t,
                                hide_timer_only: true,
                            }
                        }
                        OptionalTrigger {
                            title: "End On",
                            help: "Ends this phase when the trigger fires. Default: when another phase starts",
                            noun: "End",
                            empty: "Ends when another phase starts",
                            trigger: draft().end_trigger,
                            initial: Trigger::CombatStart,
                            hide_timer_only: true,
                            encounter_data: encounter_data.clone(),
                            on_change: move |t| draft.write().end_trigger = t,
                        }
                        SubSection { title: "Conditions", help: "State conditions that must ALL be true for this phase to activate",
                            Field { label: "Preceded By", help: "This phase can only start if the specified phase was the most recent active phase",
                                {
                                    let selected_phase = draft().preceded_by.unwrap_or_default();
                                    rsx! {
                                        select {
                                            onchange: move |e| {
                                                let v = e.value();
                                                draft.write().preceded_by = (!v.is_empty()).then_some(v);
                                            },
                                            option { value: "", selected: selected_phase.is_empty(), "(none)" }
                                            for phase_id in &phase_ids {
                                                option { value: "{phase_id}", selected: phase_id == &selected_phase, "{phase_id}" }
                                            }
                                        }
                                    }
                                }
                            }
                            ConditionsEditor {
                                conditions: draft().conditions.clone(),
                                encounter_data: encounter_data.clone(),
                                on_change: move |c| draft.write().conditions = c,
                            }

                            // Legacy counter condition: show migration banner if old field has value
                            if draft().counter_condition.is_some() {
                                div { class: "legacy-conditions-banner",
                                    i { class: "fa-solid fa-circle-info" }
                                    span { "Legacy counter condition detected." }
                                    button {
                                        class: "btn btn-sm",
                                        title: "Move legacy counter condition into the new Conditions system",
                                        onclick: move |_| {
                                            let mut d = draft.write();
                                            if let Some(cc) = d.counter_condition.take() {
                                                d.conditions.push(Condition::CounterCompare {
                                                    counter_id: cc.counter_id,
                                                    operator: cc.operator,
                                                    value: cc.value,
                                                });
                                            }
                                        },
                                        "Migrate"
                                    }
                                }
                                Field { label: "Counter (legacy)", muted: true,
                                    CounterConditionEditor {
                                        condition: draft().counter_condition.clone(),
                                        counters: encounter_data.counter_options(),
                                        on_change: move |cond| draft.write().counter_condition = cond,
                                    }
                                }
                            }
                        }

                    }
                }
            }
        }
    };

    rsx! {
        EditorDrawer {
            title: phase.name.clone(),
            dirty: has_changes(),
            on_close: on_close,
            actions: rsx! {
                ItemActions {
                    has_changes: has_changes(),
                    origin: Origin::of(is_builtin, is_modified),
                    on_save: handle_save,
                    on_delete: handle_delete,
                }
            },
            {body}
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Counter List Editor (for resets_counters)
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn CounterListEditor(
    counters: Vec<String>,
    /// (id, label) per counter
    available: Vec<(String, String)>,
    on_change: EventHandler<Vec<String>>,
) -> Element {
    let label_of = |id: &String| {
        available
            .iter()
            .find(|(i, _)| i == id)
            .map_or_else(|| id.clone(), |(_, label)| label.clone())
    };
    let remaining: Vec<&(String, String)> = available.iter().filter(|(id, _)| !counters.contains(id)).collect();
    let counters_for_add = counters.clone();

    rsx! {
        div { class: "field-stack chip-box",
            if !counters.is_empty() {
                div { class: "field-chips",
                    for (idx, counter) in counters.iter().enumerate() {
                        {
                            let counters_clone = counters.clone();
                            rsx! {
                                span { class: "chip",
                                    "{label_of(counter)}"
                                    button {
                                        title: "Remove",
                                        class: "chip-remove",
                                        onclick: move |_| {
                                            let mut next = counters_clone.clone();
                                            next.remove(idx);
                                            on_change.call(next);
                                        },
                                        "×"
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if !remaining.is_empty() {
                select {
                    onchange: move |e| {
                        let id = e.value();
                        if !id.is_empty() && !counters_for_add.contains(&id) {
                            let mut next = counters_for_add.clone();
                            next.push(id);
                            on_change.call(next);
                        }
                    },
                    option { value: "", selected: true, "(add counter...)" }
                    for (id, label) in remaining {
                        option { value: "{id}", "{label}" }
                    }
                }
            } else if available.is_empty() {
                span { class: "text-xs text-muted", "No counters defined" }
            }
        }
    }
}
