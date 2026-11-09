//! Timer overlay settings tab, shared by groups A and B.

use dioxus::prelude::*;

use crate::types::{OverlaySettings, TimerOverlayConfig};

use super::draft::SettingsDraft;
use super::widgets::{BorderRows, Check, ColorRow, DynamicBackground, FontScale, Group, OpacitySlider, ResetRow, Section};

/// Which of the timer overlays this tab edits
#[derive(Clone, Copy, PartialEq)]
pub enum TimerGroup {
    A,
    B,
}

impl TimerGroup {
    fn config(self, s: &OverlaySettings) -> &TimerOverlayConfig {
        match self {
            Self::A => &s.timers_a_overlay,
            Self::B => &s.timers_b_overlay,
        }
    }

    fn config_mut(self, s: &mut OverlaySettings) -> &mut TimerOverlayConfig {
        match self {
            Self::A => &mut s.timers_a_overlay,
            Self::B => &mut s.timers_b_overlay,
        }
    }

    fn opacity(self, s: &OverlaySettings) -> u8 {
        match self {
            Self::A => s.timers_a_opacity,
            Self::B => s.timers_b_opacity,
        }
    }

    fn opacity_mut(self, s: &mut OverlaySettings) -> &mut u8 {
        match self {
            Self::A => &mut s.timers_a_opacity,
            Self::B => &mut s.timers_b_opacity,
        }
    }
}

#[component]
pub fn TimersTab(draft: SettingsDraft, group: TimerGroup, actions: Element) -> Element {
    let current_settings = draft.current();
    let cfg = group.config(&current_settings).clone();
    let set = move |f: fn(&mut TimerOverlayConfig, bool)| move |v: bool| draft.edit(|s| f(group.config_mut(s), v));

    rsx! {
        div { class: "settings-section",
            Section { group: Group::Background,
                OpacitySlider {
                    value: group.opacity(&current_settings),
                    on_change: move |v| draft.edit(|s| *group.opacity_mut(s) = v),
                }
                DynamicBackground { checked: cfg.dynamic_background, on_change: set(|c, v| c.dynamic_background = v) }
                Check { label: "Gradient Bars", checked: cfg.bar_gradient, on_change: set(|c, v| c.bar_gradient = v) }
                BorderRows {
                    show: cfg.show_border,
                    color: cfg.border_color,
                    on_show: set(|c, v| c.show_border = v),
                    on_color: move |v| draft.edit(|s| group.config_mut(s).border_color = v),
                }
            }
            Section { group: Group::Layout,
                Check { label: "Stack from Bottom", checked: cfg.stack_from_bottom, on_change: set(|c, v| c.stack_from_bottom = v) }
            }
            Section { group: Group::Text,
                FontScale { value: cfg.font_scale, on_change: move |v| draft.edit(|s| group.config_mut(s).font_scale = v) }
                ColorRow { label: "Font Color", value: cfg.font_color, on_change: move |v| draft.edit(|s| group.config_mut(s).font_color = v) }
            }
            ResetRow {
                on_reset: move |_| draft.edit(|s| {
                    *group.config_mut(s) = TimerOverlayConfig::default();
                    *group.opacity_mut(s) = 180;
                }),
                actions,
            }
        }
    }
}
