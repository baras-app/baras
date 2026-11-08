//! Session tools in the status row, one control per job:
//! - `AlacrityPill`: the alacrity and bias numbers HOT tracking uses (they
//!   follow the character's spec and gear, so they live with the session,
//!   not in Settings)
//! - `OpTimerControl`: the operation timer, startable in one click
//! - `BossNotesMenu`: which boss the Notes overlay shows (only rendered while
//!   that overlay is on)

use dioxus::prelude::*;

use crate::api::{self, BossNotesInfo};
use crate::components::{ToastSeverity, use_toast};

fn fmt_timer(secs: u64) -> String {
    if secs >= 3600 {
        format!("{}:{:02}:{:02}", secs / 3600, (secs % 3600) / 60, secs % 60)
    } else {
        format!("{}:{:02}", secs / 60, secs % 60)
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Alacrity + bias
// ─────────────────────────────────────────────────────────────────────────

#[component]
pub fn AlacrityPill() -> Element {
    let mut open = use_signal(|| false);
    let mut alacrity = use_signal(|| 0.0f32);
    let mut latency = use_signal(|| 0i16);

    // Saved numbers seed the inputs once; edits save straight back
    let config = use_resource(|| async move { api::get_config().await });
    use_effect(move || {
        if let Some(Some(cfg)) = config.read().as_ref() {
            alacrity.set(cfg.alacrity_percent);
            latency.set(cfg.latency_ms);
        }
    });
    let save = move || {
        let (a, l) = (alacrity(), latency());
        let mut toast = use_toast();
        spawn(async move {
            if let Some(mut cfg) = api::get_config().await {
                cfg.alacrity_percent = a;
                cfg.latency_ms = l;
                if let Err(err) = api::update_config(&cfg).await {
                    toast.show(format!("Failed to save settings: {err}"), ToastSeverity::Normal);
                }
            }
        });
    };

    rsx! {
        div { class: "session-settings-menu alac-menu",
            button {
                class: if open() { "btn btn-session-settings open" } else { "btn btn-session-settings" },
                title: "Alacrity and bias",
                onclick: move |e| {
                    e.stop_propagation();
                    open.toggle();
                },
                i { class: "fa-solid fa-bolt" }
                span { class: "summary", "{alacrity():.2} %" }
                span { class: "unit", "alac" }
                i { class: "fa-solid fa-chevron-down caret" }
            }
            if open() {
                div { class: "nav-menu-backdrop", onclick: move |_| open.set(false) }
                div { class: "session-settings-pop", onclick: move |e| e.stop_propagation(),
                    span { class: "pop-title", "Alacrity" }
                    div { class: "row",
                        label { "Alacrity" }
                        div { class: "controls",
                            input {
                                r#type: "text",
                                value: "{alacrity():.2}",
                                onchange: move |e| {
                                    if let Ok(v) = e.value().parse::<f32>() {
                                        alacrity.set(v.clamp(0.0, 30.0));
                                        save();
                                    }
                                },
                            }
                            span { class: "unit", "%" }
                        }
                    }
                    div { class: "row",
                        label { "Bias" }
                        div { class: "controls",
                            input {
                                r#type: "text",
                                value: "{latency()}",
                                onchange: move |e| {
                                    if let Ok(v) = e.value().parse::<i16>() {
                                        latency.set(v.clamp(-500, 500));
                                        save();
                                    }
                                },
                            }
                            span { class: "unit", "ms" }
                        }
                    }
                    p { class: "hint", "Alacrity shortens the duration of certain cooldowns, procs, and ticking effects like HOTs and DOTs." }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Operation timer
// ─────────────────────────────────────────────────────────────────────────

/// Hourglass + play/pause; the time and reset appear once there is time on it
#[component]
pub fn OpTimerControl(
    op_timer_secs: Signal<u64>,
    op_timer_running: Signal<bool>,
    /// The service ignores timer commands outside live tailing
    live: bool,
) -> Element {
    let secs = op_timer_secs();
    let running = op_timer_running();
    let started = secs > 0 || running;

    rsx! {
        div {
            class: if running { "op-timer-control running" } else { "op-timer-control" },
            title: if live { "Operation timer" } else { "Operation timer: only runs while live" },
            i { class: "fa-solid fa-hourglass-half" }
            if started {
                span { class: "op-timer-time", "{fmt_timer(secs)}" }
            }
            button {
                class: "op-timer-btn",
                disabled: !live,
                title: if !live { "Only available while live" } else if running { "Pause" } else if started { "Resume" } else { "Start operation timer" },
                onclick: move |e| {
                    e.stop_propagation();
                    spawn(async move {
                        if running { api::stop_operation_timer().await; } else { api::start_operation_timer().await; }
                    });
                },
                i { class: if running { "fa-solid fa-pause" } else { "fa-solid fa-play" } }
            }
            if started {
                button {
                    class: "op-timer-btn reset",
                    disabled: !live,
                    title: "Reset",
                    onclick: move |e| {
                        e.stop_propagation();
                        spawn(async move { api::reset_operation_timer().await; });
                    },
                    i { class: "fa-solid fa-rotate-left" }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Boss notes
// ─────────────────────────────────────────────────────────────────────────

#[component]
pub fn BossNotesMenu(area_bosses: Signal<Vec<BossNotesInfo>>, selected_boss_id: Signal<Option<String>>) -> Element {
    let mut open = use_signal(|| false);
    let bosses: Vec<BossNotesInfo> = area_bosses().iter().filter(|b| b.has_notes).cloned().collect();
    let selected = selected_boss_id();
    let selected_name = bosses.iter().find(|b| selected.as_ref() == Some(&b.id)).map(|b| b.name.clone());
    let none = bosses.is_empty();
    let title = match (&selected_name, none) {
        (_, true) => "Boss notes: none for this area".to_string(),
        (Some(name), _) => format!("Boss notes: {name}"),
        (None, _) => "Boss notes".to_string(),
    };

    rsx! {
        div { class: "session-settings-menu",
            button {
                class: if open() { "btn btn-session-settings open" } else { "btn btn-session-settings" },
                title: "{title}",
                disabled: none,
                onclick: move |e| {
                    e.stop_propagation();
                    open.toggle();
                },
                i { class: "fa-solid fa-note-sticky" }
                i { class: "fa-solid fa-chevron-down caret" }
            }
            if open() {
                div { class: "nav-menu-backdrop", onclick: move |_| open.set(false) }
                div { class: "nav-menu-list", onclick: move |e| e.stop_propagation(),
                    for b in bosses {
                        button {
                            key: "{b.id}",
                            class: if selected.as_ref() == Some(&b.id) { "nav-menu-item active" } else { "nav-menu-item" },
                            onclick: {
                                let id = b.id.clone();
                                move |_| {
                                    let id = id.clone();
                                    selected_boss_id.set(Some(id.clone()));
                                    open.set(false);
                                    spawn(async move { let _ = api::select_boss_notes(&id).await; });
                                }
                            },
                            "{b.name}"
                        }
                    }
                }
            }
        }
    }
}
