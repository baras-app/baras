//! Log Management housekeeping: the automatic cleanup rules, the retention
//! list and a live dry run of what they would remove right now, next to the
//! history store controls. Every change saves immediately; "Run now" shows
//! the exact files first and asks once, since deleted logs cannot be recovered.

use dioxus::prelude::*;

use baras_types::history::{BackfillEvent, CleanupEvent, CleanupPlan, ContentTag, HistoryStatus, LogCleanup};

use super::Backfill;
use super::library::{fmt_bytes, tag_class};
use crate::api;
use crate::components::{ToastSeverity, use_toast};

const MB: f64 = 1_048_576.0;
const PREVIEW_ROWS: usize = 200;

async fn save_rules(rules: LogCleanup) -> Result<(), String> {
    let mut cfg = api::get_config().await.ok_or("Settings unavailable")?;
    cfg.log_cleanup = rules;
    api::update_config(&cfg).await
}

async fn save_auto_scan(on: bool) -> Result<(), String> {
    let mut cfg = api::get_config().await.ok_or("Settings unavailable")?;
    cfg.auto_scan_history = on;
    api::update_config(&cfg).await
}

#[component]
pub fn CleanupPanel(backfill: Backfill, cleanup: Signal<Option<CleanupEvent>>, status: Resource<HistoryStatus>, on_changed: EventHandler<()>) -> Element {
    let mut toast = use_toast();
    let mut rules = use_signal(LogCleanup::default);
    let mut auto_scan = use_signal(|| true);
    let mut retain = use_signal(Vec::<ContentTag>::new);
    let mut show_confirm = use_signal(|| false);

    // Saved settings seed the controls once; edits save straight back
    let config = use_resource(|| async move { api::get_config().await });
    use_effect(move || {
        if let Some(Some(cfg)) = config.read().as_ref() {
            rules.set(cfg.log_cleanup.clone());
            auto_scan.set(cfg.auto_scan_history);
        }
    });
    let retain_loaded = use_resource(|| async move { api::get_retained_content().await.unwrap_or_default() });
    use_effect(move || {
        if let Some(r) = retain_loaded.read().as_ref() {
            retain.set(r.clone());
        }
    });

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
        Some(CleanupEvent::Progress { current, total, file }) => Some(format!("{current}/{total} {file}")),
        _ => None,
    };

    let mut set_rules = move |r: LogCleanup| {
        rules.set(r.clone());
        spawn(async move {
            match save_rules(r).await {
                Ok(()) => plan.restart(),
                Err(err) => toast.show(format!("Failed to save settings: {err}"), ToastSeverity::Normal),
            }
        });
    };
    let mut toggle_retain = move |tag: ContentTag, on: bool| {
        let mut list = retain.read().clone();
        list.retain(|t| *t != tag);
        if on {
            list.push(tag);
        }
        retain.set(list.clone());
        spawn(async move {
            match api::set_retained_content(&list).await {
                Ok(_) => plan.restart(),
                Err(err) => toast.show(format!("Failed to save retention list: {err}"), ToastSeverity::Normal),
            }
        });
    };

    let r = rules.read().clone();
    let plan_read = plan.read();
    let p = plan_read.as_ref();
    let pending = status.read().as_ref().map_or(0, |s| s.stale + s.unscanned);
    let running = backfill.running();
    let progress = match *backfill.event.read() {
        Some(BackfillEvent::Progress { current, total, .. }) if total > 0 => format!(" {current}/{total}"),
        _ => String::new(),
    };

    rsx! {
        details { class: "library-settings", open: true,
            summary { class: "collapsible-summary", title: "Show or hide cleanup and scan settings",
                i { class: "fa-solid fa-sliders summary-icon" }
                "Management settings"
            }
            div { class: "library-manage",
            div { class: "run-section",
                h4 { "Automatic cleanup" }
                RuleRow {
                    on: r.delete_smaller_than > 0,
                    text: "Delete logs smaller than",
                    unit: "MB",
                    step: "0.1",
                    min: "0.1",
                    value: format!("{:.1}", r.delete_smaller_than as f64 / MB),
                    on_toggle: move |on: bool| {
                        let mut r = rules.read().clone();
                        r.delete_smaller_than = if on { MB as u64 } else { 0 };
                        set_rules(r);
                    },
                    on_value: move |v: String| {
                        if let Ok(mb) = v.parse::<f64>() && mb > 0.0 {
                            let mut r = rules.read().clone();
                            r.delete_smaller_than = (mb * MB) as u64;
                            set_rules(r);
                        }
                    },
                }
                RuleRow {
                    on: r.archive_after_days.is_some(),
                    text: "Archive logs older than",
                    unit: "days",
                    step: "1",
                    min: "1",
                    value: r.archive_after_days.unwrap_or(30).to_string(),
                    on_toggle: move |on: bool| {
                        let mut r = rules.read().clone();
                        r.archive_after_days = on.then_some(30);
                        set_rules(r);
                    },
                    on_value: move |v: String| {
                        if let Ok(d) = v.parse::<u32>() && d > 0 {
                            let mut r = rules.read().clone();
                            r.archive_after_days = Some(d);
                            set_rules(r);
                        }
                    },
                }
                RuleRow {
                    on: r.delete_after_days.is_some(),
                    text: "Delete logs older than",
                    unit: "days",
                    step: "1",
                    min: "1",
                    value: r.delete_after_days.unwrap_or(90).to_string(),
                    on_toggle: move |on: bool| {
                        let mut r = rules.read().clone();
                        r.delete_after_days = on.then_some(90);
                        set_rules(r);
                    },
                    on_value: move |v: String| {
                        if let Ok(d) = v.parse::<u32>() && d > 0 {
                            let mut r = rules.read().clone();
                            r.delete_after_days = Some(d);
                            set_rules(r);
                        }
                    },
                }
                span { class: "library-retain-title", "Never delete logs containing" }
                div { class: "library-retain-tags",
                    for tag in ContentTag::ALL {
                        {
                            let on = retain.read().contains(&tag);
                            rsx! {
                                label { class: if on { "lib-tag-toggle on lib-tag {tag_class(tag)}" } else { "lib-tag-toggle" },
                                    input { r#type: "checkbox", checked: on, onchange: move |e| toggle_retain(tag, e.checked()) }
                                    {tag.label()}
                                }
                            }
                        }
                    }
                }
                p { class: "hint",
                    "Archiving compresses a log into cold storage: it stays in the statistics, scans still cover it, and the table can restore it. Deleting removes the file for good, from the directory or the archive; its pulls stay in the statistics. Logs with the content above are archived once old enough but never deleted. Favorites, the live log and today's logs are never touched. Runs after startup."
                }
                if let Some(p) = p && !p.is_empty() {
                    div { class: "history-pick",
                        i { class: "fa-solid fa-broom" }
                        span { "Right now: {describe(p)}. Kept {p.kept} by content." }
                    }
                }
                div { class: "library-actions-center",
                    if let Some(progress) = cleaning {
                        span { class: "library-scan-progress",
                            i { class: "fa-solid fa-spinner fa-spin" }
                            " Cleaning {progress}"
                        }
                    } else {
                        button {
                            class: "btn btn-lg btn-primary",
                            disabled: p.is_none_or(|p| p.is_empty()),
                            onclick: move |_| show_confirm.set(true),
                            i { class: "fa-solid fa-broom" }
                            " Run cleanup now…"
                        }
                    }
                }
            }

            div { class: "run-section",
                h4 { "Historical data" }
                p { class: "hint",
                    "Boss Statistics and Raid Summaries are built from scanned logs, archived ones included. Rebuilding re-parses every log; favorites, tags and notes are kept."
                }
                label { class: "history-toggle",
                    input {
                        r#type: "checkbox",
                        checked: *auto_scan.read(),
                        onchange: move |e| {
                            let on = e.checked();
                            auto_scan.set(on);
                            spawn(async move {
                                if let Err(err) = save_auto_scan(on).await {
                                    toast.show(format!("Failed to save settings: {err}"), ToastSeverity::Normal);
                                }
                            });
                        },
                    }
                    " Scan new logs at startup"
                }
                div { class: "library-actions-center",
                    if running {
                        span { class: "library-scan-progress",
                            i { class: "fa-solid fa-spinner fa-spin" }
                            " Scanning{progress}"
                        }
                        button {
                            class: "btn btn-lg",
                            title: "Stop after the current log; logs already scanned keep their history",
                            onclick: move |_| backfill.cancel(),
                            i { class: "fa-solid fa-stop" }
                            " Cancel"
                        }
                    } else {
                        button {
                            class: "btn btn-lg btn-primary",
                            disabled: pending == 0,
                            title: "Parse only the logs whose history is missing or out of date",
                            onclick: move |_| backfill.start(true),
                            i { class: "fa-solid fa-rotate" }
                            " Scan missing ({pending})"
                        }
                        button {
                            class: "btn btn-lg",
                            title: "Re-parse every log in the directory and the archive, and recompute the statistics",
                            onclick: move |_| backfill.start(false),
                            i { class: "fa-solid fa-database" }
                            " Rebuild all"
                        }
                    }
                }
            }
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

/// One rule: a checkbox, its wording, and the number it applies.
#[component]
fn RuleRow(
    on: bool,
    text: String,
    unit: String,
    step: String,
    min: String,
    value: String,
    on_toggle: EventHandler<bool>,
    on_value: EventHandler<String>,
) -> Element {
    rsx! {
        div { class: "library-rule",
            label { class: "history-toggle",
                input { r#type: "checkbox", checked: on, onchange: move |e| on_toggle.call(e.checked()) }
                " {text}"
            }
            input {
                class: "library-num",
                r#type: "number",
                step: "{step}",
                min: "{min}",
                disabled: !on,
                value: "{value}",
                onchange: move |e| on_value.call(e.value()),
            }
            span { class: "hint", "{unit}" }
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
