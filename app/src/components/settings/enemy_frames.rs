//! Enemy frames (PvP) overlay settings tab.

use dioxus::prelude::*;

use crate::components::Slider;

use super::draft::SettingsDraft;
use super::widgets::{Check, Group, OpacitySlider, Section};

#[component]
pub fn EnemyFramesTab(draft: SettingsDraft, actions: Element) -> Element {
    let s = draft.current();

    rsx! {
        div { class: "settings-section",
            Section { group: Group::Background,
                OpacitySlider { value: s.enemy_frames_opacity, on_change: move |v| draft.edit(|s| s.enemy_frames_opacity = v) }
                p { class: "hint",
                    i { class: "fa-solid fa-info-circle" }
                    " HP bars use the class colors from Global Metrics Settings."
                }
            }
            Section { group: Group::Layout,
                Slider {
                    label: "Frame Scale",
                    value: s.enemy_frames.scale as f64,
                    min: 0.5,
                    max: 2.0,
                    step: 0.05,
                    on_change: move |v: f64| draft.edit(|s| s.enemy_frames.scale = (v as f32).clamp(0.5, 2.0)),
                }
            }
            Section { group: Group::Content,
                Check { label: "Show Current Target", checked: s.enemy_frames.show_target, on_change: move |v| draft.edit(|s| s.enemy_frames.show_target = v) }
            }
            // No style reset here (none before either): only the shared actions
            div { class: "setting-row reset-row", {actions} }
        }
    }
}
