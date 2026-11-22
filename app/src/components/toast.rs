//! Toast notification system for displaying user-facing messages.
//!
//! Provides a global toast manager accessible via context, with auto-dismiss
//! and manual close functionality.

use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;

use crate::api;

/// Severity level for toast notifications.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToastSeverity {
    /// Success messages - 5 second duration
    Success,
    /// Normal warnings/errors - 5 second duration
    Normal,
    /// Critical errors - 7 second duration
    Critical,
}

/// A single toast notification.
#[derive(Clone)]
pub struct Toast {
    pub id: u32,
    pub message: String,
    pub severity: ToastSeverity,
    /// Optional clickable link displayed after the message.
    pub link: Option<String>,
}

/// Global toast manager for showing notifications.
///
/// Access via `use_toast()` from any component.
#[derive(Clone, Copy)]
pub struct ToastManager {
    toasts: Signal<Vec<Toast>>,
    next_id: Signal<u32>,
}

impl ToastManager {
    /// Create a new toast manager with empty state.
    pub fn new() -> Self {
        Self {
            toasts: Signal::new(vec![]),
            next_id: Signal::new(0),
        }
    }

    /// Show a toast notification.
    ///
    /// Toast will auto-dismiss after 5 seconds (Normal) or 7 seconds (Critical).
    /// Maximum 5 toasts are shown at once - oldest is removed if exceeded.
    pub fn show(&mut self, message: impl Into<String>, severity: ToastSeverity) {
        let id = *self.next_id.peek();
        *self.next_id.write() += 1;

        let toast = Toast {
            id,
            message: message.into(),
            severity,
            link: None,
        };

        // Add toast, cap at 5 max (remove oldest if exceeded)
        {
            let mut toasts = self.toasts.write();
            if toasts.len() >= 5 {
                toasts.remove(0);
            }
            toasts.push(toast);
        }

        let duration = match severity {
            ToastSeverity::Success => 5000,
            ToastSeverity::Normal => 5000,
            ToastSeverity::Critical => 7000,
        };
        self.dismiss_after(id, duration);
    }

    /// Auto-dismiss on a plain browser task rather than a Dioxus `spawn`: toasts
    /// are also shown from Tauri event callbacks, outside the runtime, and the
    /// timer must outlive whichever component asked for the toast.
    fn dismiss_after(&self, id: u32, duration_ms: u32) {
        let mut toasts = self.toasts;
        wasm_bindgen_futures::spawn_local(async move {
            TimeoutFuture::new(duration_ms).await;
            let _ = toasts.try_write().map(|mut w| w.retain(|t| t.id != id));
        });
    }

    /// Show a persistent toast notification.
    ///
    /// Toast will NOT auto-dismiss - user must click the X button to close it.
    /// Maximum 5 toasts are shown at once - oldest is removed if exceeded.
    /// Show a toast with a clickable link that auto-dismisses after `duration_ms`.
    pub fn show_with_link(
        &mut self,
        message: impl Into<String>,
        link: String,
        severity: ToastSeverity,
        duration_ms: u32,
    ) {
        let id = *self.next_id.peek();
        *self.next_id.write() += 1;

        let toast = Toast {
            id,
            message: message.into(),
            severity,
            link: Some(link),
        };

        {
            let mut toasts = self.toasts.write();
            if toasts.len() >= 5 {
                toasts.remove(0);
            }
            toasts.push(toast);
        }

        self.dismiss_after(id, duration_ms);
    }

    /// Manually dismiss a toast by ID.
    pub fn dismiss(&mut self, id: u32) {
        self.toasts.write().retain(|t| t.id != id);
    }
}

impl Default for ToastManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Initialize toast provider at app root.
///
/// Call this once in your App component before any children that might use toasts.
pub fn use_toast_provider() -> ToastManager {
    use_context_provider(ToastManager::new)
}

/// Get the toast manager from context.
///
/// Use this in any component to show toasts.
pub fn use_toast() -> ToastManager {
    use_context::<ToastManager>()
}

/// Toast container component - renders all active toasts.
///
/// Place this once at the end of your main layout.
#[component]
pub fn ToastFrame() -> Element {
    let mut manager = use_toast();
    let toasts = manager.toasts.read();

    rsx! {
        div { class: "toast-container",
            for toast in toasts.iter() {
                div {
                    key: "{toast.id}",
                    class: match toast.severity {
                        ToastSeverity::Success => "toast toast-success",
                        ToastSeverity::Normal => "toast",
                        ToastSeverity::Critical => "toast toast-critical",
                    },
                    span { class: "toast-icon",
                        match toast.severity {
                            ToastSeverity::Success => rsx! { i { class: "fa-solid fa-circle-check" } },
                            _ => rsx! { i { class: "fa-solid fa-triangle-exclamation" } },
                        }
                    }
                    span { class: "toast-message",
                        "{toast.message}"
                        if let Some(link) = &toast.link {
                            a {
                                class: "toast-link",
                                onclick: {
                                    let link = link.clone();
                                    move |e: Event<MouseData>| {
                                        e.prevent_default();
                                        let link = link.clone();
                                        spawn(async move { api::open_url(&link).await; });
                                    }
                                },
                                "{link}"
                            }
                        }
                    }
                    button {
                        class: "toast-close",
                        title: "Dismiss",
                        onclick: {
                            let id = toast.id;
                            move |_| manager.dismiss(id)
                        },
                        i { class: "fa-solid fa-xmark" }
                    }
                }
            }
        }
    }
}
