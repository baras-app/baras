//! Cooldown tracker overlay settings tab, shared by groups A and B.

use dioxus::prelude::*;

use crate::components::Slider;
use crate::types::{CooldownTrackerConfig, OverlaySettings};

use super::draft::SettingsDraft;
use super::widgets::{BorderRows, Check, DynamicBackground, FontScale, Group, NumberRow, OpacitySlider, ResetRow, Section};

/// Which of the cooldown overlays this tab edits
#[derive(Clone, Copy, PartialEq)]
pub enum CooldownGroup {
    A,
    B,
}

impl CooldownGroup {
    fn config(self, s: &OverlaySettings) -> &CooldownTrackerConfig {
        match self {
            Self::A => &s.cooldown_tracker,
            Self::B => &s.cooldown_tracker_b,
        }
    }

    fn config_mut(self, s: &mut OverlaySettings) -> &mut CooldownTrackerConfig {
        match self {
            Self::A => &mut s.cooldown_tracker,
            Self::B => &mut s.cooldown_tracker_b,
        }
    }

    fn opacity(self, s: &OverlaySettings) -> u8 {
        match self {
            Self::A => s.cooldown_tracker_opacity,
            Self::B => s.cooldown_tracker_b_opacity,
        }
    }

    fn opacity_mut(self, s: &mut OverlaySettings) -> &mut u8 {
        match self {
            Self::A => &mut s.cooldown_tracker_opacity,
            Self::B => &mut s.cooldown_tracker_b_opacity,
        }
    }
}

#[component]
pub fn CooldownsTab(draft: SettingsDraft, group: CooldownGroup, actions: Element) -> Element {
    let current_settings = draft.current();
    let cfg = group.config(&current_settings).clone();
    let set = move |f: fn(&mut CooldownTrackerConfig, bool)| move |v: bool| draft.edit(|s| f(group.config_mut(s), v));

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
                    max: 20.0,
                    on_change: move |v: f64| draft.edit(|s| group.config_mut(s).max_display = (v as u8).clamp(1, 20)),
                }
                Check { label: "Bar Mode", checked: cfg.layout_bar, on_change: set(|c, v| c.layout_bar = v) }
                Check { label: "Stack from Bottom", checked: cfg.stack_from_bottom, on_change: set(|c, v| c.stack_from_bottom = v) }
            }
            Section { group: Group::Content,
                Check { label: "Show Header", checked: cfg.show_header, on_change: set(|c, v| c.show_header = v) }
                Check { label: "Show Ability Names", checked: cfg.show_ability_names, on_change: set(|c, v| c.show_ability_names = v) }
                Check { label: "Sort by Remaining Time", checked: cfg.sort_by_remaining, on_change: set(|c, v| c.sort_by_remaining = v) }
            }
            Section { group: Group::Text,
                FontScale { value: cfg.font_scale, on_change: move |v| draft.edit(|s| group.config_mut(s).font_scale = v) }
            }
            ResetRow {
                on_reset: move |_| draft.edit(|s| {
                    *group.config_mut(s) = CooldownTrackerConfig::default();
                    *group.opacity_mut(s) = 180;
                }),
                actions,
            }
        }
    }
}
