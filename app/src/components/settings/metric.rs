//! Settings tab shared by every metric overlay (DPS, HPS, ...), keyed by its config key.

use dioxus::prelude::*;

use crate::components::{NumberInput, Slider};
use crate::utils::{color_to_hex, parse_hex_color};

use super::draft::{SettingsDraft, appearance_for};

#[component]
pub fn MetricTab(draft: SettingsDraft, tab_key: String, actions: Element) -> Element {
    let current_settings = draft.current();
    let tab = tab_key;
    let current_appearance = appearance_for(&current_settings, &tab);
    let bar_color_hex = color_to_hex(&current_appearance.bar_color);
    let font_color_hex = color_to_hex(&current_appearance.font_color);

    rsx! {
        // Metric Settings (default tab content)
        {
            let tab_key = tab.clone();
            rsx! {
                div { class: "settings-section",
                    label { class: "settings-check",
                        input {
                            r#type: "checkbox",
                            checked: current_appearance.show_per_second,
                            onchange: {
                                let tab = tab_key.clone();
                                move |e: Event<FormData>| {
                                    let mut new_settings = draft.current();
                                    let default = new_settings.default_appearances.get(&tab).cloned().unwrap_or_default();
                                    let appearance = new_settings.appearances.entry(tab.clone()).or_insert(default);
                                    appearance.show_per_second = e.checked();
                                    draft.update(new_settings);
                                }
                            }
                        }
                        "Show Per-Second"
                    }

                    label { class: "settings-check",
                        input {
                            r#type: "checkbox",
                            checked: current_appearance.show_total,
                            onchange: {
                                let tab = tab_key.clone();
                                move |e: Event<FormData>| {
                                    let mut new_settings = draft.current();
                                    let default = new_settings.default_appearances.get(&tab).cloned().unwrap_or_default();
                                    let appearance = new_settings.appearances.entry(tab.clone()).or_insert(default);
                                    appearance.show_total = e.checked();
                                    draft.update(new_settings);
                                }
                            }
                        }
                        "Show Total"
                    }

                    label { class: "settings-check",
                        input {
                            r#type: "checkbox",
                            checked: current_appearance.show_header,
                            onchange: {
                                let tab = tab_key.clone();
                                move |e: Event<FormData>| {
                                    let mut new_settings = draft.current();
                                    let default = new_settings.default_appearances.get(&tab).cloned().unwrap_or_default();
                                    let appearance = new_settings.appearances.entry(tab.clone()).or_insert(default);
                                    appearance.show_header = e.checked();
                                    draft.update(new_settings);
                                }
                            }
                        }
                        "Show Header"
                    }

                    label { class: "settings-check",
                        input {
                            r#type: "checkbox",
                            checked: current_appearance.show_footer,
                            onchange: {
                                let tab = tab_key.clone();
                                move |e: Event<FormData>| {
                                    let mut new_settings = draft.current();
                                    let default = new_settings.default_appearances.get(&tab).cloned().unwrap_or_default();
                                    let appearance = new_settings.appearances.entry(tab.clone()).or_insert(default);
                                    appearance.show_footer = e.checked();
                                    draft.update(new_settings);
                                }
                            }
                        }
                        "Show Footer"
                    }

                    div { class: "setting-row",
                        label { "Max Entries" }
                        NumberInput {
                            value: current_appearance.max_entries as f64,
                            min: 1.0,
                            max: 16.0,
                            on_change: {
                                let tab = tab_key.clone();
                                move |v: f64| {
                                    let mut new_settings = draft.current();
                                    let default = new_settings.default_appearances.get(&tab).cloned().unwrap_or_default();
                                    let appearance = new_settings.appearances.entry(tab.clone()).or_insert(default);
                                    appearance.max_entries = v as u8;
                                    draft.update(new_settings);
                                }
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Bar Color" }
                        input {
                            r#type: "color",
                            value: "{bar_color_hex}",
                            class: "color-picker",
                            oninput: {
                                let tab = tab_key.clone();
                                move |e: Event<FormData>| {
                                    if let Some(color) = parse_hex_color(&e.value()) {
                                        let mut new_settings = draft.current();
                                        let default = new_settings.default_appearances.get(&tab).cloned().unwrap_or_default();
                                        let appearance = new_settings.appearances.entry(tab.clone()).or_insert(default);
                                        appearance.bar_color = color;
                                        draft.update(new_settings);
                                    }
                                }
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Font Color" }
                        input {
                            r#type: "color",
                            value: "{font_color_hex}",
                            class: "color-picker",
                            oninput: {
                                let tab = tab_key.clone();
                                move |e: Event<FormData>| {
                                    if let Some(color) = parse_hex_color(&e.value()) {
                                        let mut new_settings = draft.current();
                                        let default = new_settings.default_appearances.get(&tab).cloned().unwrap_or_default();
                                        let appearance = new_settings.appearances.entry(tab.clone()).or_insert(default);
                                        appearance.font_color = color;
                                        draft.update(new_settings);
                                    }
                                }
                            }
                        }
                    }

                    label { class: "settings-check",
                        input {
                            r#type: "checkbox",
                            checked: current_appearance.use_class_color,
                            onchange: {
                                let tab = tab_key.clone();
                                move |e: Event<FormData>| {
                                    let mut new_settings = draft.current();
                                    let default = new_settings.default_appearances.get(&tab).cloned().unwrap_or_default();
                                    let appearance = new_settings.appearances.entry(tab.clone()).or_insert(default);
                                    appearance.use_class_color = e.checked();
                                    draft.update(new_settings);
                                }
                            }
                        }
                        "Color Bars by Class"
                    }

                    label { class: "settings-check",
                        input {
                            r#type: "checkbox",
                            checked: current_appearance.bar_gradient,
                            onchange: {
                                let tab = tab_key.clone();
                                move |e: Event<FormData>| {
                                    let mut new_settings = draft.current();
                                    let default = new_settings.default_appearances.get(&tab).cloned().unwrap_or_default();
                                    let appearance = new_settings.appearances.entry(tab.clone()).or_insert(default);
                                    appearance.bar_gradient = e.checked();
                                    draft.update(new_settings);
                                }
                            }
                        }
                        "Gradient Bars"
                    }

                    Slider {
                        label: "Bar Spacing",
                        value: (current_appearance.bar_spacing_ratio * 100.0) as i32 as f64,
                        min: 0.0,
                        max: 60.0,
                        step: 5.0,
                        suffix: "%",
                        on_change: {
                            let tab = tab_key.clone();
                            move |v: f64| {
                                let mut new_settings = draft.current();
                                let default = new_settings.default_appearances.get(&tab).cloned().unwrap_or_default();
                                let appearance = new_settings.appearances.entry(tab.clone()).or_insert(default);
                                appearance.bar_spacing_ratio = (v as f32 / 100.0).clamp(0.0, 0.6);
                                draft.update(new_settings);
                            }
                        },
                    }

                    // Font Scale and Dynamic Background are in Global Metrics Settings

                    div { class: "setting-row reset-row",
                        button {
                            class: "btn btn-reset",
                            onclick: {
                                let tab = tab_key.clone();
                                move |_| {
                                    let mut new_settings = draft.current();
                                    let default_appearance = new_settings.default_appearances
                                        .get(&tab)
                                        .cloned()
                                        .unwrap_or_default();
                                    new_settings.appearances.insert(tab.clone(), default_appearance);
                                    draft.update(new_settings);
                                }
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
