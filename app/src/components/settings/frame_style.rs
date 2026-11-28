//! Window style (background, opacity, border, corners, font): the global values and
//! one overlay's overrides of them.

use baras_types::frame_style::{MAX_BORDER_WIDTH, MAX_CORNER_RADIUS};
use baras_types::{FrameStyleOverride, StylePreset};
use dioxus::prelude::*;

use crate::api;
use crate::components::Slider;

use super::draft::SettingsDraft;
use super::widgets::{ColorRow, Override, Row};

/// Global window style, on the Global tab
#[component]
pub fn FrameStyleSection(draft: SettingsDraft) -> Element {
    let current_settings = draft.current();
    let style = current_settings.frame_style;

    let mut system_fonts: Signal<Vec<String>> = use_signal(Vec::new);
    use_future(move || async move {
        system_fonts.set(api::list_system_fonts().await);
    });

    rsx! {
        div { class: "settings-section",
            h4 { "Window Style" }
            div { class: "setting-row",
                label { title: "Sets opacity, border, corner radii, and metric bar radius, spacing and gradient. Colors and per-overlay overrides are kept.", "Preset" }
                div { class: "style-presets",
                    for preset in StylePreset::ALL {
                        button {
                            class: "btn btn-small",
                            onclick: move |_| draft.edit(|s| preset.apply(s)),
                            "{preset.label()}"
                        }
                    }
                }
            }
            div { class: "setting-row",
                label { "Overlay Font" }
                select {
                    value: "{current_settings.overlay_font_family}",
                    onchange: move |e: Event<FormData>| {
                        let family = e.value();
                        let mut new_settings = draft.current();
                        new_settings.overlay_font_family = family.clone();
                        draft.update(new_settings);
                        // Applies to all overlays immediately, independent of Save
                        spawn(async move {
                            let _ = api::set_overlay_font_family(family).await;
                        });
                    },
                    for font in system_fonts().iter() {
                        // `selected` (not just the select's `value`) so the active
                        // font stays chosen even though options load asynchronously.
                        option {
                            key: "{font}",
                            value: "{font}",
                            selected: *font == current_settings.overlay_font_family,
                            "{font}"
                        }
                    }
                }
            }
            Slider {
                label: "Background Opacity",
                value: style.opacity as f64,
                min: 0.0,
                max: 255.0,
                on_change: move |v: f64| draft.edit(|s| s.frame_style.opacity = v.round() as u8),
            }
            ColorRow { label: "Background Color", value: style.background_color, on_change: move |v| draft.edit(|s| s.frame_style.background_color = v) }
            Slider {
                label: "Window Corner Radius",
                value: style.corner_radius as f64,
                min: 0.0,
                max: MAX_CORNER_RADIUS as f64,
                suffix: "px",
                on_change: move |v: f64| draft.edit(|s| s.frame_style.corner_radius = v as f32),
            }
            Slider {
                label: "Element Corner Radius",
                value: style.element_radius as f64,
                min: 0.0,
                max: MAX_CORNER_RADIUS as f64,
                suffix: "px",
                on_change: move |v: f64| draft.edit(|s| s.frame_style.element_radius = v as f32),
            }
            Slider {
                label: "Border Width",
                value: style.border_width as f64,
                min: 0.0,
                max: MAX_BORDER_WIDTH as f64,
                suffix: "px",
                on_change: move |v: f64| draft.edit(|s| s.frame_style.border_width = v as f32),
            }
            ColorRow { label: "Border Color", value: style.border_color, disabled: style.border_width == 0.0, on_change: move |v| draft.edit(|s| s.frame_style.border_color = v) }
        }
    }
}

/// Edit one overlay's override, dropping the entry once nothing is overridden
fn edit_override(draft: SettingsDraft, key: &str, f: impl FnOnce(&mut FrameStyleOverride)) {
    draft.edit(|s| {
        let o = s.frame_overrides.entry(key.to_string()).or_default();
        f(o);
        if o.is_empty() {
            s.frame_overrides.remove(key);
        }
    });
}

/// One overlay's window style overrides, as rows of the overrides section
#[component]
pub fn FrameOverrideRows(draft: SettingsDraft, tab_key: Signal<String>) -> Element {
    let s = draft.current();
    let key = tab_key();
    let o = s.frame_overrides.get(&key).cloned().unwrap_or_default();
    let style = s.frame_style_for(&key);
    let font = s.font_family_for(&key).to_string();

    let mut system_fonts: Signal<Vec<String>> = use_signal(Vec::new);
    use_future(move || async move {
        system_fonts.set(api::list_system_fonts().await);
    });

    rsx! {
        Override {
            custom: o.opacity.is_some(),
            on_reset: move |_| edit_override(draft, &tab_key(), |o| o.opacity = None),
            Slider {
                label: "Background Opacity",
                value: style.opacity as f64,
                min: 0.0,
                max: 255.0,
                on_change: move |v: f64| edit_override(draft, &tab_key(), |o| o.opacity = Some(v.round() as u8)),
            }
        }
        Override {
            custom: o.background_color.is_some(),
            on_reset: move |_| edit_override(draft, &tab_key(), |o| o.background_color = None),
            ColorRow {
                label: "Background Color",
                value: style.background_color,
                on_change: move |v| edit_override(draft, &tab_key(), |o| o.background_color = Some(v)),
            }
        }
        Override {
            custom: o.corner_radius.is_some(),
            on_reset: move |_| edit_override(draft, &tab_key(), |o| o.corner_radius = None),
            Slider {
                label: "Window Corner Radius",
                value: style.corner_radius as f64,
                min: 0.0,
                max: MAX_CORNER_RADIUS as f64,
                suffix: "px",
                on_change: move |v: f64| edit_override(draft, &tab_key(), |o| o.corner_radius = Some(v as f32)),
            }
        }
        Override {
            custom: o.element_radius.is_some(),
            on_reset: move |_| edit_override(draft, &tab_key(), |o| o.element_radius = None),
            Slider {
                label: "Element Corner Radius",
                value: style.element_radius as f64,
                min: 0.0,
                max: MAX_CORNER_RADIUS as f64,
                suffix: "px",
                on_change: move |v: f64| edit_override(draft, &tab_key(), |o| o.element_radius = Some(v as f32)),
            }
        }
        Override {
            custom: o.border_width.is_some(),
            on_reset: move |_| edit_override(draft, &tab_key(), |o| o.border_width = None),
            Slider {
                label: "Border Width",
                value: style.border_width as f64,
                min: 0.0,
                max: MAX_BORDER_WIDTH as f64,
                suffix: "px",
                on_change: move |v: f64| edit_override(draft, &tab_key(), |o| o.border_width = Some(v as f32)),
            }
        }
        Override {
            custom: o.border_color.is_some(),
            on_reset: move |_| edit_override(draft, &tab_key(), |o| o.border_color = None),
            ColorRow {
                label: "Border Color",
                value: style.border_color,
                on_change: move |v| edit_override(draft, &tab_key(), |o| o.border_color = Some(v)),
            }
        }
        Override {
            custom: o.font_family.is_some(),
            on_reset: move |_| edit_override(draft, &tab_key(), |o| o.font_family = None),
            Row { label: "Font",
                select {
                    value: "{font}",
                    onchange: move |e: Event<FormData>| edit_override(draft, &tab_key(), |o| o.font_family = Some(e.value())),
                    for f in system_fonts().iter() {
                        option { key: "{f}", value: "{f}", selected: *f == font, "{f}" }
                    }
                }
            }
        }
    }
}
