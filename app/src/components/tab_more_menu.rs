//! "⋯" at the end of the Data Explorer tab bar: ability naming (as logged /
//! Republic / Imperial) and fullscreen, kept out of the tab row.

use dioxus::prelude::*;

use crate::components::mirror_names::ABILITY_NAMING;
use crate::types::AbilityNaming;

const NAMINGS: [(AbilityNaming, &str, &str); 3] = [
    (AbilityNaming::AsLogged, "Log", "Ability names as they appear in the log"),
    (AbilityNaming::Republic, "Rep", "Class abilities with Republic names"),
    (AbilityNaming::Imperial, "Imp", "Class abilities with Imperial names"),
];

#[component]
pub fn TabMoreMenu(fullscreen: Signal<bool>) -> Element {
    let mut open = use_signal(|| false);
    let naming = *ABILITY_NAMING.read();

    rsx! {
        div { class: "nav-menu tab-more",
            button { class: "tab-more-btn", title: "More options", onclick: move |_| open.toggle(),
                i { class: "fa-solid fa-ellipsis" }
            }
            if open() {
                div { class: "nav-menu-backdrop", onclick: move |_| open.set(false) }
                div { class: "nav-menu-list tab-more-list",
                    div { class: "tab-more-label", "Ability names" }
                    div { class: "history-seg seg-fill",
                        for (n, label, tip) in NAMINGS {
                            button {
                                class: if naming == n { "seg active" } else { "seg" },
                                title: tip,
                                onclick: move |_| *ABILITY_NAMING.write() = n,
                                "{label}"
                            }
                        }
                    }
                    div { class: "nav-menu-divider" }
                    button {
                        class: "nav-menu-item",
                        onclick: move |_| {
                            fullscreen.toggle();
                            open.set(false);
                        },
                        if fullscreen() {
                            i { class: "fa-solid fa-down-left-and-up-right-to-center" }
                            "Exit fullscreen"
                        } else {
                            i { class: "fa-solid fa-up-right-and-down-left-from-center" }
                            "Fullscreen"
                        }
                    }
                }
            }
        }
    }
}
