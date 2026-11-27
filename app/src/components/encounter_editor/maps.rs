//! Maps tab: the replay maps of a boss, one entry per floor or room.
//!
//! Entries are saved as a whole list: their order decides which map an
//! entity is on when several contain it.

use dioxus::prelude::*;

use baras_types::{MapDefinition, MapRect};

use crate::api;
use crate::components::editor_drawer::EditorDrawer;
use crate::components::form_fields::{CheckField, NumberField, PickField, SubSection, TextField};
use crate::types::BossWithPath;

/// A number box's text for an optional value
fn opt_text(value: Option<f32>) -> String {
    value.map(|v| v.to_string()).unwrap_or_default()
}

/// Blank clears an optional value; text that is not a number leaves it alone
fn parse_opt(text: &str, current: Option<f32>) -> Option<f32> {
    if text.trim().is_empty() {
        None
    } else {
        text.trim().parse().ok().or(current)
    }
}

/// The four edges of a rectangle as number rows
#[component]
fn RectFields(rect: MapRect, on_change: EventHandler<MapRect>) -> Element {
    let edge = move |label: &'static str, value: f32, set: fn(&mut MapRect, f32)| {
        rsx! {
            NumberField {
                label,
                value: value.to_string(),
                on_input: move |text: String| {
                    if let Ok(v) = text.trim().parse::<f32>() {
                        let mut next = rect;
                        set(&mut next, v);
                        on_change.call(next);
                    }
                },
            }
        }
    };
    rsx! {
        {edge("Min X", rect.min_x, |r, v| r.min_x = v)}
        {edge("Max X", rect.max_x, |r, v| r.max_x = v)}
        {edge("Min Y", rect.min_y, |r, v| r.min_y = v)}
        {edge("Max Y", rect.max_y, |r, v| r.max_y = v)}
    }
}

#[component]
pub fn MapsTab(
    boss_with_path: BossWithPath,
    on_change: EventHandler<Vec<MapDefinition>>,
    on_status: EventHandler<(String, bool)>,
) -> Element {
    let mut maps = use_signal(|| boss_with_path.boss.maps.clone());
    let mut is_dirty = use_signal(|| false);
    let mut is_saving = use_signal(|| false);
    let phase_ids: Vec<String> = boss_with_path.boss.phases.iter().map(|p| p.id.clone()).collect();

    let boss_id = boss_with_path.boss.id.clone();
    let file_path = boss_with_path.file_path.clone();
    let save = use_callback(move |_: ()| {
        if !is_dirty() || is_saving() {
            return;
        }
        let to_save = maps();
        let (boss_id, file_path) = (boss_id.clone(), file_path.clone());
        is_saving.set(true);
        spawn(async move {
            match api::update_boss_maps(&boss_id, &file_path, &to_save).await {
                Ok(_) => {
                    on_change.call(to_save);
                    is_dirty.set(false);
                    on_status.call(("Maps saved".to_string(), false));
                }
                Err(e) => on_status.call((format!("Failed to save: {}", e), true)),
            }
            is_saving.set(false);
        });
    });
    let mut edit = move |index: usize, change: &dyn Fn(&mut MapDefinition)| {
        change(&mut maps.write()[index]);
        is_dirty.set(true);
    };
    let count = maps.read().len();

    rsx! {
        div { class: "notes-tab text-sm text-muted",
            p { "Maps are what the Replay tab draws this fight on: one entry per floor or room." }
            p {
                "An entity is on the first entry whose bounds, height range and phases contain it, so order matters. "
                "Bounds are the world area the whole image covers; Min X is the image's left edge and Min Y its top edge."
            }
            p { "Images are files in the definitions/maps folder of the config directory." }
        }
        EditorDrawer {
            title: "Replay Maps".to_string(),
            dirty: is_dirty(),
            actions: rsx! {
                button {
                    class: "btn btn-sm",
                    onclick: move |_| {
                        maps.write().push(MapDefinition::default());
                        is_dirty.set(true);
                    },
                    "Add map"
                }
                button {
                    class: "btn btn-success btn-sm",
                    disabled: !is_dirty() || is_saving(),
                    onclick: move |_| save.call(()),
                    if is_saving() { "Saving..." } else { "Save" }
                }
            },
            if count == 0 {
                div { class: "text-sm text-muted", "No maps. Replay is not offered for this boss until it has one." }
            }
            for (index, map) in maps().into_iter().enumerate() {
                SubSection { key: "{index}", title: "Map",
                    TextField {
                        label: "Image",
                        help: "File name in the definitions/maps folder, e.g. dp_tyrans.png",
                        value: map.image.clone(),
                        placeholder: "file.png",
                        on_input: move |v: String| edit(index, &|m| m.image = v.trim().to_string()),
                    }
                    TextField {
                        label: "Label",
                        help: "Name on the map tabs. Blank uses the image's file name.",
                        value: map.label.clone().unwrap_or_default(),
                        on_input: move |v: String| edit(index, &|m| m.label = (!v.trim().is_empty()).then(|| v.clone())),
                    }
                    RectFields {
                        rect: map.bounds,
                        on_change: move |rect| edit(index, &|m| m.bounds = rect),
                    }
                    NumberField {
                        label: "Rotation",
                        help: "Degrees the image is turned relative to the world axes. Blank for none.",
                        value: opt_text(map.rotation),
                        unit: "deg",
                        on_input: move |v: String| edit(index, &|m| m.rotation = parse_opt(&v, m.rotation)),
                    }
                    NumberField {
                        label: "Min height",
                        help: "Lowest z this entry covers, for stacked floors. Blank for no limit.",
                        value: opt_text(map.min_z),
                        on_input: move |v: String| edit(index, &|m| m.min_z = parse_opt(&v, m.min_z)),
                    }
                    NumberField {
                        label: "Max height",
                        help: "Highest z this entry covers. Blank for no limit.",
                        value: opt_text(map.max_z),
                        on_input: move |v: String| edit(index, &|m| m.max_z = parse_opt(&v, m.max_z)),
                    }
                    PickField {
                        label: "Phases",
                        help: "Limit this entry to some phases, when coordinates alone cannot tell two places apart.",
                        none_label: "Any phase",
                        noun: "phases",
                        options: phase_ids.clone(),
                        picked: map.phases.clone(),
                        on_change: move |picked: Vec<String>| edit(index, &|m| m.phases = picked.clone()),
                    }
                    CheckField {
                        label: "Open zoomed in",
                        help: "For a boss on a very large map: the part of the map to open on.",
                        checked: map.view.is_some(),
                        on_change: move |on: bool| edit(index, &|m| m.view = on.then_some(m.bounds)),
                    }
                    if let Some(view) = map.view {
                        RectFields {
                            rect: view,
                            on_change: move |rect| edit(index, &|m| m.view = Some(rect)),
                        }
                    }
                    div { class: "replay-map-entry-actions",
                        button {
                            class: "btn btn-sm",
                            disabled: index == 0,
                            title: "Checked earlier",
                            onclick: move |_| {
                                maps.write().swap(index, index - 1);
                                is_dirty.set(true);
                            },
                            "Move up"
                        }
                        button {
                            class: "btn btn-sm",
                            disabled: index + 1 == count,
                            title: "Checked later",
                            onclick: move |_| {
                                maps.write().swap(index, index + 1);
                                is_dirty.set(true);
                            },
                            "Move down"
                        }
                        button {
                            class: "btn btn-sm btn-danger",
                            onclick: move |_| {
                                maps.write().remove(index);
                                is_dirty.set(true);
                            },
                            "Delete"
                        }
                    }
                }
            }
        }
    }
}
