//! Raid frames overlay settings tab.

use dioxus::prelude::*;

use crate::components::Slider;
use crate::types::RaidOverlaySettings;

use super::draft::SettingsDraft;
use super::widgets::OpacitySlider;

#[component]
pub fn RaidTab(draft: SettingsDraft, actions: Element) -> Element {
    let current_settings = draft.current();

    rsx! {
        // Raid Settings
        {
            let cols = current_settings.raid_overlay.grid_columns;
            let rows = current_settings.raid_overlay.grid_rows;
            rsx! {
                div { class: "settings-section",
                    h4 { "Grid Layout" }

                    div { class: "setting-row",
                        label { "Columns" }
                        select {
                            value: "{cols}",
                            onchange: move |e: Event<FormData>| {
                                if let Ok(val) = e.value().parse::<u8>() {
                                    let mut new_settings = draft.current();
                                    new_settings.raid_overlay.grid_columns = val.clamp(1, 6);
                                    draft.update(new_settings);
                                }
                            },
                            for c in [1u8, 2, 3, 4, 5, 6] {
                                option {
                                    value: "{c}",
                                    selected: cols == c,
                                    disabled: c as u16 * rows as u16 > 24,
                                    "{c}"
                                }
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Rows" }
                        select {
                            value: "{rows}",
                            onchange: move |e: Event<FormData>| {
                                if let Ok(val) = e.value().parse::<u8>() {
                                    let mut new_settings = draft.current();
                                    new_settings.raid_overlay.grid_rows = val.clamp(1, 24);
                                    draft.update(new_settings);
                                }
                            },
                            for r in [1u8, 2, 4, 8, 12, 16, 20, 24] {
                                option {
                                    value: "{r}",
                                    selected: rows == r,
                                    disabled: cols as u16 * r as u16 > 24,
                                    "{r}"
                                }
                            }
                        }
                    }

                    Slider {
                        label: "Frame Spacing",
                        value: current_settings.raid_overlay.frame_spacing as i32 as f64,
                        min: 0.0,
                        max: 75.0,
                        suffix: "px",
                        on_change: move |v: f64| {
                            let mut new_settings = draft.current();
                            new_settings.raid_overlay.frame_spacing = (v as f32).clamp(0.0, 75.0);
                            draft.update(new_settings);
                        },
                    }

                    div { class: "setting-row",
                        span { class: "hint", "Total slots: {cols * rows}" }
                    }
                    div { class: "setting-row",
                        span { class: "hint hint-subtle", "Grid changes require toggling overlay off/on" }
                    }

                    h4 { "Appearance" }

                    OpacitySlider {
                        label: "Background Opacity",
                        value: current_settings.raid_opacity,
                        on_change: move |val| {
                            let mut new_settings = draft.current();
                            new_settings.raid_opacity = val;
                            draft.update(new_settings);
                        },
                    }

                    div { class: "setting-row",
                        label { "Max Effects per Frame" }
                        input {
                            r#type: "number",
                            min: "1",
                            max: "8",
                            value: "{current_settings.raid_overlay.max_effects_per_frame}",
                            onchange: move |e: Event<FormData>| {
                                if let Ok(val) = e.value().parse::<u8>() {
                                    let mut new_settings = draft.current();
                                    new_settings.raid_overlay.max_effects_per_frame = val.clamp(1, 8);
                                    draft.update(new_settings);
                                }
                            }
                        }
                    }

                    Slider {
                        label: "Effect Size",
                        value: current_settings.raid_overlay.effect_size as i32 as f64,
                        min: 8.0,
                        max: 36.0,
                        suffix: "px",
                        on_change: move |v: f64| {
                            let mut new_settings = draft.current();
                            new_settings.raid_overlay.effect_size = (v as f32).clamp(8.0, 36.0);
                            draft.update(new_settings);
                        },
                    }

                    Slider {
                        label: "Effect Vertical Offset",
                        value: current_settings.raid_overlay.effect_vertical_offset as i32 as f64,
                        min: -10.0,
                        max: 30.0,
                        suffix: "px",
                        on_change: move |v: f64| {
                            let mut new_settings = draft.current();
                            new_settings.raid_overlay.effect_vertical_offset = (v as f32).clamp(-10.0, 30.0);
                            draft.update(new_settings);
                        },
                    }

                    Slider {
                        label: "Effect Horizontal Offset",
                        value: current_settings.raid_overlay.effect_horizontal_offset as i32 as f64,
                        min: -10.0,
                        max: 30.0,
                        suffix: "px",
                        on_change: move |v: f64| {
                            let mut new_settings = draft.current();
                            new_settings.raid_overlay.effect_horizontal_offset = (v as f32).clamp(-10.0, 30.0);
                            draft.update(new_settings);
                        },
                    }

                    div { class: "setting-row",
                        label { "Show Role Icons" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.raid_overlay.show_role_icons,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft.current();
                                new_settings.raid_overlay.show_role_icons = e.checked();
                                draft.update(new_settings);
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Show Class Icons" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.raid_overlay.show_class_icons,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft.current();
                                new_settings.raid_overlay.show_class_icons = e.checked();
                                draft.update(new_settings);
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Show Effect Icons" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.raid_overlay.show_effect_icons,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft.current();
                                new_settings.raid_overlay.show_effect_icons = e.checked();
                                draft.update(new_settings);
                            }
                        }
                    }
                    p { class: "hint", "Display ability icons instead of colored squares (requires icon pack)" }

                    div { class: "setting-row",
                        label { "Colored Effect Borders" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.raid_overlay.effect_colored_borders,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft.current();
                                new_settings.raid_overlay.effect_colored_borders = e.checked();
                                draft.update(new_settings);
                            }
                        }
                    }
                    p { class: "hint", "Outline colored effect squares with the effect's own color" }

                    div { class: "setting-row reset-row",
                        button {
                            class: "btn btn-reset",
                            onclick: move |_| {
                                let mut new_settings = draft.current();
                                new_settings.raid_overlay = RaidOverlaySettings::default();
                                new_settings.raid_opacity = 180;
                                draft.update(new_settings);
                            },
                            i { class: "fa-solid fa-rotate-left" }
                            span { " Reset Style" }
                        }
                        {actions}
                    }
                }
            }
        }
    }
}
