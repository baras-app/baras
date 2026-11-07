//! "Overlay profile changes unsaved" notice with a one-click save, shown in
//! the session bar so its appearance never reflows the header.

use dioxus::prelude::*;

use crate::api;
use crate::components::{ToastSeverity, use_toast};

#[component]
pub fn ProfileUnsavedHint(active_profile: Signal<Option<String>>, mut profile_dirty: Signal<bool>) -> Element {
    if !profile_dirty() {
        return rsx! {};
    }
    rsx! {
        span { class: "header-unsaved-hint",
            i { class: "fa-solid fa-circle-info" }
            " Overlay profile changes unsaved"
            button {
                class: "header-unsaved-save-btn",
                title: "Save changes to the active profile",
                onclick: move |e| {
                    // Inside the session bar's collapse-on-click row
                    e.stop_propagation();
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
