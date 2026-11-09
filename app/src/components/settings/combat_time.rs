//! Combat time overlay settings tab.

use dioxus::prelude::*;

use super::draft::SettingsDraft;
use super::widgets::{Check, ColorRow, DynamicBackground, FontScale, Group, OpacitySlider, ResetRow, Section};

#[component]
pub fn CombatTimeTab(draft: SettingsDraft, actions: Element) -> Element {
    let s = draft.current();
    let cfg = s.combat_time.clone();

    rsx! {
        div { class: "settings-section",
            Section { group: Group::Background,
                OpacitySlider { value: s.combat_time_opacity, on_change: move |v| draft.edit(|s| s.combat_time_opacity = v) }
                DynamicBackground { checked: cfg.dynamic_background, on_change: move |v| draft.edit(|s| s.combat_time.dynamic_background = v) }
            }
            Section { group: Group::Content,
                Check { label: "Show Title", checked: cfg.show_title, on_change: move |v| draft.edit(|s| s.combat_time.show_title = v) }
                Check { label: "Clear After Combat", checked: cfg.clear_after_combat, on_change: move |v| draft.edit(|s| s.combat_time.clear_after_combat = v) }
            }
            Section { group: Group::Text,
                FontScale { value: cfg.font_scale, on_change: move |v| draft.edit(|s| s.combat_time.font_scale = v) }
                ColorRow { label: "Font Color", value: cfg.font_color, on_change: move |v| draft.edit(|s| s.combat_time.font_color = v) }
            }
            ResetRow {
                on_reset: move |_| draft.edit(|s| {
                    s.combat_time = Default::default();
                    s.combat_time_opacity = 180;
                }),
                actions,
            }
        }
    }
}
