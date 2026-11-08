//! Raid challenges overlay settings tab.

use dioxus::prelude::*;

use crate::components::Slider;
use crate::types::ChallengeLayout;
use crate::utils::{color_to_hex, parse_hex_color};

use super::draft::SettingsDraft;
use super::widgets::OpacitySlider;

#[component]
pub fn ChallengesTab(draft: SettingsDraft, actions: Element) -> Element {
    let current_settings = draft.current();

    rsx! {
        // Challenges Settings (global overlay settings)
        div { class: "settings-section",
            h4 { "Appearance" }

            OpacitySlider {
                label: "Background Opacity",
                value: current_settings.challenge_opacity,
                on_change: move |val| {
                    let mut new_settings = draft.current();
                    new_settings.challenge_opacity = val;
                    draft.update(new_settings);
                },
            }

            h4 { "Layout" }

            {
                let challenge_config = current_settings.challenge_overlay.clone();
                let font_hex = color_to_hex(&challenge_config.font_color);
                let bar_hex = color_to_hex(&challenge_config.default_bar_color);

                rsx! {
                    // Layout direction
                    div { class: "setting-row",
                        label { "Direction" }
                        select {
                            class: "input-inline",
                            value: match challenge_config.layout {
                                ChallengeLayout::Vertical => "vertical",
                                ChallengeLayout::Horizontal => "horizontal",
                            },
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft.current();
                                new_settings.challenge_overlay.layout = match e.value().as_str() {
                                    "horizontal" => ChallengeLayout::Horizontal,
                                    _ => ChallengeLayout::Vertical,
                                };
                                draft.update(new_settings);
                            },
                            option { value: "vertical", selected: matches!(challenge_config.layout, ChallengeLayout::Vertical), "Vertical (stacked)" }
                            option { value: "horizontal", selected: matches!(challenge_config.layout, ChallengeLayout::Horizontal), "Horizontal (side-by-side)" }
                        }
                    }

                    // Max challenges to display
                    div { class: "setting-row",
                        label { "Max Displayed" }
                        select {
                            class: "input-inline",
                            value: "{challenge_config.max_display}",
                            onchange: move |e: Event<FormData>| {
                                if let Ok(val) = e.value().parse::<u8>() {
                                    let mut new_settings = draft.current();
                                    new_settings.challenge_overlay.max_display = val.clamp(1, 8);
                                    draft.update(new_settings);
                                }
                            },
                            for n in 1..=8u8 {
                                option { value: "{n}", selected: challenge_config.max_display == n, "{n}" }
                            }
                        }
                    }

                    // Reserve space for N challenge cards
                    div { class: "setting-row",
                        label {
                            title: "The number of challenge cards to reserve space for. Cards keep this fixed size (1/N of the window) instead of stretching to fill it, and only compress when more challenges than this appear.",
                            "Sized to Fit"
                        }
                        select {
                            class: "input-inline",
                            onchange: move |e: Event<FormData>| {
                                if let Ok(val) = e.value().parse::<u8>() {
                                    let mut new_settings = draft.current();
                                    new_settings.challenge_overlay.visible_challenges = val.min(8);
                                    draft.update(new_settings);
                                }
                            },
                            option { value: "0", selected: challenge_config.visible_challenges == 0, "Auto (fill window)" }
                            for n in 1..=8u8 {
                                option { value: "{n}", selected: challenge_config.visible_challenges == n, "{n}" }
                            }
                        }
                    }

                    // Which edge cards stack from (label follows layout direction)
                    div { class: "setting-row",
                        label {
                            title: "Which edge challenge cards are anchored to when they don't fill the window.",
                            "Stack From"
                        }
                        select {
                            class: "input-inline",
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft.current();
                                new_settings.challenge_overlay.stack_from_end = e.value() == "end";
                                draft.update(new_settings);
                            },
                            option {
                                value: "start",
                                selected: !challenge_config.stack_from_end,
                                {match challenge_config.layout {
                                    ChallengeLayout::Horizontal => "Left",
                                    ChallengeLayout::Vertical => "Top",
                                }}
                            }
                            option {
                                value: "end",
                                selected: challenge_config.stack_from_end,
                                {match challenge_config.layout {
                                    ChallengeLayout::Horizontal => "Right",
                                    ChallengeLayout::Vertical => "Bottom",
                                }}
                            }
                        }
                    }

                    h4 { "Display Options" }

                    // Show footer
                    label { class: "settings-check",
                        input {
                            r#type: "checkbox",
                            checked: challenge_config.show_footer,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft.current();
                                new_settings.challenge_overlay.show_footer = e.checked();
                                draft.update(new_settings);
                            }
                        }
                        "Show Footer Totals"
                    }

                    // Show duration
                    label { class: "settings-check",
                        input {
                            r#type: "checkbox",
                            checked: challenge_config.show_duration,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft.current();
                                new_settings.challenge_overlay.show_duration = e.checked();
                                draft.update(new_settings);
                            }
                        }
                        "Show Duration"
                    }

                    label { class: "settings-check",
                        input {
                            r#type: "checkbox",
                            checked: challenge_config.show_background_bar,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft.current();
                                new_settings.challenge_overlay.show_background_bar = e.checked();
                                draft.update(new_settings);
                            }
                        }
                        "Show Background Bar"
                    }

                    label { class: "settings-check",
                        input {
                            r#type: "checkbox",
                            checked: challenge_config.bar_gradient,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft.current();
                                new_settings.challenge_overlay.bar_gradient = e.checked();
                                draft.update(new_settings);
                            }
                        }
                        "Gradient Bars"
                    }

                    Slider {
                        label: "Bar Spacing",
                        value: (challenge_config.bar_spacing_ratio * 100.0) as i32 as f64,
                        min: 0.0,
                        max: 60.0,
                        step: 5.0,
                        suffix: "%",
                        on_change: move |v: f64| {
                            let mut new_settings = draft.current();
                            new_settings.challenge_overlay.bar_spacing_ratio = (v as f32 / 100.0).clamp(0.0, 0.6);
                            draft.update(new_settings);
                        },
                    }

                    h4 { "Colors" }

                    // Default bar color
                    div { class: "setting-row",
                        label { "Default Bar Color" }
                        input {
                            r#type: "color",
                            value: "{bar_hex}",
                            class: "color-picker",
                            oninput: move |e: Event<FormData>| {
                                if let Some(color) = parse_hex_color(&e.value()) {
                                    let mut new_settings = draft.current();
                                    new_settings.challenge_overlay.default_bar_color = color;
                                    draft.update(new_settings);
                                }
                            }
                        }
                    }

                    // Font color
                    div { class: "setting-row",
                        label { "Font Color" }
                        input {
                            r#type: "color",
                            value: "{font_hex}",
                            class: "color-picker",
                            oninput: move |e: Event<FormData>| {
                                if let Some(color) = parse_hex_color(&e.value()) {
                                    let mut new_settings = draft.current();
                                    new_settings.challenge_overlay.font_color = color;
                                    draft.update(new_settings);
                                }
                            }
                        }
                    }

            Slider {
                label: "Font Scale",
                value: (current_settings.challenge_overlay.font_scale * 100.0) as i32 as f64,
                min: 30.0,
                max: 300.0,
                step: 10.0,
                suffix: "%",
                on_change: move |v: f64| {
                    let mut new_settings = draft.current();
                    new_settings.challenge_overlay.font_scale = (v as f32 / 100.0).clamp(0.3, 3.0);
                    draft.update(new_settings);
                },
            }
            label { class: "settings-check",
                input {
                    r#type: "checkbox",
                    checked: current_settings.challenge_overlay.dynamic_background,
                    onchange: move |e: Event<FormData>| {
                        let mut new_settings = draft.current();
                        new_settings.challenge_overlay.dynamic_background = e.checked();
                        draft.update(new_settings);
                    }
                }
                "Dynamic Background"
            }

                    // Reset button
                    div { class: "setting-row reset-row",
                        button {
                            class: "btn btn-reset",
                            onclick: move |_| {
                                let mut new_settings = draft.current();
                                new_settings.challenge_overlay = Default::default();
                                new_settings.challenge_opacity = 180;
                                draft.update(new_settings);
                            },
                            i { class: "fa-solid fa-rotate-left" }
                            span { " Reset to Defaults" }
                        }
                        {actions}
                    }

                    // Hint about per-challenge settings
                    p { class: "hint",
                        i { class: "fa-solid fa-info-circle" }
                        " Per-challenge settings (columns, color, enabled) are configured in the Encounter Editor."
                    }
                }
            }
        }
    }
}
