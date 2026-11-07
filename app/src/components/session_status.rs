//! Live / prior-session / historical chip at the end of the session bar.
//! The single place the app states which log it is looking at, with the
//! matching action (restart the watcher, or resume live tailing) built in.

use dioxus::prelude::*;

#[component]
pub fn SessionStatusChip(
    live_tailing: bool,
    watching: bool,
    /// Live tailing, but the last session in the log has ended
    prior: bool,
    on_restart: EventHandler<()>,
    on_resume: EventHandler<()>,
) -> Element {
    let (class, label, title) = if !live_tailing {
        ("historical", "Historical", "Viewing a past log")
    } else if prior {
        ("prior", "Prior Session", "Live, but the last session in this log has ended")
    } else if watching {
        ("live", "Live", "Tailing the newest log")
    } else {
        ("idle", "Idle", "The log watcher is not running")
    };

    rsx! {
        div { class: "session-status-group",
            // Historical mode: the way back to live is its own green button beside the chip
            if !live_tailing {
                button {
                    class: "session-status-resume",
                    title: "Go back to tailing the newest log",
                    onclick: move |e| {
                        e.stop_propagation();
                        on_resume.call(());
                    },
                    i { class: "fa-solid fa-play" }
                    " Resume Live"
                }
            }
            div { class: "session-status-chip {class}", title: "{title}",
                span { class: "status-dot" }
                span { class: "session-status-label", "{label}" }
                if live_tailing {
                    button {
                        class: "session-status-action",
                        title: "Restart watcher",
                        onclick: move |e| {
                            e.stop_propagation();
                            on_restart.call(());
                        },
                        i { class: "fa-solid fa-rotate" }
                    }
                }
            }
        }
    }
}
