//! Ability naming toggle (as logged / Republic / Imperial), shown in the
//! top-right of the Data Explorer views that list abilities.

use dioxus::prelude::*;

use crate::components::mirror_names::ABILITY_NAMING;
use crate::types::AbilityNaming;

const NAMINGS: [(AbilityNaming, &str, &str); 3] = [
    (AbilityNaming::AsLogged, "Log", "Ability names as they appear in the log"),
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
