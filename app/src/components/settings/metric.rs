//! Settings tab shared by every metric overlay (DPS, HPS, ...), keyed by its config key.
//! Font Scale and Dynamic Background live in Global Metrics Settings.

use dioxus::prelude::*;

use crate::components::Slider;
use crate::types::OverlayAppearanceConfig;

use super::draft::{SettingsDraft, appearance_for};
use super::widgets::{Check, ColorRow, Group, NumberRow, ResetRow, Section};

/// Edit this overlay's appearance, seeding it from its default on first change
fn edit_appearance(draft: SettingsDraft, tab: &str, f: impl FnOnce(&mut OverlayAppearanceConfig)) {
    draft.edit(|s| {
        let default = s.default_appearances.get(tab).cloned().unwrap_or_default();
        f(s.appearances.entry(tab.to_string()).or_insert(default));
    });
}

#[component]
pub fn MetricTab(draft: SettingsDraft, tab_key: String, actions: Element) -> Element {
    let a = appearance_for(&draft.current(), &tab_key);
    let check = |f: fn(&mut OverlayAppearanceConfig, bool)| {
        let tab = tab_key.clone();
        move |v: bool| edit_appearance(draft, &tab, |a| f(a, v))
    };
    let color = |f: fn(&mut OverlayAppearanceConfig, crate::types::Color)| {
        let tab = tab_key.clone();
        move |v| edit_appearance(draft, &tab, |a| f(a, v))
    };
    let (tab_max, tab_spacing, tab_reset) = (tab_key.clone(), tab_key.clone(), tab_key.clone());

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
                Slider {
                    label: "Bar Spacing",
                    value: (a.bar_spacing_ratio * 100.0) as i32 as f64,
                    min: 0.0,
                    max: 60.0,
                    step: 5.0,
                    suffix: "%",
                    on_change: move |v: f64| edit_appearance(draft, &tab_spacing, |a| a.bar_spacing_ratio = (v as f32 / 100.0).clamp(0.0, 0.6)),
                }
            }
            Section { group: Group::Content,
                Check { label: "Show Per-Second", checked: a.show_per_second, on_change: check(|a, v| a.show_per_second = v) }
                Check { label: "Show Total", checked: a.show_total, on_change: check(|a, v| a.show_total = v) }
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
