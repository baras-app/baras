//! Shown in place of the replay while its maps are not on this computer.

use dioxus::prelude::*;

/// Asks before the first map download, or offers a retry after a failed one
#[component]
pub fn MapPrompt(failed: bool, on_download: EventHandler) -> Element {
    let (text, action) = match failed {
        true => ("The maps for this fight could not be downloaded. ", "Retry"),
        false => (
            "Replay maps are downloaded from GitHub the first time a fight in an operation is replayed \
             (1–5 MB per operation) and kept on this computer. ",
            "Download maps",
        ),
    };
    rsx! {
        div { class: "uptime-empty",
            "{text}"
            button { class: "btn btn-small", onclick: move |_| on_download.call(()), "{action}" }
        }
    }
}
