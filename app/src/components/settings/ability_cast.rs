//! Abilities cast overlay settings tab.

use dioxus::prelude::*;

use crate::components::Slider;

use super::draft::SettingsDraft;
use super::widgets::{Check, ColorRow, DynamicBackground, FontScale, Group, OpacitySlider, ResetRow, Section};

#[component]
pub fn AbilityCastTab(draft: SettingsDraft, actions: Element) -> Element {
    let s = draft.current();
    let cfg = s.ability_cast.clone();

    rsx! {
        div { class: "settings-section",
            Section { group: Group::Background,
                OpacitySlider { value: s.ability_cast_opacity, on_change: move |v| draft.edit(|s| s.ability_cast_opacity = v) }
                DynamicBackground { checked: cfg.dynamic_background, on_change: move |v| draft.edit(|s| s.ability_cast.dynamic_background = v) }
            }
            Section { group: Group::Layout,
                Slider {
                    label: "Max Displayed",
                    value: cfg.max_display as f64,
                    min: 1.0,
                    max: 30.0,
                    on_change: move |v: f64| draft.edit(|s| s.ability_cast.max_display = (v as u8).clamp(1, 30)),
                }
                Slider {
                    label: "Icon Size",
                    value: cfg.icon_size as f64,
                    min: 16.0,
                    max: 128.0,
                    suffix: "px",
                    on_change: move |v: f64| draft.edit(|s| s.ability_cast.icon_size = (v as u8).clamp(16, 128)),
                }
                Check { label: "Stack from Bottom", checked: cfg.stack_from_bottom, on_change: move |v| draft.edit(|s| s.ability_cast.stack_from_bottom = v) }
            }
            Section { group: Group::Content,
                Slider {
                    label: "Prune After",
                    value: cfg.prune_secs as f64,
                    min: 1.0,
                    max: 60.0,
                    suffix: "s",
                    tooltip: "Casts older than this are removed from the list",
                    on_change: move |v: f64| draft.edit(|s| s.ability_cast.prune_secs = (v as f32).clamp(1.0, 60.0)),
                }
            }
            Section { group: Group::Text,
                FontScale { value: cfg.font_scale, on_change: move |v| draft.edit(|s| s.ability_cast.font_scale = v) }
                ColorRow { label: "Font Color", value: cfg.font_color, on_change: move |v| draft.edit(|s| s.ability_cast.font_color = v) }
            }
            ResetRow {
                on_reset: move |_| draft.edit(|s| {
                    s.ability_cast = Default::default();
                    s.ability_cast_opacity = 180;
                }),
                actions,
            }
        }
    }
}
