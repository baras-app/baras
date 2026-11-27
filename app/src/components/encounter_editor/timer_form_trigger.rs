//! Timer form card: what starts the timer, what cancels it, and the state it
//! needs to be active

use dioxus::prelude::*;

use crate::components::editor_common::FormCard;
use crate::components::form_fields::{Field, SubSection};
use crate::types::{BossTimerDefinition, Condition, Trigger};

use super::conditions::{ConditionsEditor, CounterConditionEditor};
use super::optional_trigger::OptionalTrigger;
use super::tabs::EncounterData;
use super::timers::PhaseSelector;
use super::triggers::ComposableTriggerEditor;

#[component]
pub(super) fn TriggerCard(mut draft: Signal<BossTimerDefinition>, encounter_data: EncounterData) -> Element {
    let d = draft.read().clone();
    rsx! {
        FormCard { icon: "fa-bolt", title: "Trigger",
            SubSection { title: "Start On", help: "The game event that starts this timer",
                ComposableTriggerEditor {
                    trigger: d.trigger.clone(),
                    encounter_data: encounter_data.clone(),
                    on_change: move |t| draft.write().trigger = t,
                }
            }
            if !d.is_alert {
                OptionalTrigger {
                    title: "Cancel On",
                    help: "Cancels this timer when the trigger fires. Default: combat end",
                    noun: "Cancel",
                    empty: "Default: combat end",
                    initial: Trigger::CombatStart,
                    trigger: d.cancel_trigger.clone(),
                    encounter_data: encounter_data.clone(),
                    on_change: move |t| draft.write().cancel_trigger = t,
                }
            }
            SubSection {
                title: "Conditions",
                help: "State conditions that must ALL be true for this timer to be active. Use for phase, counter, HP, or entity state guards.",
                ConditionsEditor {
                    conditions: d.conditions.clone(),
                    encounter_data: encounter_data.clone(),
                    on_change: move |c| draft.write().conditions = c,
                }

                // Legacy fields: show migration banner if old fields have values
                if !d.phases.is_empty() || d.counter_condition.is_some() {
                    div { class: "legacy-conditions-banner",
                        i { class: "fa-solid fa-circle-info" }
                        span { "Legacy phase/counter fields detected." }
                        button {
                            class: "btn btn-sm",
                            title: "Move legacy phase and counter conditions into the new Conditions system",
                            onclick: move |_| {
                                let mut d = draft.write();
                                if !d.phases.is_empty() {
                                    let phase_ids = std::mem::take(&mut d.phases);
                                    d.conditions.push(Condition::PhaseActive { phase_ids });
                                }
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
                    Field { label: "Phases (legacy)", muted: true,
                        PhaseSelector {
                            selected: d.phases.clone(),
                            available: encounter_data.phase_options(),
                            on_change: move |p| draft.write().phases = p,
                        }
                    }
                    Field { label: "Counter (legacy)", muted: true,
                        CounterConditionEditor {
                            condition: d.counter_condition.clone(),
                            counters: encounter_data.counter_options(),
                            on_change: move |c| draft.write().counter_condition = c,
                        }
                    }
                }
            }
        }
    }
}
