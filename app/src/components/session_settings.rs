//! Session settings behind one pill in the status row: the operation timer,
//! the alacrity and bias numbers the HOT tracking uses (they follow the
//! character's spec and gear, so they live with the session, not in
//! Settings), and the boss notes selector. The pill shows what matters at a
//! glance, the running timer or else the alacrity, and drops a small panel
//! for the rest, so the row stays one line at any window width.

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

#[component]
pub fn SessionSettingsPill(
    op_timer_secs: Signal<u64>,
    op_timer_running: Signal<bool>,
    area_bosses: Signal<Vec<BossNotesInfo>>,
    selected_boss_id: Signal<Option<String>>,
    /// "Logged on …" line for the panel footer
    session_time: Option<String>,
) -> Element {
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

    let secs = op_timer_secs();
    let running = op_timer_running();
    let timer = fmt_timer(secs);
    let bosses: Vec<BossNotesInfo> = area_bosses().iter().filter(|b| b.has_notes).cloned().collect();
    let selected = selected_boss_id();

    rsx! {
        div { class: "session-settings-menu",
            button {
                class: format!("btn btn-session-settings{}{}", if *open.read() { " open" } else { "" }, if running { " running" } else { "" }),
                title: "Op timer, alacrity and bias, boss notes",
                onclick: move |e| {
                    e.stop_propagation();
                    open.toggle();
                },
                i { class: "fa-solid fa-sliders" }
                if running {
                    i { class: "fa-solid fa-stopwatch" }
                    span { class: "summary", "{timer}" }
                } else {
                    span { class: "summary", "{alacrity():.2} %" }
                }
                i { class: "fa-solid fa-chevron-down caret" }
            }
            if *open.read() {
                div { class: "nav-menu-backdrop", onclick: move |_| open.set(false) }
                div { class: "session-settings-pop", onclick: move |e| e.stop_propagation(),
                    div { class: "row",
                        label { "Op timer" }
                        div { class: "controls",
                            span { class: if running { "op-timer-display running" } else { "op-timer-display" }, "{timer}" }
                            button {
                                class: "btn btn-op-timer",
                                title: if running { "Stop" } else if secs > 0 { "Resume" } else { "Start" },
                                onclick: move |_| {
                                    spawn(async move {
                                        if running { api::stop_operation_timer().await; } else { api::start_operation_timer().await; }
                                    });
                                },
                                i { class: if running { "fa-solid fa-pause" } else { "fa-solid fa-play" } }
                            }
                            if secs > 0 || running {
                                button {
                                    class: "btn btn-op-timer btn-op-timer-reset",
                                    title: "Reset",
                                    onclick: move |_| { spawn(async move { api::reset_operation_timer().await; }); },
                                    i { class: "fa-solid fa-rotate-left" }
                                }
                            }
                        }
                    }
                    div { class: "divider" }
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
                        label { "Bias adjustment" }
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
                    p { class: "hint", "Alacrity shortens your HOT durations. If tracked durations are consistently off, bias shifts them by this many milliseconds either way." }
                    div { class: "divider" }
                    div { class: "row",
                        label { "Boss notes" }
                        if bosses.is_empty() {
                            select { class: "notes-boss-select", disabled: true, option { value: "", "No notes for this area" } }
                        } else {
                            select {
                                class: "notes-boss-select",
                                onchange: move |e| {
                                    let id = e.value();
                                    if !id.is_empty() {
                                        selected_boss_id.set(Some(id.clone()));
                                        spawn(async move { let _ = api::select_boss_notes(&id).await; });
                                    }
                                },
                                option { value: "", "Select boss…" }
                                for b in bosses.iter() {
                                    option { value: "{b.id}", selected: selected.as_ref() == Some(&b.id), "{b.name}" }
                                }
                            }
                        }
                    }
                    if let Some(text) = session_time.as_ref() {
                        span { class: "session-time-info", i { class: "fa-regular fa-calendar" } " {text}" }
                    }
                }
            }
        }
    }
}
