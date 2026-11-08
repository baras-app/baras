//! Global settings shared by every metric overlay: opacity, bar sizing, class colours.

use dioxus::prelude::*;

use crate::components::Slider;
use crate::types::ClassIconMode;
use crate::utils::{color_to_hex, parse_hex_color};

use super::draft::SettingsDraft;
use super::widgets::OpacitySlider;

#[component]
pub fn MetricsGlobalSection(draft: SettingsDraft, mut open: Signal<bool>) -> Element {
    let current_settings = draft.current();
    let cc = &current_settings.class_colors;
    let cc_sorc = color_to_hex(&cc.sorcerer_sage);
    let cc_asin = color_to_hex(&cc.assassin_shadow);
    let cc_jugg = color_to_hex(&cc.juggernaut_guardian);
    let cc_mara = color_to_hex(&cc.marauder_sentinel);
    let cc_merc = color_to_hex(&cc.mercenary_commando);
    let cc_pt = color_to_hex(&cc.powertech_vanguard);
    let cc_oper = color_to_hex(&cc.operative_scoundrel);
    let cc_snip = color_to_hex(&cc.sniper_gunslinger);

    rsx! {
        div { class: if open() { "settings-section collapsible metrics-global open" } else { "settings-section collapsible metrics-global" },
            div {
                class: "collapsible-summary",
                onclick: move |_| open.set(!open()),
                i { class: "fa-solid fa-sliders summary-icon" }
                "Global Metrics Settings"
            }
            if open() {
                div { class: "collapsible-content",
                OpacitySlider {
                    label: "Background Opacity",
                    value: current_settings.metric_opacity,
                    on_change: move |val| {
                        let mut new_settings = draft.current();
                        new_settings.metric_opacity = val;
                        draft.update(new_settings);
                    },
                }

                Slider {
                    label: "Bar Thickness",
                    value: (current_settings.metric_scaling_factor * 100.0) as i32 as f64,
                    min: 30.0,
                    max: 200.0,
                    step: 5.0,
                    suffix: "%",
                    on_change: move |v: f64| {
                        let mut new_settings = draft.current();
                        new_settings.metric_scaling_factor = (v as f32 / 100.0).clamp(0.3, 2.0);
                        draft.update(new_settings);
                    },
                }

                label { class: "settings-check",
                    input {
                        r#type: "checkbox",
                        checked: current_settings.metric_show_empty_bars,
                        onchange: move |e: Event<FormData>| {
                            let mut new_settings = draft.current();
                            new_settings.metric_show_empty_bars = e.checked();
                            draft.update(new_settings);
                        }
                    }
                    "Show Empty Bars"
                }

                label { class: "settings-check",
                    input {
                        r#type: "checkbox",
                        checked: current_settings.metric_stack_from_bottom,
                        onchange: move |e: Event<FormData>| {
                            let mut new_settings = draft.current();
                            new_settings.metric_stack_from_bottom = e.checked();
                            draft.update(new_settings);
                        }
                    }
                    "Stack From Bottom"
                }

                div { class: "setting-row",
                    label { "Class Icons" }
                    select {
                        value: match current_settings.class_icon_mode {
                            ClassIconMode::None => "none",
                            ClassIconMode::Class => "class",
                            ClassIconMode::Discipline => "discipline",
                        },
                        onchange: move |e: Event<FormData>| {
                            let mut new_settings = draft.current();
                            new_settings.class_icon_mode = match e.value().as_str() {
                                "class" => ClassIconMode::Class,
                                "discipline" => ClassIconMode::Discipline,
                                _ => ClassIconMode::None,
                            };
                            draft.update(new_settings);
                        },
                        option { value: "none", "None" }
                        option { value: "class", "Class" }
                        option { value: "discipline", "Discipline" }
                    }
                }

                // ── Class Colors ─────────────────────────────────
                details { class: "collapsible",
                    summary { class: "collapsible-summary", "Class Colors" }
                    div { class: "setting-row",
                        label { "Sorcerer / Sage" }
                        input { r#type: "color", value: "{cc_sorc}", class: "color-picker",
                            oninput: move |e: Event<FormData>| {
                                if let Some(color) = parse_hex_color(&e.value()) {
                                    let mut s = draft.current();
                                    s.class_colors.sorcerer_sage = color;
                                    draft.update(s);
                                }
                            }
                        }
                    }
                    div { class: "setting-row",
                        label { "Assassin / Shadow" }
                        input { r#type: "color", value: "{cc_asin}", class: "color-picker",
                            oninput: move |e: Event<FormData>| {
                                if let Some(color) = parse_hex_color(&e.value()) {
                                    let mut s = draft.current();
                                    s.class_colors.assassin_shadow = color;
                                    draft.update(s);
                                }
                            }
                        }
                    }
                    div { class: "setting-row",
                        label { "Juggernaut / Guardian" }
                        input { r#type: "color", value: "{cc_jugg}", class: "color-picker",
                            oninput: move |e: Event<FormData>| {
                                if let Some(color) = parse_hex_color(&e.value()) {
                                    let mut s = draft.current();
                                    s.class_colors.juggernaut_guardian = color;
                                    draft.update(s);
                                }
                            }
                        }
                    }
                    div { class: "setting-row",
                        label { "Marauder / Sentinel" }
                        input { r#type: "color", value: "{cc_mara}", class: "color-picker",
                            oninput: move |e: Event<FormData>| {
                                if let Some(color) = parse_hex_color(&e.value()) {
                                    let mut s = draft.current();
                                    s.class_colors.marauder_sentinel = color;
                                    draft.update(s);
                                }
                            }
                        }
                    }
                    div { class: "setting-row",
                        label { "Mercenary / Commando" }
                        input { r#type: "color", value: "{cc_merc}", class: "color-picker",
                            oninput: move |e: Event<FormData>| {
                                if let Some(color) = parse_hex_color(&e.value()) {
                                    let mut s = draft.current();
                                    s.class_colors.mercenary_commando = color;
                                    draft.update(s);
                                }
                            }
                        }
                    }
                    div { class: "setting-row",
                        label { "Powertech / Vanguard" }
                        input { r#type: "color", value: "{cc_pt}", class: "color-picker",
                            oninput: move |e: Event<FormData>| {
                                if let Some(color) = parse_hex_color(&e.value()) {
                                    let mut s = draft.current();
                                    s.class_colors.powertech_vanguard = color;
                                    draft.update(s);
                                }
                            }
                        }
                    }
                    div { class: "setting-row",
                        label { "Operative / Scoundrel" }
                        input { r#type: "color", value: "{cc_oper}", class: "color-picker",
                            oninput: move |e: Event<FormData>| {
                                if let Some(color) = parse_hex_color(&e.value()) {
                                    let mut s = draft.current();
                                    s.class_colors.operative_scoundrel = color;
                                    draft.update(s);
                                }
                            }
                        }
                    }
                    div { class: "setting-row",
                        label { "Sniper / Gunslinger" }
                        input { r#type: "color", value: "{cc_snip}", class: "color-picker",
                            oninput: move |e: Event<FormData>| {
                                if let Some(color) = parse_hex_color(&e.value()) {
                                    let mut s = draft.current();
                                    s.class_colors.sniper_gunslinger = color;
                                    draft.update(s);
                                }
                            }
                        }
                    }
                }

                Slider {
                    label: "Font Scale",
                    value: (current_settings.metric_font_scale * 100.0) as i32 as f64,
                    min: 30.0,
                    max: 300.0,
                    step: 5.0,
                    suffix: "%",
                    on_change: move |v: f64| {
                        let mut new_settings = draft.current();
                        new_settings.metric_font_scale = (v as f32 / 100.0).clamp(0.3, 3.0);
                        draft.update(new_settings);
                    },
                }

                Slider {
                    label: "Gradient Intensity",
                    value: (current_settings.metric_gradient_intensity * 100.0) as i32 as f64,
                    min: 0.0,
                    max: 60.0,
                    step: 2.0,
                    suffix: "%",
                    on_change: move |v: f64| {
                        let mut new_settings = draft.current();
                        new_settings.metric_gradient_intensity = (v as f32 / 100.0).clamp(0.0, 0.6);
                        draft.update(new_settings);
                    },
                }

                label { class: "settings-check",
                    input {
                        r#type: "checkbox",
                        checked: current_settings.metric_dynamic_background,
                        onchange: move |e: Event<FormData>| {
                            let mut new_settings = draft.current();
                            new_settings.metric_dynamic_background = e.checked();
                            draft.update(new_settings);
                        }
                    }
                    "Dynamic Background"
                }

                label { class: "settings-check",
                    input {
                        r#type: "checkbox",
                        checked: current_settings.metric_show_background_bar,
                        onchange: move |e: Event<FormData>| {
                            let mut new_settings = draft.current();
                            new_settings.metric_show_background_bar = e.checked();
                            draft.update(new_settings);
                        }
                    }
                    "Show Background Bar"
                }
                }
            }
        }
    }
}
