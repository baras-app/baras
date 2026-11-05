//! Library sub-tab: every log (including archived ones) with its history and
//! metadata, store health, re-scan controls, favorites, criteria-based
//! deletion or archival, and restore.

use dioxus::prelude::*;

use baras_types::history::{BackfillEvent, ContentTag, LibraryContent, LibraryRow, LogStatus};

use super::Backfill;
use super::library_delete::DeleteModal;
use crate::api;
use super::scroll::{save_scroll, use_restore_scroll};
use crate::components::{ToastSeverity, use_file_opener, use_toast};
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
        || row.tags.iter().any(|t| t.to_lowercase().contains(search));
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
pub fn LibraryTab(state: Signal<UiSessionState>, backfill: Backfill) -> Element {
    let mut toast = use_toast();
    let opener = use_file_opener();
    let mut rows = use_resource(|| async move { api::get_library().await.unwrap_or_default() });
    use_effect(move || {
        if matches!(*backfill.event.read(), Some(BackfillEvent::Done { .. })) {
            rows.restart();
        }
    });

    // Filters live in the session state so a tab switch keeps them
    let mut search = use_signal(|| state.read().history.library_search.clone());
    let mut content = use_signal(|| state.read().history.library_content);
    let mut favorites_only = use_signal(|| state.read().history.library_favorites);
    let mut show_delete = use_signal(|| false);
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
    let stale = all.iter().filter(|r| r.status == LogStatus::Stale).count();
    // Archived: count, compressed bytes, and what compression saved (where the frame says)
    let (archived, archived_bytes, archived_saved) = all
        .iter()
        .filter(|r| r.status == LogStatus::Archived)
        .fold((0u32, 0u64, 0u64), |(n, b, saved), r| {
            (n + 1, b + r.size, saved + r.original_size.map_or(0, |o| o.saturating_sub(r.size)))
        });

    let q = search.read().to_lowercase();
    let visible: Vec<&LibraryRow> = all
        .iter()
        .filter(|r| matches(r, *content.read(), *favorites_only.read(), &q))
        .collect();
    let running = backfill.running();
    let progress = match *backfill.event.read() {
        Some(BackfillEvent::Progress { current, total, .. }) if total > 0 => format!(" {current}/{total}"),
        _ => String::new(),
    };

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
                if stale > 0 {
                    div { class: "history-card warn", title: "History for these logs is from an older version or older log bytes",
                        div { class: "k", i { class: "fa-solid fa-triangle-exclamation" } " Needs re-scan" }
                        div { class: "v warn", "{stale}" }
                    }
                }
            }

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
                span { class: "library-spacer" }
                if running {
                    span { class: "library-scan-progress",
                        i { class: "fa-solid fa-spinner fa-spin" }
                        " Scanning{progress}"
                    }
                    button {
                        class: "btn btn-sm btn-ghost",
                        title: "Stop after the current log; logs already scanned keep their history",
                        onclick: move |_| backfill.cancel(),
                        i { class: "fa-solid fa-stop" }
                        " Cancel"
                    }
                } else {
                    button {
                        class: "btn btn-sm btn-ghost",
                        disabled: stale == 0,
                        title: "Re-parse only the logs whose history is missing or out of date",
                        onclick: move |_| backfill.start(true),
                        i { class: "fa-solid fa-rotate" }
                        " Re-scan stale ({stale})"
                    }
                    button {
                        class: "btn btn-sm btn-ghost",
                        title: "Re-parse every log in the directory",
                        onclick: move |_| backfill.start(false),
                        i { class: "fa-solid fa-database" }
                        " Rebuild all"
                    }
                }
                button {
                    class: "btn btn-sm btn-ghost",
                    disabled: running,
                    title: "Pick logs by content, then archive or delete them",
                    onclick: move |_| show_delete.set(true),
                    i { class: "fa-solid fa-broom" }
                    " Clean up…"
                }
            }

            div { id: LIB_TABLE, class: "pull-history-table-wrap", onscroll: move |_| save_scroll(LIB_TABLE),
                table { class: "pull-history-table library-table",
                    thead {
                        tr {
                            th { "File" }
                            th { "Date" }
                            th { "Character" }
                            th { "Server" }
                            th { "LibraryContent" }
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
                            tr { td { colspan: "11", class: "library-empty", "No logs match." } }
                        }
                        for row in visible.iter() {
                            {
                                let (badge, badge_class, badge_title) = status_badge(row.status);
                                let filename = row.filename.clone();
                                let favorite = row.favorite;
                                let path = row.path.clone();
                                let copy_path = row.path.clone();
                                let archived = row.status == LogStatus::Archived;
                                let restore_name = row.filename.clone();
                                rsx! {
                                    tr { key: "{row.filename}",
                                        td { class: "library-file",
                                            "{row.filename}"
                                            if !row.tags.is_empty() {
                                                div { class: "library-tags",
                                                    for t in row.tags.iter() {
                                                        span { class: "run-tag small", "{t}" }
                                                    }
                                                }
                                            }
                                        }
                                        td { "{row.date}" }
                                        td { {row.character.clone().unwrap_or_else(|| "—".into())} }
                                        td { if row.server.is_empty() { "—" } else { "{row.server}" } }
                                        td { class: "library-areas", title: "{row.areas.join(\" · \")}",
                                            if row.content.is_empty() {
                                                span { class: "pull-log-missing", "—" }
                                            }
                                            for t in row.content.iter() {
                                                span { class: "lib-tag {tag_class(*t)}", {t.label()} }
                                            }
                                            if !row.areas.is_empty() {
                                                span { class: "library-area-names", {row.areas.join(" · ")} }
                                            }
                                        }
                                        td { "{row.pulls}" }
                                        td { "{row.kills}" }
                                        td { {fmt_bytes(row.size)} }
                                        td { span { class: "{badge_class}", title: "{badge_title}", "{badge}" } }
                                        td {
                                            if let Some(link) = row.parsely.clone() {
                                                a { class: "library-parsely", href: "{link}", target: "_blank", title: "Open on Parsely", i { class: "fa-solid fa-up-right-from-square" } }
                                            } else if row.parsely_fights > 0 {
                                                span { class: "library-parsely-count", title: "Fights with their own Parsely link", "{row.parsely_fights} fights" }
                                            } else {
                                                span { class: "pull-log-missing", "—" }
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
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if *show_delete.read() {
                DeleteModal {
                    on_close: move |_| show_delete.set(false),
                    on_deleted: move |_| {
                        show_delete.set(false);
                        rows.restart();
                    },
                }
            }
        }
    }
}
