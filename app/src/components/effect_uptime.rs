//! Effect Uptime tab: everything the sidebar's selected source applied, as a
//! table of effects (rows) × targets (columns), uptime % per cell. Answers
//! "my HoT on each tank", "my DoTs on the boss vs adds", "did the debuff stay up".

use std::collections::HashMap;

use dioxus::prelude::*;

use baras_types::formatting;

use crate::api::{self, EffectUptimeCell, TimeRange};
use crate::components::ability_icon::AbilityIcon;
use crate::components::class_icons::get_class_icon;

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
    /// Name → discipline icon, from the overview
    icons: Memo<HashMap<String, String>>,
    european: bool,
) -> Element {
    let mut targets = use_signal(|| Targets::All);
    // Timeline selection: a row (effect), a column (target), or a cell (both;
    // shown as the target's timeline with that effect emphasised)
    let mut picked_target = use_signal(|| None::<String>);
    let mut picked_effect = use_signal(|| None::<i64>);
    let selector = selected_source
        .clone()
        .map(|n| selected_source_id.map_or(n, |id| format!("id:{id}")));

    let cells = use_resource(use_reactive!(
        |(encounter_idx, time_range, duration_secs, selector)| async move {
            let source = selector?;
            let tr = (time_range.end > 0.0).then_some(time_range);
            api::query_effect_uptime_by_target(encounter_idx, &source, tr.as_ref(), duration_secs).await
        }
    ));

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

    // Rows: effects by total uptime across targets; columns: the source
    // itself first, then players, then NPCs, each by name
    let mut effects: Vec<(i64, &str, i64, f32)> = Vec::new();
    for c in &visible {
        match effects.iter_mut().find(|e| e.0 == c.effect_id) {
            Some(e) => e.3 += c.uptime_pct,
            None => effects.push((c.effect_id, &c.effect_name, c.ability_id, c.uptime_pct)),
        }
    }
    effects.sort_by(|a, b| b.3.total_cmp(&a.3));
    let mut target_cols: Vec<(&str, &str)> = Vec::new();
    for c in &visible {
        if !target_cols.iter().any(|r| r.0 == c.target_name) {
            target_cols.push((&c.target_name, &c.target_type));
        }
    }
    let rank = |(name, kind): &(&str, &str)| match *kind {
        _ if *name == source => 0,
        "Npc" => 2,
        _ => 1,
    };
    target_cols.sort_by(|a, b| rank(a).cmp(&rank(b)).then(a.0.cmp(b.0)));
    let cell = |target: &str, effect: i64| {
        visible
            .iter()
            .find(|c| c.target_name == target && c.effect_id == effect)
    };
    let picked = picked_target();
    let picked_row = picked_effect().filter(|_| picked.is_none());
    let picked_col = picked.as_deref().filter(|_| picked_effect().is_none());
    let timeline: Vec<EffectUptimeCell> = visible
        .iter()
        .filter(|c| match (&picked, picked_effect()) {
            (Some(t), Some(e)) => c.target_name == *t && c.effect_id == e,
            (Some(t), None) => c.target_name == *t,
            (None, Some(e)) => c.effect_id == e,
            _ => false,
        })
        .map(|c| (*c).clone())
        .collect();
    let icons = icons.read();
    // Name with its discipline icon (players; NPCs have none)
    let who = |name: &str| {
        let icon = icons.get(name).and_then(|i| get_class_icon(i)).copied();
        rsx! {
            span { class: "uptime-who",
                if let Some(src) = icon {
                    img { class: "uptime-class-icon", src, alt: "" }
                }
                strong { "{name}" }
            }
        }
    };
    let effect = |e: i64| {
        let (name, ability) = effects.iter().find(|x| x.0 == e).map_or(("", 0), |x| (x.1, x.2));
        rsx! {
            span { class: "uptime-who",
                AbilityIcon { ability_id: e, fallback_id: (ability != 0).then_some(ability), size: 16 }
                strong { "{name}" }
            }
        }
    };
    // Timeline heading: the effect when one is picked, then source → target(s)
    let arrow = rsx! { i { class: "fa-solid fa-arrow-right uptime-arrow" } };
    let title = match (&picked, picked_effect()) {
        (Some(t), Some(e)) => Some(rsx! { {effect(e)} {who(&source)} {arrow} {who(t)} }),
        (Some(t), None) => Some(rsx! { {who(&source)} {arrow} {who(t)} }),
        (None, Some(e)) => Some(rsx! {
            {effect(e)}
            {who(&source)}
            {arrow}
            span { class: "uptime-every", "every target" }
        }),
        _ => None,
    };
    let (range_start, range_end) = if time_range.end > 0.0 {
        (time_range.start, time_range.end)
    } else {
        (0.0, duration_secs)
    };

    rsx! {
        div { class: "uptime-view",
            div { class: "uptime-toolbar",
                span { class: "uptime-title", "Effects applied by " {who(&source)} }
                div { class: "filter-pills ml-auto", title: "Targets",
                    for (label, t) in [("All", Targets::All), ("Players", Targets::Players), ("NPCs", Targets::Npcs)] {
                        button { class: if shown == t { "seg active" } else { "seg" }, onclick: move |_| targets.set(t), "{label}" }
                    }
                }
            }
            if effects.is_empty() {
                div { class: "uptime-empty",
                    if cells.read().is_none() { span { class: "loading-late", "Loading…" } } else { "No effects applied by {source} on these targets" }
                }
            } else {
                div { class: "overview-table-box uptime-table-box",
                    table { class: "overview-table uptime-table",
                        thead {
                            tr {
                                th { class: "name-col", "Effect" }
                                for (target, kind) in target_cols.iter().copied() {
                                    th {
                                        class: match (picked_col == Some(target), kind == "Npc") {
                                            (true, _) => "uptime-target picked",
                                            (false, true) => "uptime-target npc",
                                            _ => "uptime-target",
                                        },
                                        title: "Show when each effect was up on {target}",
                                        onclick: {
                                            let t = target.to_string();
                                            move |_| {
                                                let same = picked_target.peek().as_deref() == Some(t.as_str())
                                                    && picked_effect.peek().is_none();
                                                picked_target.set((!same).then(|| t.clone()));
                                                picked_effect.set(None);
                                            }
                                        },
                                        span { "{target}" }
                                    }
                                }
                            }
                        }
                        tbody {
                            for (id, name, ability, _) in effects.iter().copied() {
                                tr {
                                    class: if picked_row == Some(id) { "uptime-row picked" } else { "uptime-row" },
                                    td {
                                        class: "name-col",
                                        title: "{name}: show it on every target",
                                        onclick: move |_| {
                                            let same = picked_target.peek().is_none() && *picked_effect.peek() == Some(id);
                                            picked_target.set(None);
                                            picked_effect.set((!same).then_some(id));
                                        },
                                        span { class: "name-with-icon",
                                            AbilityIcon { ability_id: id, fallback_id: (ability != 0).then_some(ability), size: 18 }
                                            span { class: "name-text", "{name}" }
                                        }
                                    }
                                    for (target, _) in target_cols.iter().copied() {
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
                if let Some(title) = title {
                    UptimeTimeline { title, by_target: picked.is_none(), cells: timeline, range_start, range_end }
                } else {
                    div { class: "usage-timeline-container",
                        div { class: "usage-timeline-hint",
                            i { class: "fa-solid fa-bars-progress" }
                            " Click an effect, a target or a cell above to see when effects were up"
                        }
                    }
                }
            }
        }
    }
}

/// Bars where effects were up, on the fight's (or range's) time axis, one lane
/// per cell, by uptime: a row's effects, a column's targets (`by_target`), or
/// the one clicked cell.
#[component]
fn UptimeTimeline(
    /// The picked effect and/or source → target(s)
    title: Element,
    by_target: bool,
    cells: Vec<EffectUptimeCell>,
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
            div { class: "uptime-timeline-title", {title} }
            for c in lanes {
                div { class: "uptime-lane",
                    if by_target {
                        div { class: "uptime-lane-label", title: "{c.target_name}",
                            span { class: if c.target_type == "Npc" { "npc" }, "{c.target_name}" }
                        }
                    } else {
                        div { class: "uptime-lane-label", title: "{c.effect_name}",
                            AbilityIcon { ability_id: c.effect_id, fallback_id: (c.ability_id != 0).then_some(c.ability_id), size: 16 }
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
