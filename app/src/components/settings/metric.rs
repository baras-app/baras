//! Settings tab shared by every metric overlay (DPS, HPS, ...), keyed by its config key.
//! Options shared by all of them (bar sizing, font scale, ...) are on the Global tab and
//! overridden per overlay in `overrides.rs`.

use dioxus::prelude::*;

use crate::types::OverlayAppearanceConfig;
use baras_types::metric_template::{MetricVar, PRESETS};

use super::draft::{SettingsDraft, appearance_for, edit_appearance};
use super::widgets::{Check, ColorRow, Group, NumberRow, ResetRow, Row, Section, SelectRow};

/// Preset dropdown value when the template matches none of `PRESETS`
const CUSTOM: &str = "custom";

#[component]
pub fn MetricTab(draft: SettingsDraft, tab_key: String, actions: Element) -> Element {
    let settings = draft.current();
    let a = appearance_for(&settings, &tab_key);
    let check = |f: fn(&mut OverlayAppearanceConfig, bool)| {
        let tab = tab_key.clone();
        move |v: bool| edit_appearance(draft, &tab, |a| f(a, v))
    };
    let color = |f: fn(&mut OverlayAppearanceConfig, crate::types::Color)| {
        let tab = tab_key.clone();
        move |v| edit_appearance(draft, &tab, |a| f(a, v))
    };
    let (tab_max, tab_reset) = (tab_key.clone(), tab_key.clone());
    let (tab_template, tab_preset) = (tab_key.clone(), tab_key.clone());
    let preset = PRESETS
        .iter()
        .find(|(t, _)| *t == a.template)
        .map_or(CUSTOM, |(t, _)| *t);
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
                // The global overrides are not this tab's style, so they survive
                on_reset: move |_| draft.edit(|s| {
                    let mut default = s.default_appearances.get(&tab_reset).cloned().unwrap_or_default();
                    if let Some(current) = s.appearances.get(&tab_reset) {
                        default.overrides = current.overrides;
                        default.bar_spacing = current.bar_spacing;
                        default.bar_radius = current.bar_radius;
                    }
                    s.appearances.insert(tab_reset.clone(), default);
                }),
                actions,
            }
        }
    }
}
