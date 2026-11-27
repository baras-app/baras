//! The Replay section of the Settings tab: map downloads and the maps kept on this computer.

use dioxus::prelude::*;

use super::helpers::save_setting;
use crate::api;

#[component]
pub(super) fn ReplayMapSettings() -> Element {
    let mut downloads = use_signal(|| false);
    // Bytes of downloaded maps on disk
    let mut stored = use_resource(api::get_replay_map_storage);
    use_future(move || async move {
        if let Some(config) = api::get_config().await {
            downloads.set(config.replay_map_downloads);
        }
    });
    let stored_mb = stored.read().unwrap_or(0) as f64 / 1_000_000.0;

    rsx! {
        div { class: "settings-section",
            h4 { "Replay" }
            label { class: "settings-check",
                input {
                    r#type: "checkbox",
                    checked: downloads(),
                    onchange: move |e| {
                        let checked = e.checked();
                        downloads.set(checked);
                        save_setting("settings", move |cfg| cfg.replay_map_downloads = checked);
                    }
                }
                "Download replay maps"
            }
            p { class: "hint",
                "Map images come from GitHub the first time a fight in an operation is replayed (1–5 MB per operation)."
            }
            div { class: "setting-row",
                label { "Downloaded maps: {stored_mb:.1} MB" }
                button {
                    class: "btn btn-small",
                    disabled: stored_mb == 0.0,
                    onclick: move |_| {
                        spawn(async move {
                            api::clear_replay_maps().await;
                            stored.restart();
                        });
                    },
                    "Clear"
                }
            }
        }
    }
}
