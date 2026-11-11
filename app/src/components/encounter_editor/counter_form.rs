//! Counter edit form, shown in the editor drawer

use dioxus::prelude::*;

use crate::components::editor_common::{FormCard, ItemActions, Origin};
use crate::components::form_fields::{CheckField, Deprecated, Field, NumberField, SubSection, TextField};
use crate::types::{CounterDefinition, EffectSelector, EffectStackConfig, EntityFilter, StackAggregation, Trigger};

use super::optional_trigger::OptionalTrigger;
use super::tabs::EncounterData;
use super::triggers::{ComposableTriggerEditor, EffectSelectorEditor, EntityFilterDropdown};

#[component]
pub(super) fn CounterEditForm(
    counter: CounterDefinition,
    #[props(default)] is_builtin: bool,
    #[props(default)] is_modified: bool,
    encounter_data: EncounterData,
    on_save: EventHandler<CounterDefinition>,
    on_delete: EventHandler<CounterDefinition>,
    #[props(default)] on_dirty: EventHandler<bool>,
) -> Element {
    let counter_id_display = counter.id.clone();
    let counter_for_delete = counter.clone();
    let counter_for_draft = counter.clone();
    let original = counter.clone();
    // Deprecated options stay visible only for counters that already use them
    let (had_set_value, had_decrement) = (counter.set_value.is_some(), counter.decrement);

    let mut draft = use_signal(|| counter_for_draft);
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

    let handle_save = move |_: ()| {
        just_saved.set(true);
        on_save.call(draft());
    };

    let handle_delete = move |_: ()| {
        on_delete.call(counter_for_delete.clone());
    };

    // Writes into the effect stack config, when the counter has one
    let mut edit_stacks = move |f: &dyn Fn(&mut EffectStackConfig)| {
        if let Some(cfg) = draft.write().track_effect_stacks.as_mut() {
            f(cfg);
        }
    };

    let reset_on = rsx! {
        ComposableTriggerEditor {
            trigger: draft().reset_on,
            encounter_data: encounter_data.clone(),
            on_change: move |t| draft.write().reset_on = t,
            hide_timer_only: true,
        }
    };

    rsx! {
        div { class: "counter-edit-form",
            div { class: "edit-grid",
                // ═══ LEFT: Identity Card ═════════════════════════════════════
                FormCard { icon: "fa-tag", title: "Identity",
                    Field { label: "Counter ID",
                        code { class: "id-display", "{counter_id_display}" }
                    }
                    TextField { label: "Name", value: draft().name, on_input: move |v| draft.write().name = v }
                    TextField {
                        label: "Display Text",
                        value: draft().display_text.unwrap_or_default(),
                        placeholder: "(defaults to name)",
                        on_input: move |v: String| draft.write().display_text = (!v.is_empty()).then_some(v),
                    }

                    // Options (hidden for effect stack counters)
                    if draft().track_effect_stacks.is_none() {
                        SubSection { title: "Options",
                            NumberField {
                                label: "Initial Value",
                                help: "Starting value for this counter when reset",
                                value: draft().initial_value.to_string(),
                                min: "0",
                                step: "1",
                                on_input: move |v: String| if let Ok(v) = v.parse::<u32>() { draft.write().initial_value = v },
                            }
                            if had_set_value || draft().set_value.is_some() {
                                CheckField {
                                    label: "Set to a fixed value",
                                    help: "Set to a specific value on trigger instead of incrementing by 1",
                                    checked: draft().set_value.is_some(),
                                    on_change: move |on: bool| draft.write().set_value = on.then_some(1),
                                }
                                if let Some(set_value) = draft().set_value {
                                    NumberField {
                                        label: "Set Value",
                                        value: set_value.to_string(),
                                        min: "0",
                                        step: "1",
                                        on_input: move |v: String| if let Ok(v) = v.parse::<u32>() { draft.write().set_value = Some(v) },
                                    }
                                }
                                Deprecated { note: "Deprecated: fixed values will be removed from counters" }
                            }
                            if had_decrement || draft().decrement {
                                CheckField {
                                    label: "Decrement",
                                    help: "Count down instead of up on each trigger",
                                    checked: draft().decrement,
                                    on_change: move |v| draft.write().decrement = v,
                                }
                                Deprecated { note: "Deprecated: use a Decrement On trigger instead" }
                            }
                        }
                    }
                }

                // ═══ RIGHT: Mode + Config Card ═══════════════════════════════
                FormCard { icon: "fa-bolt", title: "Counter Mode",
                    Field {
                        label: "Mode",
                        help: "Manual: triggers increment/decrement the counter. Effect Stacks: automatically tracks stack count of a game effect.",
                        select {
                            value: if draft().track_effect_stacks.is_some() { "effect_stacks" } else { "manual" },
                            onchange: move |e| {
                                draft.write().track_effect_stacks = (e.value() == "effect_stacks").then(|| EffectStackConfig {
                                    effects: vec![],
                                    target: EntityFilter::Any,
                                    aggregation: StackAggregation::Max,
                                });
                            },
                            option { value: "manual", "Default" }
                            option { value: "effect_stacks", "Track Effect Stacks" }
                        }
                    }

                    if let Some(config) = draft().track_effect_stacks {
                        // ─── Effect Stack Mode ─────────────────────────────────
                        SubSection { title: "Effects", help: "Which game effects to track stacks for (by ID or name)",
                            EffectSelectorEditor {
                                label: "",
                                selectors: config.effects.clone(),
                                on_change: move |sels: Vec<EffectSelector>| edit_stacks(&|c| c.effects = sels.clone()),
                            }
                        }
                        Field { label: "Target", help: "Which entities' stacks to track (e.g., local player, any player, a specific boss)",
                            EntityFilterDropdown {
                                label: "",
                                value: config.target.clone(),
                                options: EntityFilter::target_options(),
                                on_change: move |f: EntityFilter| edit_stacks(&|c| c.target = f.clone()),
                            }
                        }
                        Field { label: "Aggregation", help: "How to combine stacks when multiple entities match: Max (highest), Sum (total), Min (lowest)",
                            select {
                                value: match config.aggregation {
                                    StackAggregation::Max => "max",
                                    StackAggregation::Sum => "sum",
                                    StackAggregation::Min => "min",
                                },
                                onchange: move |e| {
                                    let agg = match e.value().as_str() {
                                        "sum" => StackAggregation::Sum,
                                        "min" => StackAggregation::Min,
                                        _ => StackAggregation::Max,
                                    };
                                    edit_stacks(&|c| c.aggregation = agg.clone());
                                },
                                option { value: "max", "Max" }
                                option { value: "sum", "Sum" }
                                option { value: "min", "Min" }
                            }
                        }
                        // Reset trigger still applies in effect stack mode
                        SubSection { title: "Reset On", help: "When to reset the counter value (effect stack state is preserved)",
                            {reset_on}
                        }
                    } else {
                        // ─── Manual Triggers Mode ──────────────────────────────
                        SubSection { title: "Increment On", help: "The game event that increments (or sets) this counter",
                            ComposableTriggerEditor {
                                trigger: draft().increment_on,
                                encounter_data: encounter_data.clone(),
                                on_change: move |t| draft.write().increment_on = t,
                                hide_timer_only: true,
                            }
                        }
                        OptionalTrigger {
                            title: "Decrement On",
                            help: "Optional separate trigger that decrements the counter",
                            noun: "Decrement",
                            empty: "No separate decrement trigger",
                            trigger: draft().decrement_on,
                            initial: Trigger::AbilityCast {
                                abilities: vec![],
                                source: EntityFilter::default(),
                                target: EntityFilter::default(),
                                position: vec![],
                            },
                            hide_timer_only: true,
                            encounter_data: encounter_data.clone(),
                            on_change: move |t| draft.write().decrement_on = t,
                        }
                        SubSection { title: "Reset On", help: "The game event that resets this counter to its initial value",
                            {reset_on}
                        }
                    }
                }
            }

            FormCard { icon: "fa-triangle-exclamation", title: "Advanced",
                CheckField { label: "Counter enabled", checked: draft().enabled, on_change: move |v| draft.write().enabled = v }
                if !draft().enabled {
                    div { class: "text-xs text-warning mt-xs",
                        i { class: "fa-solid fa-triangle-exclamation" }
                        " Disabling a counter may break phases or timers that depend on it."
                    }
                }
            }

            ItemActions {
                has_changes: has_changes(),
                origin: Origin::of(is_builtin, is_modified),
                on_save: handle_save,
                on_delete: handle_delete,
            }
        }
    }
}
