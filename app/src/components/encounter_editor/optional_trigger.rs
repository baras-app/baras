//! Optional trigger sub-section (end / decrement / cancel / clear): the
//! editor with a remove button, or a note with an add button.

use dioxus::prelude::*;

use crate::components::form_fields::SubSection;
use crate::types::Trigger;

use super::tabs::EncounterData;
use super::triggers::ComposableTriggerEditor;

#[component]
pub(super) fn OptionalTrigger(
    title: &'static str,
    help: &'static str,
    /// "End" / "Decrement", for the buttons
    noun: &'static str,
    /// Shown while unset ("Ends when another phase starts")
    empty: &'static str,
    trigger: Option<Trigger>,
    /// Trigger the add button starts from
    initial: Trigger,
    #[props(default)] hide_timer_only: bool,
    encounter_data: EncounterData,
    on_change: EventHandler<Option<Trigger>>,
) -> Element {
    rsx! {
        SubSection { title, help,
            div { class: "flex-col gap-xs",
                if let Some(t) = trigger {
                    ComposableTriggerEditor {
                        trigger: t,
                        encounter_data,
                        on_change: move |t| on_change.call(Some(t)),
                        hide_timer_only,
                    }
                    button { class: "btn btn-sm w-fit", onclick: move |_| on_change.call(None), "Remove {noun} Trigger" }
                } else {
                    span { class: "conditions-editor-empty", "{empty}" }
                    button {
                        class: "btn btn-sm w-fit",
                        onclick: move |_| on_change.call(Some(initial.clone())),
                        "+ Add {noun} Trigger"
                    }
                }
            }
        }
    }
}
