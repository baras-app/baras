//! Alerts overlay settings tab.

use dioxus::prelude::*;

use crate::components::Slider;
use crate::types::AlertsOverlayConfig;

use super::draft::SettingsDraft;
use super::widgets::OpacitySlider;

#[component]
pub fn AlertsTab(draft: SettingsDraft, actions: Element) -> Element {
    let current_settings = draft.current();

    rsx! {
        // Alerts Settings
        div { class: "settings-section",
            h4 { "Appearance" }

            OpacitySlider {
                label: "Background Opacity",
                value: current_settings.alerts_opacity,
                on_change: move |val| {
                    let mut new_settings = draft.current();
                    new_settings.alerts_opacity = val;
                    draft.update(new_settings);
                },
            }

            Slider {
                label: "Font Size",
                value: current_settings.alerts_overlay.font_size as f64,
                min: 8.0,
                max: 24.0,
                suffix: "px",
                on_change: move |v: f64| {
                    let mut new_settings = draft.current();
                    new_settings.alerts_overlay.font_size = (v as u8).clamp(8, 24);
                    draft.update(new_settings);
                },
            }

            h4 { "Display" }

            label { class: "settings-check",
                input {
                    r#type: "checkbox",
                    checked: current_settings.alerts_overlay.show_icons,
                    onchange: move |e: Event<FormData>| {
                        let mut new_settings = draft.current();
                        new_settings.alerts_overlay.show_icons = e.checked();
                        draft.update(new_settings);
                    }
                }
                "Show Icons"
            }

            label { class: "settings-check",
                input {
                    r#type: "checkbox",
                    checked: current_settings.alerts_overlay.dynamic_background,
                    onchange: move |e: Event<FormData>| {
                        let mut new_settings = draft.current();
                        new_settings.alerts_overlay.dynamic_background = e.checked();
                        draft.update(new_settings);
                    }
                }
                "Dynamic Background"
            }

            div { class: "setting-row",
                label { "Max Displayed" }
                select {
                    class: "input-inline",
                    value: "{current_settings.alerts_overlay.max_display}",
                    onchange: move |e: Event<FormData>| {
                        if let Ok(val) = e.value().parse::<u8>() {
                            let mut new_settings = draft.current();
                            new_settings.alerts_overlay.max_display = val.clamp(1, 10);
                            draft.update(new_settings);
                        }
                    },
                    for n in 1..=10u8 {
                        option { value: "{n}", selected: current_settings.alerts_overlay.max_display == n, "{n}" }
                    }
                }
            }

            h4 { "Timing" }

            Slider {
                label: "Display Duration",
                value: current_settings.alerts_overlay.default_duration as f64,
                min: 1.0,
                max: 15.0,
                step: 0.5,
                suffix: "s",
                on_change: move |v: f64| {
                    let mut new_settings = draft.current();
                    new_settings.alerts_overlay.default_duration = (v as f32).clamp(1.0, 15.0);
                    draft.update(new_settings);
                },
            }

            Slider {
                label: "Fade Duration",
                value: current_settings.alerts_overlay.fade_duration as f64,
                min: 0.0,
                max: 3.0,
                step: 0.5,
                suffix: "s",
                on_change: move |v: f64| {
                    let mut new_settings = draft.current();
                    new_settings.alerts_overlay.fade_duration = (v as f32).clamp(0.0, 3.0);
                    draft.update(new_settings);
                },
            }

            div { class: "setting-row reset-row",
                button {
                    class: "btn btn-reset",
                    onclick: move |_| {
                        let mut new_settings = draft.current();
                        new_settings.alerts_overlay = AlertsOverlayConfig::default();
                        new_settings.alerts_opacity = 180;
                        draft.update(new_settings);
                    },
                    i { class: "fa-solid fa-rotate-left" }
                    span { " Reset to Defaults" }
                }
                {actions}
            }

            // Hint about per-alert settings
            p { class: "hint",
                i { class: "fa-solid fa-info-circle" }
                " Per-alert colors are set on each alert timer in the Encounter Editor."
            }
        }
    }
}
