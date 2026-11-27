//! Help button and popup: why a timer may never fire (what the game leaves out of the
//! combat log, and what else shifts a boss's timings)

use dioxus::prelude::*;

use crate::api;

/// (what, how it affects timers, examples)
const FACTORS: [(&str, &str, &str); 4] = [
    (
        "Stealth",
        "While an entity is stealthed, most of its events stop being logged until it reappears.",
        "Example: Nahut.",
    ),
    (
        "Distance",
        "Events that happen too far from your character are not sent to your client.",
        "Examples: the Eternity Vault pylons, Styrak's Nightmare Manifestation.",
    ),
    (
        "False combat exits",
        "On some fights, dying and being revived takes you out of combat and straight back in. The game then forgets which NPCs you have used abilities on.",
        "Examples: Revan, Coratanni.",
    ),
    (
        "Boss priorities and cooldowns",
        "A boss's ability order can shift with animation delays, movement, leaps, or plain chance when two abilities share a priority. Phase and attack wave changes can delay casts further.",
        "Examples: XR-53 has five abilities at the same priority; Bestia's cooldown windows are so tight that a leap changes her attack pattern.",
    ),
];

/// Whether the user has ever opened the popup (saved in the config); `None`
/// until the config is read
static SEEN: GlobalSignal<Option<bool>> = Signal::global(|| None);

/// Toolbar button that opens the popup. Most players do not know these
/// rules, so it is highlighted until it has been opened once
#[component]
pub(super) fn LogLimitsHelp() -> Element {
    let mut open = use_signal(|| false);
    use_future(move || async move {
        if SEEN().is_none()
            && let Some(cfg) = api::get_config().await
        {
            *SEEN.write() = Some(cfg.log_limits_seen);
        }
    });
    // Quiet while the config loads, so a returning user never sees it flash
    let seen = SEEN().unwrap_or(true);
    rsx! {
        button {
            class: "btn btn-sm log-limits-btn",
            class: if !seen { "unread" },
            title: "What the game leaves out of the combat log, and what else shifts a boss's timings",
            onclick: move |_| {
                open.set(true);
                if !seen {
                    *SEEN.write() = Some(true);
                    spawn(async move {
                        if let Some(mut cfg) = api::get_config().await {
                            cfg.log_limits_seen = true;
                            let _ = api::update_config(&cfg).await;
                        }
                    });
                }
            },
            i { class: "fa-solid fa-circle-question" }
            " Timer not firing?"
        }
        if open() {
            div { class: "modal-overlay", onclick: move |_| open.set(false),
                div { class: "modal-content log-limits", onclick: move |e| e.stop_propagation(),
                    div { class: "modal-header",
                        h3 { "Why a timer may not fire" }
                    }
                    div { class: "log-limits-body",
                        p { "The game leaves some events out of the combat log. A timer that waits for one of them never starts." }

                        h4 { "What the game logs" }
                        p { "An event is written to your log only when its source or target is one of:" }
                        ol {
                            li { "A member of your group, including you" }
                            li { "An NPC you have used an ability on directly" }
                            li { "An NPC you have set as your focus target at any point" }
                        }
                        p {
                            "If you have neither used an ability on an NPC nor focus-targeted it, "
                            strong { "its ability activations and the buffs and debuffs it puts on itself are missing from your log" }
                            ". Timers built on those events will not fire for you."
                        }

                        h4 { "What else affects timers" }
                        ul {
                            for (what, how, examples) in FACTORS {
                                li {
                                    strong { "{what}: " }
                                    "{how} "
                                    span { class: "log-limits-examples", "{examples}" }
                                }
                            }
                        }
                    }
                    div { class: "modal-footer",
                        button { class: "btn", "data-esc-close": true, onclick: move |_| open.set(false), "Close" }
                    }
                }
            }
        }
    }
}
