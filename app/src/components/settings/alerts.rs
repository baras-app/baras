//! Alerts overlay settings tab.

use dioxus::prelude::*;

use crate::components::Slider;
use crate::types::AlertsOverlayConfig;

use super::draft::SettingsDraft;
use super::widgets::{Check, DynamicBackground, Group, OpacitySlider, ResetRow, Row, Section};

#[component]
pub fn AlertsTab(draft: SettingsDraft, actions: Element) -> Element {
    let s = draft.current();
    let cfg = s.alerts_overlay.clone();

    rsx! {
        div { class: "settings-section",
            Section { group: Group::Background,
                OpacitySlider { value: s.alerts_opacity, on_change: move |v| draft.edit(|s| s.alerts_opacity = v) }
                DynamicBackground { checked: cfg.dynamic_background, on_change: move |v| draft.edit(|s| s.alerts_overlay.dynamic_background = v) }
            }
            Section { group: Group::Layout,
                Row { label: "Max Displayed",
                    select {
                        class: "input-inline",
                        value: "{cfg.max_display}",
                        onchange: move |e: Event<FormData>| {
                            if let Ok(val) = e.value().parse::<u8>() {
                                draft.edit(|s| s.alerts_overlay.max_display = val.clamp(1, 10));
                            }
                        },
                        for n in 1..=10u8 {
                            option { value: "{n}", selected: cfg.max_display == n, "{n}" }
                        }
                    }
                }
            }
            Section { group: Group::Content,
                Check { label: "Show Icons", checked: cfg.show_icons, on_change: move |v| draft.edit(|s| s.alerts_overlay.show_icons = v) }
                Slider {
                    label: "Display Duration",
                    value: cfg.default_duration as f64,
                    min: 1.0,
                    max: 15.0,
                    step: 0.5,
                    suffix: "s",
                    on_change: move |v: f64| draft.edit(|s| s.alerts_overlay.default_duration = (v as f32).clamp(1.0, 15.0)),
                }
                Slider {
                    label: "Fade Duration",
                    value: cfg.fade_duration as f64,
                    min: 0.0,
                    max: 3.0,
                    step: 0.5,
                    suffix: "s",
                    on_change: move |v: f64| draft.edit(|s| s.alerts_overlay.fade_duration = (v as f32).clamp(0.0, 3.0)),
                }
            }
            Section { group: Group::Text,
                Slider {
                    label: "Font Size",
                    value: cfg.font_size as f64,
                    min: 8.0,
                    max: 24.0,
                    suffix: "px",
                    on_change: move |v: f64| draft.edit(|s| s.alerts_overlay.font_size = (v as u8).clamp(8, 24)),
                }
            }
            ResetRow {
                on_reset: move |_| draft.edit(|s| {
                    s.alerts_overlay = AlertsOverlayConfig::default();
                    s.alerts_opacity = 180;
                }),
                actions,
            }
            p { class: "hint",
                i { class: "fa-solid fa-info-circle" }
                " Per-alert colors are set on each alert timer in the Encounter Editor."
            }
        }
    }
}
