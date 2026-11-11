//! Unsaved overlay changes notice with a one-click save, shown in the session
//! bar so its appearance never reflows the header. Covers both kinds of
//! pending change: customization edits still in the settings panel's draft,
//! and toggles/moves not yet written to the active profile.

use dioxus::prelude::*;

use crate::api;
use crate::components::{ToastSeverity, use_toast};

/// The settings panel holds customization edits not yet saved (mirrored by the panel)
pub static OVERLAY_DRAFT_DIRTY: GlobalSignal<bool> = Signal::global(|| false);
/// Bumped to ask the open settings panel to run its Save
pub static OVERLAY_SAVE_REQUEST: GlobalSignal<u32> = Signal::global(|| 0);

#[component]
pub fn ProfileUnsavedHint(active_profile: Signal<Option<String>>, mut profile_dirty: Signal<bool>) -> Element {
    let draft_dirty = OVERLAY_DRAFT_DIRTY();
    if !profile_dirty() && !draft_dirty {
        return rsx! {};
    }
    let title = match active_profile() {
        Some(name) => format!("Overlay changes not saved to profile “{name}”"),
        None => "Overlay changes not saved".to_string(),
    };
    rsx! {
        span { class: "header-unsaved-hint", title: "{title}",
            span { class: "header-unsaved-dot" }
            "Unsaved"
            button {
                class: "header-unsaved-save-btn",
                title: "Save overlay changes",
                onclick: move |e| {
                    // Inside the session bar's collapse-on-click row
                    e.stop_propagation();
                    // The panel's Save writes the draft and the profile together
                    if draft_dirty {
                        *OVERLAY_SAVE_REQUEST.write() += 1;
                        return;
                    }
                    let Some(name) = active_profile() else { return };
                    let mut toast = use_toast();
                    spawn(async move {
                        if let Err(err) = api::save_profile(&name).await {
                            toast.show(format!("Failed to save profile: {}", err), ToastSeverity::Normal);
                        } else {
                            profile_dirty.set(false);
                        }
                    });
                },
                i { class: "fa-solid fa-floppy-disk" }
                " Save"
            }
        }
    }
}
