//! Options that apply to every overlay at once.

use dioxus::prelude::*;

use crate::api;
use crate::components::{Slider, ToastSeverity, use_toast};

use super::draft::SettingsDraft;
use super::frame_style::FrameStyleSection;
use super::metrics_global::MetricsGlobalSection;

#[component]
pub fn GlobalTab(draft: SettingsDraft) -> Element {
    let current_settings = draft.current();

    // Behaviour flags persist on toggle, independent of the draft and its Save
    let mut hide_when_not_live = use_signal(|| current_settings.hide_when_not_live);
    let mut hide_during_conversations = use_signal(|| current_settings.hide_during_conversations);
    let mut toast = use_toast();

    rsx! {
        div { class: "settings-section",
            h4 { "All Overlays" }
            p { class: "hint", "Always apply to every overlay. These cannot be overridden per overlay." }
            Slider {
                label: "Countdown Frame Rate",
                value: current_settings.overlay_fps as f64,
                min: 5.0,
                max: 30.0,
                suffix: "FPS",
                tooltip: "How often timers, effects, raid frames, alerts, cooldowns and DoTs redraw. Higher is smoother but uses more CPU.",
                on_change: move |v: f64| {
                    let fps = v as u8;
                    let mut new_settings = draft.current();
                    new_settings.overlay_fps = fps;
                    draft.update(new_settings);
                    // Applies to all overlays immediately, independent of Save
                    spawn(async move {
                        let _ = api::set_overlay_fps(fps).await;
                    });
                },
            }
            label { class: "settings-check",
                input {
                    r#type: "checkbox",
                    checked: hide_when_not_live(),
                    onchange: move |e: Event<FormData>| {
                        let enabled = e.checked();
                        hide_when_not_live.set(enabled);
                        spawn(async move {
                            if let Some(mut cfg) = api::get_config().await {
                                cfg.overlay_settings.hide_when_not_live = enabled;
                                if let Err(err) = api::update_config(&cfg).await {
                                    toast.show(format!("Failed to save settings: {}", err), ToastSeverity::Normal);
                                } else {
                                    api::apply_not_live_auto_hide().await;
                                }
                            }
                        });
                    },
                }
                "Auto-hide when not live"
            }
            p { class: "hint", "Hide every overlay while viewing a historical log or logged out." }
            label { class: "settings-check",
                input {
                    r#type: "checkbox",
                    checked: hide_during_conversations(),
                    onchange: move |e: Event<FormData>| {
                        let enabled = e.checked();
                        hide_during_conversations.set(enabled);
                        spawn(async move {
                            if let Err(err) = api::patch_config(|cfg| {
                                cfg.overlay_settings.hide_during_conversations = enabled;
                            }).await {
                                toast.show(format!("Failed to save settings: {}", err), ToastSeverity::Normal);
                            }
                        });
                    },
                }
                "Hide in conversations"
            }
            p { class: "hint", "Hide every overlay during in-game conversations." }
        }

        h4 { class: "overlay-options-title", "Overlay Defaults" }
        p { class: "hint", "Starting values for every overlay. One overlay can differ: Overlays tab, Override Global Settings." }
        FrameStyleSection { draft }
        MetricsGlobalSection { draft }
    }
}
