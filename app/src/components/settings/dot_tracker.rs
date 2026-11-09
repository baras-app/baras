//! DOT tracker overlay settings tab.

use dioxus::prelude::*;

use crate::components::Slider;
use crate::types::DotTrackerConfig;

use super::draft::SettingsDraft;
use super::widgets::{BorderRows, Check, ColorRow, DynamicBackground, FontScale, Group, NumberRow, OpacitySlider, ResetRow, Section};

#[component]
pub fn DotTrackerTab(draft: SettingsDraft, actions: Element) -> Element {
    let s = draft.current();
    let cfg = s.dot_tracker.clone();
    let set = move |f: fn(&mut DotTrackerConfig, bool)| move |v: bool| draft.edit(|s| f(&mut s.dot_tracker, v));

    rsx! {
        div { class: "settings-section",
            Section { group: Group::Background,
                OpacitySlider { value: s.dot_tracker_opacity, on_change: move |v| draft.edit(|s| s.dot_tracker_opacity = v) }
                DynamicBackground { checked: cfg.dynamic_background, on_change: set(|c, v| c.dynamic_background = v) }
                Check {
                    label: "Gradient Bars",
                    note: "(bar layout)",
                    checked: cfg.bar_gradient,
                    disabled: !cfg.layout_bar,
                    on_change: set(|c, v| c.bar_gradient = v),
                }
                BorderRows {
                    show: cfg.show_border,
                    color: cfg.border_color,
                    note: "(bar layout)",
                    disabled: !cfg.layout_bar,
                    on_show: set(|c, v| c.show_border = v),
                    on_color: move |v| draft.edit(|s| s.dot_tracker.border_color = v),
                }
            }
            Section { group: Group::Layout,
                Slider {
                    label: "Icon/Bar Size",
                    value: cfg.icon_size as f64,
                    min: 12.0,
                    max: 48.0,
                    suffix: "px",
                    on_change: move |v: f64| draft.edit(|s| s.dot_tracker.icon_size = (v as u8).clamp(12, 48)),
                }
                NumberRow {
                    label: "Max Targets",
                    value: cfg.max_targets as f64,
                    min: 1.0,
                    max: 10.0,
                    on_change: move |v: f64| draft.edit(|s| s.dot_tracker.max_targets = (v as u8).clamp(1, 10)),
                }
                Check { label: "Bar Mode", checked: cfg.layout_bar, on_change: set(|c, v| c.layout_bar = v) }
                Check { label: "Stack from Bottom", checked: cfg.stack_from_bottom, on_change: set(|c, v| c.stack_from_bottom = v) }
                Check {
                    label: "Right-Align Icons",
                    note: "(icon layout)",
                    checked: cfg.right_align_icons,
                    disabled: cfg.layout_bar,
                    on_change: set(|c, v| c.right_align_icons = v),
                }
            }
            Section { group: Group::Content,
                Check { label: "Show Header", checked: cfg.show_header, on_change: set(|c, v| c.show_header = v) }
                Check {
                    label: "Show Effect Names",
                    note: "(bar layout)",
                    checked: cfg.show_effect_names,
                    disabled: !cfg.layout_bar,
                    on_change: set(|c, v| c.show_effect_names = v),
                }
                Check { label: "Show Countdown", checked: cfg.show_countdown, on_change: set(|c, v| c.show_countdown = v) }
            }
            Section { group: Group::Text,
                FontScale { value: cfg.font_scale, on_change: move |v| draft.edit(|s| s.dot_tracker.font_scale = v) }
                ColorRow { label: "Font Color", value: cfg.font_color, on_change: move |v| draft.edit(|s| s.dot_tracker.font_color = v) }
            }
            ResetRow {
                on_reset: move |_| draft.edit(|s| {
                    s.dot_tracker = DotTrackerConfig::default();
                    s.dot_tracker_opacity = 180;
                }),
                actions,
            }
        }
    }
}
