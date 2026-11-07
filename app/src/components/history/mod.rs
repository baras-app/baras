//! History views: Boss Statistics (Overview), Raid Summaries (Operations),
//! Log Management (Library) and PvP Statistics, each a main tab of its own.
//!
//! One shell with the shared filter (rendered by each view in its sidebar)
//! and the backfill listener every view reads. Each view fetches its own
//! data lazily; the backend keeps every index, this module only ever holds
//! compact rows.

mod charts;
mod heatmap;
mod library;
mod library_cleanup;
mod library_meta;
mod operations;
mod overview;
mod pvp;
mod run_detail;
mod scroll;
mod toolbar;

use dioxus::prelude::*;
use wasm_bindgen::prelude::*;

use baras_types::history::{BackfillEvent, CleanupEvent, HistoryStatus, HistorySubTab, PullFilter};

use crate::api;
use crate::components::toast::ToastManager;
use crate::components::{ToastSeverity, use_toast};
use crate::types::UiSessionState;

use library::LibraryTab;
use operations::OperationsTab;
use overview::OverviewTab;
use pvp::PvpTab;

#[derive(Props, Clone, PartialEq)]
pub struct HistoryPanelProps {
    pub european: bool,
    /// Filters and selection persist here across tab switches
    pub state: Signal<UiSessionState>,
    /// Which view the active main tab maps to
    pub view: HistorySubTab,
}

/// Backfill progress shared by both tabs; `None` = idle.
#[derive(Clone, Copy)]
pub struct Backfill {
    pub event: Signal<Option<BackfillEvent>>,
    /// Bumped whenever the backend writes a history file (a live pull just
    /// ended, or a log finished parsing); views refetch on it
    pub history: Signal<u32>,
    toast: ToastManager,
}

/// Props equality: the signal handle is what matters; the toast manager is global.
impl PartialEq for Backfill {
    fn eq(&self, other: &Self) -> bool {
        self.event == other.event
    }
}

impl Backfill {
    pub fn running(&self) -> bool {
        matches!(*self.event.read(), Some(BackfillEvent::Progress { .. }))
    }

    /// Kick off a backfill; the worker's progress lands in `event`.
    pub fn start(&self, stale_only: bool) {
        let mut event = self.event;
        let mut toast = self.toast;
        spawn(async move {
            match api::start_history_backfill(stale_only).await {
                Ok(()) => event.set(Some(BackfillEvent::Progress {
                    current: 0,
                    total: 0,
                    file: String::new(),
                    pulls: 0,
                    skipped: false,
                })),
                Err(err) => toast.show(format!("Backfill failed to start: {err}"), ToastSeverity::Normal),
            }
        });
    }

    /// Stop a running backfill; the backend answers with `Done { cancelled: true }`.
    pub fn cancel(&self) {
        let mut toast = self.toast;
        spawn(async move {
            if let Err(err) = api::cancel_history_backfill().await {
                toast.show(format!("Could not cancel: {err}"), ToastSeverity::Normal);
            }
        });
    }
}

#[component]
pub fn HistoryPanel(props: HistoryPanelProps) -> Element {
    let mut toast = use_toast();

    // Initialised from the cached state, synced back one way (see data_explorer.rs)
    let filter: Signal<PullFilter> = use_signal(|| props.state.read().history.filter.clone());
    let mut state = props.state;
    use_effect(move || {
        state.write().history.filter = filter.read().clone();
    });

    // Tauri callbacks run outside the Dioxus runtime, so the listener only
    // writes the signal; the toast happens in the effect below.
    let backfill = Backfill { event: use_signal(|| None::<BackfillEvent>), history: use_signal(|| 0u32), toast };
    use_future(move || async move {
        let mut version = backfill.history;
        let closure = Closure::new(move |_: JsValue| {
            let _ = version.try_write().map(|mut w| *w += 1);
        });
        api::tauri_listen("history-updated", &closure).await;
        closure.forget();
    });
    use_future(move || async move {
        let mut event = backfill.event;
        let closure = Closure::new(move |ev: JsValue| {
            let Ok(payload) = js_sys::Reflect::get(&ev, &JsValue::from_str("payload")) else {
                return;
            };
            if let Ok(parsed) = serde_wasm_bindgen::from_value::<BackfillEvent>(payload) {
                let _ = event.try_write().map(|mut w| *w = Some(parsed));
            }
        });
        api::tauri_listen("history-backfill", &closure).await;
        closure.forget();
    });
    use_effect(move || {
        if let Some(BackfillEvent::Done { pulls, logs, failed, skipped, cancelled, .. }) = *backfill.event.read() {
            let msg = if cancelled {
                "Re-scan cancelled; logs already scanned kept their history".to_string()
            } else {
                format!("Re-scan complete: {pulls} pulls across {logs} logs, {skipped} already current, {failed} failed")
            };
            toast.show(msg, ToastSeverity::Normal);
        }
    });

    // Cleanup progress (startup or on demand) for Log Management, same shape
    let cleanup: Signal<Option<CleanupEvent>> = use_signal(|| None);
    use_future(move || async move {
        let mut cleanup = cleanup;
        let closure = Closure::new(move |ev: JsValue| {
            let Ok(payload) = js_sys::Reflect::get(&ev, &JsValue::from_str("payload")) else {
                return;
            };
            if let Ok(parsed) = serde_wasm_bindgen::from_value::<CleanupEvent>(payload) {
                let _ = cleanup.try_write().map(|mut w| *w = Some(parsed));
            }
        });
        api::tauri_listen("log-cleanup", &closure).await;
        closure.forget();
    });
    use_effect(move || {
        if let Some(CleanupEvent::Done { report: r }) = &*cleanup.read() {
            let failed = if r.failed.is_empty() { String::new() } else { format!(", {} failed", r.failed.len()) };
            toast.show(
                format!("Archived {} and deleted {} logs, freed {}{failed}", r.archived, r.deleted, library::fmt_bytes(r.bytes_freed)),
                ToastSeverity::Normal,
            );
        }
    });

    // Scan bar on the analysis views: logs with no or outdated history, and progress while scanning
    let mut status = use_resource(|| async move { api::get_history_status().await.unwrap_or_default() });
    use_effect(move || {
        if matches!(*backfill.event.read(), Some(BackfillEvent::Done { .. })) {
            status.restart();
        }
    });
    // The bar earns its space only while a scan runs or when there is no
    // history at all; a few logs missing from the statistics are the
    // Library's business, not a banner on every view
    let no_history = status
        .read()
        .as_ref()
        .is_some_and(|s| s.pulls == 0 && s.stale + s.unscanned > 0);

    let tab = props.view;
    rsx! {
        div { class: "history-panel",
            if tab != HistorySubTab::Library && (backfill.running() || no_history) {
                ScanBar { backfill, status }
            }
            match tab {
                HistorySubTab::Overview => rsx! {
                    OverviewTab {
                        european: props.european,
                        state: props.state,
                        filter,
                        backfill,
                    }
                },
                HistorySubTab::Operations => rsx! {
                    OperationsTab {
                        european: props.european,
                        state: props.state,
                        filter,
                        backfill,
                    }
                },
                HistorySubTab::Library => rsx! {
                    LibraryTab { state: props.state, backfill, cleanup, status }
                },
                HistorySubTab::Pvp => rsx! {
                    PvpTab { european: props.european, backfill }
                },
            }
        }
    }
}

/// One line under the toolbar while a scan runs, or when nothing has been
/// scanned yet, with the scan controls the Library has.
#[component]
fn ScanBar(backfill: Backfill, status: Resource<HistoryStatus>) -> Element {
    let read = status.read();
    let s = read.as_ref().cloned().unwrap_or_default();
    let what = match (s.unscanned, s.stale) {
        (0, n) => format!("{n} logs have out-of-date history"),
        (n, 0) => format!("{n} logs have not been scanned yet"),
        (u, st) => format!("{u} logs not scanned yet, {st} out of date"),
    };
    rsx! {
        div { class: "history-scan-bar",
            if let Some(BackfillEvent::Progress { current, total, file, .. }) = backfill.event.read().as_ref() {
                i { class: "fa-solid fa-spinner fa-spin" }
                span {
                    if *total > 0 { "Scanning {current}/{total} · {file}" } else { "Starting scan…" }
                }
                button { class: "btn btn-xs btn-ghost", onclick: move |_| backfill.cancel(), i { class: "fa-solid fa-stop" } " Cancel" }
            } else {
                i { class: "fa-solid fa-circle-info" }
                span { "{what}; scan them to fill in these statistics." }
                button { class: "btn btn-xs", onclick: move |_| backfill.start(true), i { class: "fa-solid fa-rotate" } " Scan now" }
            }
        }
    }
}

/// Top of a view's sidebar: title and the collapse toggle. The folded state
/// is shared by Boss Statistics and Raid Summaries and kept for the session.
#[component]
fn SidebarHeader(state: Signal<UiSessionState>, icon: &'static str, title: &'static str) -> Element {
    let collapsed = state.read().history.sidebar_collapsed;
    rsx! {
        div { class: "sidebar-header",
            div { class: "sidebar-header-row",
                if !collapsed {
                    h3 { i { class: "{icon}" } " {title}" }
                }
                button {
                    class: "sidebar-collapse-btn",
                    title: if collapsed { "Expand {title}" } else { "Collapse {title}" },
                    onclick: move |_| {
                        let mut s = state.write();
                        s.history.sidebar_collapsed = !s.history.sidebar_collapsed;
                    },
                    i { class: if collapsed { "fa-solid fa-angles-right" } else { "fa-solid fa-angles-left" } }
                }
            }
        }
    }
}
