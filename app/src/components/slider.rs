//! Reusable labeled slider with an editable numeric value, and the themed
//! number box (value + steppers) it uses, also usable on its own.
//!
//! Both drive the same `on_change` callback; the slider adds an optional
//! static unit suffix (e.g. "px", "%").
//! Values are expressed in display space (f64); callers convert to their
//! stored representation inside `on_change`.

use dioxus::prelude::*;

#[component]
pub fn Slider(
    label: &'static str,
    value: f64,
    min: f64,
    max: f64,
    #[props(default = 1.0)] step: f64,
    #[props(default = "")] suffix: &'static str,
    #[props(default = "")] tooltip: &'static str,
    #[props(default = false)] disabled: bool,
    on_change: EventHandler<f64>,
) -> Element {
    rsx! {
        div { class: "setting-row",
            label {
                "{label}"
                if !tooltip.is_empty() {
                    span { class: "help-icon", title: "{tooltip}", "?" }
                }
            }
            input {
                r#type: "range",
                min: "{min}",
                max: "{max}",
                step: "{step}",
                value: "{value}",
                disabled: disabled,
                oninput: move |e| {
                    if let Ok(v) = e.value().parse::<f64>() {
                        on_change.call(v.clamp(min, max));
                    }
                },
            }
            NumberInput { value, min, max, step, disabled, on_change }
            if !suffix.is_empty() {
                span { class: "slider-unit", "{suffix}" }
            }
        }
    }
}

/// Number box with themed steppers; native spinners differ per webview.
#[component]
pub fn NumberInput(
    value: f64,
    min: f64,
    max: f64,
    #[props(default = 1.0)] step: f64,
    #[props(default = false)] disabled: bool,
    on_change: EventHandler<f64>,
) -> Element {
    rsx! {
        div { class: "slider-number",
            input {
                r#type: "number",
                class: "slider-value-input",
                min: "{min}",
                max: "{max}",
                step: "any",
                value: "{value}",
                disabled: disabled,
                onchange: move |e| {
                    if let Ok(v) = e.value().parse::<f64>() {
                        on_change.call(v.clamp(min, max));
                    }
                },
            }
            div { class: "slider-steppers",
                button {
                    r#type: "button",
                    class: "slider-step",
                    tabindex: "-1",
                    disabled: disabled,
                    onclick: move |_| on_change.call((value + step).clamp(min, max)),
                    i { class: "fa-solid fa-caret-up" }
                }
                button {
                    r#type: "button",
                    class: "slider-step",
                    tabindex: "-1",
                    disabled: disabled,
                    onclick: move |_| on_change.call((value - step).clamp(min, max)),
                    i { class: "fa-solid fa-caret-down" }
                }
            }
        }
    }
}
