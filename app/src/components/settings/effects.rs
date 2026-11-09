//! Effects overlay settings tab, shared by groups A, B and C.

use dioxus::prelude::*;

use crate::components::Slider;
use crate::types::{EffectsAConfig, OverlaySettings};

use super::draft::SettingsDraft;
use super::widgets::{BorderRows, Check, DynamicBackground, FontScale, Group, NumberRow, OpacitySlider, ResetRow, Section};

/// Which of the effects overlays this tab edits
#[derive(Clone, Copy, PartialEq)]
pub enum EffectsGroup {
    A,
    B,
    C,
}

impl EffectsGroup {
    fn config(self, s: &OverlaySettings) -> &EffectsAConfig {
        match self {
            Self::A => &s.effects_a,
            Self::B => &s.effects_b,
            Self::C => &s.effects_c,
        }
    }

    fn config_mut(self, s: &mut OverlaySettings) -> &mut EffectsAConfig {
        match self {
            Self::A => &mut s.effects_a,
            Self::B => &mut s.effects_b,
            Self::C => &mut s.effects_c,
        }
    }

    fn opacity(self, s: &OverlaySettings) -> u8 {
        match self {
            Self::A => s.effects_a_opacity,
            Self::B => s.effects_b_opacity,
            Self::C => s.effects_c_opacity,
        }
    }

    fn opacity_mut(self, s: &mut OverlaySettings) -> &mut u8 {
        match self {
            Self::A => &mut s.effects_a_opacity,
            Self::B => &mut s.effects_b_opacity,
            Self::C => &mut s.effects_c_opacity,
        }
    }
}

#[component]
pub fn EffectsTab(draft: SettingsDraft, group: EffectsGroup, actions: Element) -> Element {
    let current_settings = draft.current();
    let cfg = group.config(&current_settings).clone();
    let set = move |f: fn(&mut EffectsAConfig, bool)| move |v: bool| draft.edit(|s| f(group.config_mut(s), v));

    rsx! {
        div { class: "settings-section",
            Section { group: Group::Background,
                OpacitySlider {
                    value: group.opacity(&current_settings),
                    on_change: move |v| draft.edit(|s| *group.opacity_mut(s) = v),
                }
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
                    on_color: move |v| draft.edit(|s| group.config_mut(s).border_color = v),
                }
            }
            Section { group: Group::Layout,
                Slider {
                    label: "Icon/Bar Size",
                    value: cfg.icon_size as f64,
                    min: 16.0,
                    max: 128.0,
                    suffix: "px",
                    on_change: move |v: f64| draft.edit(|s| group.config_mut(s).icon_size = (v as u8).clamp(16, 128)),
                }
                NumberRow {
                    label: "Max Displayed",
                    value: cfg.max_display as f64,
                    min: 1.0,
                    max: 16.0,
                    on_change: move |v: f64| draft.edit(|s| group.config_mut(s).max_display = (v as u8).clamp(1, 16)),
                }
                Check { label: "Bar Mode", checked: cfg.layout_bar, on_change: set(|c, v| c.layout_bar = v) }
                Check { label: "Vertical Layout", checked: cfg.layout_vertical, on_change: set(|c, v| c.layout_vertical = v) }
                Check { label: "Stack from Bottom", checked: cfg.stack_from_bottom, on_change: set(|c, v| c.stack_from_bottom = v) }
            }
            Section { group: Group::Content,
                Check { label: "Show Header", checked: cfg.show_header, on_change: set(|c, v| c.show_header = v) }
                Check { label: "Show Countdown", checked: cfg.show_countdown, on_change: set(|c, v| c.show_countdown = v) }
                Check { label: "Show Effect Names", checked: cfg.show_effect_names, on_change: set(|c, v| c.show_effect_names = v) }
                Check { label: "Emphasize Effect Charges", checked: cfg.stack_priority, on_change: set(|c, v| c.stack_priority = v) }
            }
            Section { group: Group::Text,
                FontScale { value: cfg.font_scale, on_change: move |v| draft.edit(|s| group.config_mut(s).font_scale = v) }
            }
            ResetRow {
                on_reset: move |_| draft.edit(|s| {
                    *group.config_mut(s) = EffectsAConfig::default();
                    *group.opacity_mut(s) = 180;
                }),
                actions,
            }
        }
    }
}
