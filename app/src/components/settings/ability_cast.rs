//! Abilities cast overlay settings tab.

use dioxus::prelude::*;

use crate::components::Slider;
use crate::utils::{color_to_hex, parse_hex_color};

use super::draft::SettingsDraft;
use super::widgets::OpacitySlider;

#[component]
pub fn AbilityCastTab(draft: SettingsDraft, actions: Element) -> Element {
    let current_settings = draft.current();

    rsx! {
        // Abilities Cast Overlay Settings
        div { class: "settings-section",
            h4 { "Appearance" }

            OpacitySlider {
                label: "Background Opacity",
                value: current_settings.ability_cast_opacity,
                on_change: move |val| {
                    let mut new_settings = draft.current();
                    new_settings.ability_cast_opacity = val;
                    draft.update(new_settings);
                },
            }

            label { class: "settings-check",
                input {
                    r#type: "checkbox",
                    checked: current_settings.ability_cast.dynamic_background,
                    onchange: move |e: Event<FormData>| {
                        let mut new_settings = draft.current();
                        new_settings.ability_cast.dynamic_background = e.checked();
                        draft.update(new_settings);
                    }
                }
                "Dynamic Background"
            }

            label { class: "settings-check",
                input {
                    r#type: "checkbox",
                    checked: current_settings.ability_cast.stack_from_bottom,
                    onchange: move |e: Event<FormData>| {
                        let mut new_settings = draft.current();
                        new_settings.ability_cast.stack_from_bottom = e.checked();
                        draft.update(new_settings);
                    }
                }
                "Stack From Bottom"
            }

            Slider {
                label: "Font Scale",
                value: (current_settings.ability_cast.font_scale * 100.0) as i32 as f64,
                min: 30.0,
                max: 300.0,
                suffix: "%",
                on_change: move |v: f64| {
                    let mut new_settings = draft.current();
                    new_settings.ability_cast.font_scale = (v as f32 / 100.0).clamp(0.3, 3.0);
                    draft.update(new_settings);
                },
            }

            Slider {
                label: "Icon Size",
                value: current_settings.ability_cast.icon_size as f64,
                min: 16.0,
                max: 128.0,
                suffix: "px",
                on_change: move |v: f64| {
                    let mut new_settings = draft.current();
                    new_settings.ability_cast.icon_size = (v as u8).clamp(16, 128);
                    draft.update(new_settings);
                },
            }

            div { class: "setting-row",
                label { "Font Color" }
                input {
                    r#type: "color",
                    value: "{color_to_hex(&current_settings.ability_cast.font_color)}",
                    class: "color-picker",
                    oninput: move |e: Event<FormData>| {
                        if let Some(color) = parse_hex_color(&e.value()) {
                            let mut new_settings = draft.current();
                            new_settings.ability_cast.font_color = color;
                            draft.update(new_settings);
                        }
                    }
                }
            }
        }

        div { class: "settings-section",
            h4 { "Display" }

            Slider {
                label: "Max Entries",
                value: current_settings.ability_cast.max_display as f64,
                min: 1.0,
                max: 30.0,
                on_change: move |v: f64| {
                    let mut new_settings = draft.current();
                    new_settings.ability_cast.max_display = (v as u8).clamp(1, 30);
                    draft.update(new_settings);
                },
            }

            Slider {
                label: "Prune After",
                value: current_settings.ability_cast.prune_secs as f64,
                min: 1.0,
                max: 60.0,
                suffix: "s",
                tooltip: "Casts older than this are removed from the list",
                on_change: move |v: f64| {
                    let mut new_settings = draft.current();
                    new_settings.ability_cast.prune_secs = (v as f32).clamp(1.0, 60.0);
                    draft.update(new_settings);
                },
            }

            div { class: "setting-row reset-row",
                button {
                    class: "btn btn-reset",
                    onclick: move |_| {
                        let mut new_settings = draft.current();
                        new_settings.ability_cast = Default::default();
                        new_settings.ability_cast_opacity = 180;
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
