//! Global settings shared by every metric overlay: opacity, bar sizing, class colours.

use dioxus::prelude::*;

use crate::components::Slider;
use crate::types::{ClassColorConfig, ClassIconMode, Color};

use super::draft::SettingsDraft;
use super::widgets::{Check, ColorRow, DynamicBackground, FontScale, Group, OpacitySlider, Section, SelectRow};

/// The eight class colours: label and field
const CLASS_COLORS: [(&str, fn(&mut ClassColorConfig) -> &mut Color); 8] = [
    ("Sorcerer / Sage", |c| &mut c.sorcerer_sage),
    ("Assassin / Shadow", |c| &mut c.assassin_shadow),
    ("Juggernaut / Guardian", |c| &mut c.juggernaut_guardian),
    ("Marauder / Sentinel", |c| &mut c.marauder_sentinel),
    ("Mercenary / Commando", |c| &mut c.mercenary_commando),
    ("Powertech / Vanguard", |c| &mut c.powertech_vanguard),
    ("Operative / Scoundrel", |c| &mut c.operative_scoundrel),
    ("Sniper / Gunslinger", |c| &mut c.sniper_gunslinger),
];

#[component]
pub fn MetricsGlobalSection(draft: SettingsDraft, mut open: Signal<bool>) -> Element {
    let s = draft.current();
    let mut class_colors = s.class_colors.clone();
    let icon_mode = match s.class_icon_mode {
        ClassIconMode::None => "none",
        ClassIconMode::Class => "class",
        ClassIconMode::Discipline => "discipline",
    };

    rsx! {
        div { class: if open() { "settings-section collapsible metrics-global open" } else { "settings-section collapsible metrics-global" },
            div {
                class: "collapsible-summary",
                onclick: move |_| open.set(!open()),
                i { class: "fa-solid fa-sliders summary-icon" }
                "Global Metrics Settings"
            }
            if open() {
                div { class: "collapsible-content",
                    Section { group: Group::Background,
                        OpacitySlider { value: s.metric_opacity, on_change: move |v| draft.edit(|s| s.metric_opacity = v) }
                        DynamicBackground { checked: s.metric_dynamic_background, on_change: move |v| draft.edit(|s| s.metric_dynamic_background = v) }
                        Check { label: "Show Background Bar", checked: s.metric_show_background_bar, on_change: move |v| draft.edit(|s| s.metric_show_background_bar = v) }
                        Slider {
                            label: "Gradient Intensity",
                            value: (s.metric_gradient_intensity * 100.0) as i32 as f64,
                            min: 0.0,
                            max: 60.0,
                            step: 2.0,
                            suffix: "%",
                            on_change: move |v: f64| draft.edit(|s| s.metric_gradient_intensity = (v as f32 / 100.0).clamp(0.0, 0.6)),
                        }
                        details { class: "collapsible",
                            summary { class: "collapsible-summary", "Class Colors" }
                            for (label, field) in CLASS_COLORS {
                                ColorRow {
                                    label,
                                    value: *field(&mut class_colors),
                                    on_change: move |v| draft.edit(|s| *field(&mut s.class_colors) = v),
                                }
                            }
                        }
                    }
                    Section { group: Group::Layout,
                        Slider {
                            label: "Bar Thickness",
                            value: (s.metric_scaling_factor * 100.0) as i32 as f64,
                            min: 30.0,
                            max: 200.0,
                            step: 5.0,
                            suffix: "%",
                            on_change: move |v: f64| draft.edit(|s| s.metric_scaling_factor = (v as f32 / 100.0).clamp(0.3, 2.0)),
                        }
                        Check { label: "Stack from Bottom", checked: s.metric_stack_from_bottom, on_change: move |v| draft.edit(|s| s.metric_stack_from_bottom = v) }
                    }
                    Section { group: Group::Content,
                        Check { label: "Show Empty Bars", checked: s.metric_show_empty_bars, on_change: move |v| draft.edit(|s| s.metric_show_empty_bars = v) }
                        SelectRow {
                            label: "Class Icons",
                            value: icon_mode.to_string(),
                            options: vec![("none", "None"), ("class", "Class"), ("discipline", "Discipline")],
                            on_change: move |v: String| draft.edit(|s| {
                                s.class_icon_mode = match v.as_str() {
                                    "class" => ClassIconMode::Class,
                                    "discipline" => ClassIconMode::Discipline,
                                    _ => ClassIconMode::None,
                                };
                            }),
                        }
                    }
                    Section { group: Group::Text,
                        FontScale { value: s.metric_font_scale, on_change: move |v| draft.edit(|s| s.metric_font_scale = v) }
                    }
                }
            }
        }
    }
}
