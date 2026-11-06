//! One place every "open this log" click goes through. Opening a large log
//! is a full re-parse that can take many seconds in a debug build, so the
//! opener remembers what is loading, the buttons disable, and a banner shows
//! the file until the backend reports the active file changed.

use dioxus::prelude::*;

use crate::api;
use crate::components::toast::{ToastManager, ToastSeverity, use_toast};

#[derive(Clone, Copy)]
pub struct FileOpener {
    /// Full path of the log being parsed, if any
    loading: Signal<Option<String>>,
    is_live_tailing: Signal<bool>,
    toast: ToastManager,
}

impl FileOpener {
    pub fn busy(&self) -> bool {
        self.loading.read().is_some()
    }

    /// Queue the open; `encounter` focuses that fight in the Data Explorer once loaded.
    pub fn open(&self, path: String, encounter: Option<u64>) {
        let mut loading = self.loading;
        let mut is_live_tailing = self.is_live_tailing;
        let mut toast = self.toast;
        spawn(async move {
            loading.set(Some(path.clone()));
            let result = match encounter {
                Some(id) => api::open_historical_encounter(&path, id).await,
                None => api::open_historical_file(&path).await,
            };
            match result {
                Ok(_) => is_live_tailing.set(false),
                Err(err) => {
                    toast.show(format!("Failed to open log file: {err}"), ToastSeverity::Normal);
                    loading.set(None);
                }
            }
        });
    }

    /// Restore an archived log into the log directory, then open it.
    pub fn restore_and_open(&self, filename: String, encounter: Option<u64>) {
        let opener = *self;
        let mut loading = self.loading;
        let mut toast = self.toast;
        spawn(async move {
            // Hold the buttons while the archive inflates; `open` takes over the banner
            loading.set(Some(filename.clone()));
            match api::restore_log(&filename).await {
                Ok(path) => opener.open(path, encounter),
                Err(err) => {
                    toast.show(format!("Restore failed: {err}"), ToastSeverity::Normal);
                    loading.set(None);
                }
            }
        });
    }

    /// The backend finished parsing `path` (from the `active-file-changed` event).
    /// Safe to call from a JS callback: only writes when it is the file we wait on.
    pub fn loaded(&self, path: &str) {
        let mut loading = self.loading;
        if loading.peek().as_deref() == Some(path) {
            let _ = loading.try_write().map(|mut w| *w = None);
        }
    }
}

pub fn use_file_opener_provider(is_live_tailing: Signal<bool>) -> FileOpener {
    let toast = use_toast();
    use_context_provider(|| FileOpener { loading: Signal::new(None), is_live_tailing, toast })
}

pub fn use_file_opener() -> FileOpener {
    use_context::<FileOpener>()
}

/// Fixed banner naming the log being parsed; renders nothing when idle.
#[component]
pub fn FileOpenBanner() -> Element {
    let opener = use_file_opener();
    let loading = opener.loading.read();
    let Some(path) = loading.as_deref() else {
        return rsx! {};
    };
    let name = path.rsplit('/').next().unwrap_or(path);
    rsx! {
        div { class: "file-open-banner",
            i { class: "fa-solid fa-spinner fa-spin" }
            span { "Opening " strong { "{name}" } "…" }
        }
    }
}
