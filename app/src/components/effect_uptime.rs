//! Effect Uptime tab: everything the sidebar's selected source applied, as a
//! table of effects (rows) × targets (columns), uptime % per cell. Answers
//! "my HoT on each tank", "my DoTs on the boss vs adds", "did the debuff stay up".

use std::collections::HashMap;

use dioxus::prelude::*;

use baras_types::formatting;

use crate::api::{self, EffectUptimeCell, TimeRange};
use crate::components::ability_icon::AbilityIcon;
use crate::components::ability_naming::{AbilityNamingPills, DEFENSIVE_ONLY, DefensivePill};
use crate::components::class_icons::get_class_icon;
use crate::components::id_menu::{IdContextMenu, IdMenu};
use crate::components::mirror_names::{display_id, display_name};

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
    let mut id_menu = use_signal(|| None::<IdMenu>);
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
        .filter(|c| !DEFENSIVE_ONLY() || c.is_defensive)
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
    // Lanes are targets for a picked effect, effects otherwise
    let timeline: Vec<UptimeLane> = visible
        .iter()
        .filter(|c| match (&picked, picked_effect()) {
            (Some(t), Some(e)) => c.target_name == *t && c.effect_id == e,
            (Some(t), None) => c.target_name == *t,
            (None, Some(e)) => c.effect_id == e,
            _ => false,
        })
        .map(|c| {
            let by_target = picked.is_none();
            UptimeLane {
                label: if by_target { c.target_name.clone() } else { display_name(c.effect_id, &c.effect_name).to_string() },
                muted: by_target && c.target_type == "Npc",
                icon: (!by_target).then(|| (display_id(c.effect_id), (c.ability_id != 0).then_some(c.ability_id))),
                uptime_pct: c.uptime_pct,
                windows: c.windows.clone(),
            }
        })
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
        let name = display_name(e, name);
        rsx! {
            span { class: "uptime-who",
                AbilityIcon { ability_id: display_id(e), fallback_id: (ability != 0).then_some(ability), size: 16 }
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
            IdContextMenu { menu: id_menu }
            div { class: "uptime-toolbar",
                span { class: "uptime-title", "Effects applied by " {who(&source)} }
                AbilityNamingPills {}
                DefensivePill {}
                div { class: "filter-pills", title: "Targets",
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
                            for (id, name, ability, _) in effects.iter().copied().map(|e| (e.0, display_name(e.0, e.1), e.2, e.3)) {
                                tr {
                                    class: if picked_row == Some(id) { "uptime-row picked" } else { "uptime-row" },
                                    oncontextmenu: move |e| id_menu.set(IdMenu::at(&e, [("Copy effect ID", id), ("Copy ability ID", ability)])),
                                    td {
                                        class: "name-col",
                                        title: "{name}: show it on every target",
                                        onclick: move |_| {
                                            let same = picked_target.peek().is_none() && *picked_effect.peek() == Some(id);
                                            picked_target.set(None);
                                            picked_effect.set((!same).then_some(id));
                                        },
                                        span { class: "name-with-icon",
                                            AbilityIcon { ability_id: display_id(id), fallback_id: (ability != 0).then_some(ability), size: 18 }
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
                    UptimeTimeline { title, lanes: timeline, range_start, range_end }
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

/// One row of an `UptimeTimeline`
#[derive(Clone, PartialEq)]
pub struct UptimeLane {
    pub label: String,
    pub muted: bool,
    /// Effect ID and the ability to fall back to for the icon
    pub icon: Option<(i64, Option<i64>)>,
    pub uptime_pct: f32,
    pub windows: Vec<(f32, f32)>,
}

/// Bars where effects were up, on the fight's (or range's) time axis, one
/// lane each, by uptime.
#[component]
pub fn UptimeTimeline(title: Element, lanes: Vec<UptimeLane>, range_start: f32, range_end: f32) -> Element {
    let span = (range_end - range_start).max(0.001);
    let pos = move |t: f32| (t - range_start) / span * 100.0;
    let mut lanes = lanes;
    lanes.sort_by(|a, b| b.uptime_pct.total_cmp(&a.uptime_pct));
    let mmss = |t: f32| formatting::format_duration(t.round() as i64);

    rsx! {
        div { class: "uptime-timeline",
            div { class: "uptime-timeline-title", {title} }
            for (i, lane) in lanes.into_iter().enumerate() {
                div {
                    class: if lane.muted { "uptime-lane muted" } else { "uptime-lane" },
                    // Lanes cycle the 16 chart series, in uptime order
                    style: "--c: var(--series-{i % 16 + 1})",
                    div { class: "uptime-lane-label", title: "{lane.label}",
                        if let Some((id, fallback_id)) = lane.icon {
                            AbilityIcon { ability_id: id, fallback_id, size: 16 }
                        }
                        span { class: if lane.muted { "npc" }, "{lane.label}" }
                    }
                    div { class: "uptime-lane-track",
                        UptimeGrid { range_start, range_end }
                        for (a, b) in lane.windows.iter().copied() {
                            div {
                                class: "uptime-bar",
                                style: "left: {pos(a)}%; width: {pos(b) - pos(a)}%",
                                title: "{mmss(a)} – {mmss(b)} ({b - a:.0}s)",
                            }
                        }
                    }
                    span { class: "uptime-lane-pct", "{lane.uptime_pct.round()}%" }
                }
            }
            UptimeAxis { range_start, range_end }
        }
    }
}

/// Round-numbered time marks across a range: (position %, combat second)
fn axis_ticks(range_start: f32, range_end: f32) -> Vec<(f32, i64)> {
    const STEPS: [i64; 12] = [1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800];
    let span = (range_end - range_start).max(0.001);
    let step = STEPS.into_iter().find(|s| span / *s as f32 <= 8.0).unwrap_or(3600);
    let first = (range_start / step as f32).ceil() as i64 * step;
    (0..)
        .map(|i| first + i * step)
        .take_while(|t| *t as f32 <= range_end)
        .map(|t| ((t as f32 - range_start) / span * 100.0, t))
        .collect()
}

/// Gridlines of a lane track, at the axis' time marks
#[component]
pub fn UptimeGrid(range_start: f32, range_end: f32) -> Element {
    rsx! {
        for (pos, _) in axis_ticks(range_start, range_end) {
            i { class: "uptime-grid", style: "left: {pos}%" }
        }
    }
}

/// Time labels under a timeline's lanes
#[component]
pub fn UptimeAxis(range_start: f32, range_end: f32) -> Element {
    rsx! {
        div { class: "uptime-lane uptime-axis",
            div { class: "uptime-lane-label" }
            div { class: "uptime-lane-track",
                for (pos, secs) in axis_ticks(range_start, range_end) {
                    span {
                        // Labels at the edges stay inside the track
                        class: if pos < 2.0 { "uptime-tick start" } else if pos > 98.0 { "uptime-tick end" } else { "uptime-tick" },
                        style: "left: {pos}%",
                        "{formatting::format_duration(secs)}"
                    }
                }
            }
            span { class: "uptime-lane-pct" }
        }
    }
}
