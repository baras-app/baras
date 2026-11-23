//! Lanes of who an entity was targeting over time, with taunts pinned above
//! the bars.

use dioxus::prelude::*;

use baras_types::AggroSegment;

use super::{Palette, Roster, mmss};
use crate::components::effect_uptime::{UptimeAxis, UptimeGrid};

#[derive(Clone, PartialEq)]
pub struct AggroMark {
    pub time: f32,
    pub text: String,
}

#[derive(Clone, PartialEq)]
pub struct AggroLane {
    pub label: String,
    pub segments: Vec<AggroSegment>,
    pub marks: Vec<AggroMark>,
}

#[component]
pub fn AggroTimeline(
    lanes: Vec<AggroLane>,
    palette: Palette,
    roster: Roster,
    /// A stretch of one lane to outline: (lane label, start, end)
    highlight: Signal<Option<(String, f32, f32)>>,
    range_start: f32,
    range_end: f32,
) -> Element {
    let span = (range_end - range_start).max(0.001);
    let pos = move |t: f32| ((t - range_start) / span * 100.0).clamp(0.0, 100.0);
    let legend = palette.legend(lanes.iter().flat_map(|l| &l.segments).map(|s| s.target.as_str()));
    // Legend entry clicked: its bars stay lit, the rest dim
    let mut focus = use_signal(|| None::<String>);
    let lit = focus().filter(|f| legend.iter().any(|k| k.0 == f));
    let dim = |label: &str| if lit.as_deref().is_some_and(|f| f != label) { " dim" } else { "" };

    rsx! {
        div { class: "uptime-timeline aggro-timeline",
            div { class: "aggro-legend",
                for (label, series) in legend.iter().copied() {
                    button {
                        key: "{label}",
                        class: "aggro-key{dim(label)}",
                        title: "Click to highlight; click again to show all",
                        onclick: {
                            let label = label.to_string();
                            move |_| {
                                let same = focus.peek().as_deref() == Some(label.as_str());
                                focus.set((!same).then(|| label.clone()));
                            }
                        },
                        span { class: "aggro-swatch", style: series_color(series) }
                        "{label}"
                    }
                }
                span { class: "aggro-key aggro-key-marks", span { class: "aggro-mark" } "Taunt" }
            }
            for lane in lanes {
                div { class: "uptime-lane",
                    div { class: "uptime-lane-label", title: "{lane.label}",
                        {roster.role(&lane.label)}
                        {roster.discipline(&lane.label)}
                        span { "{lane.label}" }
                    }
                    div { class: "uptime-lane-track",
                        UptimeGrid { range_start, range_end }
                        for s in lane.segments.iter() {
                            {
                                let (key, series) = palette.key(&s.target);
                                rsx! {
                                    div {
                                        class: "uptime-bar{dim(key)}",
                                        style: "left: {pos(s.start)}%; width: {pos(s.end) - pos(s.start)}%; {series_color(series)}",
                                        title: "{s.target}\n{mmss(s.start)} – {mmss(s.end)} ({s.end - s.start:.0}s)",
                                    }
                                }
                            }
                        }
                        LaneHighlight { lane: lane.label.clone(), highlight, range_start, range_end }
                        for m in lane.marks.iter() {
                            span {
                                class: "aggro-mark",
                                style: "left: {pos(m.time)}%",
                                title: "{m.text}",
                            }
                        }
                    }
                }
            }
            UptimeAxis { range_start, range_end }
        }
    }
}

/// The chart series colour a bar or swatch takes; series 0 keeps the grey fallback
fn series_color(series: u8) -> String {
    if series == 0 { String::new() } else { format!("--c: var(--series-{series});") }
}

/// The outline over `lane`'s hovered stretch. Its own component, so a hover
/// re-renders only this and not the lanes.
#[component]
fn LaneHighlight(
    lane: String,
    highlight: Signal<Option<(String, f32, f32)>>,
    range_start: f32,
    range_end: f32,
) -> Element {
    let span = (range_end - range_start).max(0.001);
    let pos = |t: f32| ((t - range_start) / span * 100.0).clamp(0.0, 100.0);
    let Some((_, from, to)) = highlight().filter(|h| h.0 == lane) else {
        return rsx! {};
    };
    rsx! {
        div { class: "aggro-highlight", style: "left: {pos(from)}%; width: {pos(to) - pos(from)}%" }
    }
}
