//! Two-step sound picker for timer/effect audio cues.
//!
//! A category dropdown plus a searchable filter narrow the second dropdown
//! of sound files. The picked value is persisted as `"folder/filename.mp3"`
//! (e.g. `"mechanic-sounds/Acid Deluge.mp3"`); legacy bare filenames stay
//! readable since the backend resolver falls back to the General `sounds/`
//! folder when no folder prefix is present.

use dioxus::prelude::*;
use wasm_bindgen_futures::spawn_local as spawn;

use crate::api;
use crate::components::form_fields::Field;
use crate::types::SoundCategory;

#[derive(Props, Clone, PartialEq)]
pub struct SoundPickerProps {
    /// The persisted audio file reference, or `None` for no selection.
    pub value: Option<String>,

    /// Fires when the user changes the selection, picks a new file via
    /// Browse, or clears it to `(none)`.
    pub on_change: EventHandler<Option<String>>,
}

#[component]
pub fn SoundPicker(props: SoundPickerProps) -> Element {
    let mut categories = use_signal(Vec::<SoundCategory>::new);
    let mut category_idx = use_signal(|| 0usize);
    let mut search = use_signal(String::new);

    let initial_value = props.value.clone();
    use_future(move || {
        let initial = initial_value.clone();
        async move {
            let cats = api::list_sound_files().await;
            if let Some(val) = initial.as_ref()
                && let Some(idx) = find_category_for_value(&cats, val)
            {
                category_idx.set(idx);
            }
            categories.set(cats);
        }
    });

    let on_change = props.on_change;
    let cats = categories();
    let active_cat = cats.get(category_idx()).cloned();
    let search_lc = search().to_lowercase();
    let current_value = props.value.clone().unwrap_or_default();
    let current_value_for_select = current_value.clone();
    let current_value_for_preview = current_value.clone();
    let active_cat_for_options = active_cat.clone();

    let has_file = !current_value_for_preview.is_empty();

    rsx! {
        Field { label: "Category",
            select {
                value: "{category_idx()}",
                onchange: move |e| {
                    if let Ok(idx) = e.value().parse::<usize>() {
                        category_idx.set(idx);
                        search.set(String::new());
                    }
                },
                for (i, cat) in cats.iter().enumerate() {
                    option { key: "{cat.folder}", value: "{i}", "{cat.name}" }
                }
            }
            input {
                r#type: "text",
                class: "field-short",
                placeholder: "Filter",
                value: "{search}",
                oninput: move |e| search.set(e.value()),
            }
        }
        Field { label: "Sound",
            select {
                value: "{current_value_for_select}",
                onchange: move |e| {
                    let v = e.value();
                    on_change.call(if v.is_empty() { None } else { Some(v) });
                },
                option { value: "", "(none)" }
                if let Some(cat) = active_cat_for_options.as_ref() {
                    for name in cat.files.iter() {
                        {
                            let opt_value = format!("{}/{}", cat.folder, name);
                            let is_selected = matches_selected(&current_value, &opt_value, name, cat.folder == "sounds");
                            let matches_search = search_lc.is_empty() || name.to_lowercase().contains(&search_lc);
                            rsx! {
                                if matches_search || is_selected {
                                    option { key: "{opt_value}", value: "{opt_value}", selected: is_selected, "{name}" }
                                }
                            }
                        }
                    }
                }
                if !current_value.is_empty() && !value_in_current_view(&current_value, active_cat.as_ref()) {
                    option { value: "{current_value}", selected: true, "{current_value} (custom)" }
                }
            }
            button {
                class: "btn btn-sm btn-icon",
                r#type: "button",
                disabled: !has_file,
                title: if has_file { "Preview sound" } else { "Select a sound first" },
                onclick: move |_| {
                    let file = current_value_for_preview.clone();
                    if !file.is_empty() {
                        spawn(async move { api::preview_sound(&file).await; });
                    }
                },
                i { class: "fa-solid fa-play" }
            }
            button {
                class: "btn btn-sm btn-icon",
                r#type: "button",
                title: "Choose an .mp3 or .wav file",
                onclick: move |_| {
                    spawn(async move {
                        if let Some(path) = api::pick_audio_file().await {
                            let lower = path.to_lowercase();
                            if lower.ends_with(".mp3") || lower.ends_with(".wav") {
                                on_change.call(Some(path));
                            }
                        }
                    });
                },
                i { class: "fa-solid fa-folder-open" }
            }
        }
    }
}

fn matches_selected(current: &str, opt_value: &str, name: &str, is_sounds_folder: bool) -> bool {
    current == opt_value || (is_sounds_folder && current == name)
}

fn value_in_current_view(value: &str, cat: Option<&SoundCategory>) -> bool {
    let Some(cat) = cat else {
        return false;
    };
    let prefix = format!("{}/", cat.folder);
    if let Some(name) = value.strip_prefix(prefix.as_str()) {
        return cat.files.iter().any(|n| n == name);
    }
    cat.folder == "sounds" && cat.files.iter().any(|n| n == value)
}

fn find_category_for_value(cats: &[SoundCategory], value: &str) -> Option<usize> {
    if value.starts_with('/') || value.contains(":\\") {
        return None;
    }
    if let Some((folder, _)) = value.split_once('/') {
        return cats.iter().position(|c| c.folder == folder);
    }
    cats.iter().position(|c| c.files.iter().any(|n| n == value))
}
