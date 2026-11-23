//! Ability naming toggle (as logged / Republic / Imperial), shown in the
//! top-right of the Data Explorer views that list abilities.

use dioxus::prelude::*;

use crate::components::mirror_names::ABILITY_NAMING;
use crate::types::AbilityNaming;

const NAMINGS: [(AbilityNaming, &str, &str); 3] = [
    (
        AbilityNaming::AsLogged,
        "Log",
        "Ability names as they appear in the log",
    ),
    (AbilityNaming::Republic, "Rep", "Class abilities with Republic names"),
    (AbilityNaming::Imperial, "Imp", "Class abilities with Imperial names"),
];

#[component]
pub fn AbilityNamingPills() -> Element {
    let naming = *ABILITY_NAMING.read();
    rsx! {
        div { class: "filter-pills ml-auto", title: "Ability names",
            for (n, label, tip) in NAMINGS {
                button {
                    class: if naming == n { "seg active" } else { "seg" },
                    title: tip,
                    onclick: move |_| *ABILITY_NAMING.write() = n,
                    "{label}"
                }
            }
        }
    }
}

/// Whether the effect and usage tables show defensive cooldowns only; one
/// setting shared by every table that has the button, kept for the session
pub static DEFENSIVE_ONLY: GlobalSignal<bool> = Signal::global(|| false);

/// The "defensive cooldowns only" toggle. `compact` drops the label.
#[component]
pub fn DefensivePill(#[props(default)] compact: bool, #[props(default)] class: &'static str) -> Element {
    rsx! {
        div { class: "filter-pills {class}",
            button {
                class: if DEFENSIVE_ONLY() { "seg active" } else { "seg" },
                title: "Only defensive cooldowns and their effects",
                onclick: move |_| *DEFENSIVE_ONLY.write() = !DEFENSIVE_ONLY(),
                i { class: "fa-solid fa-shield-halved" }
                if !compact {
                    " Defensive"
                }
            }
        }
    }
}
