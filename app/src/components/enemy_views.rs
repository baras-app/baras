//! Pieces of the enemy tabs (Raid Debuffs).

use dioxus::prelude::*;

use crate::types::EnemyViewsState;

/// The "Bosses only" pill; shown only when the fight has a boss to keep
#[component]
pub fn BossesOnlyToggle(has_boss: bool, state: Signal<EnemyViewsState>) -> Element {
    let on = state.read().bosses_only;
    rsx! {
        if has_boss {
            div { class: "filter-pills",
                button {
                    class: if on { "seg active" } else { "seg" },
                    title: "Hide adds",
                    onclick: move |_| state.with_mut(|s| s.bosses_only = !s.bosses_only),
                    "Bosses only"
                }
            }
        }
    }
}

/// A tab's query is still running
pub fn loading() -> Element {
    rsx! {
        div { class: "uptime-empty", span { class: "loading-late", "Loading…" } }
    }
}

/// A tab's query came back with an error (only successes are cached, so a
/// retry asks the backend again)
#[component]
pub fn LoadFailed(on_retry: EventHandler) -> Element {
    rsx! {
        div { class: "uptime-empty",
            "Could not load this encounter's data. "
            button { class: "btn btn-small", onclick: move |_| on_retry.call(()), "Retry" }
        }
    }
}

/// Live combat: these tabs read the whole fight, so they wait for its end
pub fn live_notice() -> Element {
    rsx! {
        div { class: "uptime-empty", "Available once combat ends" }
    }
}
