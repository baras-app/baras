//! Building blocks shared by the overlay settings tabs.
//!
//! Every tab is laid out from the same sections in the same order
//! (`Group`), and every value row ends in the same fixed unit column, so
//! number boxes, colour swatches and dropdowns line up whether or not the
//! row has a unit.

use dioxus::prelude::*;

use crate::components::{NumberInput, Slider};
use crate::types::Color;
use crate::utils::{color_to_hex, parse_hex_color};

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

// ─────────────────────────────────────────────────────────────────────────
// Sections
// ─────────────────────────────────────────────────────────────────────────

/// The sections an overlay tab may have. Tabs render the ones they need in
/// this order: Background & Bars, Layout, Content, Text.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Group {
    /// Background opacity, dynamic background, bar colour/gradient, borders
    Background,
    /// Size, count, direction, stacking, spacing, bar or icon mode
    Layout,
    /// Which parts are shown; timing and clearing
    Content,
    /// Font size or scale, font colours
    Text,
}

impl Group {
    fn title(self) -> &'static str {
        match self {
            Self::Background => "Background & Bars",
            Self::Layout => "Layout",
            Self::Content => "Content",
            Self::Text => "Text",
        }
    }
}

#[component]
pub(super) fn Section(group: Group, children: Element) -> Element {
    rsx! {
        div { class: "settings-group",
            h4 { "{group.title()}" }
            {children}
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Rows
// ─────────────────────────────────────────────────────────────────────────

/// Label, control(s), then the unit column (empty when there is no unit)
#[component]
pub(super) fn Row(
    label: &'static str,
    #[props(default = "")] tooltip: &'static str,
    #[props(default = "")] unit: &'static str,
    children: Element,
) -> Element {
    rsx! {
        div { class: "setting-row",
            label {
                "{label}"
                if !tooltip.is_empty() {
                    span { class: "help-icon", title: "{tooltip}", "?" }
                }
            }
            {children}
            span { class: "slider-unit", "{unit}" }
        }
    }
}

#[component]
pub(super) fn Check(
    label: &'static str,
    checked: bool,
    #[props(default = false)] disabled: bool,
    #[props(default = "")] tooltip: &'static str,
    /// Muted note after the label, e.g. "(bar layout)"
    #[props(default = "")] note: &'static str,
    on_change: EventHandler<bool>,
) -> Element {
    rsx! {
        label { class: "settings-check",
            input {
                r#type: "checkbox",
                checked,
                disabled,
                onchange: move |e: Event<FormData>| on_change.call(e.checked()),
            }
            "{label}"
            if !note.is_empty() {
                span { class: "settings-check-note", "{note}" }
            }
            if !tooltip.is_empty() {
                span { class: "help-icon", title: "{tooltip}", "?" }
            }
        }
    }
}

#[component]
pub(super) fn ColorRow(
    label: &'static str,
    value: Color,
    #[props(default = false)] disabled: bool,
    on_change: EventHandler<Color>,
) -> Element {
    rsx! {
        Row { label,
            input {
                r#type: "color",
                class: "color-picker",
                value: "{color_to_hex(&value)}",
                disabled,
                oninput: move |e: Event<FormData>| {
                    if let Some(c) = parse_hex_color(&e.value()) {
                        on_change.call(c);
                    }
                },
            }
        }
    }
}

/// A plain number box (no slider), e.g. a maximum count
#[component]
pub(super) fn NumberRow(
    label: &'static str,
    value: f64,
    min: f64,
    max: f64,
    #[props(default = 1.0)] step: f64,
    #[props(default = "")] unit: &'static str,
    #[props(default = "")] tooltip: &'static str,
    on_change: EventHandler<f64>,
) -> Element {
    rsx! {
        Row { label, unit, tooltip,
            NumberInput { value, min, max, step, on_change }
        }
    }
}

/// A dropdown; `options` are (value, label)
#[component]
pub(super) fn SelectRow(
    label: &'static str,
    value: String,
    options: Vec<(&'static str, &'static str)>,
    #[props(default = "")] tooltip: &'static str,
    #[props(default = false)] disabled: bool,
    on_change: EventHandler<String>,
) -> Element {
    rsx! {
        Row { label, tooltip,
            select {
                value: "{value}",
                disabled,
                onchange: move |e: Event<FormData>| on_change.call(e.value()),
                for (v, l) in options {
                    option { value: v, selected: value == v, "{l}" }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Rows every overlay repeats
// ─────────────────────────────────────────────────────────────────────────

#[component]
pub(super) fn OpacitySlider(
    #[props(default = "Background Opacity")] label: &'static str,
    value: u8,
    on_change: EventHandler<u8>,
) -> Element {
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

#[component]
pub(super) fn DynamicBackground(checked: bool, on_change: EventHandler<bool>) -> Element {
    rsx! { Check { label: "Dynamic Background", checked, on_change } }
}

/// Font Scale as a percentage of the base size (stored as a factor)
#[component]
pub(super) fn FontScale(value: f32, on_change: EventHandler<f32>) -> Element {
    rsx! {
        Slider {
            label: "Font Scale",
            value: (value * 100.0).round() as f64,
            min: 30.0,
            max: 300.0,
            step: 10.0,
            suffix: "%",
            on_change: move |v: f64| on_change.call((v as f32 / 100.0).clamp(0.3, 3.0)),
        }
    }
}

/// Show Border and its colour (the colour is disabled while the border is off)
#[component]
pub(super) fn BorderRows(
    show: bool,
    color: Color,
    /// Note on the checkbox when it only applies in one mode, e.g. "(bar layout)"
    #[props(default = "")] note: &'static str,
    /// Both rows off, e.g. outside bar layout
    #[props(default = false)] disabled: bool,
    on_show: EventHandler<bool>,
    on_color: EventHandler<Color>,
) -> Element {
    rsx! {
        Check { label: "Show Border", checked: show, note, disabled, on_change: on_show }
        ColorRow { label: "Border Color", value: color, disabled: disabled || !show, on_change: on_color }
    }
}

/// Bottom of every tab: Reset Style plus the tab's own actions
#[component]
pub(super) fn ResetRow(on_reset: EventHandler<()>, actions: Element) -> Element {
    rsx! {
        div { class: "setting-row reset-row",
            button { class: "btn btn-reset", onclick: move |_| on_reset.call(()),
                i { class: "fa-solid fa-rotate-left" }
                span { " Reset Style" }
            }
            {actions}
        }
    }
}
