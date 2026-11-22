//! Options shared by every metric overlay (bar sizing, class colours): the
//! global values and one overlay's overrides of them.

use dioxus::prelude::*;

use crate::components::Slider;
use crate::types::{ClassColorConfig, ClassIconMode, Color};
use baras_types::{MAX_BAR_GAP, MAX_BAR_RADIUS};

use super::draft::{SettingsDraft, appearance_for, edit_appearance};
use super::widgets::{
    Check, ColorRow, DynamicBackground, FontScale, Group, OnOff, Override, PercentSlider, Section, SelectRow,
};

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

const ICON_MODES: [(&str, &str); 3] = [("none", "None"), ("class", "Class"), ("discipline", "Discipline")];

fn icon_mode_key(mode: ClassIconMode) -> String {
    match mode {
        ClassIconMode::None => "none",
        ClassIconMode::Class => "class",
        ClassIconMode::Discipline => "discipline",
    }
    .to_string()
}

fn icon_mode_from(key: &str) -> ClassIconMode {
    match key {
        "class" => ClassIconMode::Class,
        "discipline" => ClassIconMode::Discipline,
        _ => ClassIconMode::None,
    }
}

/// Global metric options, on the Global tab
#[component]
pub fn MetricsGlobalSection(draft: SettingsDraft) -> Element {
    let s = draft.current();
    let mut class_colors = s.class_colors.clone();

    rsx! {
        div { class: "settings-section",
            h4 { "Metric Overlays" }
            p { class: "hint", "Shared by the metric overlays (DPS, HPS, ...). Class colors cannot be overridden." }
            Section { group: Group::Background,
                DynamicBackground { checked: s.metric_dynamic_background, on_change: move |v| draft.edit(|s| s.metric_dynamic_background = v) }
                Check { label: "Show Background Bar", checked: s.metric_show_background_bar, on_change: move |v| draft.edit(|s| s.metric_show_background_bar = v) }
                PercentSlider { label: "Gradient Intensity", value: s.metric_gradient_intensity, min: 0.0, max: 60.0, step: 2.0, on_change: move |v| draft.edit(|s| s.metric_gradient_intensity = v) }
                Slider {
                    label: "Bar Corner Radius",
                    value: s.metric_bar_radius as f64,
                    min: 0.0,
                    max: MAX_BAR_RADIUS as f64,
                    suffix: "px",
                    on_change: move |v: f64| draft.edit(|s| s.metric_bar_radius = v as f32),
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
                PercentSlider { label: "Bar Thickness", value: s.metric_scaling_factor, min: 30.0, max: 200.0, on_change: move |v| draft.edit(|s| s.metric_scaling_factor = v) }
                Slider { label: "Bar Spacing", value: s.metric_bar_gap as f64, min: 0.0, max: MAX_BAR_GAP as f64, suffix: "px", on_change: move |v: f64| draft.edit(|s| s.metric_bar_gap = v as f32) }
                Check { label: "Stack from Bottom", checked: s.metric_stack_from_bottom, on_change: move |v| draft.edit(|s| s.metric_stack_from_bottom = v) }
            }
            Section { group: Group::Content,
                Check { label: "Show Empty Bars", checked: s.metric_show_empty_bars, on_change: move |v| draft.edit(|s| s.metric_show_empty_bars = v) }
                SelectRow {
                    label: "Icon Type",
                    value: icon_mode_key(s.class_icon_mode),
                    options: ICON_MODES.to_vec(),
                    on_change: move |v: String| draft.edit(|s| s.class_icon_mode = icon_mode_from(&v)),
                }
            }
            Section { group: Group::Text,
                FontScale { value: s.metric_font_scale, on_change: move |v| draft.edit(|s| s.metric_font_scale = v) }
            }
        }
    }
}

/// One metric overlay's overrides of the options above, as rows of the overrides section
#[component]
pub fn MetricOverrideRows(draft: SettingsDraft, tab_key: Signal<String>) -> Element {
    let s = draft.current();
    let a = appearance_for(&s, &tab_key());
    let o = a.overrides;
    let m = s.metric_options_for(&o);
    let spacing = a.bar_gap.unwrap_or(s.metric_bar_gap);
    let radius = a.bar_radius.unwrap_or(s.metric_bar_radius);

    rsx! {
        Override {
            custom: o.dynamic_background.is_some(),
            on_toggle: move |on: bool| edit_appearance(draft, &tab_key(), |a| a.overrides.dynamic_background = on.then_some(m.dynamic_background)),
            OnOff { label: "Dynamic Background", value: m.dynamic_background, disabled: o.dynamic_background.is_none(), on_change: move |v| edit_appearance(draft, &tab_key(), |a| a.overrides.dynamic_background = Some(v)) }
        }
        Override {
            custom: o.show_background_bar.is_some(),
            on_toggle: move |on: bool| edit_appearance(draft, &tab_key(), |a| a.overrides.show_background_bar = on.then_some(m.show_background_bar)),
            OnOff { label: "Show Background Bar", value: m.show_background_bar, disabled: o.show_background_bar.is_none(), on_change: move |v| edit_appearance(draft, &tab_key(), |a| a.overrides.show_background_bar = Some(v)) }
        }
        Override {
            custom: o.gradient_intensity.is_some(),
            on_toggle: move |on: bool| edit_appearance(draft, &tab_key(), |a| a.overrides.gradient_intensity = on.then_some(m.gradient_intensity)),
            PercentSlider { label: "Gradient Intensity", value: m.gradient_intensity, min: 0.0, max: 60.0, step: 2.0, disabled: o.gradient_intensity.is_none(), on_change: move |v| edit_appearance(draft, &tab_key(), |a| a.overrides.gradient_intensity = Some(v)) }
        }
        Override {
            custom: a.bar_radius.is_some(),
            on_toggle: move |on: bool| edit_appearance(draft, &tab_key(), |a| a.bar_radius = on.then_some(radius)),
            Slider {
                label: "Bar Corner Radius",
                value: radius as f64,
                min: 0.0,
                max: MAX_BAR_RADIUS as f64,
                suffix: "px",
                disabled: a.bar_radius.is_none(),
                on_change: move |v: f64| edit_appearance(draft, &tab_key(), |a| a.bar_radius = Some(v as f32)),
            }
        }
        Override {
            custom: o.scaling_factor.is_some(),
            on_toggle: move |on: bool| edit_appearance(draft, &tab_key(), |a| a.overrides.scaling_factor = on.then_some(m.scaling_factor)),
            PercentSlider { label: "Bar Thickness", value: m.scaling_factor, min: 30.0, max: 200.0, disabled: o.scaling_factor.is_none(), on_change: move |v| edit_appearance(draft, &tab_key(), |a| a.overrides.scaling_factor = Some(v)) }
        }
        Override {
            custom: a.bar_gap.is_some(),
            on_toggle: move |on: bool| edit_appearance(draft, &tab_key(), |a| a.bar_gap = on.then_some(spacing)),
            Slider { label: "Bar Spacing", value: spacing as f64, min: 0.0, max: MAX_BAR_GAP as f64, suffix: "px", disabled: a.bar_gap.is_none(), on_change: move |v: f64| edit_appearance(draft, &tab_key(), |a| a.bar_gap = Some(v as f32)) }
        }
        Override {
            custom: o.stack_from_bottom.is_some(),
            on_toggle: move |on: bool| edit_appearance(draft, &tab_key(), |a| a.overrides.stack_from_bottom = on.then_some(m.stack_from_bottom)),
            OnOff { label: "Stack from Bottom", value: m.stack_from_bottom, disabled: o.stack_from_bottom.is_none(), on_change: move |v| edit_appearance(draft, &tab_key(), |a| a.overrides.stack_from_bottom = Some(v)) }
        }
        Override {
            custom: o.show_empty_bars.is_some(),
            on_toggle: move |on: bool| edit_appearance(draft, &tab_key(), |a| a.overrides.show_empty_bars = on.then_some(m.show_empty_bars)),
            OnOff { label: "Show Empty Bars", value: m.show_empty_bars, disabled: o.show_empty_bars.is_none(), on_change: move |v| edit_appearance(draft, &tab_key(), |a| a.overrides.show_empty_bars = Some(v)) }
        }
        Override {
            custom: o.class_icon_mode.is_some(),
            on_toggle: move |on: bool| edit_appearance(draft, &tab_key(), |a| a.overrides.class_icon_mode = on.then_some(m.class_icon_mode)),
            SelectRow {
                label: "Icon Type",
                value: icon_mode_key(m.class_icon_mode),
                options: ICON_MODES.to_vec(),
                disabled: o.class_icon_mode.is_none(),
                on_change: move |v: String| edit_appearance(draft, &tab_key(), |a| a.overrides.class_icon_mode = Some(icon_mode_from(&v))),
            }
        }
        Override {
            custom: o.font_scale.is_some(),
            on_toggle: move |on: bool| edit_appearance(draft, &tab_key(), |a| a.overrides.font_scale = on.then_some(m.font_scale)),
            FontScale { value: m.font_scale, disabled: o.font_scale.is_none(), on_change: move |v| edit_appearance(draft, &tab_key(), |a| a.overrides.font_scale = Some(v)) }
        }
    }
}
