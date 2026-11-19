//! Settings tab shared by every metric overlay (DPS, HPS, ...), keyed by its config key.
//! Font Scale, Dynamic Background and the default Bar Spacing live in Global Metrics Settings.

use dioxus::prelude::*;

use crate::components::Slider;
use crate::types::OverlayAppearanceConfig;
use baras_types::metric_template::{MetricVar, PRESETS};

use super::draft::{SettingsDraft, appearance_for};
use super::widgets::{Check, ColorRow, Group, NumberRow, ResetRow, Row, Section, SelectRow};

/// Preset dropdown value when the template matches none of `PRESETS`
const CUSTOM: &str = "custom";

/// Edit this overlay's appearance, seeding it from its default on first change
fn edit_appearance(draft: SettingsDraft, tab: &str, f: impl FnOnce(&mut OverlayAppearanceConfig)) {
    draft.edit(|s| {
        let default = s.default_appearances.get(tab).cloned().unwrap_or_default();
        f(s.appearances.entry(tab.to_string()).or_insert(default));
    });
}

#[component]
pub fn MetricTab(draft: SettingsDraft, tab_key: String, actions: Element) -> Element {
    let settings = draft.current();
    let a = appearance_for(&settings, &tab_key);
    let global_spacing = settings.metric_bar_spacing;
    let check = |f: fn(&mut OverlayAppearanceConfig, bool)| {
        let tab = tab_key.clone();
        move |v: bool| edit_appearance(draft, &tab, |a| f(a, v))
    };
    let color = |f: fn(&mut OverlayAppearanceConfig, crate::types::Color)| {
        let tab = tab_key.clone();
        move |v| edit_appearance(draft, &tab, |a| f(a, v))
    };
    let (tab_max, tab_spacing, tab_custom_spacing, tab_reset) =
        (tab_key.clone(), tab_key.clone(), tab_key.clone(), tab_key.clone());
    let (tab_template, tab_preset) = (tab_key.clone(), tab_key.clone());
    let preset = PRESETS.iter().find(|(t, _)| *t == a.template).map_or(CUSTOM, |(t, _)| *t);
    let tab_insert = tab_key.clone();

    rsx! {
        div { class: "settings-section",
            Section { group: Group::Background,
                ColorRow { label: "Bar Color", value: a.bar_color, on_change: color(|a, v| a.bar_color = v) }
                Check { label: "Color Bars by Class", checked: a.use_class_color, on_change: check(|a, v| a.use_class_color = v) }
                Check { label: "Gradient Bars", checked: a.bar_gradient, on_change: check(|a, v| a.bar_gradient = v) }
            }
            Section { group: Group::Layout,
                NumberRow {
                    label: "Max Displayed",
                    value: a.max_entries as f64,
                    min: 1.0,
                    max: 16.0,
                    on_change: move |v: f64| edit_appearance(draft, &tab_max, |a| a.max_entries = v as u8),
                }
                Check {
                    label: "Custom Bar Spacing",
                    checked: a.bar_spacing.is_some(),
                    on_change: move |on: bool| edit_appearance(draft, &tab_custom_spacing, |a| a.bar_spacing = on.then_some(global_spacing)),
                }
                if let Some(spacing) = a.bar_spacing {
                    Slider {
                        label: "Bar Spacing",
                        value: (spacing * 100.0).round() as f64,
                        min: 0.0,
                        max: 60.0,
                        step: 5.0,
                        suffix: "%",
                        on_change: move |v: f64| edit_appearance(draft, &tab_spacing, |a| a.bar_spacing = Some((v as f32 / 100.0).clamp(0.0, 0.6))),
                    }
                }
            }
            Section { group: Group::Content,
                Row {
                    label: "Values",
                    tooltip: "Text shown beside each name. Placeholders in braces are replaced with values; a | splits it into a centre and a right column.",
                    input {
                        r#type: "text",
                        class: "template-input",
                        value: "{a.template}",
                        oninput: move |e: Event<FormData>| edit_appearance(draft, &tab_template, |a| a.template = e.value()),
                    }
                }
                SelectRow {
                    label: "Preset",
                    value: preset.to_string(),
                    options: PRESETS.iter().copied().chain([(CUSTOM, "Custom")]).collect::<Vec<_>>(),
                    on_change: move |v: String| {
                        if v != CUSTOM {
                            edit_appearance(draft, &tab_preset, |a| a.template = v);
                        }
                    },
                }
                details { class: "collapsible",
                    summary { class: "collapsible-summary", "Placeholders" }
                    p { class: "hint", "Click one to add it to Values. Example: {{total}} | {{rate}} ({{ehps_pct}})" }
                    for (group, vars) in MetricVar::GROUPS {
                        h5 { class: "placeholder-group", "{group}" }
                        div { class: "placeholder-list",
                            for (name, _, description) in vars.iter() {
                                button {
                                    class: "placeholder-name",
                                    title: "Add to Values",
                                    onclick: {
                                        let tab = tab_insert.clone();
                                        move |_| edit_appearance(draft, &tab, |a| {
                                            a.template.push_str(&format!("{{{name}}}"));
                                        })
                                    },
                                    "{{{name}}}"
                                }
                                span { class: "placeholder-description", "{description}" }
                            }
                        }
                    }
                }
                Check { label: "Show Header", checked: a.show_header, on_change: check(|a, v| a.show_header = v) }
                Check { label: "Show Footer", checked: a.show_footer, on_change: check(|a, v| a.show_footer = v) }
            }
            Section { group: Group::Text,
                ColorRow { label: "Font Color", value: a.font_color, on_change: color(|a, v| a.font_color = v) }
            }
            ResetRow {
                on_reset: move |_| draft.edit(|s| {
                    let default = s.default_appearances.get(&tab_reset).cloned().unwrap_or_default();
                    s.appearances.insert(tab_reset.clone(), default);
                }),
                actions,
            }
        }
    }
}
