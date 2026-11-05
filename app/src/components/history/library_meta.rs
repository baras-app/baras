//! Inline editors for a log's own tags and one-line note in the Log
//! Management table. The backend holds the meta, so every save sends both.

use dioxus::prelude::*;

use baras_types::history::LOG_NOTE_MAX;

use crate::api;
use crate::components::toast::ToastManager;
use crate::components::{ToastSeverity, use_toast};

/// `datalist` id the tag inputs complete from; rendered once by the table
pub const TAG_OPTIONS: &str = "log-tag-options";

fn save_meta(filename: String, tags: Vec<String>, note: String, on_change: EventHandler<()>, mut toast: ToastManager) {
    spawn(async move {
        match api::set_log_meta(&filename, &tags, &note).await {
            Ok(true) => on_change.call(()),
            _ => toast.show("Could not save log tags", ToastSeverity::Normal),
        }
    });
}

#[component]
pub fn LogTagsCell(
    filename: String,
    tags: Vec<String>,
    run_tags: Vec<String>,
    note: String,
    on_change: EventHandler<()>,
) -> Element {
    let toast = use_toast();
    let mut input = use_signal(String::new);
    let save = {
        let filename = filename.clone();
        let note = note.clone();
        move |tags: Vec<String>| save_meta(filename.clone(), tags, note.clone(), on_change, toast)
    };
    let add = {
        let tags = tags.clone();
        let save = save.clone();
        move |value: String| {
            let value = value.trim().to_string();
            input.set(String::new());
            if value.is_empty() || tags.contains(&value) {
                return;
            }
            let mut tags = tags.clone();
            tags.push(value);
            save(tags);
        }
    };

    rsx! {
        div { class: "library-tag-cell",
            for t in tags.iter() {
                {
                    let remove = {
                        let rest: Vec<String> = tags.iter().filter(|x| *x != t).cloned().collect();
                        let save = save.clone();
                        move |_| save(rest.clone())
                    };
                    rsx! {
                        span { class: "run-tag small", "{t}"
                            button { title: "Remove tag", onclick: remove, i { class: "fa-solid fa-xmark" } }
                        }
                    }
                }
            }
            for t in run_tags.iter().filter(|t| !tags.contains(*t)) {
                span { class: "run-tag small muted", title: "Tag on a run in this log", "{t}" }
            }
            input {
                class: "library-tag-input",
                r#type: "text",
                list: TAG_OPTIONS,
                placeholder: "+ tag",
                value: "{input}",
                oninput: move |e| input.set(e.value()),
                onchange: {
                    let mut add = add.clone();
                    move |e| add(e.value())
                },
            }
        }
    }
}

#[component]
pub fn LogNoteCell(filename: String, tags: Vec<String>, note: String, on_change: EventHandler<()>) -> Element {
    let toast = use_toast();
    rsx! {
        input {
            class: "library-note",
            r#type: "text",
            maxlength: "{LOG_NOTE_MAX}",
            placeholder: "Note…",
            title: "{note}",
            value: "{note}",
            onchange: move |e| {
                let value: String = e.value().trim().chars().take(LOG_NOTE_MAX).collect();
                save_meta(filename.clone(), tags.clone(), value, on_change, toast);
            },
        }
    }
}
