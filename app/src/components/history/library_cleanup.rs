//! Log Management actions: scan or rebuild the history store and run the
//! cleanup rules now, as one strip in the toolbar. The rules themselves are
//! on the Settings page; "Run cleanup" shows the exact files first and asks
//! once, since deleted logs cannot be recovered.

use dioxus::prelude::*;

use crate::types::MainTab;
use baras_types::history::{BackfillEvent, CleanupEvent, CleanupPlan, HistoryStatus};

use super::Backfill;
use super::library::{fmt_bytes, tag_class};
use crate::api;
use crate::components::{ToastSeverity, use_toast};
use crate::types::UiSessionState;

const PREVIEW_ROWS: usize = 200;

#[component]
pub fn LibraryActions(state: Signal<UiSessionState>, backfill: Backfill, cleanup: Signal<Option<CleanupEvent>>, status: Resource<HistoryStatus>, on_changed: EventHandler<()>) -> Element {
    let mut show_confirm = use_signal(|| false);

    // Live dry run of the rules, refreshed after a scan or a cleanup
    let mut plan = use_resource(|| async move { api::preview_log_cleanup().await.unwrap_or_default() });
    use_effect(move || {
        if matches!(*backfill.event.read(), Some(BackfillEvent::Done { .. })) {
            plan.restart();
        }
    });
    use_effect(move || {
        if matches!(*cleanup.read(), Some(CleanupEvent::Done { .. })) {
            plan.restart();
        }
    });
    let cleaning = match &*cleanup.read() {
        Some(CleanupEvent::Progress { current, total, .. }) => Some(format!("{current}/{total}")),
        _ => None,
    };

    let plan_read = plan.read();
    let p = plan_read.as_ref();
    let due = p.map_or(0, |p| p.archive.len() + p.delete.len());
    let cleanup_title = match p {
        Some(p) if !p.is_empty() => format!("The rules would {} right now; {} kept by content. Shows the files first.", describe(p), p.kept),
        _ => "Nothing to archive or delete under the current rules".to_string(),
    };
    let pending = status.read().as_ref().map_or(0, |s| s.stale + s.unscanned);
    let running = backfill.running();
    let progress = match *backfill.event.read() {
        Some(BackfillEvent::Progress { current, total, .. }) if total > 0 => format!(" {current}/{total}"),
        _ => String::new(),
    };

    rsx! {
        div { class: "library-ops",
            if running {
                span { class: "library-scan-progress", i { class: "fa-solid fa-spinner fa-spin" } " Scanning{progress}" }
                button { class: "btn btn-xs", title: "Stop after the current log; logs already scanned keep their history",
                    onclick: move |_| backfill.cancel(),
                    i { class: "fa-solid fa-stop" } " Cancel"
                }
            } else {
                button {
                    class: if pending > 0 { "btn btn-xs btn-primary" } else { "btn btn-xs" },
                    disabled: pending == 0,
                    title: "Parse only the logs whose history is missing or out of date",
                    onclick: move |_| backfill.start(true),
                    i { class: "fa-solid fa-rotate" } " Scan missing"
                    if pending > 0 { span { class: "library-ops-count", "{pending}" } }
                }
                button { class: "btn btn-xs", title: "Re-parse every log in the directory and the archive, and recompute the statistics",
                    onclick: move |_| backfill.start(false),
                    i { class: "fa-solid fa-database" } " Rebuild all"
                }
            }
            if let Some(progress) = cleaning {
                span { class: "library-scan-progress", i { class: "fa-solid fa-spinner fa-spin" } " Cleaning {progress}" }
            } else {
                button {
                    class: "btn btn-xs",
                    disabled: due == 0,
                    title: "{cleanup_title}",
                    onclick: move |_| show_confirm.set(true),
                    i { class: "fa-solid fa-broom" } " Run cleanup…"
                    if due > 0 { span { class: "library-ops-count", "{due}" } }
                }
            }
            button { class: "btn btn-xs btn-ghost", title: "Cleanup rules, retention and startup scan live in Settings",
                onclick: move |_| state.write().active_tab = MainTab::Settings,
                i { class: "fa-solid fa-sliders" } " Rules"
            }
        }

        if *show_confirm.read() && let Some(p) = p {
            CleanupModal {
                plan: p.clone(),
                on_close: move |_| show_confirm.set(false),
                on_done: move |_| {
                    show_confirm.set(false);
                    plan.restart();
                    on_changed.call(());
                },
            }
        }
    }
}

fn is_archive(path: &str) -> bool {
    path.ends_with(".zst")
}

/// "archive 12 files (1.2 GB), delete 3 files and 4 archives (300 MB)"
fn describe(p: &CleanupPlan) -> String {
    let archives = p.delete.iter().filter(|f| is_archive(&f.path)).count();
    let files = p.delete.len() - archives;
    let mut parts = Vec::new();
    if !p.archive.is_empty() {
        parts.push(format!("archive {} files ({})", p.archive.len(), fmt_bytes(p.archive_bytes)));
    }
    if !p.delete.is_empty() {
        let what = match (files, archives) {
            (f, 0) => format!("{f} files"),
            (0, a) => format!("{a} archives"),
            (f, a) => format!("{f} files and {a} archives"),
        };
        parts.push(format!("delete {what} ({})", fmt_bytes(p.delete_bytes)));
    }
    parts.join(", ")
}

/// The exact files the rules select right now, and one confirmation.
#[component]
fn CleanupModal(plan: CleanupPlan, on_close: EventHandler<()>, on_done: EventHandler<()>) -> Element {
    let mut toast = use_toast();
    let mut busy = use_signal(|| false);
    let archive_n = plan.archive.len();
    let delete_n = plan.delete.len();
    let summary = describe(&plan);
    let rows: Vec<(&str, &baras_types::history::CleanupCandidate)> = plan
        .archive
        .iter()
        .map(|f| ("archive", f))
        .chain(plan.delete.iter().map(|f| ("delete", f)))
        .collect();
    let run = move |_| {
        busy.set(true);
        spawn(async move {
            match api::run_log_cleanup().await {
                Ok(r) => {
                    let failed = if r.failed.is_empty() { String::new() } else { format!(", {} failed", r.failed.len()) };
                    toast.show(
                        format!("Archived {} and deleted {} logs, freed {}{failed}", r.archived, r.deleted, fmt_bytes(r.bytes_freed)),
                        ToastSeverity::Normal,
                    );
                    on_done.call(());
                }
                Err(err) => toast.show(format!("Cleanup failed: {err}"), ToastSeverity::Normal),
            }
            busy.set(false);
        });
    };

    rsx! {
        div { class: "modal-overlay", onclick: move |_| on_close.call(()),
            div { class: "modal-content library-cleanup", onclick: move |e| e.stop_propagation(),
                div { class: "modal-header",
                    h3 { "Run cleanup" }
                    button { class: "btn btn-xs btn-ghost", onclick: move |_| on_close.call(()), i { class: "fa-solid fa-xmark" } }
                }
                div { class: "library-delete-body",
                    div { class: "library-preview warn",
                        strong { "{summary}" }
                        " Archived logs can be restored from the table below; deleted logs and archives cannot."
                    }
                    div { class: "library-preview-list",
                        table { class: "pull-history-table",
                            thead { tr { th { "Action" } th { "File" } th { "Content" } th { "Date" } th { "Size" } } }
                            tbody {
                                for (action, f) in rows.iter().take(PREVIEW_ROWS) {
                                    tr { key: "{f.path}",
                                        td { class: if *action == "delete" { "library-action delete" } else { "library-action" },
                                            if *action == "delete" && is_archive(&f.path) { "delete archive" } else { "{action}" }
                                        }
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
                                if rows.len() > PREVIEW_ROWS {
                                    tr { td { colspan: "5", class: "library-empty", "{rows.len() - PREVIEW_ROWS} more" } }
                                }
                            }
                        }
                    }
                }
                div { class: "modal-footer library-confirm-actions",
                    button { class: "btn btn-sm", disabled: *busy.read(), onclick: move |_| on_close.call(()), "Cancel" }
                    button {
                        class: if delete_n > 0 { "btn btn-sm btn-danger" } else { "btn btn-sm" },
                        disabled: *busy.read(),
                        onclick: run,
                        if *busy.read() { "Working…" } else if delete_n > 0 { "Archive {archive_n}, delete {delete_n} permanently" } else { "Archive {archive_n} files" }
                    }
                }
            }
        }
    }
}
