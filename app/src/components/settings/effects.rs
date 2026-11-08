//! Effects overlay settings tab, shared by groups A, B and C.

use dioxus::prelude::*;

use crate::components::Slider;
use crate::types::{EffectsAConfig, OverlaySettings};
use crate::utils::{color_to_hex, parse_hex_color};

use super::draft::SettingsDraft;
use super::widgets::OpacitySlider;

/// Which of the effects overlays this tab edits
#[derive(Clone, Copy, PartialEq)]
pub enum EffectsGroup {
    A,
    B,
    C,
}

impl EffectsGroup {
    fn config(self, s: &OverlaySettings) -> &EffectsAConfig {
        match self {
            Self::A => &s.effects_a,
            Self::B => &s.effects_b,
            Self::C => &s.effects_c,
        }
    }

    fn config_mut(self, s: &mut OverlaySettings) -> &mut EffectsAConfig {
        match self {
            Self::A => &mut s.effects_a,
            Self::B => &mut s.effects_b,
            Self::C => &mut s.effects_c,
        }
    }

    fn opacity(self, s: &OverlaySettings) -> u8 {
        match self {
            Self::A => s.effects_a_opacity,
            Self::B => s.effects_b_opacity,
            Self::C => s.effects_c_opacity,
        }
    }

    fn opacity_mut(self, s: &mut OverlaySettings) -> &mut u8 {
        match self {
            Self::A => &mut s.effects_a_opacity,
            Self::B => &mut s.effects_b_opacity,
            Self::C => &mut s.effects_c_opacity,
        }
    }
}

#[component]
pub fn EffectsTab(draft: SettingsDraft, group: EffectsGroup, actions: Element) -> Element {
    let current_settings = draft.current();
    let cfg = group.config(&current_settings).clone();

    rsx! {
        // Effects Settings
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

            Slider {
                label: "Icon/Bar Size",
                value: cfg.icon_size as f64,
                min: 16.0,
                max: 128.0,
                suffix: "px",
                on_change: move |v: f64| {
                    let mut new_settings = draft.current();
                    group.config_mut(&mut new_settings).icon_size = (v as u8).clamp(16, 128);
                    draft.update(new_settings);
                },
            }

            div { class: "setting-row",
                label { "Max Displayed" }
                select {
                    class: "input-inline",
                    value: "{cfg.max_display}",
                    onchange: move |e: Event<FormData>| {
                        if let Ok(val) = e.value().parse::<u8>() {
                            let mut new_settings = draft.current();
                            group.config_mut(&mut new_settings).max_display = val.clamp(1, 16);
                            draft.update(new_settings);
                        }
                    },
                    for n in 1..=16u8 {
                        option { value: "{n}", selected: cfg.max_display == n, "{n}" }
                    }
                }
            }

            h4 { "Layout" }

            label { class: "settings-check",
                input {
                    r#type: "checkbox",
                    checked: cfg.layout_bar,
                    onchange: move |e: Event<FormData>| {
                        let mut new_settings = draft.current();
                        group.config_mut(&mut new_settings).layout_bar = e.checked();
                        draft.update(new_settings);
                    }
                }
                "Bar Mode"
            }

            label { class: "settings-check",
                input {
                    r#type: "checkbox",
                    checked: cfg.layout_vertical,
                    onchange: move |e: Event<FormData>| {
                        let mut new_settings = draft.current();
                        group.config_mut(&mut new_settings).layout_vertical = e.checked();
                        draft.update(new_settings);
                    }
                }
                "Vertical Layout"
            }

            h4 { "Display Options" }

            label { class: "settings-check",
                input {
                    r#type: "checkbox",
                    checked: cfg.show_header,
                    onchange: move |e: Event<FormData>| {
                        let mut new_settings = draft.current();
                        group.config_mut(&mut new_settings).show_header = e.checked();
                        draft.update(new_settings);
                    }
                }
                "Show Header"
            }

            label { class: "settings-check",
                input {
                    r#type: "checkbox",
                    checked: cfg.show_countdown,
                    onchange: move |e: Event<FormData>| {
                        let mut new_settings = draft.current();
                        group.config_mut(&mut new_settings).show_countdown = e.checked();
                        draft.update(new_settings);
                    }
                }
                "Show Countdown"
            }

            label { class: "settings-check",
                input {
                    r#type: "checkbox",
                    checked: cfg.show_effect_names,
                    onchange: move |e: Event<FormData>| {
                        let mut new_settings = draft.current();
                        group.config_mut(&mut new_settings).show_effect_names = e.checked();
                        draft.update(new_settings);
                    }
                }
                "Show Effect Names"
            }

            label { class: "settings-check",
                input {
                    r#type: "checkbox",
                    checked: cfg.stack_priority,
                    onchange: move |e: Event<FormData>| {
                        let mut new_settings = draft.current();
                        group.config_mut(&mut new_settings).stack_priority = e.checked();
                        draft.update(new_settings);
                    }
                }
                "Emphasize Effect Charges"
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
                    disabled: !cfg.layout_bar,
                    onchange: move |e: Event<FormData>| {
                        let mut new_settings = draft.current();
                        group.config_mut(&mut new_settings).bar_gradient = e.checked();
                        draft.update(new_settings);
                    }
                }
                "Gradient Bars (bar layout)"
            }
            label { class: "settings-check",
                input {
                    r#type: "checkbox",
                    checked: cfg.show_border,
                    disabled: !cfg.layout_bar,
                    onchange: move |e: Event<FormData>| {
                        let mut new_settings = draft.current();
                        group.config_mut(&mut new_settings).show_border = e.checked();
                        draft.update(new_settings);
                    }
                }
                "Show Border (bar layout)"
            }
            div { class: "setting-row",
                label { "Border Color" }
                input {
                    r#type: "color",
                    value: "{color_to_hex(&cfg.border_color)}",
                    class: "color-picker",
                    disabled: !(cfg.layout_bar && cfg.show_border),
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
                        *group.config_mut(&mut new_settings) = EffectsAConfig::default();
                        *group.opacity_mut(&mut new_settings) = 180;
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
