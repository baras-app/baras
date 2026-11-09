//! Notes editing tab
//!
//! Freeform Markdown text editor for encounter notes.
//! Notes are displayed on the Notes overlay during gameplay.

use dioxus::prelude::*;

use crate::api;
use crate::components::editor_drawer::EditorDrawer;
use crate::types::BossWithPath;

// ─────────────────────────────────────────────────────────────────────────────
// Notes Tab
// ─────────────────────────────────────────────────────────────────────────────

#[component]
pub fn NotesTab(
    boss_with_path: BossWithPath,
    on_change: EventHandler<Option<String>>,
    on_status: EventHandler<(String, bool)>,
) -> Element {
    let mut notes_text = use_signal(|| boss_with_path.boss.notes.clone().unwrap_or_default());
    let mut is_dirty = use_signal(|| false);
    let mut is_saving = use_signal(|| false);

    let boss_id = boss_with_path.boss.id.clone();
    let file_path = boss_with_path.file_path.clone();
    // Button and Ctrl+S
    let save = use_callback(move |_: ()| {
        if !is_dirty() || is_saving() {
            return;
        }
        let notes_value = notes_text();
        let notes_to_save = (!notes_value.trim().is_empty()).then_some(notes_value);
        let (boss_id, file_path) = (boss_id.clone(), file_path.clone());
        is_saving.set(true);
        spawn(async move {
            match api::update_boss_notes(&boss_id, &file_path, notes_to_save.clone()).await {
                Ok(_) => {
                    on_change.call(notes_to_save);
                    is_dirty.set(false);
                    on_status.call(("Notes saved".to_string(), false));
                }
                Err(e) => on_status.call((format!("Failed to save: {}", e), true)),
            }
            is_saving.set(false);
        });
    });

    rsx! {
        // The list side: what notes are for and the Markdown they support
        div { class: "notes-tab text-sm text-muted",
            p { "Notes show on the Notes overlay during this encounter." }
            p {
                "Markdown: "
                code { "## headers" }
                ", "
                code { "**bold**" }
                ", "
                code { "*italic*" }
                ", "
                code { "- bullets" }
                ", "
                code { "1. numbered lists" }
                ", "
                code { "---" }
                " dividers. Ctrl+S saves."
            }
        }
        EditorDrawer {
            title: "Encounter Notes".to_string(),
            dirty: is_dirty(),
            actions: rsx! {
                button {
                    class: "btn btn-success btn-sm",
                    disabled: !is_dirty() || is_saving(),
                    onclick: move |_| save.call(()),
                    if is_saving() { "Saving..." } else { "Save" }
                }
            },
            textarea {
                class: "notes-textarea",
                placeholder: "## Tank Notes\n- Swap at 3 stacks\n\n## Healer Notes\n- Watch for raid damage\n\n## DPS Notes\n- Kill adds first",
                value: "{notes_text}",
                oninput: move |e| {
                    notes_text.set(e.value());
                    is_dirty.set(true);
                },
                onkeydown: move |e: Event<KeyboardData>| {
                    if e.modifiers().ctrl() && e.key() == Key::Character("s".to_string()) {
                        e.prevent_default();
                        save.call(());
                    }
                },
            }
            div { class: "text-xs text-muted mt-xs text-right", "{notes_text().len()} characters" }
        }
    }
}
