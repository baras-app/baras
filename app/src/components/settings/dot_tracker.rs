//! DOT tracker overlay settings tab.

use dioxus::prelude::*;

use crate::components::Slider;
use crate::types::DotTrackerConfig;
use crate::utils::{color_to_hex, parse_hex_color};

use super::draft::SettingsDraft;
use super::widgets::OpacitySlider;

#[component]
pub fn DotTrackerTab(draft: SettingsDraft, actions: Element) -> Element {
    let current_settings = draft.current();

    rsx! {
        // DOT Tracker Settings
        div { class: "settings-section",
            h4 { "Appearance" }

            OpacitySlider {
                label: "Background Opacity",
                value: current_settings.dot_tracker_opacity,
                on_change: move |val| {
                    let mut new_settings = draft.current();
                    new_settings.dot_tracker_opacity = val;
                    draft.update(new_settings);
                },
            }

            Slider {
                label: "Icon/Bar Size",
                value: current_settings.dot_tracker.icon_size as f64,
                min: 12.0,
                max: 48.0,
                suffix: "px",
                on_change: move |v: f64| {
                    let mut new_settings = draft.current();
                    new_settings.dot_tracker.icon_size = (v as u8).clamp(12, 48);
                    draft.update(new_settings);
                },
            }

            div { class: "setting-row",
                label { "Max Targets" }
                select {
                    class: "input-inline",
                    value: "{current_settings.dot_tracker.max_targets}",
                    onchange: move |e: Event<FormData>| {
                        if let Ok(val) = e.value().parse::<u8>() {
                            let mut new_settings = draft.current();
                            new_settings.dot_tracker.max_targets = val.clamp(1, 10);
                            draft.update(new_settings);
                        }
                    },
                    for n in 1..=10u8 {
                        option { value: "{n}", selected: current_settings.dot_tracker.max_targets == n, "{n}" }
                    }
                }
            }

            h4 { style: "margin-top: 16px;", "Display Options" }

            div { class: "setting-row",
                label { "Bar Mode" }
                input {
                    r#type: "checkbox",
                    checked: current_settings.dot_tracker.layout_bar,
                    onchange: move |e: Event<FormData>| {
                        let mut new_settings = draft.current();
                        new_settings.dot_tracker.layout_bar = e.checked();
                        draft.update(new_settings);
                    }
                }
            }

            div { class: "setting-row",
                label { "Show Header" }
                input {
                    r#type: "checkbox",
                    checked: current_settings.dot_tracker.show_header,
                    onchange: move |e: Event<FormData>| {
                        let mut new_settings = draft.current();
                        new_settings.dot_tracker.show_header = e.checked();
                        draft.update(new_settings);
                    }
                }
            }

            div { class: "setting-row",
                label { "Show Effect Names (bar layout)" }
                input {
                    r#type: "checkbox",
                    checked: current_settings.dot_tracker.show_effect_names,
                    disabled: !current_settings.dot_tracker.layout_bar,
                    onchange: move |e: Event<FormData>| {
                        let mut new_settings = draft.current();
                        new_settings.dot_tracker.show_effect_names = e.checked();
                        draft.update(new_settings);
                    }
                }
            }

            div { class: "setting-row",
                label { "Show Countdown" }
                input {
                    r#type: "checkbox",
                    checked: current_settings.dot_tracker.show_countdown,
                    onchange: move |e: Event<FormData>| {
                        let mut new_settings = draft.current();
                        new_settings.dot_tracker.show_countdown = e.checked();
                        draft.update(new_settings);
                    }
                }
            }

            div { class: "setting-row",
                label { "Font Color" }
                input {
                    r#type: "color",
                    value: "{color_to_hex(&current_settings.dot_tracker.font_color)}",
                    class: "color-picker",
                    oninput: move |e: Event<FormData>| {
                        if let Some(color) = parse_hex_color(&e.value()) {
                            let mut new_settings = draft.current();
                            new_settings.dot_tracker.font_color = color;
                            draft.update(new_settings);
                        }
                    }
                }
            }

            Slider {
                label: "Font Scale",
                value: (current_settings.dot_tracker.font_scale * 100.0) as i32 as f64,
                min: 30.0,
                max: 300.0,
                step: 10.0,
                suffix: "%",
                on_change: move |v: f64| {
                    let mut new_settings = draft.current();
                    new_settings.dot_tracker.font_scale = (v as f32 / 100.0).clamp(0.3, 3.0);
                    draft.update(new_settings);
                },
            }
            div { class: "setting-row",
                label { "Dynamic Background" }
                input {
                    r#type: "checkbox",
                    checked: current_settings.dot_tracker.dynamic_background,
                    onchange: move |e: Event<FormData>| {
                        let mut new_settings = draft.current();
                        new_settings.dot_tracker.dynamic_background = e.checked();
                        draft.update(new_settings);
                    }
                }
            }
            div { class: "setting-row",
                label { "Stack from Bottom" }
                input {
                    r#type: "checkbox",
                    checked: current_settings.dot_tracker.stack_from_bottom,
                    onchange: move |e: Event<FormData>| {
                        let mut new_settings = draft.current();
                        new_settings.dot_tracker.stack_from_bottom = e.checked();
                        draft.update(new_settings);
                    }
                }
            }
            div { class: "setting-row",
                label { "Right-Align Icons (icon layout)" }
                input {
                    r#type: "checkbox",
                    checked: current_settings.dot_tracker.right_align_icons,
                    disabled: current_settings.dot_tracker.layout_bar,
                    onchange: move |e: Event<FormData>| {
                        let mut new_settings = draft.current();
                        new_settings.dot_tracker.right_align_icons = e.checked();
                        draft.update(new_settings);
                    }
                }
            }
            div { class: "setting-row",
                label { "Gradient Bars (bar layout)" }
                input {
                    r#type: "checkbox",
                    checked: current_settings.dot_tracker.bar_gradient,
                    disabled: !current_settings.dot_tracker.layout_bar,
                    onchange: move |e: Event<FormData>| {
                        let mut new_settings = draft.current();
                        new_settings.dot_tracker.bar_gradient = e.checked();
                        draft.update(new_settings);
                    }
                }
            }
            div { class: "setting-row",
                label { "Show Border (bar layout)" }
                input {
                    r#type: "checkbox",
                    checked: current_settings.dot_tracker.show_border,
                    disabled: !current_settings.dot_tracker.layout_bar,
                    onchange: move |e: Event<FormData>| {
                        let mut new_settings = draft.current();
                        new_settings.dot_tracker.show_border = e.checked();
                        draft.update(new_settings);
                    }
                }
            }
            div { class: "setting-row",
                label { "Border Color" }
                input {
                    r#type: "color",
                    value: "{color_to_hex(&current_settings.dot_tracker.border_color)}",
                    class: "color-picker",
                    disabled: !(current_settings.dot_tracker.layout_bar && current_settings.dot_tracker.show_border),
                    oninput: move |e: Event<FormData>| {
                        if let Some(color) = parse_hex_color(&e.value()) {
                            let mut new_settings = draft.current();
                            new_settings.dot_tracker.border_color = color;
                            draft.update(new_settings);
                        }
                    }
                }
            }

            div { class: "setting-row reset-row",
                button {
                    class: "btn btn-reset",
                    onclick: move |_| {
                        let mut new_settings = draft.current();
                        new_settings.dot_tracker = DotTrackerConfig::default();
                        new_settings.dot_tracker_opacity = 180;
                        draft.update(new_settings);
                    },
                    i { class: "fa-solid fa-rotate-left" }
                    span { " Reset to Defaults" }
                }
                {actions}
            }
        }
    }
}
