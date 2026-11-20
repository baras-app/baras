//! Session tools in the status row, one control per job:
//! - `AlacrityPill`: the alacrity and bias numbers HOT tracking uses (they
//!   follow the character's spec and gear, so they live with the session,
//!   not in Settings)
//! - `OpTimerControl`: the operation timer, startable in one click
//! - `BossNotesMenu`: which boss the Notes overlay shows (only rendered while
//!   that overlay is on)

use baras_types::AlacrityPreset;
use dioxus::prelude::*;

use crate::api::{self, BossNotesInfo};
use crate::components::discipline_selector::ALL_DISCIPLINES;
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
pub fn AlacrityPill(
    /// The logging character and discipline, to tell when a saved value is in use
    player_name: Option<String>,
    player_discipline: Option<String>,
) -> Element {
    let mut open = use_signal(|| false);
    let mut alacrity = use_signal(|| 0.0f32);
    let mut latency = use_signal(|| 0i16);
    let mut presets = use_signal(Vec::<AlacrityPreset>::new);

    // Saved numbers seed the inputs; edits save straight back. The service
    // changes the alacrity itself when a saved value applies, so reload then.
    let mut config = use_resource(|| async move { api::get_config().await });
    use_effect(move || {
        if let Some(Some(cfg)) = config.read().as_ref() {
            alacrity.set(cfg.alacrity_percent);
            latency.set(cfg.latency_ms);
            presets.set(cfg.alacrity_presets.clone());
        }
    });
    let mut auto_sets = use_signal(|| 0u32);
    api::use_tauri_event("alacrity-auto-set", move |_| {
        let _ = auto_sets.try_write().map(|mut w| *w += 1);
    });
    use_effect(move || {
        if *auto_sets.read() > 0 {
            config.restart();
        }
    });
    let save = move || {
        let (a, l, p) = (alacrity(), latency(), presets());
        let mut toast = use_toast();
        spawn(async move {
            if let Err(err) = api::patch_config(|cfg| {
                cfg.alacrity_percent = a;
                cfg.latency_ms = l;
                cfg.alacrity_presets = p;
            })
            .await
            {
                toast.show(format!("Failed to save settings: {err}"), ToastSeverity::Normal);
            }
        });
    };

    // Add form: discipline, character ("" = any) and value
    let mut adding = use_signal(|| false);
    let mut new_discipline = use_signal(String::new);
    let mut new_character = use_signal(String::new);
    let mut new_value = use_signal(|| 0.0f32);
    let mut characters = use_signal(Vec::<String>::new);
    let (name, discipline) = (player_name.clone(), player_discipline.clone());
    let start_adding = move |_| {
        new_discipline.set(
            discipline
                .clone()
                .unwrap_or_else(|| ALL_DISCIPLINES[0].1[0].to_string()),
        );
        new_character.set(name.clone().unwrap_or_default());
        new_value.set(alacrity());
        adding.set(true);
        let current = name.clone();
        spawn(async move {
            let mut names: Vec<String> = api::get_character_roster()
                .await
                .map(|r| r.rows.into_iter().map(|c| c.name).collect())
                .unwrap_or_default();
            names.extend(current);
            names.sort();
            names.dedup();
            characters.set(names);
        });
    };
    let mut add = move || {
        let preset = AlacrityPreset {
            discipline: new_discipline(),
            character: Some(new_character()).filter(|c| !c.is_empty()),
            alacrity_percent: new_value(),
        };
        let mut list = presets.write();
        list.retain(|p| p.discipline != preset.discipline || p.character != preset.character);
        list.push(preset);
        list.sort_by(|a, b| (&a.discipline, &a.character).cmp(&(&b.discipline, &b.character)));
        drop(list);
        adding.set(false);
        save();
    };

    let saved = presets();
    let in_use = AlacrityPreset::find(
        &saved,
        player_discipline.as_deref().unwrap_or_default(),
        player_name.as_deref().unwrap_or_default(),
    )
    .filter(|p| p.alacrity_percent == alacrity());

    // Other characters' values stay folded away so the list stays short
    let mut show_all = use_signal(|| false);
    let mine = |p: &AlacrityPreset| p.character.is_none() || p.character == player_name;
    let others = saved.iter().filter(|p| !mine(p)).count();

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
                if let Some(p) = in_use {
                    i {
                        class: "fa-solid fa-user-check alac-auto",
                        title: "Saved value for {p.discipline} in use",
                    }
                }
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

                    span { class: "pop-title", "Saved per discipline" }
                    for (idx, p) in saved.iter().enumerate().filter(|(_, p)| show_all() || mine(p)) {
                        div { class: "alac-preset", key: "{p.discipline}|{p.character:?}",
                            span { class: "name",
                                "{p.discipline}"
                                span { class: "who",
                                    " · "
                                    {p.character.as_deref().unwrap_or("any character")}
                                }
                            }
                            span { class: "value", "{p.alacrity_percent:.2} %" }
                            button {
                                class: "btn-icon",
                                title: "Remove",
                                onclick: move |_| {
                                    presets.write().remove(idx);
                                    save();
                                },
                                i { class: "fa-solid fa-xmark" }
                            }
                        }
                    }
                    if others > 0 {
                        button { class: "alac-more", onclick: move |_| show_all.toggle(),
                            if show_all() { "Show this character only" } else { "Show {others} for other characters" }
                        }
                    }
                    if adding() {
                        div { class: "alac-add",
                            select {
                                title: "Discipline",
                                onchange: move |e| new_discipline.set(e.value()),
                                for (class, names) in ALL_DISCIPLINES {
                                    optgroup { label: "{class}",
                                        for n in names.iter() {
                                            option { value: "{n}", selected: new_discipline() == *n, "{n}" }
                                        }
                                    }
                                }
                            }
                            select {
                                title: "Character",
                                onchange: move |e| new_character.set(e.value()),
                                option { value: "", selected: new_character().is_empty(), "Any character" }
                                for c in characters() {
                                    option { value: "{c}", selected: new_character() == c, "{c}" }
                                }
                            }
                            div { class: "controls",
                                input {
                                    r#type: "text",
                                    value: "{new_value():.2}",
                                    onchange: move |e| {
                                        if let Ok(v) = e.value().parse::<f32>() {
                                            new_value.set(v.clamp(0.0, 30.0));
                                        }
                                    },
                                }
                                span { class: "unit", "%" }
                                button { class: "btn btn-sm", onclick: move |_| add(), "Save" }
                                button { class: "btn btn-sm btn-ghost", onclick: move |_| adding.set(false), "Cancel" }
                            }
                        }
                    } else {
                        button { class: "btn btn-sm btn-ghost alac-add-btn", onclick: start_adding,
                            i { class: "fa-solid fa-plus" }
                            " Save a value for a discipline"
                        }
                    }
                    p { class: "hint", "A saved value is applied when you log in or change discipline. Gear is not logged, so a discipline played in two gear sets needs the value typed in." }
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
    let selected_name = bosses
        .iter()
        .find(|b| selected.as_ref() == Some(&b.id))
        .map(|b| b.name.clone());
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
