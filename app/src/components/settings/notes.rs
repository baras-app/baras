//! Notes overlay settings tab.

use dioxus::prelude::*;

use crate::components::Slider;

use super::draft::SettingsDraft;
use super::widgets::{ColorRow, DynamicBackground, Group, OpacitySlider, ResetRow, Section};

#[component]
pub fn NotesTab(draft: SettingsDraft, actions: Element) -> Element {
    let s = draft.current();
    let cfg = s.notes_overlay.clone();

    rsx! {
        div { class: "settings-section",
            Section { group: Group::Background,
                OpacitySlider { value: s.notes_opacity, on_change: move |v| draft.edit(|s| s.notes_opacity = v) }
                DynamicBackground { checked: cfg.dynamic_background, on_change: move |v| draft.edit(|s| s.notes_overlay.dynamic_background = v) }
            }
            Section { group: Group::Text,
                Slider {
                    label: "Font Size",
                    value: cfg.font_size as f64,
                    min: 6.0,
                    max: 24.0,
                    suffix: "px",
                    on_change: move |v: f64| draft.edit(|s| s.notes_overlay.font_size = (v as u8).clamp(6, 24)),
                }
                ColorRow { label: "Font Color", value: cfg.font_color, on_change: move |v| draft.edit(|s| s.notes_overlay.font_color = v) }
            }
            ResetRow {
                on_reset: move |_| draft.edit(|s| {
                    s.notes_overlay = Default::default();
                    s.notes_opacity = 180;
                }),
                actions,
            }
        }
    }
}
