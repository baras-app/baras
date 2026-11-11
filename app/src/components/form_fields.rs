//! Rows for the editor forms (effects, timers, phases, counters, challenges,
//! entities). Every row is the same two-column grid: label, then control(s),
//! so controls line up across a card. Narrow drawers stack the label above.

use dioxus::prelude::*;

/// "?" with a hover tooltip
#[component]
pub fn Help(text: &'static str) -> Element {
    rsx! { span { class: "help-icon", title: "{text}", "?" } }
}

/// Label column + control column. `top` pins the label to the first line of a
/// tall control (trigger editors, chip lists); `muted` greys the label.
#[component]
pub fn Field(
    label: &'static str,
    #[props(default)] help: &'static str,
    #[props(default)] top: bool,
    #[props(default)] muted: bool,
    children: Element,
) -> Element {
    rsx! {
        div { class: if top { "field top" } else { "field" },
            span { class: if muted { "field-label muted" } else { "field-label" },
                "{label}"
                if !help.is_empty() { Help { text: help } }
            }
            div { class: "field-control", {children} }
        }
    }
}

/// Checkbox row starting at the label edge, its text beside it
#[component]
pub fn CheckField(
    label: &'static str,
    #[props(default)] help: &'static str,
    checked: bool,
    /// Indented under the option it depends on
    #[props(default)] nested: bool,
    on_change: EventHandler<bool>,
) -> Element {
    rsx! {
        div { class: if nested { "field check nested" } else { "field check" },
            label { class: "field-check",
                input { r#type: "checkbox", checked, onchange: move |e| on_change.call(e.checked()) }
                "{label}"
                if !help.is_empty() { Help { text: help } }
            }
        }
    }
}

#[component]
pub fn TextField(
    label: &'static str,
    #[props(default)] help: &'static str,
    value: String,
    #[props(default)] placeholder: &'static str,
    on_input: EventHandler<String>,
) -> Element {
    rsx! {
        Field { label, help,
            input { r#type: "text", value, placeholder, oninput: move |e| on_input.call(e.value()) }
        }
    }
}

/// Number box with an optional unit after it. The raw text is passed on so
/// callers decide how to parse (and whether blank means "none").
#[component]
pub fn NumberField(
    label: &'static str,
    #[props(default)] help: &'static str,
    value: String,
    #[props(default)] unit: &'static str,
    #[props(default)] min: &'static str,
    #[props(default)] max: String,
    #[props(default = "any")] step: &'static str,
    #[props(default)] placeholder: &'static str,
    on_input: EventHandler<String>,
) -> Element {
    rsx! {
        Field { label, help,
            input {
                class: "field-number",
                r#type: "number",
                value,
                min,
                max,
                step,
                placeholder,
                oninput: move |e| on_input.call(e.value()),
            }
            if !unit.is_empty() { span { class: "field-unit", "{unit}" } }
        }
    }
}

/// One of a row of pill toggles (difficulties, group size)
#[component]
pub fn TogglePill(label: String, active: bool, on_click: EventHandler<()>) -> Element {
    rsx! {
        button {
            class: if active { "toggle-btn active" } else { "toggle-btn" },
            onclick: move |_| on_click.call(()),
            "{label}"
        }
    }
}

/// Titled group inside a card, ruled off from the group above it
#[component]
pub fn SubSection(title: &'static str, #[props(default)] help: &'static str, children: Element) -> Element {
    rsx! {
        div { class: "form-subsection",
            div { class: "form-subsection-title",
                "{title}"
                if !help.is_empty() { Help { text: help } }
            }
            {children}
        }
    }
}

/// Warning line under a soft-deprecated option
#[component]
pub fn Deprecated(note: &'static str) -> Element {
    rsx! {
        div { class: "field-deprecated",
            i { class: "fa-solid fa-triangle-exclamation" }
            " {note}"
        }
    }
}
