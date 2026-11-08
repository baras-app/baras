//! Timer overlay settings tab, shared by groups A and B.

use dioxus::prelude::*;

use crate::components::Slider;
use crate::types::{OverlaySettings, TimerOverlayConfig};
use crate::utils::{color_to_hex, parse_hex_color};

use super::draft::SettingsDraft;
use super::widgets::OpacitySlider;

/// Which of the timer overlays this tab edits
#[derive(Clone, Copy, PartialEq)]
pub enum TimerGroup {
    A,
    B,
}

impl TimerGroup {
    fn config(self, s: &OverlaySettings) -> &TimerOverlayConfig {
        match self {
            Self::A => &s.timers_a_overlay,
            Self::B => &s.timers_b_overlay,
        }
    }

    fn config_mut(self, s: &mut OverlaySettings) -> &mut TimerOverlayConfig {
        match self {
            Self::A => &mut s.timers_a_overlay,
            Self::B => &mut s.timers_b_overlay,
        }
    }

    fn opacity(self, s: &OverlaySettings) -> u8 {
        match self {
            Self::A => s.timers_a_opacity,
            Self::B => s.timers_b_opacity,
        }
    }

    fn opacity_mut(self, s: &mut OverlaySettings) -> &mut u8 {
        match self {
            Self::A => &mut s.timers_a_opacity,
            Self::B => &mut s.timers_b_opacity,
        }
    }
}

#[component]
pub fn TimersTab(draft: SettingsDraft, group: TimerGroup, actions: Element) -> Element {
    let current_settings = draft.current();
    let cfg = group.config(&current_settings).clone();

    rsx! {
        // Timers Settings
        div { class: "settings-section",
            h4 { "Appearance" }

            OpacitySlider {
                label: "Background Opacity",
                value: group.opacity(&current_settings),
                on_change: move |val| {
                    let mut new_settings = draft.current();
                    *group.opacity_mut(&mut new_settings) = val;
                    draft.update(new_settings);
                },
            }

            div { class: "setting-row",
                label { "Font Color" }
                input {
                    r#type: "color",
                    value: "{color_to_hex(&cfg.font_color)}",
                    class: "color-picker",
                    oninput: move |e: Event<FormData>| {
                        if let Some(color) = parse_hex_color(&e.value()) {
                            let mut new_settings = draft.current();
                            group.config_mut(&mut new_settings).font_color = color;
                            draft.update(new_settings);
                        }
                    }
                }
            }

            Slider {
                label: "Font Scale",
                value: (cfg.font_scale * 100.0) as i32 as f64,
                min: 30.0,
                max: 300.0,
                step: 10.0,
                suffix: "%",
                on_change: move |v: f64| {
                    let mut new_settings = draft.current();
                    group.config_mut(&mut new_settings).font_scale = (v as f32 / 100.0).clamp(0.3, 3.0);
                    draft.update(new_settings);
                },
            }
            label { class: "settings-check",
                input {
                    r#type: "checkbox",
                    checked: cfg.dynamic_background,
                    onchange: move |e: Event<FormData>| {
                        let mut new_settings = draft.current();
                        group.config_mut(&mut new_settings).dynamic_background = e.checked();
                        draft.update(new_settings);
                    }
                }
                "Dynamic Background"
            }
            label { class: "settings-check",
                input {
                    r#type: "checkbox",
                    checked: cfg.stack_from_bottom,
                    onchange: move |e: Event<FormData>| {
                        let mut new_settings = draft.current();
                        group.config_mut(&mut new_settings).stack_from_bottom = e.checked();
                        draft.update(new_settings);
                    }
                }
                "Stack from Bottom"
            }
            label { class: "settings-check",
                input {
                    r#type: "checkbox",
                    checked: cfg.bar_gradient,
                    onchange: move |e: Event<FormData>| {
                        let mut new_settings = draft.current();
                        group.config_mut(&mut new_settings).bar_gradient = e.checked();
                        draft.update(new_settings);
                    }
                }
                "Gradient Bars"
            }
            label { class: "settings-check",
                input {
                    r#type: "checkbox",
                    checked: cfg.show_border,
                    onchange: move |e: Event<FormData>| {
                        let mut new_settings = draft.current();
                        group.config_mut(&mut new_settings).show_border = e.checked();
                        draft.update(new_settings);
                    }
                }
                "Show Border"
            }
            div { class: "setting-row",
                label { "Border Color" }
                input {
                    r#type: "color",
                    value: "{color_to_hex(&cfg.border_color)}",
                    class: "color-picker",
                    disabled: !cfg.show_border,
                    oninput: move |e: Event<FormData>| {
                        if let Some(color) = parse_hex_color(&e.value()) {
                            let mut new_settings = draft.current();
                            group.config_mut(&mut new_settings).border_color = color;
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
                        *group.config_mut(&mut new_settings) = TimerOverlayConfig::default();
                        *group.opacity_mut(&mut new_settings) = 180;
                        draft.update(new_settings);
                    },
                    i { class: "fa-solid fa-rotate-left" }
                    span { " Reset Style" }
                }
                {actions}
            }
        }
    }
}
