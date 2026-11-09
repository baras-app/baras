//! Raid frames overlay settings tab.

use dioxus::prelude::*;

use crate::components::Slider;
use crate::types::RaidOverlaySettings;

use super::draft::SettingsDraft;
use super::widgets::{Check, Group, NumberRow, OpacitySlider, ResetRow, Row, Section};

/// A px slider over one raid frame field
fn px_slider(
    draft: SettingsDraft,
    label: &'static str,
    value: f32,
    min: f64,
    max: f64,
    set: fn(&mut RaidOverlaySettings, f32),
) -> Element {
    rsx! {
        Slider {
            label,
            value: value as i32 as f64,
            min,
            max,
            suffix: "px",
            on_change: move |v: f64| draft.edit(|s| set(&mut s.raid_overlay, (v as f32).clamp(min as f32, max as f32))),
        }
    }
}

#[component]
pub fn RaidTab(draft: SettingsDraft, actions: Element) -> Element {
    let s = draft.current();
    let cfg = s.raid_overlay.clone();
    let (cols, rows) = (cfg.grid_columns, cfg.grid_rows);

    rsx! {
        div { class: "settings-section",
            Section { group: Group::Background,
                OpacitySlider { value: s.raid_opacity, on_change: move |v| draft.edit(|s| s.raid_opacity = v) }
                Check { label: "Colored Effect Borders", checked: cfg.effect_colored_borders, on_change: move |v| draft.edit(|s| s.raid_overlay.effect_colored_borders = v) }
                p { class: "hint", "Outline colored effect squares with the effect's own color" }
            }
            Section { group: Group::Layout,
                Row { label: "Columns",
                    select {
                        value: "{cols}",
                        onchange: move |e: Event<FormData>| {
                            if let Ok(val) = e.value().parse::<u8>() {
                                draft.edit(|s| s.raid_overlay.grid_columns = val.clamp(1, 6));
                            }
                        },
                        for c in [1u8, 2, 3, 4, 5, 6] {
                            option { value: "{c}", selected: cols == c, disabled: c as u16 * rows as u16 > 24, "{c}" }
                        }
                    }
                }
                Row { label: "Rows",
                    select {
                        value: "{rows}",
                        onchange: move |e: Event<FormData>| {
                            if let Ok(val) = e.value().parse::<u8>() {
                                draft.edit(|s| s.raid_overlay.grid_rows = val.clamp(1, 24));
                            }
                        },
                        for r in [1u8, 2, 4, 8, 12, 16, 20, 24] {
                            option { value: "{r}", selected: rows == r, disabled: cols as u16 * r as u16 > 24, "{r}" }
                        }
                    }
                }
                p { class: "hint", "Total slots: {cols * rows}" }
                p { class: "hint hint-subtle", "Grid changes require toggling overlay off/on" }
                {px_slider(draft, "Frame Spacing", cfg.frame_spacing, 0.0, 75.0, |c, v| c.frame_spacing = v)}
                NumberRow {
                    label: "Max Effects per Frame",
                    value: cfg.max_effects_per_frame as f64,
                    min: 1.0,
                    max: 8.0,
                    on_change: move |v: f64| draft.edit(|s| s.raid_overlay.max_effects_per_frame = v as u8),
                }
                {px_slider(draft, "Effect Size", cfg.effect_size, 8.0, 36.0, |c, v| c.effect_size = v)}
                {px_slider(draft, "Effect Vertical Offset", cfg.effect_vertical_offset, -10.0, 30.0, |c, v| c.effect_vertical_offset = v)}
                {px_slider(draft, "Effect Horizontal Offset", cfg.effect_horizontal_offset, -10.0, 30.0, |c, v| c.effect_horizontal_offset = v)}
            }
            Section { group: Group::Content,
                Check { label: "Show Role Icons", checked: cfg.show_role_icons, on_change: move |v| draft.edit(|s| s.raid_overlay.show_role_icons = v) }
                Check { label: "Show Class Icons", checked: cfg.show_class_icons, on_change: move |v| draft.edit(|s| s.raid_overlay.show_class_icons = v) }
                Check { label: "Show Effect Icons", checked: cfg.show_effect_icons, on_change: move |v| draft.edit(|s| s.raid_overlay.show_effect_icons = v) }
                p { class: "hint", "Display ability icons instead of colored squares (requires icon pack)" }
            }
            ResetRow {
                on_reset: move |_| draft.edit(|s| {
                    s.raid_overlay = RaidOverlaySettings::default();
                    s.raid_opacity = 180;
                }),
                actions,
            }
        }
    }
}
