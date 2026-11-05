//! History tab: cross-session pull history and log library.
//!
//! Shell with sub-tabs (Overview, Library), a shared filter toolbar, and the
//! backfill listener both tabs read. Each sub-tab fetches its own data lazily;
//! the backend keeps every index, this module only ever holds compact rows.

mod charts;
mod library;
mod library_delete;
mod operations;
mod overview;
mod run_detail;
mod toolbar;

use dioxus::prelude::*;
use wasm_bindgen::prelude::*;

use baras_types::history::{BackfillEvent, HistorySubTab, PullFilter};

use crate::api;
use crate::components::toast::ToastManager;
use crate::components::{ToastSeverity, use_toast};
use crate::types::UiSessionState;

use library::LibraryTab;
use operations::OperationsTab;
use overview::OverviewTab;
use toolbar::HistoryToolbar;

#[derive(Props, Clone, PartialEq)]
pub struct HistoryPanelProps {
    pub european: bool,
    /// Filters and selection persist here across tab switches
    pub state: Signal<UiSessionState>,
}

/// Backfill progress shared by both tabs; `None` = idle.
#[derive(Clone, Copy)]
pub struct Backfill {
    pub event: Signal<Option<BackfillEvent>>,
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
    let mut sub_tab = use_signal(|| props.state.read().history.sub_tab);
    let filter: Signal<PullFilter> = use_signal(|| props.state.read().history.filter.clone());
    let mut state = props.state;
    use_effect(move || {
        let mut s = state.write();
        s.history.sub_tab = *sub_tab.read();
        s.history.filter = filter.read().clone();
    });

    // Tauri callbacks run outside the Dioxus runtime, so the listener only
    // writes the signal; the toast happens in the effect below.
    let backfill = Backfill { event: use_signal(|| None::<BackfillEvent>), toast };
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

    let tab = *sub_tab.read();
    rsx! {
        div { class: "history-panel",
            div { class: "history-head",
                div { class: "data-tab-selector history-subtabs",
                    button {
                        class: if tab == HistorySubTab::Overview { "data-tab active" } else { "data-tab" },
                        onclick: move |_| sub_tab.set(HistorySubTab::Overview),
                        "Overview"
                    }
                    button {
                        class: if tab == HistorySubTab::Operations { "data-tab active" } else { "data-tab" },
                        onclick: move |_| sub_tab.set(HistorySubTab::Operations),
                        "Operations"
                    }
                    button {
                        class: if tab == HistorySubTab::Library { "data-tab active" } else { "data-tab" },
                        onclick: move |_| sub_tab.set(HistorySubTab::Library),
                        "Library"
                    }
                    button { class: "data-tab", disabled: true, title: "Coming later", "Stats" }
                }
                if tab != HistorySubTab::Library {
                    HistoryToolbar { filter }
                }
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
                    LibraryTab { backfill }
                },
            }
        }
    }
}
