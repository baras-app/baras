//! Toast notification system for displaying user-facing messages.
//!
//! Provides a global toast manager accessible via context, with auto-dismiss
//! and manual close functionality.

use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;

/// Severity level for toast notifications.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToastSeverity {
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
        };

        // Add toast, cap at 5 max (remove oldest if exceeded)
        {
            let mut toasts = self.toasts.write();
            if toasts.len() >= 5 {
                toasts.remove(0);
            }
            toasts.push(toast);
        }

        // Auto-dismiss after timeout
        let mut toasts_signal = self.toasts;
        let duration = match severity {
            ToastSeverity::Normal => 5000,
            ToastSeverity::Critical => 7000,
        };

        spawn(async move {
            TimeoutFuture::new(duration).await;
            toasts_signal.write().retain(|t| t.id != id);
        });
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
                        ToastSeverity::Normal => "toast",
                        ToastSeverity::Critical => "toast toast-critical",
                    },
                    span { class: "toast-icon",
                        i { class: "fa-solid fa-triangle-exclamation" }
                    }
                    span { class: "toast-message", "{toast.message}" }
                    button {
                        class: "toast-close",
                        onclick: {
                            let id = toast.id;
                            move |_| manager.dismiss(id)
                        },
                        "X"
                    }
                }
            }
        }
    }
}
