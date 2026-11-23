//! Tank swap summary: each enemy's holders in order, how long each held it,
//! what made the change, and who it went to next.

use dioxus::prelude::*;

use baras_types::{AggroRow, AggroSegment, TAUNT_SETTLE_SECS, ThreatEvent, ThreatEventKind};

use super::{Roster, mmss};

/// An enemy that turns away for less than this and back again (a cast on
/// someone else) did not change hands
const GLANCE_SECS: f32 = 2.0;
/// A death this long before a stint ends, or just after, is why it ended
const DEATH_SECS: f32 = 3.0;

#[derive(Clone, PartialEq)]
struct Stint {
    holder: String,
    start: f32,
    end: f32,
}

/// Consecutive time on one target, glances away folded in
fn stints(segments: &[AggroSegment]) -> Vec<Stint> {
    let mut runs: Vec<Stint> = Vec::new();
    for s in segments {
        match runs.last_mut() {
            Some(last) if last.holder == s.target => last.end = s.end,
            _ => runs.push(Stint { holder: s.target.clone(), start: s.start, end: s.end }),
        }
    }
    let mut out: Vec<Stint> = Vec::new();
    for (i, run) in runs.iter().enumerate() {
        let glance = run.end - run.start < GLANCE_SECS
            && i > 0
            && runs.get(i + 1).is_some_and(|next| next.holder == runs[i - 1].holder);
        match out.last_mut() {
            _ if glance => {}
            Some(last) if last.holder == run.holder => last.end = run.end,
            _ => out.push(run.clone()),
        }
    }
    out
}

/// The taunt that started `stint` on `enemy`, as its ability name
fn taunt_for<'a>(events: &'a [ThreatEvent], enemy: &str, stint: &Stint) -> Option<&'a str> {
    events
        .iter()
        .filter(|e| e.kind != ThreatEventKind::Drop && e.source == stint.holder)
        .filter(|e| e.time_secs <= stint.start + 0.1 && e.time_secs >= stint.start - TAUNT_SETTLE_SECS)
        .find(|e| e.targets.iter().any(|t| t.name == enemy))
        .map(|e| e.ability_name.as_str())
}

#[component]
pub fn TankSwaps(
    enemies: Vec<AggroRow>,
    events: Vec<ThreatEvent>,
    /// (name, combat second) of players and enemies that died
    deaths: Vec<(String, f32)>,
    /// Set to the stint under the pointer: (enemy, start, end)
    mut hovered: Signal<Option<(String, f32, f32)>>,
    roster: Roster,
) -> Element {
    let died = |name: &str, at: f32| deaths.iter().any(|d| d.0 == name && d.1 >= at - DEATH_SECS && d.1 <= at + 1.0);
    let who = |name: &str| {
        rsx! {
            span { class: "name-with-icon",
                {roster.role(name)}
                {roster.discipline(name)}
                span { class: "name-text", "{name}" }
            }
        }
    };

    rsx! {
        div { class: "threat-panel",
            div { class: "uptime-toolbar",
                span { class: "uptime-title", "Who held each enemy" }
            }
            div { class: "threat-table-box",
                table { class: "threat-table swap-table",
                    thead {
                        tr {
                            th { class: "threat-name", "On" }
                            th { class: "num threat-time", "From" }
                            th { class: "num threat-time", "To" }
                            th { class: "num threat-time", "Duration" }
                            th { class: "threat-name", "Changed by" }
                            th { class: "threat-name", "Taken from" }
                            th { "Passed to" }
                        }
                    }
                    for enemy in enemies.iter() {
                        // One body per enemy: its name row stays under the header while its rows scroll
                        tbody { key: "{enemy.name}",
                            tr { class: "swap-enemy",
                                td { colspan: "7",
                                    span { class: if enemy.is_boss { "name-text" } else { "name-text npc" }, "{enemy.name}" }
                                }
                            }
                            {
                                let stints = stints(&enemy.segments);
                                rsx! {
                                    for (i, stint) in stints.iter().enumerate() {
                                        tr {
                                            onmouseenter: {
                                                let at = (enemy.name.clone(), stint.start, stint.end);
                                                move |_| hovered.set(Some(at.clone()))
                                            },
                                            onmouseleave: move |_| hovered.set(None),
                                            td { title: "{stint.holder}", {who(&stint.holder)} }
                                            td { class: "num threat-time", "{mmss(stint.start)}" }
                                            td { class: "num threat-time", "{mmss(stint.end)}" }
                                            td { class: "num threat-time", "{mmss(stint.end - stint.start)}" }
                                            td { class: "threat-outcome",
                                                if i == 0 {
                                                    "first target"
                                                } else if died(&stints[i - 1].holder, stint.start) {
                                                    span { class: "threat-outcome took", "{stints[i - 1].holder} died" }
                                                } else if let Some(ability) = taunt_for(&events, &enemy.name, stint) {
                                                    span { class: "threat-outcome took", "{ability}" }
                                                } else {
                                                    "no taunt"
                                                }
                                            }
                                            td {
                                                if i > 0 {
                                                    {who(&stints[i - 1].holder)}
                                                }
                                            }
                                            td {
                                                span { class: "name-with-icon",
                                                    if let Some(next) = stints.get(i + 1) {
                                                        {who(&next.holder)}
                                                    }
                                                    if died(&stint.holder, stint.end) {
                                                        span { class: "threat-outcome", "{stint.holder} died" }
                                                    } else if i + 1 == stints.len() && died(&enemy.name, stint.end) {
                                                        span { class: "threat-outcome", "enemy died" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
