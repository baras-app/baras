//! Log Management: every log (including archived ones) with its history and
//! metadata, the automatic cleanup and history store controls, favorites and
//! restore.

use dioxus::prelude::*;

use baras_types::history::{BackfillEvent, ContentTag, HistoryStatus, LibraryContent, LibraryRow, LogStatus};

use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

use super::Backfill;
use super::library_cleanup::CleanupPanel;
use super::library_meta::{LogNoteCell, LogTagsCell, TAG_OPTIONS};
use super::scroll::{use_paged, use_restore_scroll};
use crate::api;
use crate::components::{ToastSeverity, use_file_opener, use_parsely_upload, use_toast};
use crate::types::UiSessionState;

const LIB_TABLE: &str = "hist-lib-table";


/// Select options: fixed entries first, then one per content tag
fn content_options() -> Vec<LibraryContent> {
    let mut v = vec![LibraryContent::Any, LibraryContent::Kills];
    v.extend(ContentTag::ALL.iter().map(|t| LibraryContent::Tag(*t)));
    v
}

pub fn fmt_bytes(b: u64) -> String {
    const MB: f64 = 1_048_576.0;
    let mb = b as f64 / MB;
    if mb >= 1024.0 {
        format!("{:.1} GB", mb / 1024.0)
    } else if mb >= 10.0 {
        format!("{mb:.0} MB")
    } else {
        format!("{mb:.1} MB")
    }
}

fn matches(row: &LibraryRow, content: LibraryContent, favorites_only: bool, search: &str) -> bool {
    let content_ok = match content {
        LibraryContent::Any => true,
        LibraryContent::Kills => row.kills > 0,
        LibraryContent::Tag(t) => row.content.contains(&t),
    };
    let search_ok = search.is_empty()
        || row.filename.to_lowercase().contains(search)
        || row.character.as_deref().is_some_and(|c| c.to_lowercase().contains(search))
        || row.areas.iter().any(|a| a.to_lowercase().contains(search))
        || row.tags.iter().chain(row.run_tags.iter()).any(|t| t.to_lowercase().contains(search))
        || row.note.to_lowercase().contains(search);
    content_ok && (!favorites_only || row.favorite) && search_ok
}

/// Clipboard via the page's navigator; paths can hold backslashes on Windows
fn copy_to_clipboard(text: &str) {
    let escaped = text.replace('\\', "\\\\").replace('`', "\\`").replace("${", "\\${");
    let _ = js_sys::eval(&format!("navigator.clipboard.writeText(`{escaped}`)"));
}

pub(super) fn tag_class(t: ContentTag) -> &'static str {
    match t {
        ContentTag::MasterOps | ContentTag::MasterFlashpoint => "nim",
        ContentTag::VeteranOps | ContentTag::VeteranFlashpoint => "hm",
        ContentTag::StoryOps => "sm",
        ContentTag::Pvp => "pvp",
        ContentTag::Dummy | ContentTag::Other | ContentTag::Empty => "misc",
    }
}

fn status_badge(status: LogStatus) -> (&'static str, &'static str, &'static str) {
    match status {
        LogStatus::Live => ("● live", "lib-status live", "Being tailed right now"),
        LogStatus::Indexed => ("indexed", "lib-status indexed", "History is current"),
        LogStatus::Stale => ("re-scan", "lib-status stale", "History is from an older version or older log bytes"),
        LogStatus::NoHistory => ("—", "lib-status none", "Never scanned, or nothing recordable"),
        LogStatus::Archived => ("archived", "lib-status archived", "Compressed in the archive; restore to open"),
    }
}

#[component]
pub fn LibraryTab(state: Signal<UiSessionState>, backfill: Backfill, status: Resource<HistoryStatus>) -> Element {
    let mut toast = use_toast();
    let opener = use_file_opener();
    let mut uploader = use_parsely_upload();
    let mut rows = use_resource(|| async move { api::get_library().await.unwrap_or_default() });
    let mut status = status;
    // (path, filename) of the log awaiting delete confirmation
    let mut confirm_delete = use_signal(|| None::<(String, String)>);
    // (filename, note cell) whose tag and note editors are mounted; every
    // other row shows plain text, so the table stays light
    let mut editing = use_signal(|| None::<(String, bool)>);
    use_effect(move || {
        if matches!(*backfill.event.read(), Some(BackfillEvent::Done { .. })) {
            rows.restart();
        }
    });
    // The upload modal announces a finished file upload on the window; the
    // callback runs outside the Dioxus runtime, so it only bumps a signal.
    let mut uploads = use_signal(|| 0u32);
    use_future(move || async move {
        let closure = Closure::<dyn FnMut()>::new(move || {
            let _ = uploads.try_write().map(|mut w| *w += 1);
        });
        if let Some(w) = web_sys::window() {
            let _ = w.add_event_listener_with_callback("parsely-upload-complete", closure.as_ref().unchecked_ref());
        }
        closure.forget();
    });
    use_effect(move || {
        if *uploads.read() > 0 {
            rows.restart();
        }
    });

    // Filters live in the session state so a tab switch keeps them
    let mut search = use_signal(|| state.read().history.library_search.clone());
    let mut content = use_signal(|| state.read().history.library_content);
    let mut favorites_only = use_signal(|| state.read().history.library_favorites);
    let mut state = state;
    use_effect(move || {
        let mut s = state.write();
        s.history.library_search = search.read().clone();
        s.history.library_content = *content.read();
        s.history.library_favorites = *favorites_only.read();
    });

    let rows_read = rows.read();
    let all: &[LibraryRow] = rows_read.as_deref().unwrap_or(&[]);
    let loading = rows_read.is_none();
    use_restore_scroll(&[LIB_TABLE], !loading);

    let total_bytes: u64 = all.iter().map(|r| r.size).sum();
    // Archived: count, compressed bytes, and what compression saved (where the frame says)
    let (archived, archived_bytes, archived_saved) = all
        .iter()
        .filter(|r| r.status == LogStatus::Archived)
        .fold((0u32, 0u64, 0u64), |(n, b, saved), r| {
            (n + 1, b + r.size, saved + r.original_size.map_or(0, |o| o.saturating_sub(r.size)))
        });

    // Every tag in use, for the tag inputs' completion list
    let mut known_tags: Vec<&str> = all.iter().flat_map(|r| r.tags.iter().chain(r.run_tags.iter())).map(String::as_str).collect();
    known_tags.sort_unstable();
    known_tags.dedup();

    let status_read = status.read();
    let hs = status_read.as_ref().cloned().unwrap_or_default();
    let pending = hs.stale + hs.unscanned;

    let q = search.read().to_lowercase();
    let visible: Vec<&LibraryRow> = all
        .iter()
        .filter(|r| matches(r, *content.read(), *favorites_only.read(), &q))
        .collect();
    // Rows render a page at a time; a filter change starts over
    let page_key = format!("{q}|{:?}|{}", *content.read(), *favorites_only.read());
    let paged = use_paged(&page_key);
    let limit = paged.limit(&page_key);
    let shown = visible.len().min(limit);
    let scroll_key = page_key.clone();
    let total = visible.len();

    rsx! {
        div { class: "library",
            div { class: "history-cards",
                div { class: "history-card", div { class: "k", "Log files" } div { class: "v", "{all.len()}" } }
                div { class: "history-card", div { class: "k", "Storage used" }
                    div { class: "v", {fmt_bytes(total_bytes)} }
                }
                div { class: "history-card", div { class: "k", "Archived" }
                    div { class: "v", "{archived} " span { class: "sub", {fmt_bytes(archived_bytes)} } }
                    if archived_saved > 0 {
                        div { class: "sub", "saved {fmt_bytes(archived_saved)}" }
                    }
                }
                div { class: "history-card", div { class: "k", "Logs scanned" } div { class: "v", "{hs.logs}" } }
                div { class: "history-card", div { class: "k", "Pulls recorded" } div { class: "v", "{hs.pulls}" } }
                if pending > 0 {
                    div { class: "history-card", title: "Logs with no history yet, or history from an older version",
                        div { class: "k", "Needs scan" }
                        div { class: "v", "{pending}" }
                    }
                }
            }

            CleanupPanel { backfill, status, on_changed: move |_| rows.restart() }

            div { class: "library-actions",
                input {
                    class: "boss-search-input",
                    r#type: "search",
                    placeholder: "Search file, character, area, tag",
                    value: "{search}",
                    oninput: move |e| search.set(e.value()),
                }
                select {
                    onchange: move |e| {
                        let i: usize = e.value().parse().unwrap_or(0);
                        content.set(content_options().get(i).copied().unwrap_or_default());
                    },
                    for (i, c) in content_options().into_iter().enumerate() {
                        option { value: "{i}", selected: c == *content.read(), {c.label()} }
                    }
                }
                label { class: "history-toggle",
                    input {
                        r#type: "checkbox",
                        checked: *favorites_only.read(),
                        onchange: move |e| favorites_only.set(e.checked()),
                    }
                    " Favorites"
                }
            }

            datalist { id: TAG_OPTIONS,
                for t in known_tags.iter() {
                    option { value: "{t}" }
                }
            }

            div { id: LIB_TABLE, class: "pull-history-table-wrap", onscroll: move |_| paged.on_scroll(LIB_TABLE, &scroll_key, total),
                table { class: "pull-history-table library-table",
                    thead {
                        tr {
                            th { "File" }
                            th { "Date" }
                            th { "Character" }
                            th { "Server" }
                            th { "Content" }
                            th { "Tags" }
                            th { "Note" }
                            th { "Pulls" }
                            th { "Kills" }
                            th { "Size" }
                            th { "Status" }
                            th { "Parsely" }
                            th { "" }
                        }
                    }
                    tbody {
                        if !loading && visible.is_empty() {
                            tr { td { colspan: "13", class: "library-empty", "No logs match." } }
                        }
                        for row in visible[..shown].iter() {
                            {
                                let (badge, badge_class, badge_title) = status_badge(row.status);
                                let area_names = row.areas.join(" · ");
                                let editor = editing.read().as_ref().filter(|(f, _)| *f == row.filename).map(|(_, note)| *note);
                                let (edit_tags, edit_note) = (row.filename.clone(), row.filename.clone());
                                let filename = row.filename.clone();
                                let favorite = row.favorite;
                                let path = row.path.clone();
                                let copy_path = row.path.clone();
                                let archived = row.status == LogStatus::Archived;
                                let restore_name = row.filename.clone();
                                let (upload_path, upload_name) = (row.path.clone(), row.filename.clone());
                                let (delete_path, delete_name) = (row.path.clone(), row.filename.clone());
                                let live = row.status == LogStatus::Live;
                                rsx! {
                                    tr { key: "{row.filename}",
                                        td { class: "library-file", "{row.filename}" }
                                        td { "{row.date}" }
                                        td { {row.character.clone().unwrap_or_else(|| "—".into())} }
                                        td { if row.server.is_empty() { "—" } else { "{row.server}" } }
                                        td { class: "library-areas", title: "{area_names}",
                                            if row.content.is_empty() {
                                                span { class: "pull-log-missing", "—" }
                                            }
                                            for t in row.content.iter() {
                                                span { class: "lib-tag {tag_class(*t)}", {t.label()} }
                                            }
                                            if !row.areas.is_empty() {
                                                span { class: "library-area-names", "{area_names}" }
                                            }
                                        }
                                        td {
                                            if let Some(note_focus) = editor {
                                                LogTagsCell {
                                                    filename: row.filename.clone(),
                                                    tags: row.tags.clone(),
                                                    run_tags: row.run_tags.clone(),
                                                    note: row.note.clone(),
                                                    focus: !note_focus,
                                                    on_change: move |_| rows.restart(),
                                                }
                                            } else {
                                                div { class: "library-tag-cell static", title: "Click to edit tags",
                                                    onclick: move |_| editing.set(Some((edit_tags.clone(), false))),
                                                    for t in row.tags.iter() {
                                                        span { class: "run-tag small", "{t}" }
                                                    }
                                                    for t in row.run_tags.iter().filter(|t| !row.tags.contains(*t)) {
                                                        span { class: "run-tag small muted", title: "Tag on a run in this log", "{t}" }
                                                    }
                                                    if row.tags.is_empty() {
                                                        span { class: "library-add-hint", "+ tag" }
                                                    }
                                                }
                                            }
                                        }
                                        td {
                                            if let Some(note_focus) = editor {
                                                LogNoteCell {
                                                    filename: row.filename.clone(),
                                                    tags: row.tags.clone(),
                                                    note: row.note.clone(),
                                                    focus: note_focus,
                                                    on_change: move |_| rows.restart(),
                                                }
                                            } else {
                                                div {
                                                    class: if row.note.is_empty() { "library-note static empty" } else { "library-note static" },
                                                    title: "{row.note}",
                                                    onclick: move |_| editing.set(Some((edit_note.clone(), true))),
                                                    if row.note.is_empty() { "Note…" } else { "{row.note}" }
                                                }
                                            }
                                        }
                                        td { "{row.pulls}" }
                                        td { "{row.kills}" }
                                        td { {fmt_bytes(row.size)} }
                                        td { span { class: "{badge_class}", title: "{badge_title}", "{badge}" } }
                                        td { class: "library-parsely-cell",
                                            if let Some(link) = row.parsely.clone() {
                                                a { class: "library-parsely", href: "{link}", target: "_blank", title: "Open on Parsely", i { class: "fa-solid fa-up-right-from-square" } }
                                            } else if archived {
                                                span { class: "pull-log-missing", "—" }
                                            } else {
                                                if row.parsely_fights > 0 {
                                                    span { class: "library-parsely-count", title: "Fights with their own Parsely link", "{row.parsely_fights} fights" }
                                                }
                                                button {
                                                    class: "btn btn-xs btn-ghost",
                                                    title: "Upload this log to Parsely",
                                                    onclick: move |_| uploader.open_file(upload_path.clone(), upload_name.clone()),
                                                    i { class: "fa-solid fa-cloud-arrow-up" }
                                                }
                                            }
                                        }
                                        td { class: "library-row-actions",
                                            if archived {
                                                button {
                                                    class: "btn btn-xs btn-ghost",
                                                    title: "Restore to the log directory",
                                                    onclick: move |_| {
                                                        let filename = restore_name.clone();
                                                        spawn(async move {
                                                            match api::restore_log(&filename).await {
                                                                Ok(_) => rows.restart(),
                                                                Err(err) => toast.show(format!("Restore failed: {err}"), ToastSeverity::Normal),
                                                            }
                                                        });
                                                    },
                                                    i { class: "fa-solid fa-box-open" }
                                                }
                                            } else {
                                                button {
                                                    class: "btn btn-xs btn-ghost",
                                                    title: "Open in Encounter Analysis",
                                                    disabled: opener.busy(),
                                                    onclick: move |_| opener.open(path.clone(), None),
                                                    i { class: "fa-solid fa-eye" }
                                                }
                                            }
                                            button {
                                                class: "btn btn-xs btn-ghost",
                                                title: "Copy file path",
                                                onclick: move |_| {
                                                    copy_to_clipboard(&copy_path);
                                                    toast.show("Path copied", ToastSeverity::Normal);
                                                },
                                                i { class: "fa-solid fa-copy" }
                                            }
                                            button {
                                                class: if favorite { "lib-star on" } else { "lib-star" },
                                                title: if favorite { "Favorite: never auto-deleted" } else { "Mark favorite" },
                                                onclick: move |_| {
                                                    let filename = filename.clone();
                                                    spawn(async move {
                                                        if api::set_log_favorite(&filename, !favorite).await.is_ok() {
                                                            rows.restart();
                                                        }
                                                    });
                                                },
                                                if favorite { "★" } else { "☆" }
                                            }
                                            button {
                                                class: "btn btn-xs btn-ghost lib-delete",
                                                title: if live { "The live log cannot be deleted" } else { "Delete this log and all of its data" },
                                                disabled: live,
                                                onclick: move |_| confirm_delete.set(Some((delete_path.clone(), delete_name.clone()))),
                                                i { class: "fa-solid fa-trash" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if shown < total {
                            tr {
                                td { colspan: "13", class: "library-empty",
                                    "{shown} of {total} shown · scroll for more "
                                    button { class: "btn btn-xs btn-ghost", onclick: move |_| paged.show_all(&page_key), "Show all" }
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Some((path, filename)) = confirm_delete.read().clone() {
            div { class: "modal-overlay", onclick: move |_| confirm_delete.set(None),
                div { class: "modal-content library-cleanup", onclick: move |e| e.stop_propagation(),
                    div { class: "modal-header",
                        h3 { "Delete log" }
                        button { class: "btn btn-xs btn-ghost", onclick: move |_| confirm_delete.set(None), i { class: "fa-solid fa-xmark" } }
                    }
                    div { class: "library-delete-body",
                        div { class: "library-preview warn",
                            strong { "{filename}" }
                            " The file, its pull history, tags, notes and Parsely links are removed. This cannot be undone."
                        }
                    }
                    div { class: "modal-footer library-confirm-actions",
                        button { class: "btn btn-sm", onclick: move |_| confirm_delete.set(None), "Cancel" }
                        button {
                            class: "btn btn-sm btn-danger",
                            onclick: move |_| {
                                let path = path.clone();
                                confirm_delete.set(None);
                                spawn(async move {
                                    match api::delete_log(&path).await {
                                        Ok(r) if r.failed.is_empty() => {
                                            toast.show(format!("Deleted, freed {}", fmt_bytes(r.bytes_freed)), ToastSeverity::Normal);
                                            rows.restart();
                                            status.restart();
                                        }
                                        Ok(_) => toast.show("Could not delete this log", ToastSeverity::Normal),
                                        Err(err) => toast.show(format!("Delete failed: {err}"), ToastSeverity::Normal),
                                    }
                                });
                            },
                            "Delete permanently"
                        }
                    }
                }
            }
        }
    }
}
