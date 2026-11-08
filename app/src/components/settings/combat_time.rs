//! Combat time overlay settings tab.

use dioxus::prelude::*;

use crate::components::Slider;
use crate::utils::{color_to_hex, parse_hex_color};

use super::draft::SettingsDraft;
use super::widgets::OpacitySlider;

#[component]
pub fn CombatTimeTab(draft: SettingsDraft, actions: Element) -> Element {
    let current_settings = draft.current();

    rsx! {
        // Combat Time Overlay Settings
        div { class: "settings-section",
            h4 { "Appearance" }

            OpacitySlider {
                label: "Background Opacity",
                value: current_settings.combat_time_opacity,
                on_change: move |val| {
                    let mut new_settings = draft.current();
                    new_settings.combat_time_opacity = val;
                    draft.update(new_settings);
                },
            }

            label { class: "settings-check",
                input {
                    r#type: "checkbox",
                    checked: current_settings.combat_time.show_title,
                    onchange: move |e: Event<FormData>| {
                        let mut new_settings = draft.current();
                        new_settings.combat_time.show_title = e.checked();
                        draft.update(new_settings);
                    }
                }
                "Show Title"
            }

            label { class: "settings-check",
                input {
                    r#type: "checkbox",
                    checked: current_settings.combat_time.dynamic_background,
                    onchange: move |e: Event<FormData>| {
                        let mut new_settings = draft.current();
                        new_settings.combat_time.dynamic_background = e.checked();
                        draft.update(new_settings);
                    }
                }
                "Dynamic Background"
            }

            label { class: "settings-check",
                input {
                    r#type: "checkbox",
                    checked: current_settings.combat_time.clear_after_combat,
                    onchange: move |e: Event<FormData>| {
                        let mut new_settings = draft.current();
                        new_settings.combat_time.clear_after_combat = e.checked();
                        draft.update(new_settings);
                    }
                }
                "Clear After Combat"
            }

            Slider {
                label: "Font Scale",
                value: (current_settings.combat_time.font_scale * 100.0) as i32 as f64,
                min: 30.0,
                max: 300.0,
                suffix: "%",
                on_change: move |v: f64| {
                    let mut new_settings = draft.current();
                    new_settings.combat_time.font_scale = (v as f32 / 100.0).clamp(0.3, 3.0);
                    draft.update(new_settings);
                },
            }

            div { class: "setting-row",
                label { "Font Color" }
                input {
                    r#type: "color",
                    value: "{color_to_hex(&current_settings.combat_time.font_color)}",
                    class: "color-picker",
                    oninput: move |e: Event<FormData>| {
                        if let Some(color) = parse_hex_color(&e.value()) {
                            let mut new_settings = draft.current();
                            new_settings.combat_time.font_color = color;
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
                        new_settings.combat_time = Default::default();
                        new_settings.combat_time_opacity = 180;
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
