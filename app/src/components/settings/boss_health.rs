//! Boss health overlay settings tab.

use dioxus::prelude::*;

use crate::components::Slider;
use crate::types::{BossHealthConfig, IconPosition};

use super::draft::SettingsDraft;
use super::widgets::{BorderRows, Check, ColorRow, DynamicBackground, FontScale, Group, OpacitySlider, ResetRow, Section, SelectRow};

#[component]
pub fn BossHealthTab(draft: SettingsDraft, actions: Element) -> Element {
    let s = draft.current();
    let cfg = s.boss_health.clone();
    let set = move |f: fn(&mut BossHealthConfig, bool)| move |v: bool| draft.edit(|s| f(&mut s.boss_health, v));

    rsx! {
        div { class: "settings-section",
            Section { group: Group::Background,
                OpacitySlider { value: s.boss_health_opacity, on_change: move |v| draft.edit(|s| s.boss_health_opacity = v) }
                DynamicBackground { checked: cfg.dynamic_background, on_change: set(|c, v| c.dynamic_background = v) }
                ColorRow { label: "Bar Color", value: cfg.bar_color, on_change: move |v| draft.edit(|s| s.boss_health.bar_color = v) }
                Check { label: "Gradient Bars", checked: cfg.bar_gradient, on_change: set(|c, v| c.bar_gradient = v) }
                BorderRows {
                    show: cfg.show_border,
                    color: cfg.border_color,
                    on_show: set(|c, v| c.show_border = v),
                    on_color: move |v| draft.edit(|s| s.boss_health.border_color = v),
                }
            }
            Section { group: Group::Layout,
                Slider {
                    label: "Bosses Sized to Fit",
                    tooltip: "The number of entries to reserve space for in the overlay window. If this number is exceeded, the entries will be compressed.",
                    value: cfg.visible_bosses as f64,
                    min: 1.0,
                    max: 7.0,
                    step: 1.0,
                    on_change: move |v: f64| draft.edit(|s| s.boss_health.visible_bosses = (v as u32).clamp(1, 7)),
                }
                Slider {
                    label: "Lower Bar Scale",
                    value: (cfg.lower_bar_scale * 100.0) as i32 as f64,
                    min: 50.0,
                    max: 200.0,
                    step: 10.0,
                    suffix: "%",
                    on_change: move |v: f64| draft.edit(|s| s.boss_health.lower_bar_scale = (v as f32 / 100.0).clamp(0.5, 2.0)),
                }
                Slider {
                    label: "Icon Scale",
                    value: (cfg.icon_scale * 100.0) as i32 as f64,
                    min: 50.0,
                    max: 200.0,
                    step: 10.0,
                    suffix: "%",
                    disabled: !cfg.show_icons,
                    on_change: move |v: f64| draft.edit(|s| s.boss_health.icon_scale = (v as f32 / 100.0).clamp(0.5, 2.0)),
                }
                SelectRow {
                    label: "Icon Position",
                    value: match cfg.icon_position {
                        IconPosition::Top => "top",
                        IconPosition::Left => "left",
                        IconPosition::Right => "right",
                        IconPosition::Bottom => "bottom",
                    }.to_string(),
                    options: vec![("top", "Top"), ("bottom", "Bottom"), ("left", "Left (4 slots)"), ("right", "Right (4 slots)")],
                    disabled: !cfg.show_icons,
                    on_change: move |v: String| {
                        let pos = match v.as_str() {
                            "top" => IconPosition::Top,
                            "left" => IconPosition::Left,
                            "right" => IconPosition::Right,
                            _ => IconPosition::Bottom,
                        };
                        draft.edit(|s| s.boss_health.icon_position = pos);
                    },
                }
            }
            Section { group: Group::Content,
                Check { label: "Show Current Target", checked: cfg.show_target, on_change: set(|c, v| c.show_target = v) }
                Check { label: "Show HP Value", checked: cfg.show_hp_value, on_change: set(|c, v| c.show_hp_value = v) }
                Check { label: "Show HP Markers", checked: cfg.show_hp_markers, on_change: set(|c, v| c.show_hp_markers = v) }
                Check { label: "Show Shields", checked: cfg.show_shield, on_change: set(|c, v| c.show_shield = v) }
                Check { label: "Show Effect Icons", checked: cfg.show_icons, on_change: set(|c, v| c.show_icons = v) }
                Check { label: "Clear After Combat", checked: cfg.clear_after_combat, on_change: set(|c, v| c.clear_after_combat = v) }
            }
            Section { group: Group::Text,
                FontScale { value: cfg.font_scale, on_change: move |v| draft.edit(|s| s.boss_health.font_scale = v) }
            }
            ResetRow {
                on_reset: move |_| draft.edit(|s| {
                    s.boss_health = BossHealthConfig::default();
                    s.boss_health_opacity = 180;
                }),
                actions,
            }
        }
    }
}
