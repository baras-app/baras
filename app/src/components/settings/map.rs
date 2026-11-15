//! Map overlay settings tab.

use dioxus::prelude::*;

use super::draft::SettingsDraft;
use super::widgets::{Check, Group, NumberRow, OpacitySlider, ResetRow, Section};

#[component]
pub fn MapTab(draft: SettingsDraft, actions: Element) -> Element {
    let s = draft.current();
    let cfg = s.map.clone();

    rsx! {
        div { class: "settings-section",
            Section { group: Group::Background,
                OpacitySlider { value: s.map_opacity, on_change: move |v| draft.edit(|s| s.map_opacity = v) }
            }
            Section { group: Group::Layout,
                Check { label: "Preserve Aspect Ratio", checked: cfg.preserve_aspect, on_change: move |v| draft.edit(|s| s.map.preserve_aspect = v) }
                Check { label: "Lock Size", checked: cfg.lock_size, on_change: move |v| draft.edit(|s| s.map.lock_size = v) }
                NumberRow { label: "Width", value: cfg.width as f64, min: 50.0, max: 2560.0, unit: "px", on_change: move |v| draft.edit(|s| s.map.width = v as u32) }
                NumberRow { label: "Height", value: cfg.height as f64, min: 50.0, max: 2048.0, unit: "px", on_change: move |v| draft.edit(|s| s.map.height = v as u32) }
            }
            ResetRow {
                on_reset: move |_| draft.edit(|s| {
                    s.map = Default::default();
                    s.map_opacity = 180;
                }),
                actions,
            }
        }
    }
}
