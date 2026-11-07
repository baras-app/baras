//! Personal stats overlay settings tab.

use dioxus::prelude::*;

use crate::components::Slider;
use crate::types::{PersonalOverlayConfig, PersonalStat};
use crate::utils::{color_to_hex, parse_hex_color};

use super::draft::SettingsDraft;
use super::widgets::OpacitySlider;

#[component]
pub fn PersonalTab(draft: SettingsDraft, actions: Element) -> Element {
    let current_settings = draft.current();
    let personal_font_color_hex = color_to_hex(&current_settings.personal_overlay.font_color);
    let personal_label_font_color_hex = color_to_hex(&current_settings.personal_overlay.label_color);

    rsx! {
        // Personal Settings
        {
            let visible_stats = current_settings.personal_overlay.visible_stats.clone();
            let stat_count = visible_stats.len();

            rsx! {
                div { class: "settings-section",
                    p { class: "hint", "Displayed stats:" }

                    div { class: "stat-order-list",
                        for (idx, stat) in visible_stats.into_iter().enumerate() {
                            div { class: "stat-order-item", key: "{idx}",
                                span { class: "stat-name", "{stat.label()}" }
                                div { class: "stat-controls",
                                    button {
                                        class: "btn-order",
                                        disabled: idx == 0,
                                        onclick: move |_| {
                                            let mut new_settings = draft.current();
                                            let stats = &mut new_settings.personal_overlay.visible_stats;
                                            if idx > 0 { stats.swap(idx, idx - 1); }
                                            draft.update(new_settings);
                                        },
                                        "▲"
                                    }
                                    button {
                                        class: "btn-order",
                                        disabled: idx >= stat_count - 1,
                                        onclick: move |_| {
                                            let mut new_settings = draft.current();
                                            let stats = &mut new_settings.personal_overlay.visible_stats;
                                            if idx < stats.len() - 1 { stats.swap(idx, idx + 1); }
                                            draft.update(new_settings);
                                        },
                                        "▼"
                                    }
                                    button {
                                        class: "btn-remove",
                                        onclick: move |_| {
                                            let mut new_settings = draft.current();
                                            let stats = &mut new_settings.personal_overlay.visible_stats;
                                            if idx < stats.len() { stats.remove(idx); }
                                            draft.update(new_settings);
                                        },
                                        "✕"
                                    }
                                }
                            }
                        }
                    }

                    // Add stats section
                    div { class: "stat-add-section",
                        p { class: "hint", "Add stats:" }
                        div { class: "stat-add-grid",
                            for stat in PersonalStat::all() {
                                {
                                    let stat = *stat;
                                    // Separator can always be added (multiple times)
                                    // Other stats can only be added once
                                    let is_visible = stat != PersonalStat::Separator
                                        && current_settings.personal_overlay.visible_stats.contains(&stat);
                                    if !is_visible {
                                        rsx! {
                                            button {
                                                class: "btn-add-stat",
                                                onclick: move |_| {
                                                    let mut new_settings = draft.current();
                                                    if stat == PersonalStat::Separator
                                                        || !new_settings.personal_overlay.visible_stats.contains(&stat)
                                                    {
                                                        new_settings.personal_overlay.visible_stats.push(stat);
                                                    }
                                                    draft.update(new_settings);
                                                },
                                                "+ {stat.label()}"
                                            }
                                        }
                                    } else {
                                        rsx! {}
                                    }
                                }
                            }
                        }
                    }

                    h4 { "Appearance" }

                    OpacitySlider {
                        label: "Background Opacity",
                        value: current_settings.personal_opacity,
                        on_change: move |val| {
                            let mut new_settings = draft.current();
                            new_settings.personal_opacity = val;
                            draft.update(new_settings);
                        },
                    }

                    div { class: "setting-row",
                        label { "Value Font Color" }
                        input {
                            r#type: "color",
                            value: "{personal_font_color_hex}",
                            class: "color-picker",
                            oninput: move |e: Event<FormData>| {
                                if let Some(color) = parse_hex_color(&e.value()) {
                                    let mut new_settings = draft.current();
                                    new_settings.personal_overlay.font_color = color;
                                    draft.update(new_settings);
                                }
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Label Font Color" }
                        input {
                            r#type: "color",
                            value: "{personal_label_font_color_hex}",
                            class: "color-picker",
                            oninput: move |e: Event<FormData>| {
                                if let Some(color) = parse_hex_color(&e.value()) {
                                    let mut new_settings = draft.current();
                                    new_settings.personal_overlay.label_color = color;
                                    draft.update(new_settings);
                                }
                            }
                        }
                    }

            Slider {
                label: "Font Scale",
                value: (current_settings.personal_overlay.font_scale * 100.0) as i32 as f64,
                min: 30.0,
                max: 300.0,
                step: 10.0,
                suffix: "%",
                on_change: move |v: f64| {
                    let mut new_settings = draft.current();
                    new_settings.personal_overlay.font_scale = (v as f32 / 100.0).clamp(0.3, 3.0);
                    draft.update(new_settings);
                },
            }
            div { class: "setting-row",
                label { "Dynamic Background" }
                input {
                    r#type: "checkbox",
                    checked: current_settings.personal_overlay.dynamic_background,
                    onchange: move |e: Event<FormData>| {
                        let mut new_settings = draft.current();
                        new_settings.personal_overlay.dynamic_background = e.checked();
                        draft.update(new_settings);
                    }
                }
            }

            div { class: "setting-row",
                label { "Auto-color Values" }
                input {
                    r#type: "checkbox",
                    checked: current_settings.personal_overlay.auto_color_values,
                    onchange: move |e: Event<FormData>| {
                        let mut new_settings = draft.current();
                        new_settings.personal_overlay.auto_color_values = e.checked();
                        draft.update(new_settings);
                    }
                }
            }

            div { class: "setting-row",
                label { "Hide Empty Values" }
                input {
                    r#type: "checkbox",
                    checked: current_settings.personal_overlay.hide_empty_values,
                    onchange: move |e: Event<FormData>| {
                        let mut new_settings = draft.current();
                        new_settings.personal_overlay.hide_empty_values = e.checked();
                        draft.update(new_settings);
                    }
                }
            }

            Slider {
                label: "Line Spacing",
                value: (current_settings.personal_overlay.line_spacing * 100.0) as i32 as f64,
                min: 70.0,
                max: 150.0,
                step: 10.0,
                suffix: "%",
                on_change: move |v: f64| {
                    let mut new_settings = draft.current();
                    new_settings.personal_overlay.line_spacing = (v as f32 / 100.0).clamp(0.7, 1.5);
                    draft.update(new_settings);
                },
            }

                    div { class: "setting-row reset-row",
                        button {
                            class: "btn btn-reset",
                            onclick: move |_| {
                                let mut new_settings = draft.current();
                                new_settings.personal_overlay = PersonalOverlayConfig::default();
                                new_settings.personal_opacity = 180;
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
    }
}
