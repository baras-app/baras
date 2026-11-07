//! Enemy frames (PvP) overlay settings tab.

use dioxus::prelude::*;

use crate::components::Slider;

use super::draft::SettingsDraft;
use super::widgets::OpacitySlider;

#[component]
pub fn EnemyFramesTab(draft: SettingsDraft, actions: Element) -> Element {
    let current_settings = draft.current();

    rsx! {
        // Enemy Frames Settings (PvP enemy HP overlay)
        div { class: "settings-section",
                h4 { "Enemy Frames" }

                Slider {
                    label: "Frame Scale",
                    value: current_settings.enemy_frames.scale as f64,
                    min: 0.5,
                    max: 2.0,
                    step: 0.05,
                    on_change: move |v: f64| {
                        let mut new_settings = draft.current();
                        new_settings.enemy_frames.scale = (v as f32).clamp(0.5, 2.0);
                        draft.update(new_settings);
                    },
                }

                div { class: "setting-row",
                    label { "Show Current Target" }
                    input {
                        r#type: "checkbox",
                        checked: current_settings.enemy_frames.show_target,
                        onchange: move |e: Event<FormData>| {
                            let mut new_settings = draft.current();
                            new_settings.enemy_frames.show_target = e.checked();
                            draft.update(new_settings);
                        }
                    }
                }

                OpacitySlider {
                    label: "Background Opacity",
                    value: current_settings.enemy_frames_opacity,
                    on_change: move |val| {
                        let mut new_settings = draft.current();
                        new_settings.enemy_frames_opacity = val;
                        draft.update(new_settings);
                    },
                }

                p { class: "text-muted text-sm", style: "margin-top: 12px;",
                    i { class: "fa-solid fa-info-circle" }
                    " HP bars use the class colors from Global Metrics Settings."
                }

            div { class: "setting-row reset-row",
                {actions}
            }
        }
    }
}
