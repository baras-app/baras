//! Operation timer overlay settings tab.

use dioxus::prelude::*;

use super::draft::SettingsDraft;
use super::widgets::{Check, ColorRow, DynamicBackground, FontScale, Group, OpacitySlider, ResetRow, Section};

#[component]
pub fn OperationTimerTab(draft: SettingsDraft, actions: Element) -> Element {
    let s = draft.current();
    let cfg = s.operation_timer.clone();

    rsx! {
        div { class: "settings-section",
            Section { group: Group::Background,
                OpacitySlider { value: s.operation_timer_opacity, on_change: move |v| draft.edit(|s| s.operation_timer_opacity = v) }
                DynamicBackground { checked: cfg.dynamic_background, on_change: move |v| draft.edit(|s| s.operation_timer.dynamic_background = v) }
            }
            Section { group: Group::Content,
                Check { label: "Show Title", checked: cfg.show_title, on_change: move |v| draft.edit(|s| s.operation_timer.show_title = v) }
            }
            Section { group: Group::Text,
                FontScale { value: cfg.font_scale, on_change: move |v| draft.edit(|s| s.operation_timer.font_scale = v) }
                ColorRow { label: "Font Color", value: cfg.font_color, on_change: move |v| draft.edit(|s| s.operation_timer.font_color = v) }
            }
            ResetRow {
                on_reset: move |_| draft.edit(|s| {
                    s.operation_timer = Default::default();
                    s.operation_timer_opacity = 180;
                }),
                actions,
            }
        }
    }
}
