//! Boss health overlay settings tab.

use dioxus::prelude::*;

use crate::components::Slider;
use crate::types::{BossHealthConfig, IconPosition};
use crate::utils::{color_to_hex, parse_hex_color};

use super::draft::SettingsDraft;
use super::widgets::OpacitySlider;

#[component]
pub fn BossHealthTab(draft: SettingsDraft, actions: Element) -> Element {
    let current_settings = draft.current();
    let boss_bar_hex = color_to_hex(&current_settings.boss_health.bar_color);

    rsx! {
        // Boss Health Settings
        div { class: "settings-section",
            h4 { "Appearance" }

            OpacitySlider {
                label: "Background Opacity",
                value: current_settings.boss_health_opacity,
                on_change: move |val| {
                    let mut new_settings = draft.current();
                    new_settings.boss_health_opacity = val;
                    draft.update(new_settings);
                },
            }

            div { class: "setting-row",
                label { "Bar Color" }
                input {
                    r#type: "color",
                    value: "{boss_bar_hex}",
                    class: "color-picker",
                    oninput: move |e: Event<FormData>| {
                        if let Some(color) = parse_hex_color(&e.value()) {
                            let mut new_settings = draft.current();
                            new_settings.boss_health.bar_color = color;
                            draft.update(new_settings);
                        }
                    }
                }
            }

                    div { class: "setting-row",
                        label { "Show current target" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.boss_health.show_target,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft.current();
                                new_settings.boss_health.show_target = e.checked();
                                draft.update(new_settings);
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Show HP value" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.boss_health.show_hp_value,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft.current();
                                new_settings.boss_health.show_hp_value = e.checked();
                                draft.update(new_settings);
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Show HP markers" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.boss_health.show_hp_markers,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft.current();
                                new_settings.boss_health.show_hp_markers = e.checked();
                                draft.update(new_settings);
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Show shields" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.boss_health.show_shield,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft.current();
                                new_settings.boss_health.show_shield = e.checked();
                                draft.update(new_settings);
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Show effect icons" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.boss_health.show_icons,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft.current();
                                new_settings.boss_health.show_icons = e.checked();
                                draft.update(new_settings);
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Icon Position" }
                        select {
                            class: "input-inline",
                            disabled: !current_settings.boss_health.show_icons,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft.current();
                                new_settings.boss_health.icon_position = match e.value().as_str() {
                                    "top" => IconPosition::Top,
                                    "left" => IconPosition::Left,
                                    "right" => IconPosition::Right,
                                    _ => IconPosition::Bottom,
                                };
                                draft.update(new_settings);
                            },
                            option { value: "top", selected: current_settings.boss_health.icon_position == IconPosition::Top, "Top" }
                            option { value: "bottom", selected: current_settings.boss_health.icon_position == IconPosition::Bottom, "Bottom" }
                            option { value: "left", selected: current_settings.boss_health.icon_position == IconPosition::Left, "Left (4 slots)" }
                            option { value: "right", selected: current_settings.boss_health.icon_position == IconPosition::Right, "Right (4 slots)" }
                        }
                    }

            Slider {
                label: "Font Scale",
                value: (current_settings.boss_health.font_scale * 100.0) as i32 as f64,
                min: 30.0,
                max: 300.0,
                step: 10.0,
                suffix: "%",
                on_change: move |v: f64| {
                    let mut new_settings = draft.current();
                    new_settings.boss_health.font_scale = (v as f32 / 100.0).clamp(0.3, 3.0);
                    draft.update(new_settings);
                },
            }
            Slider {
                label: "Lower Bar Scale",
                value: (current_settings.boss_health.lower_bar_scale * 100.0) as i32 as f64,
                min: 50.0,
                max: 200.0,
                step: 10.0,
                suffix: "%",
                on_change: move |v: f64| {
                    let mut new_settings = draft.current();
                    new_settings.boss_health.lower_bar_scale = (v as f32 / 100.0).clamp(0.5, 2.0);
                    draft.update(new_settings);
                },
            }
            Slider {
                label: "Icon Scale",
                value: (current_settings.boss_health.icon_scale * 100.0) as i32 as f64,
                min: 50.0,
                max: 200.0,
                step: 10.0,
                suffix: "%",
                disabled: !current_settings.boss_health.show_icons,
                on_change: move |v: f64| {
                    let mut new_settings = draft.current();
                    new_settings.boss_health.icon_scale = (v as f32 / 100.0).clamp(0.5, 2.0);
                    draft.update(new_settings);
                },
            }
            Slider {
                label: "Bosses Sized to Fit",
                tooltip: "The number of entries to reserve space for in the overlay window. If this number is exceeded, the entries will be compressed.",
                value: current_settings.boss_health.visible_bosses as f64,
                min: 1.0,
                max: 7.0,
                step: 1.0,
                on_change: move |v: f64| {
                    let mut new_settings = draft.current();
                    new_settings.boss_health.visible_bosses = (v as u32).clamp(1, 7);
                    draft.update(new_settings);
                },
            }
            div { class: "setting-row",
                label { "Dynamic Background" }
                input {
                    r#type: "checkbox",
                    checked: current_settings.boss_health.dynamic_background,
                    onchange: move |e: Event<FormData>| {
                        let mut new_settings = draft.current();
                        new_settings.boss_health.dynamic_background = e.checked();
                        draft.update(new_settings);
                    }
                }
            }
            div { class: "setting-row",
                label { "Clear After Combat" }
                input {
                    r#type: "checkbox",
                    checked: current_settings.boss_health.clear_after_combat,
                    onchange: move |e: Event<FormData>| {
                        let mut new_settings = draft.current();
                        new_settings.boss_health.clear_after_combat = e.checked();
                        draft.update(new_settings);
                    }
                }
            }
            div { class: "setting-row",
                label { "Gradient Bars" }
                input {
                    r#type: "checkbox",
                    checked: current_settings.boss_health.bar_gradient,
                    onchange: move |e: Event<FormData>| {
                        let mut new_settings = draft.current();
                        new_settings.boss_health.bar_gradient = e.checked();
                        draft.update(new_settings);
                    }
                }
            }
            div { class: "setting-row",
                label { "Show Border" }
                input {
                    r#type: "checkbox",
                    checked: current_settings.boss_health.show_border,
                    onchange: move |e: Event<FormData>| {
                        let mut new_settings = draft.current();
                        new_settings.boss_health.show_border = e.checked();
                        draft.update(new_settings);
                    }
                }
            }
            div { class: "setting-row",
                label { "Border Color" }
                input {
                    r#type: "color",
                    value: "{color_to_hex(&current_settings.boss_health.border_color)}",
                    class: "color-picker",
                    disabled: !current_settings.boss_health.show_border,
                    oninput: move |e: Event<FormData>| {
                        if let Some(color) = parse_hex_color(&e.value()) {
                            let mut new_settings = draft.current();
                            new_settings.boss_health.border_color = color;
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
                        new_settings.boss_health = BossHealthConfig::default();
                        new_settings.boss_health_opacity = 180;
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
