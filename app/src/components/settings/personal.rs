//! Personal stats overlay settings tab.

use dioxus::prelude::*;

use crate::components::Slider;
use crate::types::{PersonalOverlayConfig, PersonalStat};

use super::draft::SettingsDraft;
use super::widgets::{Check, ColorRow, DynamicBackground, FontScale, Group, OpacitySlider, ResetRow, Section};

#[component]
pub fn PersonalTab(draft: SettingsDraft, actions: Element) -> Element {
    let s = draft.current();
    let cfg = s.personal_overlay.clone();
    let visible_stats = cfg.visible_stats.clone();
    let stat_count = visible_stats.len();

    rsx! {
        div { class: "settings-section",
            Section { group: Group::Background,
                OpacitySlider { value: s.personal_opacity, on_change: move |v| draft.edit(|s| s.personal_opacity = v) }
                DynamicBackground { checked: cfg.dynamic_background, on_change: move |v| draft.edit(|s| s.personal_overlay.dynamic_background = v) }
            }
            Section { group: Group::Layout,
                Slider {
                    label: "Line Spacing",
                    value: (cfg.line_spacing * 100.0) as i32 as f64,
                    min: 70.0,
                    max: 150.0,
                    step: 10.0,
                    suffix: "%",
                    on_change: move |v: f64| draft.edit(|s| s.personal_overlay.line_spacing = (v as f32 / 100.0).clamp(0.7, 1.5)),
                }
            }
            Section { group: Group::Content,
                p { class: "hint", "Displayed stats:" }
                div { class: "stat-order-list",
                    for (idx, stat) in visible_stats.into_iter().enumerate() {
                        div { class: "stat-order-item", key: "{idx}",
                            span { class: "stat-name", "{stat.label()}" }
                            div { class: "stat-controls",
                                button {
                                    class: "btn-order",
                                    disabled: idx == 0,
                                    onclick: move |_| draft.edit(|s| {
                                        if idx > 0 {
                                            s.personal_overlay.visible_stats.swap(idx, idx - 1);
                                        }
                                    }),
                                    "▲"
                                }
                                button {
                                    class: "btn-order",
                                    disabled: idx >= stat_count - 1,
                                    onclick: move |_| draft.edit(|s| {
                                        let stats = &mut s.personal_overlay.visible_stats;
                                        if idx < stats.len() - 1 {
                                            stats.swap(idx, idx + 1);
                                        }
                                    }),
                                    "▼"
                                }
                                button {
                                    class: "btn-remove",
                                    onclick: move |_| draft.edit(|s| {
                                        let stats = &mut s.personal_overlay.visible_stats;
                                        if idx < stats.len() {
                                            stats.remove(idx);
                                        }
                                    }),
                                    "✕"
                                }
                            }
                        }
                    }
                }
                div { class: "stat-add-section",
                    p { class: "hint", "Add stats:" }
                    div { class: "stat-add-grid",
                        for stat in PersonalStat::all() {
                            {
                                let stat = *stat;
                                // Separator can be added any number of times, other stats once
                                let is_visible = stat != PersonalStat::Separator && cfg.visible_stats.contains(&stat);
                                if !is_visible {
                                    rsx! {
                                        button {
                                            class: "btn-add-stat",
                                            onclick: move |_| draft.edit(|s| {
                                                let stats = &mut s.personal_overlay.visible_stats;
                                                if stat == PersonalStat::Separator || !stats.contains(&stat) {
                                                    stats.push(stat);
                                                }
                                            }),
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
                Check { label: "Hide Empty Values", checked: cfg.hide_empty_values, on_change: move |v| draft.edit(|s| s.personal_overlay.hide_empty_values = v) }
            }
            Section { group: Group::Text,
                FontScale { value: cfg.font_scale, on_change: move |v| draft.edit(|s| s.personal_overlay.font_scale = v) }
                ColorRow { label: "Value Font Color", value: cfg.font_color, on_change: move |v| draft.edit(|s| s.personal_overlay.font_color = v) }
                ColorRow { label: "Label Font Color", value: cfg.label_color, on_change: move |v| draft.edit(|s| s.personal_overlay.label_color = v) }
                Check { label: "Auto-color Values", checked: cfg.auto_color_values, on_change: move |v| draft.edit(|s| s.personal_overlay.auto_color_values = v) }
            }
            ResetRow {
                on_reset: move |_| draft.edit(|s| {
                    s.personal_overlay = PersonalOverlayConfig::default();
                    s.personal_opacity = 180;
                }),
                actions,
            }
        }
    }
}
