//! Unsaved overlay changes notice with a one-click save, shown in the session
//! bar so its appearance never reflows the header. Covers customization edits
//! still in the settings panel's draft; toggles and moves save themselves.

use dioxus::prelude::*;


/// The settings panel holds customization edits not yet saved (mirrored by the panel)
pub static OVERLAY_DRAFT_DIRTY: GlobalSignal<bool> = Signal::global(|| false);
/// Bumped to ask the open settings panel to run its Save
pub static OVERLAY_SAVE_REQUEST: GlobalSignal<u32> = Signal::global(|| 0);

#[component]
pub fn ProfileUnsavedHint(active_profile: Signal<Option<String>>) -> Element {
    if !OVERLAY_DRAFT_DIRTY() {
        return rsx! {};
    }
    let title = match active_profile() {
        Some(name) => format!("Overlay changes not saved to profile “{name}”"),
        None => "Overlay changes not saved".to_string(),
    };
    rsx! {
        span { class: "header-unsaved-hint", title: "{title}",
            i { class: "fa-solid fa-layer-group" }
            "Unsaved"
            button {
                class: "header-unsaved-save-btn",
                title: "Save overlay changes",
                onclick: move |e| {
                    // Inside the session bar's collapse-on-click row
                    e.stop_propagation();
                    // The panel's Save writes the draft and the profile together
                    *OVERLAY_SAVE_REQUEST.write() += 1;
                },
                i { class: "fa-solid fa-floppy-disk" }
                " Save"
            }
        }
    }
}
