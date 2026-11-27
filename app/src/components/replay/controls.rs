//! Playback controls under the replay canvas

use dioxus::prelude::*;

use baras_types::formatting;

use super::ReplayState;

const SPEEDS: [f64; 4] = [0.5, 1.0, 2.0, 4.0];

/// A combat time as mm:ss.s
fn clock(secs: f32) -> String {
    format!("{:02}:{:04.1}", (secs / 60.0) as u32, secs % 60.0)
}

#[component]
pub(super) fn Controls() -> Element {
    let state = use_context::<ReplayState>();
    let ReplayState {
        time, playing, player, ..
    } = state.clone();
    let duration = state.loaded.data.duration_secs;
    let mut speed = use_signal(|| 1.0f64);
    let mut scale = use_signal(|| 1.0f64);
    let mut show_dps = use_signal(|| false);
    let mut show_hps = use_signal(|| false);
    let with_player = move |f: &dyn Fn(&super::Player)| {
        if let Some(player) = player.read().as_ref() {
            f(player);
        }
    };
    let rates = (show_dps() || show_hps()).then(|| state.loaded.raid_rates(time()));
    let compact = |v: f32| formatting::format_compact(v as i64, state.european);

    rsx! {
        div { class: "replay-controls",
            button {
                class: "btn btn-small",
                onclick: move |_| with_player(&|p| if playing() { p.pause() } else { p.play() }),
                if playing() { "Pause" } else { "Play" }
            }
            button {
                class: "btn btn-small",
                onclick: move |_| with_player(&|p| p.seek(0.0)),
                "Restart"
            }
            select {
                class: "replay-speed",
                title: "Playback speed",
                onchange: move |e| {
                    if let Ok(v) = e.value().parse::<f64>() {
                        speed.set(v);
                        with_player(&|p| p.set_speed(v));
                    }
                },
                for s in SPEEDS {
                    option { value: "{s}", selected: speed() == s, "{s}×" }
                }
            }
            span { class: "replay-clock", "{clock(time())}" }
            input {
                class: "replay-scrubber",
                r#type: "range",
                min: "0",
                max: "{duration}",
                step: "0.1",
                value: "{time()}",
                oninput: move |e| {
                    if let Ok(v) = e.value().parse::<f32>() {
                        with_player(&|p| p.seek(v));
                    }
                },
            }
            span { class: "replay-clock", "{clock(duration)}" }
            label { class: "replay-option", title: "Zoom",
                "Scale"
                input {
                    r#type: "range",
                    min: "0.5",
                    max: "4",
                    step: "0.1",
                    value: "{scale()}",
                    oninput: move |e| {
                        if let Ok(v) = e.value().parse::<f64>() {
                            scale.set(v);
                            with_player(&|p| p.set_scale(v));
                        }
                    },
                }
            }
            label { class: "replay-option",
                input { r#type: "checkbox", checked: show_dps(), onchange: move |e| show_dps.set(e.checked()) }
                "Raid DPS"
            }
            label { class: "replay-option",
                input { r#type: "checkbox", checked: show_hps(), onchange: move |e| show_hps.set(e.checked()) }
                "Raid HPS"
            }
            if let Some((dps, hps)) = rates {
                span { class: "replay-rates",
                    if show_dps() { span { "DPS {compact(dps)}" } }
                    if show_hps() { span { "HPS {compact(hps)}" } }
                }
            }
        }
    }
}
