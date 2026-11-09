//! On/off switch with a label: the app's control for boolean filters and
//! options (replaces bare checkboxes in toolbars).

use dioxus::prelude::*;

#[component]
pub fn ToggleSwitch(
    label: String,
    checked: bool,
    #[props(default)] title: String,
    /// Extra class on the label, for placement (e.g. "history-switch")
    #[props(default = "")] class: &'static str,
    /// Keep the click from reaching a clickable parent (a switch inside a button)
    #[props(default = false)] stop_propagation: bool,
    on_change: EventHandler<bool>,
) -> Element {
    rsx! {
        label {
            class: "toggle-switch-label {class}",
            title,
            onclick: move |e| {
                if stop_propagation {
                    e.stop_propagation();
                }
            },
            span { class: "toggle-switch",
                input {
                    r#type: "checkbox",
                    checked,
                    onchange: move |e| on_change.call(e.checked()),
                }
                span { class: "toggle-slider" }
            }
            span { class: "toggle-text", "{label}" }
        }
    }
}
