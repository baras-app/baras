//! Effect Uptime tab: everything the sidebar's selected source applied, as a
//! table of targets (rows) × effects (columns), uptime % per cell. Answers
//! "my HoT on each tank", "my DoTs on the boss vs adds", "did the debuff stay up".

use dioxus::prelude::*;

use baras_types::formatting;

use crate::api::{self, EffectUptimeCell, TimeRange};
use crate::components::ability_icon::AbilityIcon;

#[derive(Clone, Copy, PartialEq)]
enum Targets {
    All,
    Players,
    Npcs,
}

#[component]
pub fn EffectUptimeView(
    encounter_idx: Option<u32>,
    time_range: TimeRange,
    duration_secs: f32,
    selected_source: Option<String>,
    /// Instance id when the source is one NPC spawn among same-named copies
    selected_source_id: Option<i64>,
    european: bool,
) -> Element {
    let mut targets = use_signal(|| Targets::All);
    // Timeline selection: a row (target), a column (effect), or a cell (both;
    // shown as the target's timeline with that effect emphasised)
    let mut picked_target = use_signal(|| None::<String>);
    let mut picked_effect = use_signal(|| None::<i64>);
    let selector = selected_source.clone().map(|n| selected_source_id.map_or(n, |id| format!("id:{id}")));

    let cells = use_resource(use_reactive!(|(encounter_idx, time_range, duration_secs, selector)| async move {
        let source = selector?;
        let tr = (time_range.end > 0.0).then_some(time_range);
        api::query_effect_uptime_by_target(encounter_idx, &source, tr.as_ref(), duration_secs).await
    }));

    let Some(source) = selected_source else {
        return rsx! { div { class: "uptime-empty", "Pick a source in the sidebar to see the effects it applied" } };
    };

    let all = cells.read().clone().flatten().unwrap_or_default();
    let shown = targets();
    let visible: Vec<&EffectUptimeCell> = all
        .iter()
        .filter(|c| match shown {
            Targets::All => true,
            Targets::Players => matches!(c.target_type.as_str(), "Player" | "Companion" | "Self"),
            Targets::Npcs => c.target_type == "Npc",
        })
        .collect();

    // Columns: effects by total uptime across targets; rows: the source
    // itself first, then players, then NPCs, each by name
    let mut effects: Vec<(i64, &str, i64, f32)> = Vec::new();
    for c in &visible {
        match effects.iter_mut().find(|e| e.0 == c.effect_id) {
            Some(e) => e.3 += c.uptime_pct,
            None => effects.push((c.effect_id, &c.effect_name, c.ability_id, c.uptime_pct)),
        }
    }
    effects.sort_by(|a, b| b.3.total_cmp(&a.3));
    let mut rows: Vec<(&str, &str)> = Vec::new();
    for c in &visible {
        if !rows.iter().any(|r| r.0 == c.target_name) {
            rows.push((&c.target_name, &c.target_type));
        }
    }
    let rank = |(name, kind): &(&str, &str)| match *kind {
        _ if *name == source => 0,
        "Npc" => 2,
        _ => 1,
    };
    rows.sort_by(|a, b| rank(a).cmp(&rank(b)).then(a.0.cmp(b.0)));
    let cell = |target: &str, effect: i64| visible.iter().find(|c| c.target_name == target && c.effect_id == effect);
    let picked = picked_target();
    let picked_col = picked_effect().filter(|_| picked.is_none());
    let timeline: Vec<EffectUptimeCell> = visible
        .iter()
        .filter(|c| match (&picked, picked_col) {
            (Some(t), _) => c.target_name == *t,
            (None, Some(e)) => c.effect_id == e,
            _ => false,
        })
        .map(|c| (*c).clone())
        .collect();
    let subject = match (&picked, picked_col) {
        (Some(t), _) => Some(t.clone()),
        (None, Some(e)) => effects.iter().find(|x| x.0 == e).map(|x| x.1.to_string()),
        _ => None,
    };
    let (range_start, range_end) = if time_range.end > 0.0 { (time_range.start, time_range.end) } else { (0.0, duration_secs) };

    rsx! {
        div { class: "uptime-view",
            div { class: "uptime-toolbar",
                span { class: "uptime-title", "Effects applied by " strong { "{source}" } }
                div { class: "history-seg ml-auto", title: "Targets",
                    for (label, t) in [("All", Targets::All), ("Players", Targets::Players), ("NPCs", Targets::Npcs)] {
                        button { class: if shown == t { "seg active" } else { "seg" }, onclick: move |_| targets.set(t), "{label}" }
                    }
                }
            }
            if rows.is_empty() {
                div { class: "uptime-empty",
                    if cells.read().is_none() { "Loading…" } else { "No effects applied by {source} on these targets" }
                }
            } else {
                div { class: "overview-table-box uptime-table-box",
                    table { class: "overview-table uptime-table",
                        thead {
                            tr {
                                th { class: "name-col", "Target" }
                                for (id, name, ability, _) in effects.iter().copied() {
                                    th {
                                        class: if picked_col == Some(id) { "uptime-effect picked" } else { "uptime-effect" },
                                        title: "{name}: show it on every target",
                                        onclick: move |_| {
                                            let same = picked_target.peek().is_none() && *picked_effect.peek() == Some(id);
                                            picked_target.set(None);
                                            picked_effect.set((!same).then_some(id));
                                        },
                                        AbilityIcon { ability_id: if ability != 0 { ability } else { id }, size: 18 }
                                        span { "{name}" }
                                    }
                                }
                            }
                        }
                        tbody {
                            for (target, kind) in rows.iter().copied() {
                                tr {
                                    class: if picked.as_deref() == Some(target) { "uptime-row picked" } else { "uptime-row" },
                                    td {
                                        class: if kind == "Npc" { "name-col npc" } else { "name-col" },
                                        title: "Show when each effect was up on {target}",
                                        onclick: {
                                            let t = target.to_string();
                                            move |_| {
                                                let same = picked_target.peek().as_deref() == Some(t.as_str());
                                                picked_target.set((!same).then(|| t.clone()));
                                                picked_effect.set(None);
                                            }
                                        },
                                        "{target}"
                                    }
                                    for (id, name, _, _) in effects.iter().copied() {
                                        if let Some(c) = cell(target, id) {
                                            td {
                                                class: if picked.as_deref() == Some(target) && picked_effect() == Some(id) { "num uptime-cell picked" } else { "num uptime-cell" },
                                                onclick: {
                                                    let t = target.to_string();
                                                    move |_| {
                                                        picked_target.set(Some(t.clone()));
                                                        picked_effect.set(Some(id));
                                                    }
                                                },
                                                style: "--pct: {c.uptime_pct}",
                                                title: "{name} on {target}: {formatting::format_pct(c.uptime_pct as f64, european)} · {c.count} applied",
                                                "{c.uptime_pct.round()}%"
                                            }
                                        } else {
                                            td { class: "num uptime-cell none" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if let Some(subject) = subject {
                    UptimeTimeline { subject, by_target: picked.is_none(), cells: timeline, focus: picked_effect(), range_start, range_end }
                } else {
                    div { class: "usage-timeline-container",
                        div { class: "usage-timeline-hint",
                            i { class: "fa-solid fa-bars-progress" }
                            " Click a target, an effect or a cell above to see when effects were up"
                        }
                    }
                }
            }
        }
    }
}

/// Bars where effects were up, on the fight's (or range's) time axis, one lane
/// per cell, by uptime: the picked target's effects (the clicked one
/// emphasised), or with `by_target` the picked effect on each target.
#[component]
fn UptimeTimeline(
    /// The picked target, or the picked effect's name
    subject: String,
    by_target: bool,
    cells: Vec<EffectUptimeCell>,
    focus: Option<i64>,
    range_start: f32,
    range_end: f32,
) -> Element {
    let span = (range_end - range_start).max(0.001);
    let pos = move |t: f32| (t - range_start) / span * 100.0;
    let mut lanes: Vec<&EffectUptimeCell> = cells.iter().collect();
    lanes.sort_by(|a, b| b.uptime_pct.total_cmp(&a.uptime_pct));
    let ticks: Vec<f32> = (0..=4).map(|i| range_start + span * i as f32 / 4.0).collect();
    let mmss = |t: f32| formatting::format_duration(t.round() as i64);

    rsx! {
        div { class: "uptime-timeline",
            div { class: "uptime-timeline-title",
                if by_target { "Timeline of " } else { "Timeline on " }
                strong { "{subject}" }
            }
            for c in lanes {
                div { class: if !by_target && focus == Some(c.effect_id) { "uptime-lane focus" } else { "uptime-lane" },
                    if by_target {
                        div { class: "uptime-lane-label", title: "{c.target_name}",
                            span { class: if c.target_type == "Npc" { "npc" }, "{c.target_name}" }
                        }
                    } else {
                        div { class: "uptime-lane-label", title: "{c.effect_name}",
                            AbilityIcon { ability_id: if c.ability_id != 0 { c.ability_id } else { c.effect_id }, size: 16 }
                            span { "{c.effect_name}" }
                        }
                    }
                    div { class: "uptime-lane-track",
                        for (a, b) in c.windows.iter().copied() {
                            div {
                                class: "uptime-bar",
                                style: "left: {pos(a)}%; width: {pos(b) - pos(a)}%",
                                title: "{mmss(a)} – {mmss(b)} ({b - a:.0}s)",
                            }
                        }
                    }
                    span { class: "uptime-lane-pct", "{c.uptime_pct.round()}%" }
                }
            }
            div { class: "uptime-lane uptime-axis",
                div { class: "uptime-lane-label" }
                div { class: "uptime-lane-track",
                    for t in ticks.iter().copied() {
                        span { class: "uptime-tick", style: "left: {pos(t)}%", "{mmss(t)}" }
                    }
                }
                span { class: "uptime-lane-pct" }
            }
        }
    }
}
