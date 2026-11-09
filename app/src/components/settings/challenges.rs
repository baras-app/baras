//! Raid challenges overlay settings tab.

use dioxus::prelude::*;

use crate::components::Slider;
use crate::types::ChallengeLayout;

use super::draft::SettingsDraft;
use super::widgets::{Check, ColorRow, DynamicBackground, FontScale, Group, OpacitySlider, ResetRow, Row, Section, SelectRow};

#[component]
pub fn ChallengesTab(draft: SettingsDraft, actions: Element) -> Element {
    let s = draft.current();
    let cfg = s.challenge_overlay.clone();
    let horizontal = matches!(cfg.layout, ChallengeLayout::Horizontal);
    let direction = if horizontal { "horizontal" } else { "vertical" }.to_string();
    let stack_from = if cfg.stack_from_end { "end" } else { "start" }.to_string();
    let stack_options = if horizontal { vec![("start", "Left"), ("end", "Right")] } else { vec![("start", "Top"), ("end", "Bottom")] };

    rsx! {
        div { class: "settings-section",
            Section { group: Group::Background,
                OpacitySlider { value: s.challenge_opacity, on_change: move |v| draft.edit(|s| s.challenge_opacity = v) }
                DynamicBackground { checked: cfg.dynamic_background, on_change: move |v| draft.edit(|s| s.challenge_overlay.dynamic_background = v) }
                ColorRow { label: "Default Bar Color", value: cfg.default_bar_color, on_change: move |v| draft.edit(|s| s.challenge_overlay.default_bar_color = v) }
                Check { label: "Show Background Bar", checked: cfg.show_background_bar, on_change: move |v| draft.edit(|s| s.challenge_overlay.show_background_bar = v) }
                Check { label: "Gradient Bars", checked: cfg.bar_gradient, on_change: move |v| draft.edit(|s| s.challenge_overlay.bar_gradient = v) }
            }
            Section { group: Group::Layout,
                SelectRow {
                    label: "Direction",
                    value: direction,
                    options: vec![("vertical", "Vertical (stacked)"), ("horizontal", "Horizontal (side-by-side)")],
                    on_change: move |v: String| draft.edit(|s| {
                        s.challenge_overlay.layout = if v == "horizontal" { ChallengeLayout::Horizontal } else { ChallengeLayout::Vertical };
                    }),
                }
                Row { label: "Max Displayed",
                    select {
                        value: "{cfg.max_display}",
                        onchange: move |e: Event<FormData>| {
                            if let Ok(val) = e.value().parse::<u8>() {
                                draft.edit(|s| s.challenge_overlay.max_display = val.clamp(1, 8));
                            }
                        },
                        for n in 1..=8u8 {
                            option { value: "{n}", selected: cfg.max_display == n, "{n}" }
                        }
                    }
                }
                Row {
                    label: "Sized to Fit",
                    tooltip: "The number of challenge cards to reserve space for. Cards keep this fixed size (1/N of the window) instead of stretching to fill it, and only compress when more challenges than this appear.",
                    select {
                        onchange: move |e: Event<FormData>| {
                            if let Ok(val) = e.value().parse::<u8>() {
                                draft.edit(|s| s.challenge_overlay.visible_challenges = val.min(8));
                            }
                        },
                        option { value: "0", selected: cfg.visible_challenges == 0, "Auto (fill window)" }
                        for n in 1..=8u8 {
                            option { value: "{n}", selected: cfg.visible_challenges == n, "{n}" }
                        }
                    }
                }
                SelectRow {
                    label: "Stack From",
                    tooltip: "Which edge challenge cards are anchored to when they don't fill the window.",
                    value: stack_from,
                    options: stack_options,
                    on_change: move |v: String| draft.edit(|s| s.challenge_overlay.stack_from_end = v == "end"),
                }
                Slider {
                    label: "Bar Spacing",
                    value: (cfg.bar_spacing_ratio * 100.0) as i32 as f64,
                    min: 0.0,
                    max: 60.0,
                    step: 5.0,
                    suffix: "%",
                    on_change: move |v: f64| draft.edit(|s| s.challenge_overlay.bar_spacing_ratio = (v as f32 / 100.0).clamp(0.0, 0.6)),
                }
            }
            Section { group: Group::Content,
                Check { label: "Show Footer Totals", checked: cfg.show_footer, on_change: move |v| draft.edit(|s| s.challenge_overlay.show_footer = v) }
                Check { label: "Show Duration", checked: cfg.show_duration, on_change: move |v| draft.edit(|s| s.challenge_overlay.show_duration = v) }
            }
            Section { group: Group::Text,
                FontScale { value: cfg.font_scale, on_change: move |v| draft.edit(|s| s.challenge_overlay.font_scale = v) }
                ColorRow { label: "Font Color", value: cfg.font_color, on_change: move |v| draft.edit(|s| s.challenge_overlay.font_color = v) }
            }
            ResetRow {
                on_reset: move |_| draft.edit(|s| {
                    s.challenge_overlay = Default::default();
                    s.challenge_opacity = 180;
                }),
                actions,
            }
            p { class: "hint",
                i { class: "fa-solid fa-info-circle" }
                " Per-challenge settings (columns, color, enabled) are configured in the Encounter Editor."
            }
        }
    }
}
