//! Small building blocks shared by the settings tabs.

use dioxus::prelude::*;

use crate::components::Slider;


#[component]
pub(super) fn TabButton(label: &'static str, active: bool, onclick: EventHandler<()>) -> Element {
    rsx! {
        button {
            class: if active { "tab-btn active" } else { "tab-btn" },
            onclick: move |_| onclick.call(()),
            "{label}"
        }
    }
}

#[component]
pub(super) fn OpacitySlider(label: &'static str, value: u8, on_change: EventHandler<u8>) -> Element {
    rsx! {
        Slider {
            label,
            value: value as f64,
            min: 0.0,
            max: 255.0,
            on_change: move |v: f64| on_change.call(v.round() as u8),
        }
    }
}
