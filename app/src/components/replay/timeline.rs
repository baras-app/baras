//! The replay's scrubber: phases as bands, boss casts and deaths as marks,
//! and a playhead. Click or drag to seek; click a death to land before it.

use dioxus::prelude::*;

use baras_types::ReplayEntityKind;

use super::{DEATH_LEAD_SECS, ReplayState};

const TIMELINE_ID: &str = "replay-timeline";

fn mmss(secs: f32) -> String {
    format!("{}:{:02}", (secs / 60.0) as u32, (secs % 60.0) as u32)
}

/// Playback time under a pointer position, from where it is over the bar
fn time_at(client_x: f64, duration: f32) -> Option<f32> {
    let bar = web_sys::window()?.document()?.get_element_by_id(TIMELINE_ID)?;
    let rect = bar.get_bounding_client_rect();
    let share = ((client_x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0);
    Some(share as f32 * duration)
}

#[component]
pub(super) fn Timeline() -> Element {
    let state = use_context::<ReplayState>();
    let ReplayState {
        time,
        player,
        mut highlight,
        ..
    } = state.clone();
    let duration = state.loaded.data.duration_secs.max(0.001);
    let pct = move |t: f32| (t / duration * 100.0).clamp(0.0, 100.0);
    let mut dragging = use_signal(|| false);
    let seek = move |t: f32| {
        if let Some(player) = player.read().as_ref() {
            player.seek(t);
        }
    };
    // Fixed for the fight: (time, entity id, name) per player death, (time, name) per boss cast
    let marks = use_hook(|| {
        let data = &state.loaded.data;
        let deaths: Vec<(f32, i64, String)> = data
            .entities
            .iter()
            .filter(|e| e.kind == ReplayEntityKind::Player)
            .flat_map(|e| e.dead.iter().map(|(at, _)| (*at, e.id, e.name.clone())))
            .collect();
        let casts: Vec<(f32, String)> = data
            .casts
            .iter()
            .filter(|c| data.entities.iter().any(|e| e.id == c.entity_id && e.is_boss))
            .map(|c| (c.start, c.name.clone()))
            .collect();
        std::rc::Rc::new((deaths, casts))
    });
    let (deaths, casts) = (&marks.0, &marks.1);

    rsx! {
        div {
            id: TIMELINE_ID,
            class: "replay-timeline",
            onmousedown: move |e| {
                dragging.set(true);
                if let Some(t) = time_at(e.client_coordinates().x, duration) {
                    seek(t);
                }
            },
            onmousemove: move |e| {
                if dragging()
                    && let Some(t) = time_at(e.client_coordinates().x, duration)
                {
                    seek(t);
                }
            },
            onmouseup: move |_| dragging.set(false),
            onmouseleave: move |_| dragging.set(false),
            for (i, phase) in state.phases.iter().enumerate() {
                div {
                    key: "{i}",
                    class: if i % 2 == 0 { "replay-phase" } else { "replay-phase alt" },
                    style: "left: {pct(phase.start_secs)}%; width: {pct(phase.end_secs) - pct(phase.start_secs)}%",
                    title: "{phase.phase_name} ({mmss(phase.start_secs)} to {mmss(phase.end_secs)})",
                    span { "{phase.phase_name}" }
                }
            }
            for (i, (at, name)) in casts.iter().enumerate() {
                div {
                    key: "c{i}",
                    class: "replay-mark-cast",
                    style: "left: {pct(*at)}%",
                    title: "{name} at {mmss(*at)}",
                }
            }
            for (i, (at, id, name)) in deaths.iter().cloned().enumerate() {
                div {
                    key: "d{i}",
                    class: "replay-mark-death",
                    style: "left: {pct(at)}%",
                    title: "{name} died at {mmss(at)}",
                    // Its own click: not a seek to the exact spot
                    onmousedown: move |e| e.stop_propagation(),
                    onclick: move |_| {
                        seek((at - DEATH_LEAD_SECS).max(0.0));
                        highlight.set(Some(id));
                    },
                }
            }
            div { class: "replay-playhead", style: "left: {pct(time())}%" }
        }
    }
}
