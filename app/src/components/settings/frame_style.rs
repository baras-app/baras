//! Window style (background, opacity, border, corners, font): the global values and
//! one overlay's overrides of them.

use baras_types::FrameStyleOverride;
use baras_types::frame_style::{MAX_BORDER_WIDTH, MAX_CORNER_RADIUS};
use dioxus::prelude::*;

use crate::api;
use crate::components::Slider;

use super::draft::SettingsDraft;
use super::widgets::{Check, ColorRow, Row};

/// Global window style, on the Global tab
#[component]
pub fn FrameStyleSection(draft: SettingsDraft) -> Element {
    let style = draft.current().frame_style;

    rsx! {
        div { class: "settings-section",
            h4 { "Window Style" }
            Slider {
                label: "Background Opacity",
                value: style.opacity as f64,
                min: 0.0,
                max: 255.0,
                on_change: move |v: f64| draft.edit(|s| s.frame_style.opacity = v.round() as u8),
            }
            ColorRow { label: "Background Color", value: style.background_color, on_change: move |v| draft.edit(|s| s.frame_style.background_color = v) }
            Slider {
                label: "Corner Radius",
                value: style.corner_radius as f64,
                min: 0.0,
                max: MAX_CORNER_RADIUS as f64,
                suffix: "px",
                on_change: move |v: f64| draft.edit(|s| s.frame_style.corner_radius = v as f32),
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
            p { class: "hint", "Applies to every overlay. Each overlay can override these under Window Style in its own options." }
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

/// One overlay's overrides, above its own options. Each value follows the
/// global style until its "Custom" box is ticked.
#[component]
pub fn FrameOverrideSection(draft: SettingsDraft, tab_key: Signal<String>) -> Element {
    let s = draft.current();
    let key = tab_key();
    let o = s.frame_overrides.get(&key).cloned().unwrap_or_default();
    let style = s.frame_style_for(&key);
    let font = s.font_family_for(&key).to_string();
    let global_font = s.overlay_font_family.clone();

    let mut system_fonts: Signal<Vec<String>> = use_signal(Vec::new);
    use_future(move || async move {
        system_fonts.set(api::list_system_fonts().await);
    });

    rsx! {
        div { class: "settings-section",
            details { class: "collapsible", open: true,
                summary { class: "collapsible-summary",
                    "Window Style"
                    if !o.is_empty() {
                        span { class: "settings-check-note", "(customized)" }
                    }
                }
                Check {
                    label: "Custom Background Opacity",
                    checked: o.opacity.is_some(),
                    on_change: move |on: bool| edit_override(draft, &tab_key(), |o| o.opacity = on.then_some(style.opacity)),
                }
                if o.opacity.is_some() {
                    Slider {
                        label: "Background Opacity",
                        value: style.opacity as f64,
                        min: 0.0,
                        max: 255.0,
                        on_change: move |v: f64| edit_override(draft, &tab_key(), |o| o.opacity = Some(v.round() as u8)),
                    }
                }
                Check {
                    label: "Custom Background Color",
                    checked: o.background_color.is_some(),
                    on_change: move |on: bool| edit_override(draft, &tab_key(), |o| o.background_color = on.then_some(style.background_color)),
                }
                if o.background_color.is_some() {
                    ColorRow { label: "Background Color", value: style.background_color, on_change: move |v| edit_override(draft, &tab_key(), |o| o.background_color = Some(v)) }
                }
                Check {
                    label: "Custom Corner Radius",
                    checked: o.corner_radius.is_some(),
                    on_change: move |on: bool| edit_override(draft, &tab_key(), |o| o.corner_radius = on.then_some(style.corner_radius)),
                }
                if o.corner_radius.is_some() {
                    Slider {
                        label: "Corner Radius",
                        value: style.corner_radius as f64,
                        min: 0.0,
                        max: MAX_CORNER_RADIUS as f64,
                        suffix: "px",
                        on_change: move |v: f64| edit_override(draft, &tab_key(), |o| o.corner_radius = Some(v as f32)),
                    }
                }
                Check {
                    label: "Custom Border",
                    checked: o.border_width.is_some(),
                    on_change: move |on: bool| edit_override(draft, &tab_key(), |o| {
                        o.border_width = on.then_some(style.border_width);
                        o.border_color = on.then_some(style.border_color);
                    }),
                }
                if o.border_width.is_some() {
                    Slider {
                        label: "Border Width",
                        value: style.border_width as f64,
                        min: 0.0,
                        max: MAX_BORDER_WIDTH as f64,
                        suffix: "px",
                        on_change: move |v: f64| edit_override(draft, &tab_key(), |o| o.border_width = Some(v as f32)),
                    }
                    ColorRow { label: "Border Color", value: style.border_color, on_change: move |v| edit_override(draft, &tab_key(), |o| o.border_color = Some(v)) }
                }
                Check {
                    label: "Custom Font",
                    checked: o.font_family.is_some(),
                    on_change: move |on: bool| {
                        let global_font = global_font.clone();
                        edit_override(draft, &tab_key(), |o| o.font_family = on.then_some(global_font));
                    },
                }
                if o.font_family.is_some() {
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
    }
}
