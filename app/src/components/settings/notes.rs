//! Notes overlay settings tab.

use dioxus::prelude::*;

use crate::components::Slider;
use crate::utils::{color_to_hex, parse_hex_color};

use super::draft::SettingsDraft;
use super::widgets::OpacitySlider;

#[component]
pub fn NotesTab(draft: SettingsDraft, actions: Element) -> Element {
    let current_settings = draft.current();

    rsx! {
        // Notes Overlay Settings
        div { class: "settings-section",
            h4 { "Appearance" }

            OpacitySlider {
                label: "Background Opacity",
                value: current_settings.notes_opacity,
                on_change: move |val| {
                    let mut new_settings = draft.current();
                    new_settings.notes_opacity = val;
                    draft.update(new_settings);
                },
            }

            label { class: "settings-check",
                input {
                    r#type: "checkbox",
                    checked: current_settings.notes_overlay.dynamic_background,
                    onchange: move |e: Event<FormData>| {
                        let mut new_settings = draft.current();
                        new_settings.notes_overlay.dynamic_background = e.checked();
                        draft.update(new_settings);
                    }
                }
                "Dynamic Background"
            }

            Slider {
                label: "Font Size",
                value: current_settings.notes_overlay.font_size as f64,
                min: 6.0,
                max: 24.0,
                suffix: "px",
                on_change: move |v: f64| {
                    let mut new_settings = draft.current();
                    new_settings.notes_overlay.font_size = (v as u8).clamp(6, 24);
                    draft.update(new_settings);
                },
            }

            div { class: "setting-row",
                label { "Font Color" }
                input {
                    r#type: "color",
                    value: "{color_to_hex(&current_settings.notes_overlay.font_color)}",
                    class: "color-picker",
                    oninput: move |e: Event<FormData>| {
                        if let Some(color) = parse_hex_color(&e.value()) {
                            let mut new_settings = draft.current();
                            new_settings.notes_overlay.font_color = color;
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
                        new_settings.notes_overlay = Default::default();
                        new_settings.notes_opacity = 180;
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
