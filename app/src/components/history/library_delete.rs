//! Delete-or-archive modal. The keep list is the only control: every tag
//! starts ticked (from the saved retention list), the user unticks what
//! should go, and a live dry run from the backend shows exactly which files
//! will be touched. Archive is one click; Delete asks once more because the
//! files cannot be recovered.

use dioxus::prelude::*;

use baras_types::history::{ContentTag, DeleteCriteria};

use super::library::{fmt_bytes, tag_class};
use crate::api;
use crate::components::{ToastSeverity, use_toast};

const PREVIEW_ROWS: usize = 200;

#[component]
pub fn DeleteModal(on_close: EventHandler<()>, on_deleted: EventHandler<()>) -> Element {
    let mut toast = use_toast();
    let mut criteria = use_signal(|| DeleteCriteria {
        keep: ContentTag::ALL.to_vec(),
        keep_history: true,
        ..Default::default()
    });
    let mut busy = use_signal(|| false);
    let mut confirm_delete = use_signal(|| false);

    // The saved retention list seeds the keep list; an unset list means keep everything.
    // Edits stay local until "Save as default", since the startup auto-delete reads the saved one.
    let mut saved = use_signal(|| ContentTag::ALL.to_vec());
    let retain_loaded = use_resource(move || async move { api::get_retained_content().await.unwrap_or_default() });
    use_effect(move || {
        if let Some(retain) = retain_loaded.read().as_ref()
            && !retain.is_empty()
        {
            saved.set(retain.clone());
            criteria.write().keep = retain.clone();
        }
    });
    let mut toggle_keep = move |tag: ContentTag, on: bool| {
        let mut c = criteria.write();
        c.keep.retain(|t| *t != tag);
        if on {
            c.keep.push(tag);
        }
        confirm_delete.set(false);
    };
    let save_default = move |_| {
        let keep = criteria.read().keep.clone();
        spawn(async move {
            if api::set_retained_content(&keep).await.is_ok() {
                saved.set(keep);
            }
        });
    };

    let preview = use_resource(move || {
        let c = criteria.read().clone();
        async move { api::preview_log_delete(&c).await.unwrap_or_default() }
    });
    let preview_read = preview.read();
    let p = preview_read.as_ref();
    let count = p.map_or(0, |p| p.files.len());
    let total = p.map_or(0, |p| p.total_bytes);
    let c = criteria.read().clone();
    let keep_differs = {
        let mut a = c.keep.clone();
        let mut b = saved.read().clone();
        a.sort();
        b.sort();
        a != b
    };

    let paths_now = move || preview.read().as_ref().map(|p| p.files.iter().map(|f| f.path.clone()).collect::<Vec<_>>());
    let mut finish = move |outcome: Result<String, String>, verb: &str| {
        match outcome {
            Ok(msg) => {
                toast.show(msg, ToastSeverity::Normal);
                on_deleted.call(());
            }
            Err(err) => toast.show(format!("{verb} failed: {err}"), ToastSeverity::Normal),
        }
        busy.set(false);
    };
    let archive = move |_| {
        let Some(paths) = paths_now() else { return };
        busy.set(true);
        spawn(async move {
            let outcome = api::archive_logs(&paths).await.map(|r| {
                let failed = if r.failed.is_empty() { String::new() } else { format!(", {} failed", r.failed.len()) };
                format!("Archived {} logs: {} → {}{failed}", r.archived, fmt_bytes(r.bytes_before), fmt_bytes(r.bytes_after))
            });
            finish(outcome, "Archive");
        });
    };
    let delete = move |_| {
        let Some(paths) = paths_now() else { return };
        let keep_history = criteria.read().keep_history;
        busy.set(true);
        spawn(async move {
            let outcome = api::delete_logs(&paths, keep_history).await.map(|r| {
                let failed = if r.failed.is_empty() { String::new() } else { format!(", {} refused", r.failed.len()) };
                format!("Deleted {} logs, freed {}{failed}", r.deleted, fmt_bytes(r.bytes_freed))
            });
            finish(outcome, "Delete");
        });
    };

    rsx! {
        div { class: "modal-overlay", onclick: move |_| on_close.call(()),
            div { class: "modal-content library-delete", onclick: move |e| e.stop_propagation(),
                div { class: "modal-header",
                    h3 { "Delete or archive logs" }
                    button { class: "btn btn-xs btn-ghost", onclick: move |_| on_close.call(()), i { class: "fa-solid fa-xmark" } }
                }
                div { class: "library-delete-body",
                    div { class: "library-retain",
                        div { class: "library-retain-head",
                            span { class: "library-retain-title", "Keep logs containing" }
                            if keep_differs {
                                button { class: "btn btn-xs btn-ghost", title: "Use this as the retention list for the startup auto-delete too", onclick: save_default, "Save as default" }
                            }
                        }
                        div { class: "library-retain-tags",
                            for tag in ContentTag::ALL {
                                {
                                    let on = c.keep.contains(&tag);
                                    rsx! {
                                        label { class: if on { "lib-tag-toggle on" } else { "lib-tag-toggle" },
                                            input { r#type: "checkbox", checked: on, onchange: move |e| toggle_keep(tag, e.checked()) }
                                            {tag.label()}
                                        }
                                    }
                                }
                            }
                        }
                        span { class: "library-criteria-hint", "Untick what should go. A log stays if it holds any ticked content; favorites and the live log always stay." }
                    }

                    if let Some(p) = p {
                        div { class: if count > 0 { "library-preview warn" } else { "library-preview" },
                            strong { "{count} files · {fmt_bytes(p.total_bytes)}" }
                            " Kept: {p.kept} by content, {p.skipped_favorites} favorites, {p.skipped_live} live."
                        }
                        div { class: "library-preview-list",
                            table { class: "pull-history-table",
                                thead { tr { th { "File" } th { "Content" } th { "Date" } th { "Size" } } }
                                tbody {
                                    for f in p.files.iter().take(PREVIEW_ROWS) {
                                        tr { key: "{f.path}",
                                            td { "{f.filename}" }
                                            td {
                                                for t in f.content.iter() {
                                                    span { class: "lib-tag {tag_class(*t)}", {t.label()} }
                                                }
                                            }
                                            td { "{f.date}" }
                                            td { {fmt_bytes(f.size)} }
                                        }
                                    }
                                    if p.files.len() > PREVIEW_ROWS {
                                        tr { td { colspan: "4", class: "library-empty", "{p.files.len() - PREVIEW_ROWS} more" } }
                                    }
                                }
                            }
                        }
                    } else {
                        div { class: "library-preview", "Previewing…" }
                    }

                    div { class: "library-choice",
                        button {
                            class: "library-choice-card",
                            disabled: count == 0 || *busy.read(),
                            onclick: archive,
                            strong {
                                i { class: if *busy.read() { "fa-solid fa-spinner fa-spin" } else { "fa-solid fa-box-archive" } }
                                if *busy.read() { " Archiving…" } else { " Archive {count} files" }
                            }
                            span { "Compresses them into the BARAS config folder. Restore any of them later from the Library; pulls stay in the Overview." }
                        }
                        button {
                            class: "library-choice-card danger",
                            disabled: count == 0 || *busy.read(),
                            onclick: move |_| confirm_delete.set(true),
                            strong { i { class: "fa-solid fa-trash" } " Delete {count} files" }
                            span { "Removes the files for good. There is no undo and no trash; you will be asked once more." }
                        }
                    }
                }
                if *confirm_delete.read() {
                    div { class: "modal-footer library-confirm",
                        div { class: "library-confirm-text",
                            i { class: "fa-solid fa-triangle-exclamation" }
                            " Permanently delete {count} files ({fmt_bytes(total)})? The log files cannot be recovered."
                        }
                        label { class: "library-keep",
                            input { r#type: "checkbox", checked: c.keep_history, onchange: move |e| criteria.write().keep_history = e.checked() }
                            " Keep their pulls in the Overview (history rows)"
                        }
                        div { class: "library-confirm-actions",
                            button { class: "btn btn-sm", disabled: *busy.read(), onclick: move |_| confirm_delete.set(false), "Back" }
                            button {
                                class: "btn btn-sm btn-danger",
                                disabled: count == 0 || *busy.read(),
                                onclick: delete,
                                if *busy.read() { "Deleting…" } else { "Yes, delete {count} files permanently" }
                            }
                        }
                    }
                }
            }
        }
    }
}
